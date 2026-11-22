//! #14 WebDAV 备份同步客户端。
//!
//! - 仅使用 Basic 认证（用户名 + 密码/应用密码），凭据不写入 URL，避免日志泄露
//! - 默认仅允许 https；只有 localhost / 127.0.0.1 在 `allow_http = true` 时允许 http
//! - `PROPFIND` 做连接测试与目录列举，`PUT` 上传时间戳备份，`GET` 下载备份
//! - 复用设置中的 HTTP 代理（`reqwest::Proxy::all`），与链接检测保持一致
//! - PROPFIND 响应用轻量字符串解析（不新增 XML 依赖，见设计 §8 Q1）

use std::time::{Duration, Instant};

use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{CoreError, CoreResult};

/// WebDAV 连接配置。密码仅存本机 settings；不实现 `Display`，避免被日志输出。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavConfig {
    /// 服务器根 URL，例如 `https://dav.example.com/remote.php/dav`
    pub url: String,
    pub user: String,
    pub password: String,
    /// 远端目录（相对服务器 URL），如 `bookmarks`；`/` 表示根目录
    pub dir: String,
    /// 是否允许 http（仅对 localhost / 127.0.0.1 生效）
    pub allow_http: bool,
}

/// 远端 HTML 备份文件摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavFile {
    pub name: String,
    /// 字节数，解析失败时为 0
    pub size: i64,
    /// Unix 秒，解析失败时为 0
    pub modified: i64,
}

/// WebDAV 客户端：Basic 认证 + 可选代理，默认仅 https。
pub struct WebDavClient {
    cfg: WebDavConfig,
    base: Url,
    client: Client,
}

impl WebDavClient {
    /// 构造客户端并校验 URL 安全策略。
    ///
    /// `proxy` 为非空字符串时通过 `reqwest::Proxy::all` 复用代理设置。
    pub fn new(cfg: WebDavConfig, proxy: Option<String>) -> CoreResult<Self> {
        let base = validate_url(&cfg)?;
        let timeout = Duration::from_secs(20);
        let mut builder = Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout)
            .redirect(reqwest::redirect::Policy::limited(5));

        let proxy = proxy.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        if let Some(proxy) = proxy {
            let p = reqwest::Proxy::all(&proxy)
                .map_err(|e| CoreError::Other(format!("代理地址无效: {e}")))?;
            builder = builder.proxy(p);
        } else {
            builder = builder.no_proxy();
        }

