//! Netscape 收藏夹 HTML 格式的导入与导出。
//!
//! 兼容 Chrome / Edge / Firefox 导出的 `NETSCAPE-Bookmark-file-1` 格式：
//! - 大小写不敏感的标签与属性解析（`<DT><A HREF=...>`、`<H3>`、`<DL>`、`<DD>`）
//! - 引号内出现 `>` 的属性不会被截断
//! - 未闭合标签、缺失 `<DL>`、Firefox 的 `<DD>` 描述等异常均可容忍
//! - `ADD_DATE` 同时兼容秒级与毫秒级时间戳
//! - 导出保留完整文件夹层级与 `ADD_DATE`/`ICON`
//!
//! 解析模型：`stack[0]` 是虚拟根；`<H3>` 产生 pending 文件夹，紧随的 `<DL>`
//! 将 pending 压栈成为容器，`</DL>` 弹栈并挂到父级；`<A>` 挂到栈顶。

use crate::model::{NewNode, NodeKind};

/// 导入结果：虚拟根，children 为文件顶层条目。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportedRoot {
    pub children: Vec<NewNode>,
}

impl ImportedRoot {
    pub fn into_tree(self, root_title: &str) -> NewNode {
        NewNode {
            kind: NodeKind::Folder,
            title: root_title.to_string(),
            url: None,
            add_date: None,
            icon: None,
            children: self.children,
        }
    }
}

/// 单位时间戳上限：1999 年之后的秒级时间戳不会超过该值。
const MAX_VALID_SECONDS: i64 = 253_402_300_799;

// ---------------------------------------------------------------------------
// 导入
// ---------------------------------------------------------------------------

/// 解析 Netscape 收藏夹 HTML。
pub fn parse(html: &str) -> Result<ImportedRoot, crate::CoreError> {
    let mut p = Parser {
        b: html.as_bytes(),
        i: 0,
    };

    let root = Folder {
        title: String::new(),
        add_date: None,
        children: Vec::new(),
    };
    let mut stack: Vec<Folder> = vec![root]; // stack[0] = 虚拟根，永不弹出（未闭合时收尾处理）
    let mut pending: Option<Folder> = None; // 已读 <H3>、等待其 <DL> 的文件夹

    macro_rules! commit_pending {
        () => {
            if let Some(f) = pending.take() {
                stack.last_mut().unwrap().children.push(f.into_new_node());
            }
        };
    }

    while let Some(lt) = find_sub(p.b, b"<", p.i) {
        p.i = lt;
        let rest = &p.b[p.i..];

        if starts_with_tag(rest, b"<!--") {
            p.i += 4;
            p.i = find_sub(p.b, b"-->", p.i).map_or(p.b.len(), |e| e + 3);
            continue;
        }

        if starts_with_tag(rest, b"<dl") {
            p.i += 3;
            p.i = p.scan_attrs_to_tag_end();
            // 规则：
            // 1) 有 pending（刚读过 <H3>）→ pending 成为本次 <dl> 的容器
            // 2) 无 pending 且已在容器内 → 匿名文件夹（容错）
            // 3) 无 pending 且栈只有虚拟根 → 根级 <dl>，虚拟根直接作容器，不压栈
            if let Some(f) = pending.take() {
                stack.push(f);
            } else if stack.len() > 1 {
                stack.push(Folder {
                    title: String::new(),
                    add_date: None,
                    children: Vec::new(),
                });
            }
            continue;
        }

        if starts_with_tag(rest, b"</dl") {
            p.i += 4;
            p.i = p.scan_attrs_to_tag_end();
            commit_pending!();
            if stack.len() > 1 {
                let f = stack.pop().unwrap();
                stack.last_mut().unwrap().children.push(f.into_new_node());
            }
            // stack.len() == 1：多余的 </dl>，忽略
            continue;
        }

        if starts_with_tag(rest, b"<dt") {
            // <DT> 仅为行项标记，消费后由主循环的 <h3>/<a> 分支处理
            p.i += 3;
            p.i = p.scan_attrs_to_tag_end();
            continue;
        }

        if starts_with_tag(rest, b"<dd") {
            p.i += 3;
            p.i = p.scan_attrs_to_tag_end();
            // <DD> 描述文本：MVP 阶段忽略（后续文本由主循环自然跳过）
            continue;
        }

        if starts_with_tag(rest, b"<h3") {
            commit_pending!();
            p.i += 3; // 跳过 "<h3"
            let (attrs, _) = p.read_tag_rest();
            let title_end = p.find_until(&[b"</h3", b"<dt", b"<dl", b"</dl", b"<dd"]);
            let raw_title = p.take_text_until(title_end);
            pending = Some(Folder {
                title: decode_entities(&strip_tags(&raw_title)).trim().to_string(),
                add_date: attr_i64(&attrs, "add_date"),
                children: Vec::new(),
            });
            continue;
        }

        if starts_with_tag(rest, b"<a") {
            commit_pending!();
            p.i += 2; // 跳过 "<a"
            let (attrs, _) = p.read_tag_rest();
            let href = attrs
                .iter()
                .find(|(k, _)| k == "href")
                .map(|(_, v)| v.clone());
            let title_end = p.find_until(&[b"</a", b"<dt", b"<dl", b"</dl"]);
            let raw_title = p.take_text_until(title_end);
            let title = decode_entities(&strip_tags(&raw_title)).trim().to_string();
            let url = href
                .filter(|h| !h.trim().is_empty())
                .map(|h| decode_entities(&h));
            let bm = NewNode {
                kind: NodeKind::Bookmark,
                title: if title.is_empty() {
                    url.clone().unwrap_or_default()
                } else {
                    title
                },
                url,
                add_date: attr_i64(&attrs, "add_date"),
                icon: attrs.iter().find(|(k, _)| k == "icon").map(|(_, v)| v.clone()),
                children: Vec::new(),
            };
            stack.last_mut().unwrap().children.push(bm);
            continue;
        }

        if starts_with_tag(rest, b"</a") || starts_with_tag(rest, b"</h3") {
            // 标题读取阶段已在边界停下，此处防御性消费闭合标签
            p.skip_tag();
            continue;
        }

        // 其他标签（META / TITLE / H1 / HR / P / BUTTON 等）：整体跳过
        p.skip_tag();
    }

    commit_pending!();
    // 未闭合的 <DL>：从内向外依次挂回
    while stack.len() > 1 {
        let f = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(f.into_new_node());
    }

    Ok(ImportedRoot {
        children: std::mem::take(&mut stack[0].children),
    })
}

