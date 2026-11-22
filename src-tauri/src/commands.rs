//! Tauri 命令层：薄封装 bookmark-core，负责 DTO 转换、路径展示与事件推送。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use bookmark_core::checker::{test_proxy as core_test_proxy, CheckOptions, CheckStatus, CheckTarget, LinkChecker};
use bookmark_core::dedup;
use bookmark_core::model::{NewNode, Node, NodeKind, NodeId, ROOT_ID};
use bookmark_core::netscape;
use bookmark_core::rules::{Rule, RuleField, RuleInput, RuleSet};
use bookmark_core::webdav::{WebDavClient, WebDavConfig};
use bookmark_core::Store;
use bookmark_core::store::{
    is_http_url, ImportMode, ImportStats, MergeFolderGroup, MergeOutcome, Stats, TrashEntry,
};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::state::{AppState, CheckRunHandle};

fn es<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// #9 模糊去重开关 DTO：直接复用 core 的 `FuzzyOptions`（serde camelCase + default）。
/// 前端传入的四项开关原样交给 `find_duplicate_groups_with`，apply 不另读 settings。
pub type FuzzyOptionsDto = bookmark_core::urlnorm::FuzzyOptions;

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

#[derive(Debug, Clone, Serialize)]
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
    /// 分组用哈希（严格 hash 仅用于 #11/#12，二者用途隔离）
    pub hash: String,
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
// 第 2 期 DTO 骨架
//
// T01 定义骨架 DTO，T02 已实装 #11/#12 命令，T03-T05 随各自 core 实现继续补命令。
// 输出型 DTO 只需 Serialize；作为命令入参的 DTO 额外派生 Deserialize。
// core 侧的 Rule / RuleInput / WebDavConfig 属于 T04 / T05 的 rules.rs / webdav.rs，
// 此处以 *Dto 命名避免与其重名，届时可做薄转换。
// ---------------------------------------------------------------------------

/// #11 增量检测计划：四类原因计数与实际待检测总数（预览数量 = 实际 total）。
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementalPlanDto {
    pub new_count: usize,
    pub url_changed_count: usize,
    pub stale_count: usize,
    pub skipped_count: usize,
    pub total: usize,
}

/// #10 同名文件夹分组预览（标题 trim + 大小写不敏感）。
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeGroupDto {
    pub key: String,
    pub folders: Vec<ItemDto>,
    pub total_bookmarks: i64,
    /// DFS 顺序第一个，UI 允许改选
    pub suggested_keep_id: NodeId,
}

