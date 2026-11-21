//! 验收场景测试（对应用户规格中的验收标准示例）：
//!
//! 导入含 100 条书签（嵌套 3 层文件夹、10 个失效链接、5 个重复 URL）的 HTML 后：
//! 1. 树形结构还原一致
//! 2. 精确去重移除 5 个重复项
//! 3. 检测完成后 10 个失效项被标记
//! 4. 一键删除后：备份文件存在、导出 HTML 中失效项已移除、回收站可恢复全部 10 项

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use bookmark_core::backup;
use bookmark_core::checker::{CheckOptions, CheckStatus, LinkChecker};
use bookmark_core::dedup;
use bookmark_core::model::{NewNode, NodeKind};
use bookmark_core::netscape;
use bookmark_core::store::{ImportMode, Store};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOTAL: usize = 100;
const DEAD_404: std::ops::Range<usize> = 85..89; // 4 个 404
const DEAD_REFUSED: std::ops::Range<usize> = 89..92; // 3 个连接拒绝
const DEAD_SLOW: std::ops::Range<usize> = 92..95; // 3 个超时
const DUPS: std::ops::Range<usize> = 95..100; // 5 个重复

/// 构造第 i 条书签（0..100）。
fn make_bookmark(i: usize, base: &str, dead_port: u16) -> NewNode {
    let url = if i < DEAD_404.start {
        format!("{base}/ok?page={i}")
    } else if DEAD_404.contains(&i) {
        format!("{base}/dead404?page={i}")
    } else if DEAD_REFUSED.contains(&i) {
        format!("http://127.0.0.1:{dead_port}/refused{i}")
    } else if DEAD_SLOW.contains(&i) {
        format!("{base}/slow/{i}")
    } else {
        // 5 个重复项：分别用 空fragment / 大写scheme+host / 首尾空白 / 无协议头 / 无协议头+空fragment
        let n = i - DUPS.start;
        let origin = format!("{base}/ok?page={}", 10 + n);
        match n {
            0 => format!("{origin}#"),
            1 => origin.replace("http://", "HTTP://"),
            2 => format!("   {origin}   "),
            3 => origin.trim_start_matches("http://").to_string(),
            _ => format!("{}#", origin.trim_start_matches("http://")),
        }
    };
    NewNode::bookmark(format!("书签{i:03}"), url, Some(1700000000 + i as i64), None)
}

/// 15 个文件夹（3 个一级、6 个二级、6 个三级），DFS 顺序为每个容器填书签直到 100 条。
fn build_fixture(base: &str, dead_port: u16) -> Vec<NewNode> {
    let l1_names = ["开发", "生活", "学习"];
    let mut l1s: Vec<NewNode> = Vec::new();
    for (li, name) in l1_names.iter().enumerate() {
        let mut l2s: Vec<NewNode> = Vec::new();
        for k in 0..2 {
            l2s.push(NewNode::folder(format!("L2-{li}-{k}"), vec![NewNode::folder(format!("L3-{li}-{k}"), vec![])]));
        }
        l1s.push(NewNode::folder(*name, l2s));
    }

    let mut counter = 0usize;
    fn fill(node: &mut NewNode, counter: &mut usize, base: &str, dead_port: u16, per: usize) {
        if node.kind != NodeKind::Folder {
            return;
        }
        for _ in 0..per {
            if *counter >= TOTAL {
                return;
            }
            let bm = make_bookmark(*counter, base, dead_port);
            node.children.push(bm);
            *counter += 1;
        }
        for c in node.children.iter_mut() {
            fill(c, counter, base, dead_port, per);
        }
    }
    for l1 in l1s.iter_mut() {
        fill(l1, &mut counter, base, dead_port, 7);
    }
    assert_eq!(counter, TOTAL, "夹具必须恰好生成 100 条书签");
    l1s
}

/// 收集树中所有书签 (路径, url)。
fn collect_paths(tree: &NewNode, prefix: &str, out: &mut Vec<(String, String)>) {
    for c in &tree.children {
        let p = if prefix.is_empty() { c.title.clone() } else { format!("{prefix}/{}", c.title) };
        if c.kind == NodeKind::Bookmark {
            out.push((p, c.url.clone().unwrap_or_default()));
        } else {
            collect_paths(c, &p, out);
        }
    }
}

fn count_kind(tree: &NewNode, kind: NodeKind) -> usize {
    let mut n = usize::from(tree.kind == kind);
    for c in &tree.children {
        n += count_kind(c, kind);
    }
    n
}

fn as_root(children: Vec<NewNode>) -> NewNode {
    NewNode { kind: NodeKind::Folder, title: "书签栏".into(), url: None, add_date: None, icon: None, children }
}