struct Folder {
    title: String,
    add_date: Option<i64>,
    children: Vec<NewNode>,
}

impl Folder {
    fn into_new_node(self) -> NewNode {
        NewNode {
            kind: NodeKind::Folder,
            title: if self.title.is_empty() {
                "未命名文件夹".to_string()
            } else {
                self.title
            },
            url: None,
            add_date: self.add_date,
            icon: None,
            children: self.children,
        }
    }
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    /// 查找任一标记最先出现的位置（大小写不敏感）。
    fn find_until(&self, pats: &[&[u8]]) -> Option<usize> {
        let mut best: Option<usize> = None;
        for pat in pats {
            if let Some(pos) = find_ci(self.b, pat, self.i) {
                best = Some(match best {
                    Some(b) => b.min(pos),
                    None => pos,
                });
            }
        }
        best
    }

    /// 消费 [i, end) 文本并推进到 end（找不到标记则取到结尾）。
    fn take_text_until(&mut self, end: Option<usize>) -> String {
        let end = end.unwrap_or(self.b.len()).min(self.b.len());
        let text = String::from_utf8_lossy(&self.b[self.i..end]).into_owned();
        self.i = end;
        text
    }

    fn skip_ws(&mut self) -> usize {
        let start = self.i;
        while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
        self.i - start
    }

    /// 当前 i 指向 '<'：跳过整个标签。
    fn skip_tag(&mut self) {
        if self.i < self.b.len() && self.b[self.i] == b'<' {
            self.i += 1;
            self.i = self.scan_attrs_to_tag_end();
        }
    }

    /// 当前 i 已越过标签名：引号感知地扫描到 '>'，返回属性列表。
    fn read_tag_rest(&mut self) -> (Vec<(String, String)>, bool) {
        let (end, attrs) = self.scan_attrs_to_tag_end_with_attrs();
        self.i = end;
        (attrs, false)
    }