/// #10 用户在预览中确认的分组选择。
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeChoiceDto {
    pub keep_id: NodeId,
    pub merge_ids: Vec<NodeId>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResultDto {
    pub backup_path: String,
    pub merged_groups: usize,
    pub moved_nodes: usize,
}

/// #12 重定向候选（旧地址 → 最终地址），仅 ok/suspicious 且 HTTP 协议。
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedirectDto {
    pub node_id: NodeId,
    pub title: String,
    pub old_url: String,
    pub new_url: String,
    pub status: CheckStatus,
    pub checked_at: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedirectApplyDto {
    pub backup_path: String,
    pub updated: usize,
}

/// #13 规则 DTO 直接复用 core 类型，避免两处字段定义漂移（camelCase 由 core 派生）。
pub type RuleFieldDto = RuleField;
pub type RuleDto = Rule;
pub type RuleInputDto = RuleInput;

/// #13 归类预览行：命中规则与目标路径，冲突项标红。
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifyItemDto {
    pub node_id: NodeId,
    pub title: String,
    pub current_path: String,
    pub matched_rule_id: String,
    pub matched_rule_pattern: String,
    pub target_path: String,
    pub conflict: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifyChoiceDto {
    pub node_id: NodeId,
    pub target_path: String,
}

/// #14 WebDAV 配置（先测后存；密码明文仅存本机 settings）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavConfigDto {
    pub url: String,
    pub user: String,
    pub password: String,
    pub dir: String,
    pub allow_http: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavFileDto {
    pub name: String,
    pub size: i64,
    pub modified: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavUploadDto {
    pub backup_path: String,
    pub remote_path: String,
    pub bytes: i64,
}

// ---------------------------------------------------------------------------
// 内部工具
// ---------------------------------------------------------------------------

/// #14 WebDAV settings 键（与 phase2-design §3.2 一致）。
const WEBDAV_URL_KEY: &str = "webdav_url";
const WEBDAV_USER_KEY: &str = "webdav_user";
const WEBDAV_PASSWORD_KEY: &str = "webdav_password";
const WEBDAV_DIR_KEY: &str = "webdav_dir";
const WEBDAV_ALLOW_HTTP_KEY: &str = "webdav_allow_http";

/// WebDAV 导入临时文件序号：同一毫秒内多次导入也不冲突。
static WEBDAV_TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// 规则 id 序号：同一毫秒内新增多条也不冲突。
static RULE_ID_SEQ: AtomicU64 = AtomicU64::new(0);

fn next_rule_id() -> String {
    format!(
        "r{}-{}",
        chrono::Utc::now().timestamp_millis(),
        RULE_ID_SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// 从 settings.classify_rules 读取规则集；缺失/非法时回退示例规则。
fn load_rule_set(store: &Store) -> Result<RuleSet, String> {
    let raw = store.get_setting("classify_rules").map_err(es)?;
    Ok(match raw {
        Some(value) if !value.trim().is_empty() => RuleSet::from_settings_json(&value),
        _ => RuleSet::sample(),
    })
}

fn save_rule_set(store: &mut Store, rules: &RuleSet) -> Result<(), String> {
    store
        .set_setting("classify_rules", &rules.to_settings_json())
        .map_err(es)
}

/// 从本机 settings 读取 WebDAV 配置；缺省目录回退为 `/`。
fn load_webdav_config(store: &Store) -> Result<WebDavConfig, String> {
    let get = |key: &str| store.get_setting(key).map_err(es);
    let dir = get(WEBDAV_DIR_KEY)?.unwrap_or_else(|| "/".to_string());
    Ok(WebDavConfig {
        url: get(WEBDAV_URL_KEY)?.unwrap_or_default(),
        user: get(WEBDAV_USER_KEY)?.unwrap_or_default(),
        password: get(WEBDAV_PASSWORD_KEY)?.unwrap_or_default(),
        dir: if dir.trim().is_empty() { "/".to_string() } else { dir },
        allow_http: matches!(get(WEBDAV_ALLOW_HTTP_KEY)?.as_deref(), Some("1")),
    })
}

/// 读取配置（单独加锁，避免与网络请求交叉持锁）。
fn webdav_config_from_state(state: &AppState) -> Result<WebDavConfig, String> {
    let store = state.store.lock().unwrap();
    load_webdav_config(&store)
}

/// 构造 WebDAV 客户端并复用设置中的 HTTP 代理。
fn webdav_client(state: &AppState, cfg: WebDavConfig) -> Result<WebDavClient, String> {
    let proxy = {
        let store = state.store.lock().unwrap();
        store
            .get_setting("proxy")
            .map_err(es)?
            .filter(|p| !p.trim().is_empty())
    };
    WebDavClient::new(cfg, proxy).map_err(es)
}

fn validate_rule_fields(pattern: &str, target_path: &str) -> Result<(), String> {
    if pattern.trim().is_empty() {
        return Err("请输入匹配值".to_string());
    }
    if target_path.trim().is_empty() {
        return Err("请输入目标目录".to_string());
    }
    Ok(())
}

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

/// 在指定父文件夹下新建子文件夹，返回新节点 id。
#[tauri::command]
pub async fn create_folder(
    state: State<'_, AppState>,
    parent_id: NodeId,
    title: String,
    index: i64,
) -> Result<NodeId, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("文件夹名称不能为空".into());
    }
    let mut store = state.store.lock().unwrap();
    store.create_folder(parent_id, title, index).map_err(es)
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

/// 手动删除选中的书签/文件夹（软删除进入回收站，可恢复）。
#[tauri::command]
pub async fn delete_nodes(state: State<'_, AppState>, ids: Vec<NodeId>) -> Result<usize, String> {
    let mut store = state.store.lock().unwrap();
    store.soft_delete(&ids).map_err(es)
}

// ---------------------------------------------------------------------------
// 去重
// ---------------------------------------------------------------------------

/// 分组结果 → 前端 DTO（精确去重与模糊去重共用，保证字段一致）。
fn dup_group_dtos(groups: Vec<dedup::DupGroup>, nodes: &[Node]) -> Vec<DupGroupDto> {
    if groups.is_empty() {
        return Vec::new();
    }
    let node_map: HashMap<NodeId, Node> = nodes.iter().map(|n| (n.id, n.clone())).collect();
    let paths = alive_paths(nodes);
    groups
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
                hash: g.hash,
                keep,
                duplicates,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn dedup_preview(state: State<'_, AppState>) -> Result<Vec<DupGroupDto>, String> {
    let store = state.store.lock().unwrap();
    let dfs = store.dfs_bookmarks().map_err(es)?;
    let groups = dedup::find_duplicate_groups(&dfs);
    let nodes = store.all_nodes().map_err(es)?;
    Ok(dup_group_dtos(groups, &nodes))
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
// #9 模糊去重（preview / apply 使用前端传入的同一份 opts）
// ---------------------------------------------------------------------------

/// 按前端开关重算分组；`opts` 原样透传，后端不读取 settings。
#[tauri::command]
pub async fn fuzzy_dedup_preview(
    state: State<'_, AppState>,
    opts: FuzzyOptionsDto,
) -> Result<Vec<DupGroupDto>, String> {
    let store = state.store.lock().unwrap();
    let dfs = store.dfs_bookmarks().map_err(es)?;
    let groups = dedup::find_duplicate_groups_with(&dfs, opts);
    let nodes = store.all_nodes().map_err(es)?;
    Ok(dup_group_dtos(groups, &nodes))
}

/// 用与预览完全相同的 `opts` 重算分组并软删除重复项。
/// 四个开关全关（STRICT）时，结果与第 1 期 `dedup_apply` 完全一致。
#[tauri::command]
pub async fn fuzzy_dedup_apply(
    state: State<'_, AppState>,
    opts: FuzzyOptionsDto,
) -> Result<usize, String> {
    let mut store = state.store.lock().unwrap();
    let dfs = store.dfs_bookmarks().map_err(es)?;
    let groups = dedup::find_duplicate_groups_with(&dfs, opts);
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
// #10 合并文件夹（preview / apply）
// ---------------------------------------------------------------------------

/// 同名文件夹分组预览：组名、成员路径、书签数、默认保留项（DFS 第一个）。
#[tauri::command]
pub async fn merge_folders_preview(state: State<'_, AppState>) -> Result<Vec<MergeGroupDto>, String> {
    let store = state.store.lock().unwrap();
    let groups = store.merge_folder_groups().map_err(es)?;
    let nodes = store.all_nodes().map_err(es)?;
    let node_map: HashMap<NodeId, Node> = nodes.iter().map(|n| (n.id, n.clone())).collect();
    let paths = alive_paths(&nodes);
    Ok(groups
        .into_iter()
        .map(|g: MergeFolderGroup| MergeGroupDto {
            key: g.key,
            folders: g
                .folder_ids
                .iter()
                .filter_map(|id| item_dto(*id, &node_map, &paths))
                .collect(),
            total_bookmarks: g.total_bookmarks,
            suggested_keep_id: g.suggested_keep_id,
        })
        .collect())
}

/// 应用合并：先生成 HTML 备份，再在 core 的单一事务内迁移子节点并软删除空壳。
#[tauri::command]
pub async fn merge_folders_apply(
    state: State<'_, AppState>,
    groups: Vec<MergeChoiceDto>,
) -> Result<MergeResultDto, String> {
    let backup_path = {
        let store = state.store.lock().unwrap();
        let dir = state.data_dir.join("backups");
        bookmark_core::backup::create_backup(&store, &dir).map_err(es)?
    };
    let pairs: Vec<(NodeId, Vec<NodeId>)> = groups
        .into_iter()
        .map(|g| (g.keep_id, g.merge_ids))
        .collect();
    let outcome: MergeOutcome = {
        let mut store = state.store.lock().unwrap();
        store.merge_folders(&pairs).map_err(es)?
    };
    Ok(MergeResultDto {
        backup_path: backup_path.display().to_string(),
        merged_groups: outcome.merged_groups,
        moved_nodes: outcome.moved_nodes,
    })
}

// ---------------------------------------------------------------------------
// #13 规则归类：规则 CRUD + 预览 / 应用
// ---------------------------------------------------------------------------

/// 规则列表；设置项缺失时回填示例规则并持久化。
#[tauri::command]
pub async fn rule_list(state: State<'_, AppState>) -> Result<Vec<RuleDto>, String> {
    let mut store = state.store.lock().unwrap();
    let rules = load_rule_set(&store)?;
    let raw = store.get_setting("classify_rules").map_err(es)?;
    if raw.as_deref().unwrap_or("").trim().is_empty() {
        save_rule_set(&mut store, &rules)?;
    }
    Ok(rules.rules)
}

/// 新增规则：后端生成 id，回填后整集写回 settings（保持列表顺序）。
#[tauri::command]
pub async fn rule_add(
    state: State<'_, AppState>,
    rule: RuleInputDto,
) -> Result<RuleDto, String> {
    validate_rule_fields(&rule.pattern, &rule.target_path)?;
    let mut store = state.store.lock().unwrap();
    let mut rules = load_rule_set(&store)?;
    let created = RuleInput {
        field: rule.field,
        pattern: rule.pattern.trim().to_string(),
        target_path: rule.target_path.trim().to_string(),
        enabled: rule.enabled,
    }
    .into_rule(next_rule_id());
    rules.rules.push(created.clone());
    save_rule_set(&mut store, &rules)?;
    Ok(created)
}

/// 按 id 原位更新规则（id 不可修改）。
#[tauri::command]
pub async fn rule_update(state: State<'_, AppState>, rule: RuleDto) -> Result<(), String> {
    validate_rule_fields(&rule.pattern, &rule.target_path)?;
    let mut store = state.store.lock().unwrap();
    let mut rules = load_rule_set(&store)?;
    let entry = rules
        .rules
        .iter_mut()
        .find(|r| r.id == rule.id)
        .ok_or_else(|| "规则不存在".to_string())?;
    *entry = rule;
    save_rule_set(&mut store, &rules)?;
    Ok(())
}

#[tauri::command]
pub async fn rule_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut store = state.store.lock().unwrap();
    let mut rules = load_rule_set(&store)?;
    let before = rules.rules.len();
    rules.rules.retain(|r| r.id != id);
    if rules.rules.len() == before {
        return Err("规则不存在".to_string());
    }
    save_rule_set(&mut store, &rules)?;
    Ok(())
}

/// 按传入 id 顺序重排；未列出的规则由 core 追加在末尾，不会丢失。
#[tauri::command]
pub async fn rule_reorder(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<(), String> {
    let mut store = state.store.lock().unwrap();
    let mut rules = load_rule_set(&store)?;
    rules.reorder(&ids);
    save_rule_set(&mut store, &rules)?;
    Ok(())
}

/// 归类预览：只读。作用域 = 根目录未归类书签 ∪ 显式勾选，开关打开时扩展到全部存活书签。
/// 每项取自上而下首个命中规则；冲突项标红但不移动任何节点。
#[tauri::command]
pub async fn classify_preview(
    state: State<'_, AppState>,
    include_classified: bool,
    selected_ids: Vec<NodeId>,
) -> Result<Vec<ClassifyItemDto>, String> {
    let store = state.store.lock().unwrap();
    let rules = load_rule_set(&store)?;
    let candidates = store
        .classify_candidates(include_classified, &selected_ids)
        .map_err(es)?;
    let nodes = store.all_nodes().map_err(es)?;
    let node_map: HashMap<NodeId, Node> = nodes
        .iter()
        .filter(|n| !n.deleted)
        .map(|n| (n.id, n.clone()))
        .collect();

    let mut out = Vec::new();
    for id in candidates {
        let node = match node_map.get(&id) {
            Some(n) => n,
            None => continue,
        };
        let url = node.url.clone().unwrap_or_default();
        let matched = match rules.first_match(&url, &node.title) {
            Some(r) => r,
            None => continue,
        };
        let target = matched.target_path.trim();
        if target.is_empty() {
            continue;
        }
        out.push(ClassifyItemDto {
            node_id: id,
            title: node.title.clone(),
            current_path: store.node_path(id).map_err(es)?,
            matched_rule_id: matched.id.clone(),
            matched_rule_pattern: matched.pattern.clone(),
            target_path: target.to_string(),
            conflict: store.path_has_conflict(target).map_err(es)?,
        });
    }
    Ok(out)
}

/// 归类应用：仅处理勾选且无冲突的项，逐级建目录后走 move_nodes（含自身子树校验）。
#[tauri::command]
pub async fn classify_apply(
    state: State<'_, AppState>,
    items: Vec<ClassifyChoiceDto>,
) -> Result<usize, String> {
    let mut store = state.store.lock().unwrap();

    // 先整批复检，避免部分项目已移动后才发现冲突
    let mut plan: Vec<(NodeId, String)> = Vec::new();
    {
        let nodes = store.all_nodes().map_err(es)?;
        let alive: HashMap<NodeId, Node> = nodes
            .iter()
            .filter(|n| !n.deleted)
            .map(|n| (n.id, n.clone()))
            .collect();
        for item in items {
            let target = item.target_path.trim();
            if target.is_empty() {
                continue;
            }
            match alive.get(&item.node_id) {
                Some(n) if n.kind == NodeKind::Bookmark && n.id != ROOT_ID => {}
                _ => continue,
            }
            if store.path_has_conflict(target).map_err(es)? {
                continue;
            }
            plan.push((item.node_id, target.to_string()));
        }
    }

    let mut classified = 0usize;
    for (node_id, target) in plan {
        let folder_id = store.create_missing_path(&target).map_err(es)?;
        store.move_nodes(&[node_id], folder_id, 99999).map_err(es)?;
        classified += 1;
    }
    Ok(classified)
}

// ---------------------------------------------------------------------------
// 链接检测
// ---------------------------------------------------------------------------

/// 第 1 期全量检测与第 2 期增量检测共用的「检测执行段」。
///
/// 调用方只负责选目标；本函数负责复用代理配置、LinkChecker、取消机制，
/// 以及 `check://progress` / `check://finished` 事件与结果写回。
/// 跳过项不会进入 targets，因此不会写 check_results、不会清空原状态。
fn run_check_targets(
    app: tauri::AppHandle,
    state: &AppState,
    targets: Vec<CheckTarget>,
) -> Result<StartCheckResult, String> {
    let total = targets.len();
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

    let targets: Vec<CheckTarget> = {
        let store = state.store.lock().unwrap();
        let dfs = store.dfs_bookmarks().map_err(es)?;
        match scope.as_str() {
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
        }
    };
    run_check_targets(app, state.inner(), targets)
}

#[tauri::command]
pub async fn cancel_check(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(handle) = state.run.lock().unwrap().as_ref() {
        handle.cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// #11 增量检测
// ---------------------------------------------------------------------------

/// 预览增量检测计划：只读筛选，不执行网络请求、不改动任何检测结果。
#[tauri::command]
pub async fn incremental_preview(
    state: State<'_, AppState>,
    days: i64,
) -> Result<IncrementalPlanDto, String> {
    let store = state.store.lock().unwrap();
    let plan = store.incremental_targets(days).map_err(es)?;
    Ok(IncrementalPlanDto {
        new_count: plan.new_count,
        url_changed_count: plan.url_changed_count,
        stale_count: plan.stale_count,
        skipped_count: plan.skipped_count,
        total: plan.targets.len(),
    })
}

/// 增量检测：仅对筛选出的目标复用第 1 期检测流程、代理、事件与取消机制。
#[tauri::command]
pub async fn start_incremental_check(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    days: i64,
) -> Result<StartCheckResult, String> {
    {
        let run = state.run.lock().unwrap();
        if run.is_some() {
            return Err("已有检测任务在进行中".into());
        }
    }

    let targets: Vec<CheckTarget> = {
        let store = state.store.lock().unwrap();
        let plan = store.incremental_targets(days).map_err(es)?;
        let selected: std::collections::HashSet<NodeId> = plan.targets.into_iter().collect();
        // 按 DFS 顺序取 URL；selected 只含 HTTP 目标，跳过项不会进入检测队列
        store
            .dfs_bookmarks()
            .map_err(es)?
            .into_iter()
            .filter(|(id, _)| selected.contains(id))
            .map(|(node_id, url)| CheckTarget { node_id, url })
            .collect()
    };
    run_check_targets(app, state.inner(), targets)
}

// ---------------------------------------------------------------------------
// #12 重定向更新
// ---------------------------------------------------------------------------

/// 重定向候选：final_url 非空且 ≠ 当前地址、状态 ok/suspicious、HTTP 协议，按标题排序。
#[tauri::command]
pub async fn list_redirects(state: State<'_, AppState>) -> Result<Vec<RedirectDto>, String> {
    let store = state.store.lock().unwrap();
    let checks = store.check_results_map().map_err(es)?;
    let nodes = store.all_nodes().map_err(es)?;
    let node_map: HashMap<NodeId, Node> = nodes.iter().map(|n| (n.id, n.clone())).collect();

    let mut result: Vec<RedirectDto> = checks
        .into_iter()
        .filter(|(_, r)| matches!(r.status, CheckStatus::Ok | CheckStatus::Suspicious))
        .filter_map(|(id, r)| {
            let n = node_map.get(&id)?;
            if n.deleted || n.kind != NodeKind::Bookmark {
                return None;
            }
            let old_url = n.url.clone().unwrap_or_default();
            let new_url = r.final_url.clone().unwrap_or_default();
            if old_url.trim().is_empty()
                || new_url.trim().is_empty()
                || old_url == new_url
                || !is_http_url(&old_url)
                || !is_http_url(&new_url)
            {
                return None;
            }
            Some(RedirectDto {
                node_id: id,
                title: n.title.clone(),
                old_url,
                new_url,
                status: r.status,
                checked_at: r.checked_at,
            })
        })
        .collect();
    result.sort_by(|a, b| {
        a.title
            .trim()
            .to_lowercase()
            .cmp(&b.title.trim().to_lowercase())
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    Ok(result)
}

/// 应用重定向：先备份，再在同一事务内更新 nodes.url 与 check_results.url/url_hash。
#[tauri::command]
pub async fn apply_redirects(
    state: State<'_, AppState>,
    ids: Vec<NodeId>,
) -> Result<RedirectApplyDto, String> {
    let backup_path = {
        let store = state.store.lock().unwrap();
        let dir = state.data_dir.join("backups");
        bookmark_core::backup::create_backup(&store, &dir).map_err(es)?
    };
    let updated = {
        let mut store = state.store.lock().unwrap();
        store.apply_redirects(&ids).map_err(es)?
    };
    Ok(RedirectApplyDto {
        backup_path: backup_path.display().to_string(),
        updated,
    })
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

/// 通用设置读取（#11 用 `check_incremental_days` 读写超期阈值，缺省时前端回退 7 天）。
#[tauri::command]
pub async fn get_setting(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    let store = state.store.lock().unwrap();
    store.get_setting(&key).map_err(es)
}

/// 通用设置写入（仅本机 SQLite settings 表）。
#[tauri::command]
pub async fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    let mut store = state.store.lock().unwrap();
    store.set_setting(&key, &value).map_err(es)
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

// ---------------------------------------------------------------------------
// #14 WebDAV 备份同步（先测后存：test 用当前表单，其余从 settings 读）
// ---------------------------------------------------------------------------

/// 测试连接：只测不存，配置来自 UI 当前表单。
#[tauri::command]
pub async fn webdav_test(
    state: State<'_, AppState>,
    cfg: WebDavConfigDto,
) -> Result<ProxyTestResult, String> {
    let cfg = WebDavConfig {
        url: cfg.url,
        user: cfg.user,
        password: cfg.password,
        dir: cfg.dir,
        allow_http: cfg.allow_http,
    };
    let client = webdav_client(&state, cfg)?;
    match client.test().await {
        Ok(ms) => Ok(ProxyTestResult {
            ok: true,
            latency_ms: Some(ms.max(0) as u64),
            message: format!("连接成功，延迟 {ms} ms"),
        }),
        Err(e) => Ok(ProxyTestResult {
            ok: false,
            latency_ms: None,
            message: e.to_string(),
        }),
    }
}

/// 上传备份：生成时间戳 HTML 后 PUT 到远端，永不覆盖同名文件。
#[tauri::command]
pub async fn webdav_upload(state: State<'_, AppState>) -> Result<WebDavUploadDto, String> {
    let cfg = webdav_config_from_state(&state)?;
    let client = webdav_client(&state, cfg)?;

    let backup_path = {
        let store = state.store.lock().unwrap();
        bookmark_core::backup::create_backup(&store, &state.data_dir.join("backups")).map_err(es)?
    };
    let name = backup_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "备份文件名无效".to_string())?
        .to_string();
    let bytes = std::fs::read(&backup_path).map_err(|e| format!("读取备份文件失败: {e}"))?;
    let remote_path = client.upload(&name, &bytes).await.map_err(es)?;

    Ok(WebDavUploadDto {
        backup_path: backup_path.display().to_string(),
        remote_path,
        bytes: bytes.len() as i64,
    })
}

/// 列出远端 HTML 备份文件。
#[tauri::command]
pub async fn webdav_list(state: State<'_, AppState>) -> Result<Vec<WebDavFileDto>, String> {
    let cfg = webdav_config_from_state(&state)?;
    let client = webdav_client(&state, cfg)?;
    let files = client.list().await.map_err(es)?;
    Ok(files
        .into_iter()
        .map(|f| WebDavFileDto {
            name: f.name,
            size: f.size,
            modified: f.modified,
        })
        .collect())
}

/// 下载远端备份到唯一临时文件，按 replace/append 同语义导入，成功失败都清理。
#[tauri::command]
pub async fn webdav_import(
    state: State<'_, AppState>,
    name: String,
    mode: String,
) -> Result<ImportSummary, String> {
    let import_mode = match mode.as_str() {
        "append" => ImportMode::Append,
        "replace" => ImportMode::Replace,
        _ => return Err("导入方式只能是 replace 或 append".to_string()),
    };
    let cfg = webdav_config_from_state(&state)?;
    let client = webdav_client(&state, cfg)?;
    let bytes = client.download(&name).await.map_err(es)?;

    let dir = state.data_dir.join("tmp");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建临时目录失败: {e}"))?;
    let tmp_path = dir.join(format!(
        "webdav_import_{}_{}.html",
        chrono::Utc::now().timestamp_millis(),
        WEBDAV_TMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&tmp_path, &bytes).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        format!("写入临时文件失败: {e}")
    })?;

    let result = (|| -> Result<ImportSummary, String> {
        let html = std::fs::read_to_string(&tmp_path)
            .map_err(|e| format!("远端备份不是有效的 UTF-8 HTML: {e}"))?;
        let parsed = netscape::parse(&html).map_err(es)?;
        let stats: ImportStats = {
            let mut store = state.store.lock().unwrap();
            store.import_children(&parsed.children, import_mode).map_err(es)?
        };
        Ok(ImportSummary {
            folders: stats.folders,
            bookmarks: stats.bookmarks,
        })
    })();

    let _ = std::fs::remove_file(&tmp_path);
    result
}
