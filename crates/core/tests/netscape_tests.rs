//! Netscape 导入/导出集成测试：Chrome / Firefox 风格夹具 + 往返一致性 + 容错。

use bookmark_core::model::{NewNode, NodeKind};
use bookmark_core::netscape;

fn flatten(tree: &NewNode, path: &str, out: &mut Vec<(String, String, NodeKind)>) {
    let p = if path.is_empty() {
        tree.title.clone()
    } else {
        format!("{path}/{}", tree.title)
    };
    if tree.kind == NodeKind::Bookmark {
        out.push((p.clone(), tree.url.clone().unwrap_or_default(), tree.kind));
    }
    for c in &tree.children {
        flatten(c, &p, out);
    }
}

/// 摘要一棵树用于比较（不含 add_date/icon）。
fn tree_digest(tree: &NewNode) -> Vec<(String, String, NodeKind)> {
    let mut out = Vec::new();
    flatten(tree, "", &mut out);
    out
}

#[test]
fn chrome_fixture_structure() {
    let html = include_str!("fixtures/chrome_like.html");
    let root = netscape::parse(html).unwrap();
    assert_eq!(root.children.len(), 2, "顶层应为 书签栏/其他书签 两个文件夹");

    let top1 = &root.children[0];
    assert_eq!(top1.title, "书签栏");
    assert_eq!(top1.kind, NodeKind::Folder);
    assert_eq!(top1.children.len(), 4); // Google / GitHub / 开发工具 / Crates.io
    assert_eq!(top1.children[0].title, "Google");
    assert_eq!(top1.children[0].url.as_deref(), Some("https://www.google.com/"));
    assert_eq!(top1.children[0].add_date, Some(1600000010));

    let dev = &top1.children[2];
    assert_eq!(dev.title, "开发工具");
    assert_eq!(dev.children.len(), 2);
    let front = &dev.children[1];
    assert_eq!(front.title, "前端");
    assert_eq!(front.children[0].title, "Vue");
    assert_eq!(front.children[0].url.as_deref(), Some("https://vuejs.org/"));

    let top2 = &root.children[1];
    assert_eq!(top2.title, "其他书签");
    let icon = top2.children[0].icon.as_deref().unwrap();
    assert!(icon.starts_with("data:image/png;base64,"));
    // 实体解码
    assert_eq!(
        top2.children[1].url.as_deref(),
        Some("https://example.org/path?q=1&lang=zh")
    );
}

#[test]
fn firefox_fixture_structure() {
    let html = include_str!("fixtures/firefox_like.html");
    let root = netscape::parse(html).unwrap();
    // 顶层: 书签工具栏(folder) / 根级书签(bookmark) / 其他书签(folder)
    assert_eq!(root.children.len(), 3);
    assert_eq!(root.children[0].kind, NodeKind::Folder);
    assert_eq!(root.children[0].title, "书签工具栏");
    assert_eq!(root.children[0].children.len(), 3);
    assert_eq!(root.children[1].kind, NodeKind::Bookmark);
    assert_eq!(root.children[1].url.as_deref(), Some("https://root-bookmark.example/"));
    assert_eq!(root.children[2].title, "其他书签");
    // 嵌套层
    let nested = &root.children[2].children[1];
    assert_eq!(nested.title, "嵌套层");
    assert_eq!(nested.children[0].url.as_deref(), Some("https://nested.example/deep"));
    // 小写标签 + 实体查询参数
    assert_eq!(
        root.children[0].children[0].url.as_deref(),
        Some("https://mozilla.org/")
    );
    assert_eq!(
        root.children[2].children[0].url.as_deref(),
        Some("https://example.org/?a=1&b=2")
    );
    // 单引号属性
    assert_eq!(
        root.children[0].children[1].url.as_deref(),
        Some("https://single-quote.example/path")
    );
}

#[test]
fn roundtrip_preserves_hierarchy() {
    let html = include_str!("fixtures/chrome_like.html");
    let first = netscape::parse(html).unwrap();
    let tree = first.into_tree("书签栏");
    let exported = netscape::export(&tree, "书签栏");
    let second = netscape::parse(&exported).unwrap();
    assert_eq!(
        tree_digest(&tree),
        tree_digest(&NewNode { kind: NodeKind::Folder, title: "书签栏".into(), url: None, add_date: None, icon: None, children: second.children }),
        "导出再导入后结构必须一致"
    );
}

#[test]
fn malformed_html_is_tolerated() {
    // 未闭合 </DL>、缺失 </A>、混合大小写、未知标签
    let html = r#"
    <meta http-equiv="content-type" content="text/html; charset=UTF-8">
    <h1>收集夹</h1>
    <dl><p>
      <DT><H3>文件夹甲
      <DL><p>
        <DT><A HREF="https://a.example/">A
        <DT><a href="https://b.example/">B</a>
      <dt><a href="https://c.example/">C</a>
    </dl><p>
    "#;
    let root = netscape::parse(html).unwrap();
    // 缺失 </DL>：C 属于未闭合的文件夹甲（容错语义）
    assert_eq!(root.children.len(), 1);
    let f = &root.children[0];
    assert_eq!(f.title, "文件夹甲");
    assert_eq!(f.children.len(), 3);
    assert_eq!(f.children[0].url.as_deref(), Some("https://a.example/"));
    assert_eq!(f.children[2].url.as_deref(), Some("https://c.example/"));
}

#[test]
fn javascript_url_bookmark_is_preserved() {
    let html = r#"
    <DL><p>
      <DT><A HREF="javascript:void(0)" ADD_DATE="1600000000">JS 书签</A>
      <DT><A HREF="mailto:a@b.com">邮件</A>
    </DL><p>"#;
    let root = netscape::parse(html).unwrap();
    assert_eq!(root.children[0].url.as_deref(), Some("javascript:void(0)"));
    assert_eq!(root.children[1].url.as_deref(), Some("mailto:a@b.com"));
}

#[test]
fn millisecond_add_date_is_normalized() {
    let html = r#"
    <DL><p>
      <DT><A HREF="https://ms.example/" ADD_DATE="1600000000000">毫秒</A>
    </DL><p>"#;
    let root = netscape::parse(html).unwrap();
    assert_eq!(root.children[0].add_date, Some(1600000000));
}