    fn scan_attrs_to_tag_end(&mut self) -> usize {
        self.scan_attrs_to_tag_end_with_attrs().0
    }

    fn scan_attrs_to_tag_end_with_attrs(&self) -> (usize, Vec<(String, String)>) {
        let mut i = self.i;
        let mut buf = String::new();
        let mut in_quote: Option<u8> = None;
        while i < self.b.len() {
            let c = self.b[i];
            if let Some(q) = in_quote {
                buf.push(c as char);
                if c == q {
                    in_quote = None;
                }
                i += 1;
                continue;
            }
            if c == b'"' || c == b'\'' {
                in_quote = Some(c);
                buf.push(c as char);
                i += 1;
                continue;
            }
            if c == b'>' {
                break;
            }
            buf.push(c as char);
            i += 1;
        }
        let mut attrs = Vec::new();
        parse_attrs_into(&buf, &mut attrs);
        (i.min(self.b.len()), attrs)
    }
}

/// 在 `haystack[from..]` 中查找 `pat`（大小写敏感）。
fn find_sub(haystack: &[u8], pat: &[u8], from: usize) -> Option<usize> {
    if pat.is_empty() || from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|p| p + from)
}

/// 大小写不敏感查找 ASCII 标记。
fn find_ci(haystack: &[u8], pat: &[u8], from: usize) -> Option<usize> {
    if pat.is_empty() || from >= haystack.len() {
        return None;
    }
    let plen = pat.len();
    haystack[from..]
        .windows(plen)
        .position(|w| w.iter().zip(pat.iter()).all(|(a, b)| a.eq_ignore_ascii_case(b)))
        .map(|p| p + from)
}

/// 判断 rest 是否以 tag 开头且其后是空白 / '>' / '/'。
fn starts_with_tag(rest: &[u8], tag: &[u8]) -> bool {
    if rest.len() < tag.len() || !rest[..tag.len()].eq_ignore_ascii_case(tag) {
        return false;
    }
    match rest.get(tag.len()) {
        None => true,
        Some(c) => c.is_ascii_whitespace() || *c == b'>' || *c == b'/',
    }
}

/// 解析属性文本为 (小写名, 值) 列表。
fn parse_attrs_into(text: &str, out: &mut Vec<(String, String)>) {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let mut name = String::new();
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            name.push(chars[i]);
            i += 1;
        }
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                let q = chars[i];
                i += 1;
                while i < chars.len() && chars[i] != q {
                    value.push(chars[i]);
                    i += 1;
                }
                i += 1;
            } else {
                while i < chars.len() && !chars[i].is_whitespace() {
                    value.push(chars[i]);
                    i += 1;
                }
            }
        }
        if !name.is_empty() {
            out.push((name.to_lowercase(), value));
        }
    }
}

fn attr_i64(attrs: &[(String, String)], key: &str) -> Option<i64> {
    let raw = attrs.iter().find(|(k, _)| k == key)?.1.trim().to_string();
    if raw.is_empty() {
        return None;
    }
    match raw.parse::<i64>() {
        Ok(v) => {
            if v > MAX_VALID_SECONDS {
                Some(v / 1000) // 毫秒级时间戳归一为秒级
            } else {
                Some(v)
            }
        }
        Err(_) => None,
    }
}

/// 去除文本中的内联标签（如标题中夹带的 <B>）。
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// 解码常见 HTML 实体与数字实体。
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(semi) = find_sub(bytes, b";", i + 1) {
                let ent = &s[i + 1..semi];
                let decoded = match ent {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    _ => {
                        if let Some(num) = ent.strip_prefix('#') {
                            let code = if let Some(hex) =
                                num.strip_prefix('x').or_else(|| num.strip_prefix('X'))
                            {
                                u32::from_str_radix(hex, 16).ok()
                            } else {
                                num.parse::<u32>().ok()
                            };
                            code.and_then(char::from_u32)
                        } else {
                            None
                        }
                    }
                };
                if let Some(c) = decoded {
                    out.push(c);
                    i = semi + 1;
                    continue;
                }
            }
        }
        let ch_len = utf8_len(bytes[i]);
        let end = (i + ch_len).min(s.len());
        out.push_str(&s[i..end]);
        i = end.max(i + 1);
    }
    out
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else if b >> 3 == 0b11110 {
        4
    } else {
        1
    }
}

