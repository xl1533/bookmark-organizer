//! 链接有效性检测。
//!
//! 规则（第 1 期）：
//! - HEAD 优先，405/501（方法不支持）时降级 GET；GET 仅读取响应头，不读取 body，
//!   不执行任何 JS，不提交任何数据
//! - HTTP 401/403/429 判「可疑」（登录墙 / 风控 / Cloudflare 拦截，浏览器可访问，
//!   直接判失效有误删风险）；其余 4xx/5xx、DNS 失败、连接/读取超时判「失效」
//! - 连接与整体超时 10 秒；失败（网络错误与 5xx）重试 2 次，退避 400ms×次数
//! - 并发上限默认 8，通过令牌桶（Semaphore）控制
//! - 记录最终 URL（跟随重定向），供第 2 期「重定向链接自动更新」使用
//!
//! 判定局限（UI 中需注明）：HTTP 检测无法识别 Cloudflare 拦截页和 SPA 假 200，
//! 属尽力检测。

use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use futures::StreamExt;
use serde::Serialize;
use tokio::sync::{mpsc, Semaphore};

use crate::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// 正常（2xx）
    Ok,
    /// 可疑（401/403/429 等登录墙 / 风控；1xx；裸 3xx）
    Suspicious,
    /// 失效（其余 4xx/5xx、DNS 失败、超时、无法连接）
    Dead,
}

impl CheckStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckStatus::Ok => "ok",
            CheckStatus::Suspicious => "suspicious",
            CheckStatus::Dead => "dead",
        }
    }
}

/// 单条检测结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub node_id: NodeId,
    pub url: String,
    pub status: CheckStatus,
    pub http_code: Option<u16>,
    /// 跟随重定向后的最终地址（与原地址不同时记录）
    pub final_url: Option<String>,
    pub message: Option<String>,
    pub elapsed_ms: u64,
    pub checked_at: i64,
}

/// 待检测目标。
#[derive(Debug, Clone)]
pub struct CheckTarget {
    pub node_id: NodeId,
    pub url: String,
}

/// 检测配置。
#[derive(Debug, Clone)]
pub struct CheckOptions {
    /// 单次尝试的整体超时（秒），默认 10
    pub timeout_secs: u64,
    /// 失败重试次数，默认 2（总尝试 = 1 + retries）
    pub retries: u32,
    /// 并发上限，默认 8
    pub concurrency: usize,
    /// HTTP 代理，如 http://host:port 或 http://user:pass@host:port 或 socks5://...
    pub proxy: Option<String>,
    pub user_agent: String,
}

impl Default for CheckOptions {
    fn default() -> Self {
        CheckOptions {
            timeout_secs: 10,
            retries: 2,
            concurrency: 8,
            proxy: None,
            user_agent: "Mozilla/5.0 (compatible; BookmarkOrganizer/0.1; +link-check)".to_string(),
        }
    }
}

const MAX_REDIRECTS: usize = 10;

pub struct LinkChecker {
    client: reqwest::Client,
}

impl LinkChecker {
    /// 根据配置构建 HTTP 客户端（rustls，避免依赖系统 OpenSSL）。
    pub fn new(opts: &CheckOptions) -> Result<Self, String> {
        let timeout = Duration::from_secs(opts.timeout_secs.max(1));
        let mut builder = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout)
            .user_agent(opts.user_agent.as_str())
            .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS));

        if let Some(proxy) = &opts.proxy {
            let p = reqwest::Proxy::all(proxy)
                .map_err(|e| format!("代理地址无效: {e}"))?;
            builder = builder.proxy(p);
        } else {
            builder = builder.no_proxy();
        }

        let client = builder.build().map_err(|e| format!("HTTP 客户端构建失败: {e}"))?;
        Ok(LinkChecker { client })
    }

    /// 批量检测：内部按并发上限调度，结果通过 mpsc 通道实时返回。
    ///
    /// 必须在 tokio 运行时内调用。检测过程中持续检查 `cancel`，
    /// 取消后未开始的 URL 不再发起，未完成的 URL 不产生结果。
    pub fn check_many(
        &self,
        targets: Vec<CheckTarget>,
        opts: CheckOptions,
        cancel: Arc<AtomicBool>,
    ) -> mpsc::Receiver<CheckResult> {
        let (tx, rx) = mpsc::channel::<CheckResult>(32);
        let semaphore = Arc::new(Semaphore::new(opts.concurrency.max(1)));
        let client = self.client.clone();

        tokio::spawn(async move {
            let in_flight = Arc::new(AtomicUsize::new(0));
            let mut stream = futures::stream::iter(targets)
                .map(|target| {
                    let permit_sem = semaphore.clone();
                    let client = client.clone();
                    let tx = tx.clone();
                    let cancel = cancel.clone();
                    let in_flight = in_flight.clone();
                    let opts = opts.clone();
                    async move {
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        // 在任务内获取许可，保证同时在网的请求 ≤ concurrency
                        let _permit = permit_sem.acquire_owned().await;
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        let cur = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                        let result = check_one(&client, &target, &opts, &cancel).await;
                        in_flight.fetch_sub(1, Ordering::SeqCst);
                        let _ = cur;
                        if let Some(result) = result {
                            let _ = tx.send(result).await;
                        }
                    }
                })
                .buffer_unordered(opts.concurrency.max(1));

            while stream.next().await.is_some() {}
        });

        rx
    }
}