        let client = builder
            .build()
            .map_err(|e| CoreError::Other(format!("HTTP 客户端构建失败: {e}")))?;
        Ok(WebDavClient { cfg, base, client })
    }

    /// 连接测试：`PROPFIND <dir>` + `Depth: 0`，返回耗时毫秒。
    pub async fn test(&self) -> CoreResult<i64> {
        let url = self.collection_url()?;
        let started = Instant::now();
        let resp = self
            .request(propfind_method()?, url)
            .header("Depth", "0")
            .send()
            .await
            .map_err(|e| CoreError::Other(describe_request_error(&e, "连接测试")))?;
        ensure_success(resp.status(), "连接测试")?;
        Ok(started.elapsed().as_millis() as i64)
    }

    /// 上传备份：`PUT <dir>/<file_name>`，带 `If-None-Match: *` 保证永不覆盖。
    ///
    /// `file_name` 只接受单个文件名（不接受路径分隔符）；调用方应传带时间戳的备份文件名。
    /// 返回远端路径（URL 的 path 部分）。
    pub async fn upload(&self, file_name: &str, bytes: &[u8]) -> CoreResult<String> {
        validate_file_name(file_name)?;
        let url = self.file_url(file_name)?;
        let resp = self
            .request(Method::PUT, url.clone())
            .header("If-None-Match", "*")
            .body(bytes.to_vec())
            .send()
            .await
            .map_err(|e| CoreError::Other(describe_request_error(&e, "上传备份")))?;
        ensure_success(resp.status(), "上传备份")?;
        Ok(url.path().to_string())
    }

    /// 列出远端 HTML 备份：`PROPFIND <dir>` + `Depth: 1`，仅返回 `.html` / `.htm`。
    pub async fn list(&self) -> CoreResult<Vec<WebDavFile>> {
        let url = self.collection_url()?;
        let resp = self
            .request(propfind_method()?, url)
            .header("Depth", "1")
            .send()
            .await
            .map_err(|e| CoreError::Other(describe_request_error(&e, "读取远端列表")))?;
        ensure_success(resp.status(), "读取远端列表")?;
        let body = resp
            .text()
            .await
            .map_err(|e| CoreError::Other(describe_request_error(&e, "读取远端列表")))?;
        Ok(parse_propfind(&body))
    }

    /// 下载远端备份字节：`GET <dir>/<name>`。
    pub async fn download(&self, name: &str) -> CoreResult<Vec<u8>> {
        validate_file_name(name)?;
        let url = self.file_url(name)?;
        let resp = self
            .request(Method::GET, url)
            .send()
            .await
            .map_err(|e| CoreError::Other(describe_request_error(&e, "下载备份")))?;
        ensure_success(resp.status(), "下载备份")?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CoreError::Other(describe_request_error(&e, "下载备份")))?;
        Ok(bytes.to_vec())
    }

    /// 基本认证请求构造器：用户名/密码只进 Authorization 头，不进 URL。
    fn request(&self, method: Method, url: Url) -> reqwest::RequestBuilder {
        self.client
            .request(method, url)
            .basic_auth(self.cfg.user.as_str(), Some(self.cfg.password.as_str()))
    }

    /// 目录 URL（保证以 `/` 结尾，`dir = "/"` 时就是服务器 URL 的根）。
    fn collection_url(&self) -> CoreResult<Url> {
        let mut url = self.base.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| CoreError::Other("WebDAV 服务器 URL 不能作为目录使用".to_string()))?;
            segments.pop_if_empty();
            for part in self.cfg.dir.trim_matches('/').split('/') {
                if !part.is_empty() {
                    segments.push(part);
                }
            }
            segments.push("");
        }
        Ok(url)
    }

    /// 目录内某个文件的 URL。
    fn file_url(&self, name: &str) -> CoreResult<Url> {
        let mut url = self.collection_url()?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| CoreError::Other("WebDAV 服务器 URL 不能作为目录使用".to_string()))?;
            segments.pop_if_empty();
            segments.push(name);
        }
        Ok(url)
    }
}

/// URL 安全校验：默认仅 https；http 仅 localhost/127.0.0.1 且显式允许。
fn validate_url(cfg: &WebDavConfig) -> CoreResult<Url> {
    if cfg.url.trim().is_empty() {
        return Err(CoreError::Other("请先配置 WebDAV 服务器 URL".to_string()));
    }
    let mut url = Url::parse(cfg.url.trim())
        .map_err(|_| CoreError::Other("WebDAV 服务器 URL 无效，请填写完整地址".to_string()))?;

    // 禁止在 URL 里附带凭据（reqwest 错误可能回显 URL，避免密码泄露到日志）
    if !url.username().is_empty() || url.password().is_some() {
        return Err(CoreError::Other(
            "请勿在 WebDAV URL 中写入用户名/密码，请填写到username/password字段".to_string(),
        ));
    }
    url.set_username("").map_err(|_| {
        CoreError::Other("WebDAV 服务器 URL 无效（无法清除内嵌凭据）".to_string())
    })?;
    url.set_password(None).map_err(|_| {
        CoreError::Other("WebDAV 服务器 URL 无效（无法清除内嵌凭据）".to_string())
    })?;
    url.set_query(None);
    url.set_fragment(None);

    match url.scheme() {
        "https" => Ok(url),
        "http" => {
            let host = url.host_str().unwrap_or_default();
            if cfg.allow_http && (host == "localhost" || host == "127.0.0.1") {
                Ok(url)
            } else {
                Err(CoreError::Other(
                    "出于安全考虑 WebDAV 默认仅允许 https；仅 localhost / 127.0.0.1 可在显式允许 http 后使用"
                        .to_string(),
                ))
            }
        }
        _ => Err(CoreError::Other(
            "WebDAV 仅支持 https（本地 localhost / 127.0.0.1 可显式允许 http）".to_string(),
        )),
    }
}

