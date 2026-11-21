//! Tauri 命令层：薄封装 bookmark-core，负责 DTO 转换、路径展示与事件推送。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bookmark_core::checker::{test_proxy as core_test_proxy, CheckOptions, CheckStatus, CheckTarget, LinkChecker};
use bookmark_core::dedup;
use bookmark_core::model::{NewNode, Node, NodeKind, NodeId, ROOT_ID};
use bookmark_core::netscape;
use bookmark_core::store::{ImportMode, ImportStats, Stats, TrashEntry};

use serde::Serialize;
use tauri::{Emitter, State};

use crate::state::{AppState, CheckRunHandle};

fn es<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ---------------------------------------------------------------------------
// DTO
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckDto {
    pub status: CheckStatus,
    pub http_code: Option<u16>,
    pub final_url: Option<String>,
    pub message: Option<String>,
    pub checked_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNodeDto {
    pub id: NodeId,
    pub parent_id: NodeId,
    pub kind: NodeKind,
    pub title: String,
    pub url: Option<String>,
    pub add_date: Option<i64>,
    pub position: i64,
    pub check: Option<CheckDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDto {
    pub id: NodeId,
    pub title: String,
    /// 如 "开发/前端"，根目录不显示
    pub path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupGroupDto {
    pub normalized_url: String,
    pub keep: ItemDto,
    pub duplicates: Vec<ItemDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadItemDto {
    pub id: NodeId,
    pub title: String,
    pub url: String,
    pub http_code: Option<u16>,
    pub message: Option<String>,
    pub checked_at: i64,
    pub path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDeadResult {
    pub backup_path: String,
    pub deleted: usize,
    pub items: Vec<DeadItemDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub folders: i64,
    pub bookmarks: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartCheckResult {
    pub total: usize,
}

// ---------------------------------------------------------------------------
// 内部工具
// ---------------------------------------------------------------------------

/// 计算每个存活节点的展示路径（不含根目录名）。
fn alive_paths(nodes: &[Node]) -> HashMap<NodeId, String> {
    let map: HashMap<NodeId, Node> = nodes.iter().filter(|n| !n.deleted).map(|n| (n.id, n.clone())).collect();
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for n in map.values() {
        // 根节点自引用不进入 children 映射，避免死循环
        if n.parent_id != n.id {
            children.entry(n.parent_id).or_default().push(n.id);
        }
    }
    for v in children.values_mut() {
        v.sort_by_key(|id| map.get(id).map(|n| n.position).unwrap_or(0));
    }
    let mut out = HashMap::new();
    fn walk(
        id: NodeId,
        prefix: &str,
        map: &HashMap<NodeId, Node>,
        children: &HashMap<NodeId, Vec<NodeId>>,
        out: &mut HashMap<NodeId, String>,
    ) {
        if id != ROOT_ID {
            let title = map.get(&id).map(|n| n.title.clone()).unwrap_or_default();
            let path = if prefix.is_empty() { title } else { format!("{prefix}/{title}") };
            out.insert(id, path.clone());
            if let Some(kids) = children.get(&id) {
                for &k in kids {
                    walk(k, &path, map, children, out);
                }
            }
        } else if let Some(kids) = children.get(&id) {
            for &k in kids {
                walk(k, prefix, map, children, out);
            }
        }
    }
    walk(ROOT_ID, "", &map, &children, &mut out);
    out
}

fn item_dto(id: NodeId, nodes: &HashMap<NodeId, Node>, paths: &HashMap<NodeId, String>) -> Option<ItemDto> {
    let n = nodes.get(&id)?;
    Some(ItemDto {
        id,
        title: n.title.clone(),
        path: paths.get(&id).cloned().unwrap_or_default(),
    })
}

// ---------------------------------------------------------------------------
// 树与统计
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_tree(state: State<'_, AppState>) -> Result<Vec<TreeNodeDto>, String> {
    let store = state.store.lock().unwrap();
    let nodes = store.all_nodes().map_err(es)?;
    let checks = store.check_results_map().map_err(es)?;
    Ok(nodes
        .into_iter()
        .filter(|n| !n.deleted)
        .map(|n| {
            let check = checks.get(&n.id).map(|c| CheckDto {
                status: c.status,
                http_code: c.http_code,
                final_url: c.final_url.clone(),
                message: c.message.clone(),
                checked_at: c.checked_at,
            });
            TreeNodeDto {
                id: n.id,
                parent_id: n.parent_id,
                kind: n.kind,
                title: n.title,
                url: n.url,
                add_date: n.add_date,
                position: n.position,
                check,
            }
        })
        .collect())
}

#[tauri::command]
pub async fn get_stats(state: State<'_, AppState>) -> Result<Stats, String> {
    let store = state.store.lock().unwrap();
    store.stats().map_err(es)
}

#[tauri::command]
pub async fn get_db_path(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.data_dir.join("bookmarks.db").display().to_string())
}

// ---------------------------------------------------------------------------
// 导入 / 导出
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn import_html(
    state: State<'_, AppState>,
    path: String,
    mode: String,
) -> Result<ImportSummary, String> {
    let html = std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败: {e}"))?;
    let parsed = netscape::parse(&html).map_err(es)?;
    let import_mode = match mode.as_str() {
        "append" => ImportMode::Append,
        _ => ImportMode::Replace,
    };
    let stats: ImportStats = {
        let mut store = state.store.lock().unwrap();
        store.import_children(&parsed.children, import_mode).map_err(es)?
    };
    Ok(ImportSummary {
        folders: stats.folders,
        bookmarks: stats.bookmarks,
    })
}

#[tauri::command]
pub async fn export_html(state: State<'_, AppState>, path: String) -> Result<usize, String> {
    let tree: NewNode = {
        let store = state.store.lock().unwrap();
        store.export_tree().map_err(es)?
    };
    let html = netscape::export(&tree, &tree.title);
    std::fs::write(&path, html).map_err(|e| format!("写入文件失败: {e}"))?;
    Ok(tree.children.len())
}

// ---------------------------------------------------------------------------
// 树操作
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn move_nodes(state: State<'_, AppState>, ids: Vec<NodeId>, target_id: NodeId, index: i64) -> Result<(), String> {
    let mut store = state.store.lock().unwrap();
    store.move_nodes(&ids, target_id, index).map_err(es)
}

#[tauri::command]
pub async fn find_empty_folders(state: State<'_, AppState>) -> Result<Vec<ItemDto>, String> {
    let store = state.store.lock().unwrap();
    let ids = store.find_empty_folder_ids().map_err(es)?;
    let nodes = store.all_nodes().map_err(es)?;
    let node_map: HashMap<NodeId, Node> = nodes.iter().map(|n| (n.id, n.clone())).collect();
    let paths = alive_paths(&nodes);
    Ok(ids
        .into_iter()
        .filter_map(|id| item_dto(id, &node_map, &paths))
        .collect())
}

#[tauri::command]
pub async fn delete_empty_folders(state: State<'_, AppState>, ids: Vec<NodeId>) -> Result<usize, String> {
    let mut store = state.store.lock().unwrap();
    store.soft_delete(&ids).map_err(es)
}

// ---------------------------------------------------------------------------
// 去重
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn dedup_preview(state: State<'_, AppState>) -> Result<Vec<DupGroupDto>, String> {
    let store = state.store.lock().unwrap();
    let dfs = store.dfs_bookmarks().map_err(es)?;
    let groups = dedup::find_duplicate_groups(&dfs);
    if groups.is_empty() {
        return Ok(Vec::new());
    }
    let nodes = store.all_nodes().map_err(es)?;
    let node_map: HashMap<NodeId, Node> = nodes.iter().map(|n| (n.id, n.clone())).collect();
    let paths = alive_paths(&nodes);
    Ok(groups
        .into_iter()
        .filter_map(|g| {
            let keep = item_dto(g.keep_node_id, &node_map, &paths)?;
            let duplicates = g
                .duplicate_node_ids
                .into_iter()
                .filter_map(|id| item_dto(id, &node_map, &paths))
                .collect();
            Some(DupGroupDto {
                normalized_url: g.normalized_url,
                keep,
                duplicates,
            })
        })
        .collect())
}

#[tauri::command]
pub async fn dedup_apply(state: State<'_, AppState>) -> Result<usize, String> {
    let mut store = state.store.lock().unwrap();
    let dfs = store.dfs_bookmarks().map_err(es)?;
    let groups = dedup::find_duplicate_groups(&dfs);
    let ids: Vec<NodeId> = groups
        .iter()
        .flat_map(|g| g.duplicate_node_ids.iter().copied())
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    store.soft_delete(&ids).map_err(es)
}

// ---------------------------------------------------------------------------
// 链接检测
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn start_check(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    scope: String,
    ids: Vec<NodeId>,
) -> Result<StartCheckResult, String> {
    {
        let run = state.run.lock().unwrap();
        if run.is_some() {
            return Err("已有检测任务在进行中".into());
        }
    }

    let (targets, total) = {
        let store = state.store.lock().unwrap();
        let dfs = store.dfs_bookmarks().map_err(es)?;
        let list: Vec<CheckTarget> = match scope.as_str() {
            "selected" => {
                let set: std::collections::HashSet<NodeId> = ids.iter().copied().collect();
                dfs.into_iter()
                    .filter(|(id, _)| set.contains(id))
                    .map(|(node_id, url)| CheckTarget { node_id, url })
                    .collect()
            }
            _ => dfs
                .into_iter()
                .map(|(node_id, url)| CheckTarget { node_id, url })
                .collect(),
        };
        let total = list.len();
        (list, total)
    };

    if total == 0 {
        return Ok(StartCheckResult { total: 0 });
    }

    let proxy = {
        let store = state.store.lock().unwrap();
        store.get_setting("proxy").map_err(es)?
    };
    let opts = CheckOptions {
        proxy,
        ..Default::default()
    };
    let checker = LinkChecker::new(&opts).map_err(es)?;

    let cancel = Arc::new(AtomicBool::new(false));
    *state.run.lock().unwrap() = Some(CheckRunHandle { cancel: cancel.clone() });

    let mut rx = checker.check_many(targets, opts, cancel.clone());
    let store_for_task = state.store.clone();
    let run_state = state.run.clone();
    let total = total;

    tauri::async_runtime::spawn(async move {
        let mut done: i64 = 0;
        let mut ok: i64 = 0;
        let mut suspicious: i64 = 0;
        let mut dead: i64 = 0;
        while let Some(r) = rx.recv().await {
            match r.status {
                CheckStatus::Ok => ok += 1,
                CheckStatus::Suspicious => suspicious += 1,
                CheckStatus::Dead => dead += 1,
            }
            done += 1;
            {
                let mut st = store_for_task.lock().unwrap();
                let _ = st.upsert_check_result(&r);
            }
            let _ = app.emit(
                "check://progress",
                serde_json::json!({
                    "done": done,
                    "total": total,
                    "nodeId": r.node_id,
                    "url": r.url,
                    "status": r.status,
                    "httpCode": r.http_code,
                    "message": r.message,
                }),
            );
        }
        let canceled = cancel.load(Ordering::Relaxed);
        let _ = app.emit(
            "check://finished",
            serde_json::json!({
                "done": done, "total": total, "ok": ok,
                "suspicious": suspicious, "dead": dead, "canceled": canceled,
            }),
        );
        *run_state.lock().unwrap() = None;
    });

    Ok(StartCheckResult { total })
}

#[tauri::command]
pub async fn cancel_check(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(handle) = state.run.lock().unwrap().as_ref() {
        handle.cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// 失效项列表（删除前确认用）。
#[tauri::command]
pub async fn list_dead(state: State<'_, AppState>) -> Result<Vec<DeadItemDto>, String> {
    let store = state.store.lock().unwrap();
    let checks = store.check_results_map().map_err(es)?;
    let nodes = store.all_nodes().map_err(es)?;
    let node_map: HashMap<NodeId, Node> = nodes.iter().map(|n| (n.id, n.clone())).collect();
    let paths = alive_paths(&nodes);
    Ok(checks
        .into_iter()
        .filter(|(_, r)| r.status == CheckStatus::Dead)
        .filter_map(|(id, r)| {
            let n = node_map.get(&id)?;
            Some(DeadItemDto {
                id,
                title: n.title.clone(),
                url: r.url.clone(),
                http_code: r.http_code,
                message: r.message.clone(),
                checked_at: r.checked_at,
                path: paths.get(&id).cloned().unwrap_or_default(),
            })
        })
        .collect())
}

/// 一键删除失效项：① 自动生成带时间戳的 HTML 备份 ② 软删除（可从回收站撤销）。
#[tauri::command]
pub async fn delete_dead(state: State<'_, AppState>) -> Result<DeleteDeadResult, String> {
    // ① 自动备份（删除前的快照，含将删除的项）
    let backup_path = {
        let store = state.store.lock().unwrap();
        let dir = state.data_dir.join("backups");
        bookmark_core::backup::create_backup(&store, &dir).map_err(es)?
    };
    // ② 重新计算失效项并软删除
    let result = {
        let mut store = state.store.lock().unwrap();
        let checks = store.check_results_map().map_err(es)?;
        let nodes = store.all_nodes().map_err(es)?;
        let node_map: HashMap<NodeId, Node> = nodes.iter().map(|n| (n.id, n.clone())).collect();
        let paths = alive_paths(&nodes);
        let dead: Vec<DeadItemDto> = checks
            .iter()
            .filter(|(_, r)| r.status == CheckStatus::Dead)
            .filter_map(|(id, r)| {
                let n = node_map.get(id)?;
                Some(DeadItemDto {
                    id: *id,
                    title: n.title.clone(),
                    url: r.url.clone(),
                    http_code: r.http_code,
                    message: r.message.clone(),
                    checked_at: r.checked_at,
                    path: paths.get(id).cloned().unwrap_or_default(),
                })
            })
            .collect();
        let ids: Vec<NodeId> = dead.iter().map(|d| d.id).collect();
        let deleted = store.soft_delete(&ids).map_err(es)?;
        DeleteDeadResult {
            backup_path: backup_path.display().to_string(),
            deleted,
            items: dead,
        }
    };
    Ok(result)
}

// ---------------------------------------------------------------------------
// 回收站
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn trash_list(state: State<'_, AppState>) -> Result<Vec<TrashEntry>, String> {
    let store = state.store.lock().unwrap();
    store.list_trash().map_err(es)
}

#[tauri::command]
pub async fn trash_restore(state: State<'_, AppState>, entry_id: i64) -> Result<(), String> {
    let mut store = state.store.lock().unwrap();
    store.trash_restore(entry_id).map_err(es)
}

#[tauri::command]
pub async fn trash_purge(state: State<'_, AppState>, entry_id: i64) -> Result<(), String> {
    let mut store = state.store.lock().unwrap();
    store.trash_purge(entry_id).map_err(es)
}

// ---------------------------------------------------------------------------
// 代理设置
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_proxy(state: State<'_, AppState>) -> Result<String, String> {
    let store = state.store.lock().unwrap();
    Ok(store.get_setting("proxy").map_err(es)?.unwrap_or_default())
}

#[tauri::command]
pub async fn set_proxy(state: State<'_, AppState>, proxy: String) -> Result<(), String> {
    let mut store = state.store.lock().unwrap();
    store.set_setting("proxy", &proxy).map_err(es)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyTestResult {
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub message: String,
}

#[tauri::command]
pub async fn test_proxy(proxy: String) -> Result<ProxyTestResult, String> {
    let proxy_opt = if proxy.trim().is_empty() { None } else { Some(proxy.trim().to_string()) };
    match core_test_proxy(proxy_opt.as_deref()).await {
        Ok(ms) => Ok(ProxyTestResult {
            ok: true,
            latency_ms: Some(ms),
            message: format!("连接成功，延迟 {ms} ms"),
        }),
        Err(e) => Ok(ProxyTestResult {
            ok: false,
            latency_ms: None,
            message: e,
        }),
    }
}