fn classify_status(code: u16) -> (CheckStatus, Option<String>) {
    if (200..300).contains(&code) {
        (CheckStatus::Ok, None)
    } else if matches!(code, 401 | 403 | 429) {
        (
            CheckStatus::Suspicious,
            Some("登录墙/风控拦截（401/403/429），浏览器中可能可访问".to_string()),
        )
    } else if (400..500).contains(&code) {
        (CheckStatus::Dead, Some(format!("HTTP {code}")) )
    } else if (500..600).contains(&code) {
        (CheckStatus::Dead, Some(format!("HTTP {code}（服务器错误）")))
    } else {
        // 1xx / 裸 3xx
        (CheckStatus::Suspicious, Some(format!("异常状态码 {code}")))
    }
}

fn classify_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "连接或读取超时".to_string()
    } else if err.is_connect() {
        if error_chain_contains(err, "dns") {
            "DNS 解析失败".to_string()
        } else {
            "连接失败（DNS/网络不可达/拒绝连接）".to_string()
        }
    } else if err.is_redirect() {
        "重定向次数过多或重定向循环".to_string()
    } else {
        format!("网络错误: {err}")
    }
}

fn error_chain_contains(err: &dyn std::error::Error, needle: &str) -> bool {
    let mut cur: Option<&dyn std::error::Error> = Some(err);
    while let Some(e) = cur {
        if e.to_string().to_lowercase().contains(needle) {
            return true;
        }
        cur = e.source();
    }
    false
}

/// 检测单个 URL。取消时返回 None（不产生结果）。
async fn check_one(
    client: &reqwest::Client,
    target: &CheckTarget,
    opts: &CheckOptions,
    cancel: &AtomicBool,
) -> Option<CheckResult> {
    let url = target.url.trim();
    if url.is_empty() {
        return None;
    }
    let lower = url.to_lowercase();
    let is_http = lower.starts_with("http://") || lower.starts_with("https://");
    if !is_http {
        // 非 HTTP 协议（javascript:/mailto:/chrome:/place: 等）不检测，
        // 也不写入结果（UI 显示为“未检测”）
        return None;
    }

    let started = Instant::now();
    let attempts = 1 + opts.retries.max(0);
    let mut last_error: Option<String> = None;

    for attempt in 0..attempts {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        if attempt > 0 {
            let backoff = Duration::from_millis(400 * attempt as u64);
            tokio::time::sleep(backoff).await;
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
        }

        // 1) HEAD 优先
        match client.head(url).send().await {
            Ok(resp) => {
                let code = resp.status().as_u16();
                let final_url = resp.url().to_string();
                if code == 405 || code == 501 {
                    // 2) HEAD 不支持 → GET 兜底（不读 body）
                    match client.get(url).send().await {
                        Ok(get_resp) => {
                            let gcode = get_resp.status().as_u16();
                            let gfinal = get_resp.url().to_string();
                            let (status, msg) = classify_status(gcode);
                            if status != CheckStatus::Dead || !should_retry_code(gcode) {
                                return Some(finish(target, url, status, Some(gcode), &gfinal, msg, started));
                            }
                            last_error = msg;
                        }
                        Err(e) => {
                            last_error = Some(classify_error(&e));
                        }
                    }
                } else {
                    let (status, msg) = classify_status(code);
                    if status != CheckStatus::Dead || !should_retry_code(code) {
                        return Some(finish(target, url, status, Some(code), &final_url, msg, started));
                    }
                    last_error = msg;
                }
            }
            Err(e) => {
                last_error = Some(classify_error(&e));
            }
        }
        // 5xx / 网络错误进入下一轮重试
    }

    let elapsed = started.elapsed().as_millis() as u64;
    Some(CheckResult {
        node_id: target.node_id,
        url: url.to_string(),
        status: CheckStatus::Dead,
        http_code: None,
        final_url: None,
        message: last_error,
        elapsed_ms: elapsed,
        checked_at: crate::now_ts(),
    })
}

fn should_retry_code(code: u16) -> bool {
    (500..600).contains(&code) || code == 408
}

fn finish(
    target: &CheckTarget,
    original: &str,
    status: CheckStatus,
    http_code: Option<u16>,
    final_url: &str,
    message: Option<String>,
    started: Instant,
) -> CheckResult {
    let final_url = if final_url != original && !final_url.is_empty() {
        Some(final_url.to_string())
    } else {
        None
    };
    CheckResult {
        node_id: target.node_id,
        url: original.to_string(),
        status,
        http_code,
        final_url,
        message,
        elapsed_ms: started.elapsed().as_millis() as u64,
        checked_at: crate::now_ts(),
    }
}

/// 测试代理连通性：返回延迟毫秒，失败返回错误信息。
/// 探测目标 https://www.baidu.com（HEAD，10 秒超时），不提交任何数据。
pub async fn test_proxy(proxy: Option<&str>) -> Result<u64, String> {
    let opts = CheckOptions {
        proxy: proxy.map(|s| s.to_string()),
        ..Default::default()
    };
    let checker = LinkChecker::new(&opts)?;
    let started = Instant::now();
    let resp = checker
        .client
        .head("https://www.baidu.com")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| classify_error(&e))?;
    let _ = resp.status();
    Ok(started.elapsed().as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify() {
        assert_eq!(classify_status(200).0, CheckStatus::Ok);
        assert_eq!(classify_status(204).0, CheckStatus::Ok);
        assert_eq!(classify_status(404).0, CheckStatus::Dead);
        assert_eq!(classify_status(410).0, CheckStatus::Dead);
        assert_eq!(classify_status(500).0, CheckStatus::Dead);
        assert_eq!(classify_status(502).0, CheckStatus::Dead);
        assert_eq!(classify_status(403).0, CheckStatus::Suspicious);
        assert_eq!(classify_status(401).0, CheckStatus::Suspicious);
        assert_eq!(classify_status(429).0, CheckStatus::Suspicious);
        assert!(should_retry_code(503));
        assert!(should_retry_code(408));
        assert!(!should_retry_code(404));
    }
}