#[tokio::test]
async fn full_acceptance_scenario() {
    // ---------------- mock server ----------------
    let server = MockServer::start().await;
    let base = server.uri().trim_end_matches('/').to_string();

    Mock::given(method("HEAD"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("HEAD"))
        .and(path("/dead404"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(wiremock::matchers::path_regex("^/slow/"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)))
        .mount(&server)
        .await;

    // 预留一个“连接拒绝”端口
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = listener.local_addr().unwrap().port();
    drop(listener);

    // ---------------- 1. 生成并导入 HTML ----------------
    let tree = NewNode::folder("书签栏", build_fixture(&base, dead_port));
    assert_eq!(count_kind(&tree, NodeKind::Bookmark), TOTAL, "夹具应有 100 条书签");
    assert_eq!(count_kind(&tree, NodeKind::Folder), 16, "根 + 15 个文件夹");

    let html = netscape::export(&tree, "书签栏");
    let parsed = netscape::parse(&html).unwrap();
    let imported_tree = parsed.into_tree("书签栏");
    assert_eq!(count_kind(&imported_tree, NodeKind::Bookmark), TOTAL, "HTML 往返后书签数必须一致");
    assert_eq!(count_kind(&imported_tree, NodeKind::Folder), count_kind(&tree, NodeKind::Folder));

    let mut store = Store::open_in_memory().unwrap();
    let stats = store.import_children(&imported_tree.children, ImportMode::Replace).unwrap();
    assert_eq!(stats.bookmarks, TOTAL as i64);
    assert_eq!(stats.folders, 15);

    // 树形结构还原一致：三级嵌套路径 + 总数
    let exported = store.export_tree().unwrap();
    let mut paths = Vec::new();
    collect_paths(&exported, "", &mut paths);
    assert!(paths.iter().any(|(p, _)| p.contains("开发/L2-0-0/L3-0-0/")), "三级嵌套必须保留");
    assert_eq!(paths.len(), TOTAL);

    // ---------------- 2. 精确去重 ----------------
    let dfs = store.dfs_bookmarks().unwrap();
    let groups = dedup::find_duplicate_groups(&dfs);
    let dup_total: usize = groups.iter().map(|g| g.duplicate_node_ids.len()).sum();
    assert_eq!(dup_total, 5, "应发现 5 个重复 URL");
    assert_eq!(groups.len(), 5);
    let dup_ids: Vec<i64> = groups.iter().flat_map(|g| g.duplicate_node_ids.iter().copied()).collect();
    store.soft_delete(&dup_ids).unwrap();
    assert_eq!(store.stats().unwrap().bookmarks, 95);

    // ---------------- 3. 全量检测 ----------------
    let opts = CheckOptions {
        timeout_secs: 1, // 测试加速；生产默认 10
        retries: 0,      // 测试加速；生产默认 2
        concurrency: 8,
        proxy: None,
        user_agent: "BookmarkOrganizer-Test/0.1".into(),
    };
    let checker = LinkChecker::new(&opts).unwrap();
    let targets = store
        .dfs_bookmarks()
        .unwrap()
        .into_iter()
        .map(|(id, url)| bookmark_core::CheckTarget { node_id: id, url })
        .collect::<Vec<_>>();
    assert_eq!(targets.len(), 95);

    let mut rx = checker.check_many(targets, opts, Arc::new(AtomicBool::new(false)));
    let mut results = Vec::new();
    while let Some(r) = rx.recv().await {
        store.upsert_check_result(&r).unwrap();
        results.push(r);
    }
    assert_eq!(results.len(), 95, "全部 http 书签都应有结果");

    let s = store.stats().unwrap();
    assert_eq!(s.dead, 10, "10 个失效项必须被标记");
    assert_eq!(s.ok, 85);
    assert_eq!(s.suspicious, 0);

    // ---------------- 4. 一键删除失效项 ----------------
    let dead_ids: Vec<i64> = store
        .check_results_map()
        .unwrap()
        .into_iter()
        .filter(|(_, r)| r.status == CheckStatus::Dead)
        .map(|(id, _)| id)
        .collect();
    assert_eq!(dead_ids.len(), 10);

    let backup_dir = tempfile::tempdir().unwrap();
    let backup_path = backup::create_backup(&store, backup_dir.path()).unwrap();
    assert!(backup_path.exists());
    assert!(
        backup_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("bookmarks_backup_"),
        "备份文件名必须带时间戳"
    );
    let backup_html = std::fs::read_to_string(&backup_path).unwrap();
    let backup_tree = netscape::parse(&backup_html).unwrap();
    assert_eq!(count_kind(&as_root(backup_tree.children), NodeKind::Bookmark), 95, "备份应包含删除前全部 95 条");

    let entries = store.soft_delete(&dead_ids).unwrap();
    assert_eq!(entries, 10);
    assert_eq!(store.stats().unwrap().bookmarks, 85);

    // 导出 HTML 不再包含失效项
    let clean_tree = store.export_tree().unwrap();
    let clean_html = netscape::export(&clean_tree, "书签栏");
    assert!(!clean_html.contains("/dead404"));
    assert!(!clean_html.contains("/slow/"));
    assert!(!clean_html.contains("/refused"));
    let mut clean_paths = Vec::new();
    collect_paths(&clean_tree, "", &mut clean_paths);
    assert_eq!(clean_paths.len(), 85);

    // ---------------- 5. 回收站恢复全部 10 项 ----------------
    let all_trash = store.list_trash().unwrap();
    // 15 条 = 10 条失效项 + 5 条去重删除项
    assert_eq!(all_trash.len(), 15);
    let trash: Vec<_> = all_trash
        .into_iter()
        .filter(|t| dead_ids.contains(&t.node_id))
        .collect();
    assert_eq!(trash.len(), 10);
    for t in &trash {
        assert_eq!(t.item_count, 1, "单条失效书签的回收站条目应含 1 个书签");
        store.trash_restore(t.id).unwrap();
    }
    assert_eq!(store.stats().unwrap().bookmarks, 95);
    let restored_tree = store.export_tree().unwrap();
    let restored_html = netscape::export(&restored_tree, "书签栏");
    assert!(restored_html.contains("/dead404"), "恢复后失效项应回到树中");
    // 历史记录完整保留
    assert_eq!(store.check_history_count().unwrap(), 95);
}
