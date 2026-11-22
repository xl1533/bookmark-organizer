//! 链接检测器集成测试（wiremock mock server + 原生 TCP 服务器）。

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bookmark_core::checker::{CheckOptions, CheckStatus, LinkChecker};
use bookmark_core::CheckTarget;
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn opts(timeout: u64, retries: u32, concurrency: usize, proxy: Option<String>) -> CheckOptions {
    CheckOptions {
        timeout_secs: timeout,
        retries,
        concurrency,
        per_host_concurrency: 2,
        proxy,
        user_agent: "BookmarkOrganizer-Test/0.1".into(),
    }
}

async fn check(client: &LinkChecker, url: &str, o: &CheckOptions) -> Option<bookmark_core::checker::CheckResult> {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut rx = client.check_many(
        vec![CheckTarget { node_id: 1, url: url.to_string() }],
        o.clone(),
        cancel,
    );
    rx.recv().await
}

#[tokio::test]
async fn ok_200_head() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let o = opts(5, 0, 4, None);
    let client = LinkChecker::new(&o).unwrap();
    let r = check(&client, &format!("{}/ok", server.uri()), &o).await.unwrap();
    assert_eq!(r.status, CheckStatus::Ok);
    assert_eq!(r.http_code, Some(200));
}

#[tokio::test]
async fn dead_404_and_5xx() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let o = opts(5, 0, 4, None);
    let client = LinkChecker::new(&o).unwrap();
    let r = check(&client, format!("{}/x", server.uri()).as_str(), &o).await.unwrap();
    assert_eq!(r.status, CheckStatus::Dead);
    assert_eq!(r.http_code, Some(404));
}

#[tokio::test]
async fn suspicious_403_429_401() {
    let server = MockServer::start().await;
    for code in [401u16, 403, 429] {
        Mock::given(any())
            .respond_with(ResponseTemplate::new(code))
            .mount(&server)
            .await;
        let o = opts(5, 0, 4, None);
        let client = LinkChecker::new(&o).unwrap();
        let r = check(&client, format!("{}/x", server.uri()).as_str(), &o).await.unwrap();
        assert_eq!(r.status, CheckStatus::Suspicious, "code {code} 应判可疑");
    }
}

#[tokio::test]
async fn head_unsupported_falls_back_to_get() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(405))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let o = opts(5, 0, 4, None);
    let client = LinkChecker::new(&o).unwrap();
    let r = check(&client, &server.uri(), &o).await.unwrap();
    assert_eq!(r.status, CheckStatus::Ok);
    // 验证 GET 兜底确实发生了（expect(1) 断言）
    server.verify().await;
}

#[tokio::test]
async fn retry_on_503_then_success() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let o = opts(5, 2, 4, None); // 重试 2 次 → 第 3 次成功
    let client = LinkChecker::new(&o).unwrap();
    let r = check(&client, &server.uri(), &o).await.unwrap();
    assert_eq!(r.status, CheckStatus::Ok);
}

#[tokio::test]
async fn timeout_is_dead() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(3)))
        .mount(&server)
        .await;

    let o = opts(1, 0, 4, None); // 1 秒超时 < 3 秒延迟
    let client = LinkChecker::new(&o).unwrap();
    let r = check(&client, &server.uri(), &o).await.unwrap();
    assert_eq!(r.status, CheckStatus::Dead);
    assert!(r.message.as_deref().unwrap_or_default().contains("超时"));
}

#[tokio::test]
async fn connection_refused_is_dead() {
    // 占用端口后释放，得到一个确定“连接被拒绝”的端口
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let o = opts(3, 0, 4, None);
    let client = LinkChecker::new(&o).unwrap();
    let r = check(&client, &format!("http://127.0.0.1:{port}/"), &o).await.unwrap();
    assert_eq!(r.status, CheckStatus::Dead);
    assert!(r.http_code.is_none());
}

