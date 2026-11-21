//! URL 标准化与哈希。
//!
//! 精确去重（第 1 期）使用 [`normalize`] 的严格标准化：
//! - 去除首尾空白
//! - 无协议头的 URL 视为 `http://`（如 `example.com/a`）
//! - scheme 与 host 小写（`url` crate 遵循 WHATWG 规范已做，此处兜底）
//! - 移除默认端口（http:80 / https:443）
//! - 移除空 fragment（`#`）
//! - http/https 的空路径补 `/`
//! - 保留非空 fragment、query 与路径原文（第 2 期模糊去重再做可配置放宽）
//!
//! 哈希使用 SHA-256（十六进制），避免直接存储明文聚合冲突。

use sha2::{Digest, Sha256};

/// 第 2 期模糊去重的可配置项（第 1 期仅实现结构，默认关闭）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FuzzyOptions {
    /// 忽略整个 query 串
    pub ignore_query: bool,
    /// 路径部分忽略大小写（scheme/host 始终小写）
    pub ignore_case: bool,
    /// 忽略路径末尾斜杠
    pub ignore_trailing_slash: bool,
    /// 忽略 fragment
    pub ignore_fragment: bool,
}

impl Default for FuzzyOptions {
    fn default() -> Self {
        FuzzyOptions {
            ignore_query: false,
            ignore_case: false,
            ignore_trailing_slash: false,
            ignore_fragment: false,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UrlNormError {
    #[error("URL 为空")]
    Empty,
    #[error("无法解析 URL: {0}")]
    Invalid(String),
}

/// 严格标准化（等价于 `normalize_with(raw, FuzzyOptions::default())`）。
pub fn normalize(raw: &str) -> Result<String, UrlNormError> {
    normalize_with(raw, FuzzyOptions::default())
}

/// 按选项标准化 URL。
pub fn normalize_with(raw: &str, opts: FuzzyOptions) -> Result<String, UrlNormError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(UrlNormError::Empty);
    }

    let with_scheme = if has_scheme(trimmed) {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };

    let parsed = url::Url::parse(&with_scheme).map_err(|_| UrlNormError::Invalid(raw.to_string()))?;

    match parsed.scheme() {
        "http" | "https" => normalize_http_https(&parsed, raw, opts),
        // mailto:/javascript:/chrome: 等特殊协议：仅做 scheme 小写与空白清理，保持原文
        _ => Ok(parsed.to_string()),
    }
}

fn has_scheme(s: &str) -> bool {
    // 形如 `abcd1234+.-:` 前缀才视为协议头；带点的 head（如 example.com:8080）
    // 只认可已知协议，避免误判为协议导致无法补 http://
    if let Some(idx) = s.find(':') {
        let head = &s[..idx];
        if head.is_empty() || head.len() < 2 {
            return false;
        }
        if !head
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            return false;
        }
        if !head
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        {
            return false;
        }
        if head.contains('.') {
            const KNOWN: &[&str] = &["moz-extension", "chrome-extension"];
            return KNOWN.contains(&head);
        }
        true
    } else {
        false
    }
}

fn normalize_http_https(parsed: &url::Url, raw: &str, opts: FuzzyOptions) -> Result<String, UrlNormError> {
    let host = parsed
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| UrlNormError::Invalid(raw.to_string()))?;
    let scheme = parsed.scheme();
    let mut out = String::with_capacity(raw.len() + 8);
    out.push_str(scheme);
    out.push_str("://");
    out.push_str(host);

    if let Some(port) = parsed.port() {
        let default = if scheme == "https" { 443 } else { 80 };
        if port != default {
            out.push(':');
            out.push_str(&port.to_string());
        }
    }

    let mut path = parsed.path().to_string();
    if path.is_empty() {
        path = "/".to_string();
    }
    if opts.ignore_trailing_slash && path.len() > 1 && path.ends_with('/') {
        path.truncate(path.len() - 1);
    }
    if opts.ignore_case {
        path = path.to_lowercase();
    }
    out.push_str(&path);

    if !opts.ignore_query {
        if let Some(q) = parsed.query() {
            out.push('?');
            out.push_str(q);
        }
    }

    if !opts.ignore_fragment {
        if let Some(f) = parsed.fragment() {
            if !f.is_empty() {
                out.push('#');
                out.push_str(f);
            }
        }
    }

    Ok(out)
}

/// SHA-256 哈希（64 位小写十六进制）。
pub fn url_hash(normalized: &str) -> String {
    let mut h = Sha256::new();
    h.update(normalized.as_bytes());
    let digest = h.finalize();
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 标准化 + 哈希一步完成。
pub fn url_hash_of(raw: &str) -> Result<(String, String), UrlNormError> {
    let norm = normalize(raw)?;
    let hash = url_hash(&norm);
    Ok((norm, hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strict_normalization() {
        assert_eq!(normalize("HTTPS://Example.COM:443/a?x=1#top").unwrap(), "https://example.com/a?x=1#top");
        assert_eq!(normalize("http://example.com:80/").unwrap(), "http://example.com/");
        assert_eq!(normalize("  https://Example.COM  ").unwrap(), "https://example.com/");
        assert_eq!(normalize("example.com").unwrap(), "http://example.com/");
        assert_eq!(normalize("example.com:8080/x").unwrap(), "http://example.com:8080/x");
        // 非 http(s) 协议保持原文（scheme 已小写）
        assert_eq!(normalize("MAILTO:a@b.com").unwrap(), "mailto:a@b.com");
        // 空路径补 /
        assert_eq!(normalize("https://example.org").unwrap(), "https://example.org/");
        // 非默认端口保留
        assert_eq!(normalize("https://example.org:8443/api").unwrap(), "https://example.org:8443/api");
    }

    #[test]
    fn test_errors() {
        assert_eq!(normalize("   "), Err(UrlNormError::Empty));
        assert!(normalize("http://").is_err());
    }

    #[test]
    fn test_fuzzy_options() {
        let o = FuzzyOptions {
            ignore_query: true,
            ignore_trailing_slash: true,
            ..Default::default()
        };
        assert_eq!(normalize_with("https://a.com/p/?q=1", o).unwrap(), "https://a.com/p");
        assert_ne!(normalize("https://a.com/p?q=1").unwrap(), normalize("https://a.com/p?q=2").unwrap());
    }

    #[test]
    fn test_hash_stable() {
        let h1 = url_hash(&normalize("https://example.com/a").unwrap());
        let h2 = url_hash(&normalize("HTTPS://EXAMPLE.COM/a").unwrap());
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }
}
