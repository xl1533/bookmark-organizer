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
    pub fn move_nodes(&mut self, ids: &[NodeId], new_parent: NodeId, index: i64) -> CoreResult<()> {
        let mut ids: Vec<NodeId> = ids.to_vec();
        ids.retain(|&id| id != ROOT_ID);
        ids.dedup();
        if ids.is_empty() {
            return Ok(());
        }

        // 目标必须是未删除文件夹
        let target: Option<Node> = self
            .conn
            .query_row("SELECT * FROM nodes WHERE id = ?1 AND deleted = 0", params![new_parent], row_node_by_full_row)
            .optional()?;
        let target = target.ok_or_else(|| crate::CoreError::Other("目标文件夹不存在".into()))?;
        if target.kind != NodeKind::Folder {
            return Err(crate::CoreError::Other("目标不是文件夹".into()));
        }

        let tx = self.conn.transaction()?;

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

        // 按目标顺序 base_positions 显式编号，使拖拽落点（index）真正生效
        for (seq, &nid) in base_positions.iter().enumerate() {
            tx.execute(
                "UPDATE nodes SET position = ?1 WHERE id = ?2 AND parent_id = ?3",
                params![seq as i64, nid, new_parent],
            )?;
        }

        // 重新编号受影响的老父节点（new_parent 已按 base_positions 编好，跳过）
        for parent in affected_parents {
            if parent != new_parent {
                renumber_children(&tx, parent)?;
            }
        }
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
        let now = crate::now_ts();
        let tx = self.conn.transaction()?;
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
            let sub = subtree_ids(&tx, id)?;
            let item_count: i64 = subtree_bookmark_count(&tx, id)?;
            for sid in &sub {
                tx.execute("UPDATE nodes SET deleted = 1 WHERE id = ?1", params![sid])?;
            }
            tx.execute(
                "INSERT INTO trash (node_id, title, kind, item_count, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, title, kind, item_count, now],
            )?;
            count += 1;
        }
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

fn subtree_bookmark_count(tx: &rusqlite::Transaction<'_>, root: NodeId) -> CoreResult<i64> {
    let sql = "WITH RECURSIVE sub(id) AS (
            SELECT ?1
            UNION
            SELECT n.id FROM nodes n JOIN sub s ON n.parent_id = s.id
        ) SELECT COUNT(*) FROM nodes WHERE id IN (SELECT id FROM sub) AND kind = 'bookmark'";
    let c: i64 = tx.query_row(sql, params![root], |r| r.get(0))?;
    Ok(c)
}
