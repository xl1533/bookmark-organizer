//! SQLite 持久化（rusqlite，bundled 编译）。
//!
//! 表结构：
//! - `nodes`：书签树（软删除标记 `deleted`）
//! - `check_results`：每个节点最新检测结果（按 node_id 主键）
//! - `check_history`：历次检测流水
//! - `trash`：回收站条目（软删除记录）
//! - `settings`：键值设置（代理等）

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::checker::{CheckResult, CheckStatus};
use crate::empty;
use crate::model::{NewNode, Node, NodeKind, NodeId, ROOT_ID};
use crate::CoreResult;

/// 回收站条目。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    pub id: i64,
    pub node_id: NodeId,
    pub title: String,
    pub kind: NodeKind,
    /// 条目子树内的书签数量
    pub item_count: i64,
    pub deleted_at: i64,
}

/// 导入统计。
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStats {
    pub folders: i64,
    pub bookmarks: i64,
}

/// 全局统计。
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub folders: i64,
    pub bookmarks: i64,
    pub ok: i64,
    pub suspicious: i64,
    pub dead: i64,
    pub unchecked: i64,
    pub trash_count: i64,
}

/// #11 增量检测计划：待检测目标 + 四类原因计数。
///
/// 计数口径（与第 1 期全量检测共用 `dfs_bookmarks()` 作用域，即有 URL 的存活书签；
/// 每本只落入一类，四类之和 = 该作用域总数，与预览 UI 自洽）：
/// - 优先级 `新增 > URL 变更 > 超期 > 跳过`；
/// - 同时命中「URL 变更 + 超期」时计入 `url_changed_count`（变更语义更强，重检一次即可）；
/// - 非 HTTP 协议（javascript:/mailto:/chrome:/place: 等）按第 1 期规则永不入队，计入 `skipped_count`；
/// - 恒等式：`targets.len() == new_count + url_changed_count + stale_count == 总数 - skipped_count`。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementalPlan {
    pub targets: Vec<NodeId>,
    pub new_count: usize,
    pub url_changed_count: usize,
    pub stale_count: usize,
    pub skipped_count: usize,
}

/// #10 同名文件夹分组预览（标题 trim + 大小写不敏感，DFS 顺序）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeFolderGroup {
    /// 规范化后的分组键（trim 后小写）
    pub key: String,
    /// 组成员文件夹 id，按 DFS 顺序排列
    pub folder_ids: Vec<NodeId>,
    /// 组内各文件夹子树书签数之和
    pub total_bookmarks: i64,
    /// DFS 顺序第一个文件夹
    pub suggested_keep_id: NodeId,
}

