//! Store（SQLite 持久化）集成测试。

use bookmark_core::model::{NewNode, NodeKind};
use bookmark_core::store::{ImportMode, Store};
use bookmark_core::{now_ts, CheckResult, CheckStatus};

fn sample_tree() -> Vec<NewNode> {
    vec![
        NewNode::folder(
            "开发",
            vec![
                NewNode::bookmark("GitHub", "https://github.com/", Some(1600000001), None),
                NewNode::folder(
                    "前端",
                    vec![
                        NewNode::bookmark("Vue", "https://vuejs.org/", None, None),
                        NewNode::bookmark("React", "https://react.dev/", None, None),
                    ],
                ),
            ],
        ),
        NewNode::folder("空文件夹", vec![]),
        NewNode::bookmark("首页", "https://home.example/", None, None),
    ]
}

#[test]
fn import_replace_and_export_roundtrip() {
    let mut store = Store::open_in_memory().unwrap();
    let stats = store.import_children(&sample_tree(), ImportMode::Replace).unwrap();
    assert_eq!(stats.folders, 3); // 开发 / 前端 / 空文件夹
    assert_eq!(stats.bookmarks, 4);

    let tree = store.export_tree().unwrap();
    assert_eq!(tree.children.len(), 3);
    assert_eq!(tree.children[0].children[1].children[0].url.as_deref(), Some("https://vuejs.org/"));

    // Replace 导入应清空旧数据并重建根
    let stats2 = store
        .import_children(&[NewNode::bookmark("唯一", "https://only.example/", None, None)], ImportMode::Replace)
        .unwrap();
    assert_eq!(stats2.bookmarks, 1);
    assert_eq!(store.export_tree().unwrap().children.len(), 1);
}

#[test]
fn import_append() {
    let mut store = Store::open_in_memory().unwrap();
    store.import_children(&sample_tree(), ImportMode::Replace).unwrap();
    store
        .import_children(&[NewNode::bookmark("追加", "https://append.example/", None, None)], ImportMode::Append)
        .unwrap();
    let tree = store.export_tree().unwrap();
    assert_eq!(tree.children.len(), 4);
    assert_eq!(tree.children[3].title, "追加");
}

#[test]
fn move_nodes_and_validation() {
    let mut store = Store::open_in_memory().unwrap();
    store.import_children(&sample_tree(), ImportMode::Replace).unwrap();

    let nodes = store.all_nodes().unwrap();
    let find = |title: &str| nodes.iter().find(|n| n.title == title).unwrap().id;

    let github = find("GitHub");
    let empty_folder = find("空文件夹");
    let dev = find("开发");

    // 移动 GitHub 到 空文件夹
    store.move_nodes(&[github], empty_folder, 0).unwrap();
    let nodes = store.all_nodes().unwrap();
    let github_node = nodes.iter().find(|n| n.id == github).unwrap();
    assert_eq!(github_node.parent_id, empty_folder);
    assert_eq!(github_node.position, 0);

    // 把 空文件夹 移入 开发
    store.move_nodes(&[empty_folder], dev, 0).unwrap();
    let nodes = store.all_nodes().unwrap();
    let ef = nodes.iter().find(|n| n.id == empty_folder).unwrap();
    assert_eq!(ef.parent_id, dev);

    // 非法移动：把 开发 移入 空文件夹（其子树内）
    let err = store.move_nodes(&[dev], empty_folder, 0).unwrap_err();
    assert!(err.to_string().contains("子树"));

    // 批量移动：前端 + 空文件夹 → 根目录
    let front = find("前端");
    store.move_nodes(&[front, empty_folder], 1, 0).unwrap();
    let nodes = store.all_nodes().unwrap();
    for id in [front, empty_folder] {
        let n = nodes.iter().find(|x| x.id == id).unwrap();
        assert_eq!(n.parent_id, 1);
    }
}