/// 文件名合法性：只接受单个文件名，拒绝路径穿越。
fn validate_file_name(name: &str) -> CoreResult<()> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed == "."
        || trimmed == ".."
    {
        return Err(CoreError::Other("非法的远端文件名".to_string()));
    }
    Ok(())
}

fn propfind_method() -> CoreResult<Method> {
    Method::from_bytes(b"PROPFIND")
        .map_err(|e| CoreError::Other(format!("WebDAV 方法构造失败: {e}")))
}

fn ensure_success(status: StatusCode, action: &str) -> CoreResult<()> {
    if status.is_success() {
        Ok(())
    } else {
        Err(CoreError::Other(describe_status(status, action)))
    }
}

/// HTTP 状态码 → 可读错误（不含密码）。
fn describe_status(status: StatusCode, action: &str) -> String {
    let code = status.as_u16();
    match code {
        401 | 403 => format!("{action}失败：认证失败或权限不足（HTTP {code}）"),
        404 => format!("{action}失败：远端目录或文件不存在（HTTP 404）"),
        405 => format!("{action}失败：服务器不支持该 WebDAV 方法（HTTP 405）"),
        412 => format!("{action}失败：远端已存在同名文件，按“永不覆盖”策略未上传（HTTP 412）"),
        500..=599 => format!("{action}失败：服务器错误（HTTP {code}）"),
        _ => format!("{action}失败：HTTP {code}"),
    }
}

/// reqwest 错误 → 可读错误（证书 / 超时 / 连接失败分别提示）。
fn describe_request_error(err: &reqwest::Error, action: &str) -> String {
    if error_chain_contains(err, "certificate")
        || error_chain_contains(err, "rustls")
        || error_chain_contains(err, "invalid peer")
    {
        return format!("{action}失败：证书校验或 TLS 握手失败，请检查服务器证书与系统时间");
    }
    if err.is_timeout() {
        return format!("{action}失败：连接或读取超时，请检查服务器与代理设置");
    }
    if err.is_connect() {
        return format!("{action}失败：无法连接服务器（DNS / 网络不可达 / 端口拒绝）");
    }
    if err.is_builder() {
        return format!("{action}失败：请求配置无效，请检查 URL 与代理设置");
    }
    format!("{action}失败：{err}")
}

fn error_chain_contains(err: &dyn std::error::Error, needle: &str) -> bool {
    let mut current: Option<&dyn std::error::Error> = Some(err);
    while let Some(e) = current {
        if e.to_string().to_lowercase().contains(needle) {
            return true;
        }
        current = e.source();
    }
    false
}