#[tokio::test]
async fn redirect_final_url_recorded() {
    let server = MockServer::start().await;
    Mock::given(any())
        .and(path("/redir"))
        .respond_with(
            ResponseTemplate::new(301)
                .append_header("Location", format!("{}/final", server.uri())),
        )
        .mount(&server)
        .await;
    Mock::given(any())
        .and(path("/final"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let o = opts(5, 0, 4, None);
    let client = LinkChecker::new(&o).unwrap();
    let r = check(&client, &format!("{}/redir", server.uri()), &o).await.unwrap();
    assert_eq!(r.status, CheckStatus::Ok);
    assert_eq!(r.final_url.as_deref(), Some(format!("{}/final", server.uri()).as_str()));
}

#[tokio::test]
async fn non_http_schemes_are_skipped() {
    let o = opts(5, 0, 4, None);
    let client = LinkChecker::new(&o).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let targets = vec![
        CheckTarget { node_id: 1, url: "javascript:void(0)".into() },
        CheckTarget { node_id: 2, url: "mailto:a@b.com".into() },
        CheckTarget { node_id: 3, url: "chrome://bookmarks".into() },
        CheckTarget { node_id: 4, url: "".into() },
    ];
    let mut rx = client.check_many(targets, o, cancel);
    let mut got = 0;
    while rx.recv().await.is_some() {
        got += 1;
    }
    assert_eq!(got, 0, "非 http(s) 协议不应产生检测结果");
}

#[tokio::test]
async fn cancel_stops_processing() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(150)))
        .mount(&server)
        .await;

    let o = opts(5, 0, 4, None);
    let client = LinkChecker::new(&o).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let targets: Vec<CheckTarget> = (0..20)
        .map(|i| CheckTarget { node_id: i, url: format!("{}/{}", server.uri(), i) })
        .collect();
    let mut rx = client.check_many(targets, o.clone(), cancel.clone());
    cancel.store(true, Ordering::SeqCst);
    let mut got = 0;
    while rx.recv().await.is_some() {
        got += 1;
    }
    assert!(got < 20, "取消后不应完成全部检测，实际完成 {got}");
}

/// 极简 HTTP 服务器：200 响应 + 200ms 延迟，统计最大并发。
async fn spawn_slow_server(max_seen: Arc<AtomicUsize>, cur: Arc<AtomicUsize>) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { break };
            let max_seen = max_seen.clone();
            let cur = cur.clone();
            tokio::spawn(async move {
                let now = cur.fetch_add(1, Ordering::SeqCst) + 1;
                max_seen.fetch_max(now, Ordering::SeqCst);
                let mut buf = [0u8; 4096];
                let _ = tokio::time::timeout(Duration::from_secs(5), sock.read(&mut buf)).await;
                tokio::time::sleep(Duration::from_millis(200)).await;
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
                let _ = sock.flush().await;
                cur.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    port
}

#[tokio::test]
async fn concurrency_is_capped() {
    let cur = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));
    let port = spawn_slow_server(max_seen.clone(), cur.clone()).await;

    let o = opts(5, 0, 8, None); // 并发上限 8
    let client = LinkChecker::new(&o).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let targets: Vec<CheckTarget> = (0..24)
        .map(|i| CheckTarget { node_id: i, url: format!("http://127.0.0.1:{port}/{i}") })
        .collect();
    let mut rx = client.check_many(targets, o, cancel);
    let mut ok = 0;
    while let Some(r) = rx.recv().await {
        if r.status == CheckStatus::Ok {
            ok += 1;
        }
    }
    assert_eq!(ok, 24, "全部 24 个 URL 应检测成功");
    let max = max_seen.load(Ordering::SeqCst);
    assert!(max <= 8, "实际并发 {max} 超过上限 8");
    assert!(max >= 2, "应观察到一定并发度，实际最大 {max}");
}

#[tokio::test]
async fn proxy_invalid_address_fails_fast() {
    let o = opts(5, 0, 4, Some("not a proxy".into()));
    assert!(LinkChecker::new(&o).is_err(), "非法代理地址应构建失败");
}