/// #10 合并结果（备份路径由命令层生成）。
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOutcome {
    pub merged_groups: usize,
    pub moved_nodes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    /// 清空后整库导入
    Replace,
    /// 追加到当前根目录下
    Append,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    // ------------------------------------------------------------------
    // 打开与初始化
    // ------------------------------------------------------------------

    pub fn open(path: &Path) -> CoreResult<Store> {
        let conn = Connection::open(path)?;
        let store = Store::init(conn, true)?;
        Ok(store)
    }

    /// 内存库（单元测试用）。
    pub fn open_in_memory() -> CoreResult<Store> {
        let conn = Connection::open_in_memory()?;
        Store::init(conn, false)
    }

    fn init(conn: Connection, wal: bool) -> CoreResult<Store> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        if wal {
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
        }
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS nodes (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                parent_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                kind      TEXT NOT NULL CHECK(kind IN ('folder','bookmark')),
                title     TEXT NOT NULL,
                url       TEXT,
                add_date  INTEGER,
                icon      TEXT,
                position  INTEGER NOT NULL DEFAULT 0,
                deleted   INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_nodes_parent ON nodes(parent_id, position);
            CREATE INDEX IF NOT EXISTS idx_nodes_deleted ON nodes(deleted);
            CREATE INDEX IF NOT EXISTS idx_nodes_url ON nodes(url);

            CREATE TABLE IF NOT EXISTS check_results (
                node_id    INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
                url        TEXT NOT NULL,
                url_hash   TEXT NOT NULL,
                status     TEXT NOT NULL CHECK(status IN ('ok','suspicious','dead')),
                http_code  INTEGER,
                final_url  TEXT,
                message    TEXT,
                elapsed_ms INTEGER,
                checked_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_check_hash ON check_results(url_hash);

            CREATE TABLE IF NOT EXISTS check_history (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                node_id    INTEGER NOT NULL,
                url        TEXT NOT NULL,
                url_hash   TEXT NOT NULL,
                status     TEXT NOT NULL,
                http_code  INTEGER,
                message    TEXT,
                checked_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_history_node ON check_history(node_id, checked_at);

            CREATE TABLE IF NOT EXISTS trash (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                node_id    INTEGER NOT NULL,
                title      TEXT NOT NULL,
                kind       TEXT NOT NULL,
                item_count INTEGER NOT NULL,
                deleted_at INTEGER NOT NULL,
                restored   INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT
            );
            "#,
        )?;

        // 确保根节点存在（自引用 parent_id）
        let root_exists: Option<i64> = conn
            .query_row("SELECT id FROM nodes WHERE id = ?1", params![ROOT_ID], |r| r.get(0))
            .optional()?;
        if root_exists.is_none() {
            conn.execute(
                "INSERT INTO nodes (id, parent_id, kind, title, position) VALUES (?1, ?1, 'folder', ?2, 0)",
                params![ROOT_ID, "书签栏"],
            )?;
        }
        Ok(Store { conn })
    }

    // ------------------------------------------------------------------
    // 导入 / 导出
    // ------------------------------------------------------------------

    /// 导入一棵树的 children。
    pub fn import_children(&mut self, children: &[NewNode], mode: ImportMode) -> CoreResult<ImportStats> {
        let tx = self.conn.transaction()?;
        let mut stats = ImportStats { folders: 0, bookmarks: 0 };

        if mode == ImportMode::Replace {
            tx.execute("DELETE FROM check_results", [])?;
            tx.execute("DELETE FROM check_history", [])?;
            tx.execute("DELETE FROM trash", [])?;
            tx.execute("DELETE FROM nodes", [])?;
            tx.execute(
                "INSERT INTO nodes (id, parent_id, kind, title, position) VALUES (?1, ?1, 'folder', ?2, 0)",
                params![ROOT_ID, "书签栏"],
            )?;
            insert_nodes(&tx, children, ROOT_ID, 0, &mut stats)?;
        } else {
            // 追加模式：接续根目录现有子节点的 position
            let base: i64 = tx.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM nodes WHERE parent_id = ?1 AND deleted = 0",
                params![ROOT_ID],
                |r| r.get(0),
            )?;
            insert_nodes(&tx, children, ROOT_ID, base, &mut stats)?;
        }

        tx.commit()?;
        Ok(stats)
    }

    /// 导出当前（未删除）整棵树。
    pub fn export_tree(&self) -> CoreResult<NewNode> {
        let nodes = self.load_nodes(false)?;
        let map: HashMap<NodeId, Node> = nodes.into_iter().map(|n| (n.id, n)).collect();
        Ok(build_new_tree(&map, ROOT_ID))
    }

    fn load_nodes(&self, include_deleted: bool) -> CoreResult<Vec<Node>> {
        let sql = if include_deleted {
            "SELECT id, parent_id, kind, title, url, add_date, icon, position, deleted FROM nodes"
        } else {
            "SELECT id, parent_id, kind, title, url, add_date, icon, position, deleted FROM nodes WHERE deleted = 0"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], row_to_node)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// 全量节点（含删除），供内存分析。
    pub fn all_nodes(&self) -> CoreResult<Vec<Node>> {
        self.load_nodes(true)
    }

    /// DFS 顺序遍历所有未删除书签（供去重）。
    pub fn dfs_bookmarks(&self) -> CoreResult<Vec<(NodeId, String)>> {
        let nodes = self.load_nodes(false)?;
        let map: HashMap<NodeId, Node> = nodes.into_iter().map(|n| (n.id, n)).collect();
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
        let mut out = Vec::new();
        let mut stack = vec![ROOT_ID];
        // 递归改迭代：前序 DFS
        fn walk(
            id: NodeId,
            map: &HashMap<NodeId, Node>,
            children: &HashMap<NodeId, Vec<NodeId>>,
            out: &mut Vec<(NodeId, String)>,
        ) {
            if let Some(n) = map.get(&id) {
                if n.kind == NodeKind::Bookmark {
                    if let Some(u) = &n.url {
                        out.push((id, u.clone()));
                    }
                }
            }
            if let Some(kids) = children.get(&id) {
                for &k in kids {
                    walk(k, map, children, out);
                }
            }
        }
        walk(ROOT_ID, &map, &children, &mut out);
        stack.clear();
        Ok(out)
    }

    // ------------------------------------------------------------------
    // 树操作
    // ------------------------------------------------------------------

    /// 批量移动节点到目标文件夹的 index 位置。
    ///
    /// 具体逻辑放在 [`move_nodes_in_tx`]，便于 #10 合并文件夹在同一事务内复用，
    /// 保证「移动 + 删除空壳」要么全部成功、要么全部回滚。
    pub fn move_nodes(&mut self, ids: &[NodeId], new_parent: NodeId, index: i64) -> CoreResult<()> {
        let tx = self.conn.transaction()?;
        move_nodes_in_tx(&tx, ids, new_parent, index)?;
        tx.commit()?;
        Ok(())
    }

    /// 在指定父文件夹下新建子文件夹，返回新节点 id。
    pub fn create_folder(&mut self, parent_id: NodeId, title: &str, index: i64) -> CoreResult<NodeId> {
        let tx = self.conn.transaction()?;
        let parent: Option<(String, String)> = tx
            .query_row(
                "SELECT kind, title FROM nodes WHERE id = ?1 AND deleted = 0",
                params![parent_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (kind, _) = parent.ok_or_else(|| crate::CoreError::Other("目标文件夹不存在".into()))?;
        if kind != "folder" {
            return Err(crate::CoreError::Other("目标不是文件夹".into()));
        }

        let mut stmt = tx.prepare(
            "SELECT id FROM nodes WHERE parent_id = ?1 AND deleted = 0 ORDER BY position, id",
        )?;
        let existing: Vec<NodeId> = stmt
            .query_map(params![parent_id], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        let at = index.clamp(0, existing.len() as i64) as usize;
        tx.execute(
            "INSERT INTO nodes (parent_id, kind, title, position) VALUES (?1, 'folder', ?2, 0)",
            params![parent_id, title],
        )?;
        let new_id = tx.last_insert_rowid();
        // 按目标顺序逐条编号，使 index 指定的插入位置生效
        let mut order = existing;
        order.insert(at, new_id);
        for (seq, id) in order.iter().enumerate() {
            tx.execute("UPDATE nodes SET position = ?1 WHERE id = ?2", params![seq as i64, id])?;
        }
        tx.commit()?;
        Ok(new_id)
    }

    /// 查找最顶层空文件夹。
    pub fn find_empty_folder_ids(&self) -> CoreResult<Vec<NodeId>> {
        let nodes = self.load_nodes(true)?;
        let map: HashMap<NodeId, Node> = nodes.into_iter().map(|n| (n.id, n)).collect();
        Ok(empty::find_empty_folders(&map, ROOT_ID))
    }

    // ------------------------------------------------------------------
    // 软删除 / 回收站
    // ------------------------------------------------------------------

    /// 软删除子树（写入回收站条目），返回条目数。
    pub fn soft_delete(&mut self, ids: &[NodeId]) -> CoreResult<usize> {
        let tx = self.conn.transaction()?;
        let count = soft_delete_in_tx(&tx, ids)?;
        tx.commit()?;
        Ok(count)
    }

    /// 回收站列表（未恢复）。
    pub fn list_trash(&self) -> CoreResult<Vec<TrashEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, node_id, title, kind, item_count, deleted_at FROM trash WHERE restored = 0 ORDER BY deleted_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(TrashEntry {
                id: r.get(0)?,
                node_id: r.get(1)?,
                title: r.get(2)?,
                kind: NodeKind::from_str(&r.get::<_, String>(3)?),
                item_count: r.get(4)?,
                deleted_at: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// 从回收站恢复（子树取消删除标记）。
    pub fn trash_restore(&mut self, entry_id: i64) -> CoreResult<()> {
        let node_id: Option<NodeId> = self
            .conn
            .query_row("SELECT node_id FROM trash WHERE id = ?1 AND restored = 0", params![entry_id], |r| r.get(0))
            .optional()?;
        let node_id = node_id.ok_or_else(|| crate::CoreError::Other("回收站条目不存在".into()))?;

        let tx = self.conn.transaction()?;
        let sub = subtree_ids_including_deleted(&tx, node_id)?;
        for sid in &sub {
            tx.execute("UPDATE nodes SET deleted = 0 WHERE id = ?1", params![sid])?;
        }
        // 原父节点若已被删除或不存在 → 移动到根目录末尾
        let parent: Option<NodeId> = tx
            .query_row(
                "SELECT parent_id FROM nodes WHERE id = ?1",
                params![node_id],
                |r| r.get(0),
            )
            .optional()?;
        let alive_parent: Option<i64> = match parent {
            Some(p) => tx
                .query_row(
                    "SELECT 1 FROM nodes WHERE id = ?1 AND deleted = 0",
                    params![p],
                    |r| r.get(0),
                )
                .optional()?,
            None => None,
        };
        if parent.is_none() || parent == Some(node_id) || alive_parent.is_none() {
            let max_pos: i64 = tx.query_row(
                "SELECT COALESCE(MAX(position), -1) FROM nodes WHERE parent_id = ?1 AND deleted = 0",
                params![ROOT_ID],
                |r| r.get(0),
            )?;
            tx.execute(
                "UPDATE nodes SET parent_id = ?1, position = ?2 WHERE id = ?3",
                params![ROOT_ID, max_pos + 1, node_id],
            )?;
        }
        renumber_children(&tx, parent.unwrap_or(ROOT_ID))?;
        renumber_children(&tx, ROOT_ID)?;
        tx.execute("UPDATE trash SET restored = 1 WHERE id = ?1", params![entry_id])?;
        tx.commit()?;
        Ok(())
    }

    /// 彻底删除回收站条目（硬删除子树）。
    pub fn trash_purge(&mut self, entry_id: i64) -> CoreResult<()> {
        let node_id: Option<NodeId> = self
            .conn
            .query_row("SELECT node_id FROM trash WHERE id = ?1", params![entry_id], |r| r.get(0))
            .optional()?;
        let node_id = node_id.ok_or_else(|| crate::CoreError::Other("回收站条目不存在".into()))?;
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM nodes WHERE id = ?1", params![node_id])?; // CASCADE 递归? — 见下
        tx.execute("DELETE FROM trash WHERE id = ?1", params![entry_id])?;
        tx.commit()?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // 检测结果
    // ------------------------------------------------------------------

    pub fn upsert_check_result(&mut self, r: &CheckResult) -> CoreResult<()> {
        let hash = crate::urlnorm::url_hash_of(&r.url).map(|(_, h)| h).unwrap_or_default();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO check_results (node_id, url, url_hash, status, http_code, final_url, message, elapsed_ms, checked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(node_id) DO UPDATE SET
               url=excluded.url, url_hash=excluded.url_hash, status=excluded.status,
               http_code=excluded.http_code, final_url=excluded.final_url, message=excluded.message,
               elapsed_ms=excluded.elapsed_ms, checked_at=excluded.checked_at",
            params![
                r.node_id,
                r.url,
                hash,
                r.status.as_str(),
                r.http_code,
                r.final_url,
                r.message,
                r.elapsed_ms as i64,
                r.checked_at
            ],
        )?;
        tx.execute(
            "INSERT INTO check_history (node_id, url, url_hash, status, http_code, message, checked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                r.node_id,
                r.url,
                hash,
                r.status.as_str(),
                r.http_code,
                r.message,
                r.checked_at
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 存活节点的最新检测结果。
    pub fn check_results_map(&self) -> CoreResult<HashMap<NodeId, CheckResult>> {
        let mut stmt = self.conn.prepare(
            "SELECT cr.node_id, cr.url, cr.status, cr.http_code, cr.final_url, cr.message, cr.elapsed_ms, cr.checked_at
             FROM check_results cr JOIN nodes n ON n.id = cr.node_id
             WHERE n.deleted = 0",
        )?;
        let rows = stmt.query_map([], |r| {
            let node_id: NodeId = r.get(0)?;
            let url: String = r.get(1)?;
            let status: String = r.get(2)?;
            let http_code: Option<i64> = r.get(3)?;
            let final_url: Option<String> = r.get(4)?;
            let message: Option<String> = r.get(5)?;
            let elapsed: i64 = r.get(6)?;
            let checked_at: i64 = r.get(7)?;
            Ok((
                node_id,
                CheckResult {
                    node_id,
                    url,
                    status: match status.as_str() {
                        "ok" => CheckStatus::Ok,
                        "suspicious" => CheckStatus::Suspicious,
                        _ => CheckStatus::Dead,
                    },
                    http_code: http_code.map(|v| v as u16),
                    final_url,
                    message,
                    elapsed_ms: elapsed.max(0) as u64,
                    checked_at,
                },
            ))
        })?;
        let mut map = HashMap::new();
        for row in rows {
            let (k, v) = row?;
            map.insert(k, v);
        }
        Ok(map)
    }

    /// 某节点的历史检测记录（新→旧）。
    pub fn check_history_for(&self, node_id: NodeId) -> CoreResult<Vec<(String, Option<i64>, String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT status, http_code, COALESCE(message,''), checked_at FROM check_history
             WHERE node_id = ?1 ORDER BY checked_at DESC, id DESC LIMIT 50",
        )?;
        let rows = stmt.query_map(params![node_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// 清空全部检测结果（重新检测用）。
    pub fn clear_check_results(&mut self) -> CoreResult<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM check_results", [])?;
        tx.commit()?;
        Ok(())
    }

    /// 检测历史总条数。
    pub fn check_history_count(&self) -> CoreResult<i64> {
        let c: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM check_history", [], |r| r.get(0))?;
        Ok(c)
    }

    // ------------------------------------------------------------------
    // 第 2 期：增量检测 / 重定向
    // ------------------------------------------------------------------

    /// #11 计算增量检测目标（只读，不改动任何检测结果）。
    ///
    /// 判定规则（三者取并集，按「新增 > URL 变更 > 超期」优先级归入唯一计数桶）：
    /// 1. `check_results` 中无该 node_id 行 → 新增；
    /// 2. 当前 `nodes.url` 的**严格** `urlnorm::url_hash_of` 与 `check_results.url_hash` 不等 → URL 变更。
    ///    刻意不用模糊 hash（FuzzyOptions::default / normalize_with），否则 `?utm_source=` 变化
    ///    会被误判为「未变更」，长期复用过期结果；
    /// 3. `now - checked_at > days * 86400` → 超期；
    /// 4. 其余跳过。非 HTTP 协议按第 1 期 checker 入队规则一律跳过，
    ///    不进入 targets、不写 check_results、保留原 status/checked_at/final_url。
    pub fn incremental_targets(&self, days: i64) -> CoreResult<IncrementalPlan> {
        let now = crate::now_ts();
        let threshold = days.max(0).saturating_mul(86_400);
        let cutoff = now.saturating_sub(threshold);

        let bookmarks = self.dfs_bookmarks()?;
        let mut records: HashMap<NodeId, (String, i64)> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT cr.node_id, cr.url_hash, cr.checked_at FROM check_results cr
                 JOIN nodes n ON n.id = cr.node_id WHERE n.deleted = 0",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, NodeId>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                let (id, hash, checked_at) = row?;
                records.insert(id, (hash, checked_at));
            }
        }

        let mut plan = IncrementalPlan {
            targets: Vec::with_capacity(bookmarks.len()),
            new_count: 0,
            url_changed_count: 0,
            stale_count: 0,
            skipped_count: 0,
        };
        for (node_id, url) in bookmarks {
            if !is_http_url(&url) {
                plan.skipped_count += 1;
                continue;
            }
            match records.get(&node_id) {
                None => {
                    plan.new_count += 1;
                    plan.targets.push(node_id);
                }
                Some((stored_hash, checked_at)) => {
                    // 严格 hash，与 upsert_check_result 的写入口径一致
                    let strict_hash = crate::urlnorm::url_hash_of(&url)
                        .map(|(_, hash)| hash)
                        .unwrap_or_default();
                    if strict_hash != *stored_hash {
                        plan.url_changed_count += 1;
                        plan.targets.push(node_id);
                    } else if *checked_at < cutoff {
                        plan.stale_count += 1;
                        plan.targets.push(node_id);
                    } else {
                        plan.skipped_count += 1;
                    }
                }
            }
        }
        Ok(plan)
    }

    /// #12 把检测得到的最终地址写回书签，并同步刷新检测结果缓存。
    ///
    /// `nodes.url` 与 `check_results.url/url_hash` 在同一事务内更新，避免中途失败导致两表不一致；
    /// `status` / `checked_at` / `final_url` 保持不变。同步严格 hash 后，下一次 #11 不会因
    /// hash 不一致把同一链接再检一遍。
    pub fn apply_redirects(&mut self, ids: &[NodeId]) -> CoreResult<usize> {
        let tx = self.conn.transaction()?;
        let mut updated = 0usize;
        for &id in ids {
            let final_url: Option<String> = tx
                .query_row(
                    "SELECT cr.final_url FROM check_results cr
                     JOIN nodes n ON n.id = cr.node_id
                     WHERE cr.node_id = ?1 AND n.deleted = 0",
                    params![id],
                    |r| r.get(0),
                )
                .optional()?;
            let new_url = match final_url {
                Some(url) if !url.trim().is_empty() => url,
                _ => continue,
            };
            let strict_hash = match crate::urlnorm::url_hash_of(&new_url) {
                Ok((_, hash)) => hash,
                Err(_) => continue,
            };
            tx.execute(
                "UPDATE nodes SET url = ?1 WHERE id = ?2 AND deleted = 0",
                params![new_url, id],
            )?;
            tx.execute(
                "UPDATE check_results SET url = ?1, url_hash = ?2 WHERE node_id = ?3",
                params![new_url, strict_hash, id],
            )?;
            updated += 1;
        }
        tx.commit()?;
        Ok(updated)
    }

    // ------------------------------------------------------------------
    // #10 合并文件夹
    // ------------------------------------------------------------------

    /// 预览同名文件夹组：按标题 trim + 大小写不敏感分组，仅返回至少 2 个成员的组。
    ///
    /// 只遍历未删除节点；根节点不参与分组；`suggested_keep_id` 取 DFS 顺序第一个。
    pub fn merge_folder_groups(&self) -> CoreResult<Vec<MergeFolderGroup>> {
        let nodes = self.load_nodes(false)?;
        let mut groups: Vec<MergeFolderGroup> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for folder in dfs_folder_order(&nodes) {
            let key = folder.title.trim().to_lowercase();
            if key.is_empty() {
                continue;
            }
            let bookmarks = subtree_bookmark_count(&self.conn, folder.id)?;
            match index.get(&key).copied() {
                Some(i) => {
                    groups[i].folder_ids.push(folder.id);
                    groups[i].total_bookmarks += bookmarks;
                }
                None => {
                    index.insert(key.clone(), groups.len());
                    groups.push(MergeFolderGroup {
                        key,
                        folder_ids: vec![folder.id],
                        total_bookmarks: bookmarks,
                        suggested_keep_id: folder.id,
                    });
                }
            }
        }
        Ok(groups.into_iter().filter(|g| g.folder_ids.len() >= 2).collect())
    }

    /// 应用合并：`groups` 中每项为 `(保留文件夹, 被合并文件夹列表)`。
    ///
    /// 每个被合并文件夹的直接子节点按原相对顺序追加到保留项末尾；同名子文件夹
    /// 递归合并（不再产生重复层级）；迁空的壳文件夹软删除进回收站。
    /// 全程在同一事务内，任一失败整体回滚。
    pub fn merge_folders(&mut self, groups: &[(NodeId, Vec<NodeId>)]) -> CoreResult<MergeOutcome> {
        let tx = self.conn.transaction()?;
        let mut outcome = MergeOutcome {
            merged_groups: 0,
            moved_nodes: 0,
        };
        for (keep_id, merge_ids) in groups {
            if *keep_id == ROOT_ID || merge_ids.is_empty() {
                continue;
            }
            let keep_kind: Option<String> = tx
                .query_row(
                    "SELECT kind FROM nodes WHERE id = ?1 AND deleted = 0",
                    params![*keep_id],
                    |r| r.get(0),
                )
                .optional()?;
            if keep_kind.as_deref() != Some("folder") {
                return Err(crate::CoreError::Other(format!("保留项 {keep_id} 不是存活文件夹")));
            }
            for &merge_id in merge_ids {
                if merge_id == ROOT_ID || merge_id == *keep_id {
                    continue;
                }
                let merge_kind: Option<String> = tx
                    .query_row(
                        "SELECT kind FROM nodes WHERE id = ?1 AND deleted = 0",
                        params![merge_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if merge_kind.as_deref() != Some("folder") {
                    continue;
                }
                // 若保留项位于被合并项的子树内，迁空后软删除会连带删掉保留项，必须拒绝。
                if is_ancestor_in_tx(&tx, merge_id, *keep_id)? {
                    return Err(crate::CoreError::Other("保留项不能位于被合并文件夹的子树内".into()));
                }
                merge_one_folder_in_tx(&tx, *keep_id, merge_id, &mut outcome)?;
            }
            outcome.merged_groups += 1;
        }
        tx.commit()?;
        Ok(outcome)
    }

    // ------------------------------------------------------------------
    // 设置
    // ------------------------------------------------------------------

    pub fn set_setting(&mut self, key: &str, value: &str) -> CoreResult<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> CoreResult<Option<String>> {
        let v = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
            .optional()?;
        Ok(v)
    }

    // ------------------------------------------------------------------
    // 统计
    // ------------------------------------------------------------------

    pub fn stats(&self) -> CoreResult<Stats> {
        let folders: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE deleted = 0 AND kind = 'folder' AND id != ?1",
            params![ROOT_ID],
            |r| r.get(0),
        )?;
        let bookmarks: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM nodes WHERE deleted = 0 AND kind = 'bookmark'", [], |r| r.get(0))?;
        let mut ok = 0i64;
        let mut suspicious = 0i64;
        let mut dead = 0i64;
        let mut stmt = self.conn.prepare(
            "SELECT cr.status, COUNT(*) FROM check_results cr JOIN nodes n ON n.id = cr.node_id
             WHERE n.deleted = 0 GROUP BY cr.status",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let status: String = row.get(0)?;
            let cnt: i64 = row.get(1)?;
            match status.as_str() {
                "ok" => ok = cnt,
                "suspicious" => suspicious = cnt,
                "dead" => dead = cnt,
                _ => {}
            }
        }
        let trash_count: i64 =
            self.conn.query_row("SELECT COUNT(*) FROM trash WHERE restored = 0", [], |r| r.get(0))?;
        Ok(Stats {
            folders,
            bookmarks,
            ok,
            suspicious,
            dead,
            unchecked: (bookmarks - ok - suspicious - dead).max(0),
            trash_count,
        })
    }
}

// ----------------------------------------------------------------------
// 辅助函数
// ----------------------------------------------------------------------

/// `move_nodes` 的事务内实现，供 #10 合并文件夹复用（不另开事务）。
fn move_nodes_in_tx(
    tx: &rusqlite::Transaction<'_>,
    ids: &[NodeId],
    new_parent: NodeId,
    index: i64,
) -> CoreResult<()> {
    let mut ids: Vec<NodeId> = ids.to_vec();
    ids.retain(|&id| id != ROOT_ID);
    ids.dedup();
    if ids.is_empty() {
        return Ok(());
    }

    // 目标必须是未删除文件夹
    let target: Option<Node> = tx
        .query_row("SELECT * FROM nodes WHERE id = ?1 AND deleted = 0", params![new_parent], row_node_by_full_row)
        .optional()?;
    let target = target.ok_or_else(|| crate::CoreError::Other("目标文件夹不存在".into()))?;
    if target.kind != NodeKind::Folder {
        return Err(crate::CoreError::Other("目标不是文件夹".into()));
    }

    // 校验被移动节点存在、存活、且目标不在其子树内
    let id_set: std::collections::HashSet<NodeId> = ids.iter().copied().collect();
    for &id in &ids {
        let node: Option<(NodeId, NodeId, String, i64)> = tx
            .query_row(
                "SELECT id, parent_id, kind, position FROM nodes WHERE id = ?1 AND deleted = 0",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let (_, parent, kind, _) =
            node.ok_or_else(|| crate::CoreError::Other(format!("节点 {id} 不存在或已删除")))?;
        if kind != "bookmark" && kind != "folder" {
            return Err(crate::CoreError::Other("节点类型错误".into()));
        }
        // 祖先链包含移动集内节点 → 不能移入自身子树
        let mut cur = new_parent;
        let mut hops = 0;
        while cur != ROOT_ID && hops < 1000 {
            if id_set.contains(&cur) {
                return Err(crate::CoreError::Other("不能把节点移动到其自身子树内".into()));
            }
            cur = tx.query_row("SELECT parent_id FROM nodes WHERE id = ?1", params![cur], |r| r.get(0))?;
            hops += 1;
        }
        let _ = parent;
    }

    // 目标文件夹当前的存活子节点（排除被移动者）
    let mut stmt = tx.prepare(
        "SELECT id FROM nodes WHERE parent_id = ?1 AND deleted = 0 ORDER BY position, id",
    )?;
    let existing: Vec<NodeId> = stmt
        .query_map(params![new_parent], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .filter(|id| !id_set.contains(id))
        .collect();
    drop(stmt);

    let index = index.clamp(0, existing.len() as i64) as usize;
    let mut base_positions: Vec<NodeId> = existing[..index].to_vec();
    base_positions.extend(ids.iter().copied());
    base_positions.extend(existing[index..].to_vec());

    // 记录受影响的老父节点（重排序用）
    let mut affected_parents: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
    for &id in &ids {
        let parent: NodeId =
            tx.query_row("SELECT parent_id FROM nodes WHERE id = ?1", params![id], |r| r.get(0))?;
        affected_parents.insert(parent);
    }

    // 执行移动：先改父节点
    for &nid in &ids {
        tx.execute(
            "UPDATE nodes SET parent_id = ?1 WHERE id = ?2",
            params![new_parent, nid],
        )?;
    }
    affected_parents.insert(new_parent);

    // 按目标顺序 base_positions 显式编号，使插入位置真正生效且 position 连续
    for (seq, &nid) in base_positions.iter().enumerate() {
        tx.execute(
            "UPDATE nodes SET position = ?1 WHERE id = ?2 AND parent_id = ?3",
            params![seq as i64, nid, new_parent],
        )?;
    }

    // 重新编号受影响的老父节点（new_parent 已按 base_positions 编好，跳过）
    for parent in affected_parents {
        if parent != new_parent {
            renumber_children(tx, parent)?;
        }
    }
    Ok(())
}

/// `soft_delete` 的事务内实现，供 #10 合并文件夹复用。
fn soft_delete_in_tx(tx: &rusqlite::Transaction<'_>, ids: &[NodeId]) -> CoreResult<usize> {
    let now = crate::now_ts();
    let mut count = 0;
    for &id in ids {
        if id == ROOT_ID {
            continue;
        }
        let info: Option<(String, String)> = tx
            .query_row("SELECT title, kind FROM nodes WHERE id = ?1 AND deleted = 0", params![id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        let (title, kind) = match info {
            Some(v) => v,
            None => continue,
        };
        let sub = subtree_ids(tx, id)?;
        let item_count: i64 = subtree_bookmark_count(tx, id)?;
        for sid in &sub {
            tx.execute("UPDATE nodes SET deleted = 1 WHERE id = ?1", params![sid])?;
        }
        tx.execute(
            "INSERT INTO trash (node_id, title, kind, item_count, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, title, kind, item_count, now],
        )?;
        count += 1;
    }
    Ok(count)
}

/// 未删除文件夹的 DFS 顺序（前序，兄弟按 position + id 排序），不含根节点。
fn dfs_folder_order(nodes: &[Node]) -> Vec<Node> {
    let map: HashMap<NodeId, &Node> = nodes.iter().map(|n| (n.id, n)).collect();
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for n in nodes {
        if n.parent_id != n.id {
            children.entry(n.parent_id).or_default().push(n.id);
        }
    }
    for v in children.values_mut() {
        v.sort_by_key(|id| {
            map.get(id)
                .map(|n| (n.position, n.id))
                .unwrap_or((0, 0))
        });
    }
    let mut out = Vec::new();
    fn walk(
        id: NodeId,
        map: &HashMap<NodeId, &Node>,
        children: &HashMap<NodeId, Vec<NodeId>>,
        out: &mut Vec<Node>,
    ) {
        if let Some(n) = map.get(&id) {
            if id != ROOT_ID && n.kind == NodeKind::Folder {
                out.push((**n).clone());
            }
        }
        if let Some(kids) = children.get(&id) {
            for &k in kids {
                walk(k, map, children, out);
            }
        }
    }
    walk(ROOT_ID, &map, &children, &mut out);
    out
}

/// 某父节点当前的存活直接子节点 id，按 position + id 排序。
fn direct_child_ids(tx: &rusqlite::Transaction<'_>, parent: NodeId) -> CoreResult<Vec<NodeId>> {
    let mut stmt = tx.prepare(
        "SELECT id FROM nodes WHERE parent_id = ?1 AND deleted = 0 ORDER BY position, id",
    )?;
    let ids: Vec<NodeId> = stmt
        .query_map(params![parent], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(ids)
}

/// `ancestor` 是否为 `node` 本身或其祖先（含自引用根节点的兜底）。
fn is_ancestor_in_tx(
    tx: &rusqlite::Transaction<'_>,
    ancestor: NodeId,
    node: NodeId,
) -> CoreResult<bool> {
    let mut cur = node;
    for _ in 0..1000 {
        if cur == ancestor {
            return Ok(true);
        }
        let parent: Option<NodeId> = tx
            .query_row("SELECT parent_id FROM nodes WHERE id = ?1", params![cur], |r| r.get(0))
            .optional()?;
        match parent {
            Some(p) if p != cur => cur = p,
            _ => return Ok(false),
        }
    }
    Ok(false)
}

/// 递归合并一个被合并文件夹到保留文件夹：
/// 普通子节点按原顺序追加；同名子文件夹继续递归；最后软删除空的壳文件夹。
fn merge_one_folder_in_tx(
    tx: &rusqlite::Transaction<'_>,
    keep_id: NodeId,
    merge_id: NodeId,
    outcome: &mut MergeOutcome,
) -> CoreResult<()> {
    let mut keep_folders: HashMap<String, NodeId> = HashMap::new();
    for id in direct_child_ids(tx, keep_id)? {
        let info: Option<(String, String)> = tx
            .query_row("SELECT kind, title FROM nodes WHERE id = ?1", params![id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        if let Some((kind, title)) = info {
            if kind == "folder" {
                keep_folders.insert(title.trim().to_lowercase(), id);
            }
        }
    }

    let mut plain_ids: Vec<NodeId> = Vec::new();
    let mut same_name_pairs: Vec<(NodeId, NodeId)> = Vec::new();
    // 同名子文件夹若因祖先关系无法递归合并，则保留壳文件夹，绝不软删除丢数据。
    let mut skipped_pairs = 0usize;
    for id in direct_child_ids(tx, merge_id)? {
        let info: Option<(String, String)> = tx
            .query_row("SELECT kind, title FROM nodes WHERE id = ?1", params![id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        match info {
            Some((kind, title)) if kind == "folder" => {
                let key = title.trim().to_lowercase();
                if let Some(&target) = keep_folders.get(&key) {
                    // 同名子文件夹：不搬整个文件夹，只递归搬其子节点，避免重复层级
                    same_name_pairs.push((target, id));
                } else {
                    plain_ids.push(id);
                    // 同一被合并文件夹内出现多个同名子文件夹时，后续者合并进前者
                    keep_folders.insert(key, id);
                }
            }
            Some(_) => plain_ids.push(id),
            None => {}
        }
    }

    if !plain_ids.is_empty() {
        let at = direct_child_ids(tx, keep_id)?.len() as i64;
        move_nodes_in_tx(tx, &plain_ids, keep_id, at)?;
        outcome.moved_nodes += plain_ids.len();
    }
    for (target, source) in same_name_pairs {
        if target != source && !is_ancestor_in_tx(tx, source, target)? {
            merge_one_folder_in_tx(tx, target, source, outcome)?;
        } else {
            skipped_pairs += 1;
        }
    }
    // 子节点已全部迁出，软删除空壳文件夹（进回收站，不裸删节点）
    if skipped_pairs == 0 {
        let _ = soft_delete_in_tx(tx, &[merge_id])?;
    }
    Ok(())
}

fn row_to_node(r: &rusqlite::Row<'_>) -> rusqlite::Result<Node> {
    let kind: String = r.get("kind")?;
    Ok(Node {
        id: r.get("id")?,
        parent_id: r.get("parent_id")?,
        kind: NodeKind::from_str(&kind),
        title: r.get("title")?,
        url: r.get("url")?,
        add_date: r.get("add_date")?,
        icon: r.get("icon")?,
        position: r.get("position")?,
        deleted: r.get::<_, i64>("deleted")? != 0,
    })
}

/// `SELECT *` 场景下的行转换（列顺序见建表语句）。
#[allow(dead_code)]
fn row_node_by_full_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Node> {
    row_to_node(r)
}

fn insert_nodes(
    tx: &rusqlite::Transaction<'_>,
    children: &[NewNode],
    parent_id: NodeId,
    position_base: i64,
    stats: &mut ImportStats,
) -> CoreResult<()> {
    for (pos, item) in children.iter().enumerate() {
        let kind_str = item.kind.as_str();
        tx.execute(
            "INSERT INTO nodes (parent_id, kind, title, url, add_date, icon, position) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                parent_id,
                kind_str,
                item.title,
                item.url,
                item.add_date,
                item.icon,
                position_base + pos as i64
            ],
        )?;
        let id = tx.last_insert_rowid();
        match item.kind {
            NodeKind::Folder => stats.folders += 1,
            NodeKind::Bookmark => stats.bookmarks += 1,
        }
        if !item.children.is_empty() {
            insert_nodes(tx, &item.children, id, 0, stats)?;
        }
    }
    Ok(())
}

fn build_new_tree(map: &HashMap<NodeId, Node>, root: NodeId) -> NewNode {
    let node = map.get(&root);
    let (kind, title, url, add_date, icon) = match node {
        Some(n) => (n.kind, n.title.clone(), n.url.clone(), n.add_date, n.icon.clone()),
        None => (NodeKind::Folder, "书签栏".to_string(), None, None, None),
    };
    let mut children_map: Vec<&Node> = map
        .values()
        .filter(|n| n.parent_id == root && n.id != root)
        .collect();
    children_map.sort_by_key(|n| (n.position, n.id));
    let children = children_map
        .into_iter()
        .map(|n| build_new_tree(map, n.id))
        .collect();
    NewNode {
        kind,
        title,
        url,
        add_date,
        icon,
        children,
    }
}

fn renumber_children(tx: &rusqlite::Transaction<'_>, parent: NodeId) -> CoreResult<()> {
    let mut stmt = tx.prepare(
        "SELECT id FROM nodes WHERE parent_id = ?1 AND deleted = 0 ORDER BY position, id",
    )?;
    let ids: Vec<NodeId> = stmt
        .query_map(params![parent], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    for (seq, id) in ids.iter().enumerate() {
        tx.execute("UPDATE nodes SET position = ?1 WHERE id = ?2", params![seq as i64, id])?;
    }
    Ok(())
}

fn subtree_ids(tx: &rusqlite::Transaction<'_>, root: NodeId) -> CoreResult<Vec<NodeId>> {
    subtree_impl(tx, root, false)
}

fn subtree_ids_including_deleted(tx: &rusqlite::Transaction<'_>, root: NodeId) -> CoreResult<Vec<NodeId>> {
    subtree_impl(tx, root, true)
}

fn subtree_impl(
    tx: &rusqlite::Transaction<'_>,
    root: NodeId,
    include_deleted: bool,
) -> CoreResult<Vec<NodeId>> {
    let filter = if include_deleted { "" } else { "WHERE n.deleted = 0" };
    let sql = format!(
        "WITH RECURSIVE sub(id) AS (
            SELECT ?1
            UNION
            SELECT n.id FROM nodes n JOIN sub s ON n.parent_id = s.id {filter}
        ) SELECT id FROM sub"
    );
    let mut stmt = tx.prepare(&sql)?;
    let ids: Vec<NodeId> = stmt
        .query_map(params![root], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(ids)
}

fn subtree_bookmark_count(conn: &rusqlite::Connection, root: NodeId) -> CoreResult<i64> {
    let sql = "WITH RECURSIVE sub(id) AS (
            SELECT ?1
            UNION
            SELECT n.id FROM nodes n JOIN sub s ON n.parent_id = s.id
        ) SELECT COUNT(*) FROM nodes WHERE id IN (SELECT id FROM sub) AND kind = 'bookmark'";
    let c: i64 = conn.query_row(sql, params![root], |r| r.get(0))?;
    Ok(c)
}

/// 与第 1 期 `checker::check_one` 一致的 HTTP(S) 入队判据。
/// javascript:/mailto:/chrome:/place: 等非 HTTP 协议不入队、不写检测结果。
pub fn is_http_url(raw: &str) -> bool {
    let lower = raw.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::{CheckResult, CheckStatus};
    use crate::model::NewNode;

    fn seed(urls: &[&str]) -> (Store, Vec<NodeId>) {
        let mut store = Store::open_in_memory().unwrap();
        let items: Vec<NewNode> = urls
            .iter()
            .map(|u| NewNode::bookmark(*u, *u, None, None))
            .collect();
        store.import_children(&items, ImportMode::Replace).unwrap();
        let ids: Vec<NodeId> = store
            .dfs_bookmarks()
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        (store, ids)
    }

    fn cached(node_id: NodeId, url: &str, checked_at: i64) -> CheckResult {
        CheckResult {
            node_id,
            url: url.to_string(),
            status: CheckStatus::Ok,
            http_code: Some(200),
            final_url: None,
            message: None,
            elapsed_ms: 1,
            checked_at,
        }
    }

    #[test]
    fn test_incremental_targets_three_reasons_and_union() {
        let (mut store, ids) = seed(&[
            "https://a.com/new",
            "https://a.com/p?x=2",
            "https://a.com/stale",
            "https://a.com/fresh",
            "javascript:alert(1)",
        ]);
        let now = crate::now_ts();
        // URL 变更：缓存里是 x=1，当前节点是 x=2
        store
            .upsert_check_result(&cached(ids[1], "https://a.com/p?x=1", now))
            .unwrap();
        // 超期：8 天前，超过 7 天阈值
        store
            .upsert_check_result(&cached(ids[2], "https://a.com/stale", now - 8 * 86_400))
            .unwrap();
        // 有效缓存
        store
            .upsert_check_result(&cached(ids[3], "https://a.com/fresh", now))
            .unwrap();

        let plan = store.incremental_targets(7).unwrap();
        assert_eq!(plan.new_count, 1);
        assert_eq!(plan.url_changed_count, 1);
        assert_eq!(plan.stale_count, 1);
        // fresh（缓存有效）+ javascript（非 HTTP）均跳过
        assert_eq!(plan.skipped_count, 2);
        assert_eq!(plan.targets.len(), 3);
        assert_eq!(
            plan.new_count + plan.url_changed_count + plan.stale_count + plan.skipped_count,
            5
        );
        assert!(plan.targets.contains(&ids[0]));
        assert!(plan.targets.contains(&ids[1]));
        assert!(plan.targets.contains(&ids[2]));
        assert!(!plan.targets.contains(&ids[3]));
        assert!(!plan.targets.contains(&ids[4]));

        // 跳过项三字段逐字节不变
        let checks = store.check_results_map().unwrap();
        let skipped = checks.get(&ids[3]).unwrap();
        assert_eq!(skipped.url, "https://a.com/fresh");
        assert_eq!(skipped.status, CheckStatus::Ok);
        assert_eq!(skipped.checked_at, now);
        assert_eq!(skipped.final_url, None);
    }

    #[test]
    fn test_strict_hash_detects_query_change() {
        // 严格 hash 必须识别 ?x=1 → ?x=2；模糊 hash 会把它们归为同一类，此处回归保护
        let (mut store, ids) = seed(&["https://a.com/p?x=2"]);
        store
            .upsert_check_result(&cached(ids[0], "https://a.com/p?x=1", crate::now_ts()))
            .unwrap();
        let plan = store.incremental_targets(365).unwrap();
        assert_eq!(plan.url_changed_count, 1);
        assert_eq!(plan.skipped_count, 0);
        assert_eq!(plan.targets, vec![ids[0]]);
    }

    #[test]
    fn test_apply_redirects_syncs_check_result() {
        let (mut store, ids) = seed(&["https://a.com/old"]);
        let checked_at = crate::now_ts() - 100;
        store
            .upsert_check_result(&CheckResult {
                node_id: ids[0],
                url: "https://a.com/old".to_string(),
                status: CheckStatus::Suspicious,
                http_code: Some(403),
                final_url: Some("https://a.com/new".to_string()),
                message: None,
                elapsed_ms: 5,
                checked_at,
            })
            .unwrap();

        let updated = store.apply_redirects(&[ids[0]]).unwrap();
        assert_eq!(updated, 1);

        let node = store
            .all_nodes()
            .unwrap()
            .into_iter()
            .find(|n| n.id == ids[0])
            .unwrap();
        assert_eq!(node.url.as_deref(), Some("https://a.com/new"));

        let (url, hash, status, at) = store
            .conn
            .query_row(
                "SELECT url, url_hash, status, checked_at FROM check_results WHERE node_id = ?1",
                params![ids[0]],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                },
            )
            .unwrap();
        let expected_hash = crate::urlnorm::url_hash_of("https://a.com/new").unwrap().1;
        assert_eq!(url, "https://a.com/new");
        assert_eq!(hash, expected_hash);
        // 保留原状态与时间
        assert_eq!(status, "suspicious");
        assert_eq!(at, checked_at);

        // 同步后不会被下一次增量检测判为 URL 变更
        let plan = store.incremental_targets(7).unwrap();
        assert!(plan.targets.is_empty());
        assert_eq!(plan.skipped_count, 1);
    }

    #[test]
    fn test_merge_folders_preview_and_apply_keeps_order() {
        let mut store = Store::open_in_memory().unwrap();
        let tree = vec![
            NewNode::folder("开发", vec![NewNode::bookmark("a", "https://a.com", None, None)]),
            NewNode::folder(
                " 开发 ",
                vec![
                    NewNode::bookmark("b", "https://b.com", None, None),
                    NewNode::folder("UI", vec![NewNode::bookmark("c", "https://c.com", None, None)]),
                ],
            ),
            NewNode::folder("开发", vec![NewNode::bookmark("d", "https://d.com", None, None)]),
        ];
        store.import_children(&tree, ImportMode::Replace).unwrap();

        let groups = store.merge_folder_groups().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].key, "开发");
        assert_eq!(groups[0].folder_ids.len(), 3);
        assert_eq!(groups[0].total_bookmarks, 4);
        let keep = groups[0].suggested_keep_id;
        let merges: Vec<NodeId> = groups[0]
            .folder_ids
            .iter()
            .copied()
            .filter(|id| *id != keep)
            .collect();
        let outcome = store.merge_folders(&[(keep, merges)]).unwrap();
        assert_eq!(outcome.merged_groups, 1);
        assert_eq!(outcome.moved_nodes, 3);

        let nodes = store.all_nodes().unwrap();
        let mut children: Vec<&Node> = nodes
            .iter()
            .filter(|n| n.parent_id == keep && !n.deleted)
            .collect();
        children.sort_by_key(|n| n.position);
        let titles: Vec<&str> = children.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["a", "b", "UI", "d"]);
        let positions: Vec<i64> = children.iter().map(|n| n.position).collect();
        assert_eq!(positions, vec![0, 1, 2, 3]);
        let alive: Vec<&Node> = nodes
            .iter()
            .filter(|n| {
                n.parent_id == ROOT_ID && n.id != ROOT_ID && n.kind == NodeKind::Folder && !n.deleted
            })
            .collect();
        assert_eq!(alive.len(), 1);
        assert_eq!(alive[0].id, keep);
        assert_eq!(store.list_trash().unwrap().len(), 2);
    }

    #[test]
    fn test_merge_folders_recursive_same_name_subfolder() {
        let mut store = Store::open_in_memory().unwrap();
        let tree = vec![
            NewNode::folder(
                "Dev",
                vec![NewNode::folder(
                    "UI",
                    vec![NewNode::bookmark("one", "https://one.com", None, None)],
                )],
            ),
            NewNode::folder(
                "dev",
                vec![NewNode::folder(
                    "ui",
                    vec![NewNode::bookmark("two", "https://two.com", None, None)],
                )],
            ),
        ];
        store.import_children(&tree, ImportMode::Replace).unwrap();

        let groups = store.merge_folder_groups().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].folder_ids.len(), 2);
        let keep = groups[0].suggested_keep_id;
        let merge = groups[0].folder_ids[1];
        let outcome = store.merge_folders(&[(keep, vec![merge])]).unwrap();
        assert_eq!(outcome.moved_nodes, 1);

        let nodes = store.all_nodes().unwrap();
        let ui_folders: Vec<&Node> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Folder && n.title.eq_ignore_ascii_case("ui") && !n.deleted)
            .collect();
        assert_eq!(ui_folders.len(), 1);
        let ui_children: Vec<&Node> = nodes
            .iter()
            .filter(|n| n.parent_id == ui_folders[0].id && !n.deleted)
            .collect();
        assert_eq!(ui_children.len(), 2);
    }
}