#[test]
fn soft_delete_trash_restore_purge() {
    let mut store = Store::open_in_memory().unwrap();
    store.import_children(&sample_tree(), ImportMode::Replace).unwrap();

    let nodes = store.all_nodes().unwrap();
    let dev = nodes.iter().find(|n| n.title == "开发").unwrap().id;
    let empty_folder = nodes.iter().find(|n| n.title == "空文件夹").unwrap().id;

    // 软删除 开发 子树（含 前端/两个书签）
    let entries = store.soft_delete(&[dev]).unwrap();
    assert_eq!(entries, 1);

    let trash = store.list_trash().unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].title, "开发");
    assert_eq!(trash[0].item_count, 3); // GitHub + Vue + React

    let stats = store.stats().unwrap();
    assert_eq!(stats.bookmarks, 1); // 只剩 首页

    // 软删除 空文件夹
    store.soft_delete(&[empty_folder]).unwrap();
    assert_eq!(store.list_trash().unwrap().len(), 2);

    // 恢复 开发
    let dev_entry = store.list_trash().unwrap().into_iter().find(|t| t.title == "开发").unwrap();
    store.trash_restore(dev_entry.id).unwrap();
    let stats = store.stats().unwrap();
    assert_eq!(stats.bookmarks, 4);
    let tree = store.export_tree().unwrap();
    assert!(tree.children.iter().any(|c| c.title == "开发"));
    // 已恢复的条目不再出现在回收站
    assert!(store.list_trash().unwrap().iter().all(|t| t.title != "开发"));

    // 彻底删除 空文件夹 条目
    let ef_entry = store.list_trash().unwrap().into_iter().next().unwrap();
    store.trash_purge(ef_entry.id).unwrap();
    assert_eq!(store.list_trash().unwrap().len(), 0);
    let nodes = store.all_nodes().unwrap();
    assert!(!nodes.iter().any(|n| n.id == ef_entry.node_id));
}

#[test]
fn check_results_upsert_and_history() {
    let mut store = Store::open_in_memory().unwrap();
    store.import_children(&sample_tree(), ImportMode::Replace).unwrap();
    let nodes = store.all_nodes().unwrap();
    let github = nodes.iter().find(|n| n.title == "GitHub").unwrap().id;

    let mk = |status: CheckStatus, code: Option<u16>| CheckResult {
        node_id: github,
        url: "https://github.com/".into(),
        status,
        http_code: code,
        final_url: None,
        message: None,
        elapsed_ms: 100,
        checked_at: now_ts(),
    };

    store.upsert_check_result(&mk(CheckStatus::Ok, Some(200))).unwrap();
    store.upsert_check_result(&mk(CheckStatus::Dead, Some(404))).unwrap();

    let map = store.check_results_map().unwrap();
    assert_eq!(map.get(&github).unwrap().status, CheckStatus::Dead);
    assert_eq!(map.get(&github).unwrap().http_code, Some(404));

    let history = store.check_history_for(github).unwrap();
    assert_eq!(history.len(), 2); // 两次记录均保留
    assert_eq!(history[0].0, "dead"); // 最新在前

    // 软删除后结果不再返回
    store.soft_delete(&[github]).unwrap();
    assert!(store.check_results_map().unwrap().is_empty());
}

#[test]
fn settings_and_stats() {
    let mut store = Store::open_in_memory().unwrap();
    store.import_children(&sample_tree(), ImportMode::Replace).unwrap();

    store.set_setting("proxy", "http://127.0.0.1:7890").unwrap();
    assert_eq!(store.get_setting("proxy").unwrap().as_deref(), Some("http://127.0.0.1:7890"));
    store.set_setting("proxy", "").unwrap();
    assert_eq!(store.get_setting("proxy").unwrap().as_deref(), Some(""));

    let stats = store.stats().unwrap();
    assert_eq!(stats.folders, 3);
    assert_eq!(stats.bookmarks, 4);
    assert_eq!(stats.unchecked, 4);
    assert_eq!(stats.trash_count, 0);
}

#[test]
fn dfs_order_is_stable() {
    let mut store = Store::open_in_memory().unwrap();
    store.import_children(&sample_tree(), ImportMode::Replace).unwrap();
    let dfs = store.dfs_bookmarks().unwrap();
    let urls: Vec<&str> = dfs.iter().map(|(_, u)| u.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            "https://github.com/",
            "https://vuejs.org/",
            "https://react.dev/",
            "https://home.example/"
        ]
    );
}

#[test]
fn clear_check_results() {
    let mut store = Store::open_in_memory().unwrap();
    store.import_children(&sample_tree(), ImportMode::Replace).unwrap();
    let nodes = store.all_nodes().unwrap();
    let github = nodes.iter().find(|n| n.title == "GitHub").unwrap().id;
    store
        .upsert_check_result(&CheckResult {
            node_id: github,
            url: "https://github.com/".into(),
            status: CheckStatus::Ok,
            http_code: Some(200),
            final_url: None,
            message: None,
            elapsed_ms: 50,
            checked_at: now_ts(),
        })
        .unwrap();
    store.clear_check_results().unwrap();
    assert!(store.check_results_map().unwrap().is_empty());
    assert_eq!(store.stats().unwrap().unchecked, 4);
    let _ = NodeKind::Folder; // 引用避免未使用告警
}
