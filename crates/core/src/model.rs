//! 书签树数据模型。
//!
//! `Node` 是数据库中一行节点（文件夹或书签）的完整表示；
//! `NewNode` 是导入/导出使用的树形结构（无 id，含子节点）。

use serde::{Deserialize, Serialize};

pub type NodeId = i64;

/// 根节点的固定 id。
pub const ROOT_ID: NodeId = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    #[default]
    Folder,
    Bookmark,
}

impl NodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::Folder => "folder",
            NodeKind::Bookmark => "bookmark",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "bookmark" => NodeKind::Bookmark,
            _ => NodeKind::Folder,
        }
    }
}

/// 数据库中的节点。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: NodeId,
    pub parent_id: NodeId,
    pub kind: NodeKind,
    pub title: String,
    pub url: Option<String>,
    pub add_date: Option<i64>,
    pub icon: Option<String>,
    pub position: i64,
    pub deleted: bool,
}

/// 树形结构节点，用于导入与导出。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NewNode {
    pub kind: NodeKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_date: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub children: Vec<NewNode>,
}

impl NewNode {
    pub fn folder(title: impl Into<String>, children: Vec<NewNode>) -> Self {
        NewNode {
            kind: NodeKind::Folder,
            title: title.into(),
            url: None,
            add_date: None,
            icon: None,
            children,
        }
    }

    pub fn bookmark(
        title: impl Into<String>,
        url: impl Into<String>,
        add_date: Option<i64>,
        icon: Option<String>,
    ) -> Self {
        NewNode {
            kind: NodeKind::Bookmark,
            title: title.into(),
            url: Some(url.into()),
            add_date,
            icon,
            children: Vec::new(),
        }
    }

    /// 统计树中文件夹与书签数量。
    pub fn counts(&self) -> (usize, usize) {
        let mut folders = usize::from(self.kind == NodeKind::Folder);
        let mut bookmarks = usize::from(self.kind == NodeKind::Bookmark);
        for c in &self.children {
            let (f, b) = c.counts();
            folders += f;
            bookmarks += b;
        }
        (folders, bookmarks)
    }
}