// ---------------------------------------------------------------------------
// 导出
// ---------------------------------------------------------------------------

/// HTML 属性转义。
pub fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// HTML 文本转义。
pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// 将树导出为 Netscape 收藏夹 HTML（`root` 的 children 为顶层条目）。
/// `root_title` 写入 `<TITLE>` 与 `<H1>`。
pub fn export(root: &NewNode, root_title: &str) -> String {
    let mut out = String::with_capacity(4096 + root.children.len() * 128);
    out.push_str("<!DOCTYPE NETSCAPE-Bookmark-file-1>\n");
    out.push_str("<!-- This is an automatically generated file.\n");
    out.push_str("     It will be read and overwritten.\n");
    out.push_str("     DO NOT EDIT! -->\n");
    out.push_str("<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n");
    let title = escape_text(root_title);
    out.push_str(&format!("<TITLE>{title}</TITLE>\n"));
    out.push_str(&format!("<H1>{title}</H1>\n"));
    out.push_str("<DL><p>\n");
    write_items(&mut out, &root.children, 0);
    out.push_str("</DL><p>\n");
    out
}

fn write_items(out: &mut String, items: &[NewNode], depth: usize) {
    let pad = "    ".repeat(depth);
    for item in items {
        match item.kind {
            NodeKind::Folder => {
                let mut attrs = String::new();
                if let Some(d) = item.add_date {
                    attrs.push_str(&format!(" ADD_DATE=\"{d}\""));
                }
                out.push_str(&format!(
                    "{pad}<DT><H3{attrs}>{}</H3>\n{pad}<DL><p>\n",
                    escape_text(&item.title)
                ));
                write_items(out, &item.children, depth + 1);
                out.push_str(&format!("{pad}</DL><p>\n"));
            }
            NodeKind::Bookmark => {
                let mut attrs =
                    format!(" HREF=\"{}\"", escape_attr(item.url.as_deref().unwrap_or("")));
                if let Some(d) = item.add_date {
                    attrs.push_str(&format!(" ADD_DATE=\"{d}\""));
                }
                if let Some(icon) = &item.icon {
                    if !icon.is_empty() {
                        attrs.push_str(&format!(" ICON=\"{}\"", escape_attr(icon)));
                    }
                }
                out.push_str(&format!(
                    "{pad}<DT><A{attrs}>{}</A>\n",
                    escape_text(&item.title)
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_entities() {
        assert_eq!(decode_entities("a&amp;b"), "a&b");
        assert_eq!(decode_entities("&#65;&#x42;"), "AB");
        assert_eq!(decode_entities("中&amp;文"), "中&文");
        assert_eq!(decode_entities("plain"), "plain");
        assert_eq!(decode_entities("x&unknown;y"), "x&unknown;y");
    }

    #[test]
    fn test_attr_parse_quoted_gt() {
        let mut attrs = Vec::new();
        parse_attrs_into(
            " HREF=\"https://a.com/x?q=a>b\" ADD_DATE=\"1600000000\"",
            &mut attrs,
        );
        assert_eq!(attrs[0].1, "https://a.com/x?q=a>b");
        assert_eq!(attrs[1].1, "1600000000");
    }

    #[test]
    fn test_roundtrip_simple() {
        let tree = NewNode::folder(
            "root",
            vec![
                NewNode::bookmark("Example", "https://example.com/?a=1&b=2", Some(1600000000), None),
                NewNode::folder(
                    "子文件夹",
                    vec![NewNode::bookmark("测试", "https://example.cn/中文", None, None)],
                ),
            ],
        );
        let html = export(&tree, "Bookmarks");
        let parsed = parse(&html).unwrap();
        assert_eq!(parsed.children.len(), 2);
        match &parsed.children[0] {
            NewNode { kind: NodeKind::Bookmark, url: Some(u), .. } => {
                assert_eq!(u, "https://example.com/?a=1&b=2");
            }
            other => panic!("expected bookmark, got {other:?}"),
        }
        match &parsed.children[1] {
            NewNode { kind: NodeKind::Folder, title, children, .. } => {
                assert_eq!(title, "子文件夹");
                assert_eq!(children.len(), 1);
            }
            other => panic!("expected folder, got {other:?}"),
        }
    }
}