/// 轻量解析 PROPFIND XML：按 `<d:response>` 块取 href / getcontentlength / getlastmodified。
/// 支持带命名空间前缀（d: / D:）与默认命名空间（无前缀）的写法。
fn parse_propfind(xml: &str) -> Vec<WebDavFile> {
    let mut out = Vec::new();
    for block in find_tag_values(xml, "response", true) {
        let href = match find_tag_values(&block, "href", false).into_iter().next() {
            Some(href) => href,
            None => continue,
        };
        let name = name_from_href(&href);
        if name.is_empty() {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if !lower.ends_with(".html") && !lower.ends_with(".htm") {
            continue;
        }
        let size = find_tag_values(&block, "getcontentlength", false)
            .into_iter()
            .next()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .unwrap_or(0);
        let modified = find_tag_values(&block, "getlastmodified", false)
            .into_iter()
            .next()
            .and_then(|s| chrono::DateTime::parse_from_rfc2822(s.trim()).ok())
            .map(|dt| dt.timestamp())
            .unwrap_or(0);
        out.push(WebDavFile {
            name,
            size,
            modified,
        });
    }
    out
}

/// 从 href 取最后一个路径段并做百分号解码 / XML 实体反转义。
fn name_from_href(href: &str) -> String {
    let clean = href.split(|c| c == '?' || c == '#').next().unwrap_or(href);
    let last = clean.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    percent_decode(&xml_unescape(last))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push(hi * 16 + lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        // &amp; 最后替换，避免把 &amp;lt; 二次解码成 '<'
        .replace("&amp;", "&")
}

/// 找出所有 `<前缀:local>` 元素的文本内容；`all=false` 时只返回第一个。
/// 不解析属性语义，仅用于 WebDAV PROPFIND 的轻量字段提取。
fn find_tag_values(xml: &str, local: &str, all: bool) -> Vec<String> {
    let bytes = xml.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if xml[i..].starts_with("<!--") {
            match xml[i + 4..].find("-->") {
                Some(end) => i += 4 + end + 3,
                None => break,
            }
            continue;
        }
        if xml[i..].starts_with("<?") {
            match xml[i + 2..].find("?>") {
                Some(end) => i += 2 + end + 2,
                None => break,
            }
            continue;
        }
        if xml[i..].starts_with("<!") {
            match xml[i..].find('>') {
                Some(end) => i += end + 1,
                None => break,
            }
            continue;
        }

        let closing = i + 1 < bytes.len() && bytes[i + 1] == b'/';
        let name_start = if closing { i + 2 } else { i + 1 };
        let mut j = name_start;
        while j < bytes.len()
            && !matches!(bytes[j], b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n')
        {
            j += 1;
        }
        let name = &xml[name_start..j];
        let gt = match xml[j..].find('>') {
            Some(rel) => j + rel,
            None => break,
        };

        if !closing
            && local_name(name).eq_ignore_ascii_case(local)
            && !xml[j..gt].trim_end().ends_with('/')
        {
            let content_start = gt + 1;
            let mut k = content_start;
            while k + 1 < bytes.len() {
                if bytes[k] == b'<' && bytes[k + 1] == b'/' {
                    let close_start = k + 2;
                    let mut m = close_start;
                    while m < bytes.len()
                        && !matches!(bytes[m], b'>' | b' ' | b'\t' | b'\r' | b'\n')
                    {
                        m += 1;
                    }
                    let close_name = &xml[close_start..m];
                    if local_name(close_name).eq_ignore_ascii_case(local) {
                        out.push(xml_unescape(xml[content_start..k].trim()));
                        if !all {
                            return out;
                        }
                        i = k + 2;
                        break;
                    }
                }
                k += 1;
            }
            if k + 1 < bytes.len() {
                continue;
            }
        }
        i = gt + 1;
    }
    out
}

fn local_name(tag: &str) -> &str {
    match tag.rfind(':') {
        Some(pos) => &tag[pos + 1..],
        None => tag,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(url: &str, allow_http: bool) -> WebDavConfig {
        WebDavConfig {
            url: url.to_string(),
            user: "user".to_string(),
            password: "secret".to_string(),
            dir: "/backups".to_string(),
            allow_http,
        }
    }

    #[test]
    fn url_validation_https_only_by_default() {
        assert!(validate_url(&cfg("https://dav.example.com/dav", false)).is_ok());
        assert!(validate_url(&cfg("http://dav.example.com/dav", true)).is_err());

        let local = validate_url(&cfg("http://127.0.0.1:8080/dav", true)).unwrap();
        assert_eq!(local.host_str(), Some("127.0.0.1"));
        assert!(validate_url(&cfg("http://localhost:8080/dav", true)).is_ok());
        assert!(validate_url(&cfg("http://localhost:8080/dav", false)).is_err());
        assert!(validate_url(&cfg("ftp://dav.example.com", true)).is_err());
    }

    #[test]
    fn url_rejects_embedded_credentials() {
        let err = validate_url(&cfg("https://user:secret@dav.example.com", false))
            .unwrap_err()
            .to_string();
        assert!(err.contains("URL"));
        assert!(!err.contains("secret"));
    }

    #[test]
    fn propfind_parses_only_html_files() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/dav/backups/</d:href>
    <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/backups/bookmarks_backup_20260101_000000.html</d:href>
    <d:propstat><d:prop>
      <d:getcontentlength>123</d:getcontentlength>
      <d:getlastmodified>Tue, 02 Jan 2024 00:00:00 GMT</d:getlastmodified>
    </d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/backups/note.txt</d:href>
    <d:propstat><d:prop><d:getcontentlength>9</d:getcontentlength></d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;
        let files = parse_propfind(xml);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "bookmarks_backup_20260101_000000.html");
        assert_eq!(files[0].size, 123);
        assert!(files[0].modified > 0);
    }
}
