//! 空文件夹检测与清理。
//!
//! “空文件夹”定义：子树内不包含任何书签节点的文件夹（即使嵌套了
//! 若干子文件夹，只要整棵子树没有任何书签即视为空）。
//! 清理时只返回“最顶层”的空文件夹（祖先为空即剪枝，不再下钻），
//! 避免父子重复删除。

use std::collections::HashMap;

use crate::model::{Node, NodeKind};
use crate::NodeId;

/// 返回最顶层空文件夹 id 列表（DFS 顺序）。
pub fn find_empty_folders(nodes: &HashMap<NodeId, Node>, root: NodeId) -> Vec<NodeId> {
    // 阶段一：后序统计每个节点子树内的书签数
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for n in nodes.values() {
        // 根节点自引用（parent == 自身）不进入 children 映射，避免死循环
        if !n.deleted && n.parent_id != n.id {
            children.entry(n.parent_id).or_default().push(n.id);
        }
    }
    for v in children.values_mut() {
        v.sort_by_key(|id| nodes.get(id).map(|n| n.position).unwrap_or(0));
    }

    let mut counts: HashMap<NodeId, usize> = HashMap::new();
    count_subtree(nodes, &children, root, &mut counts);

    // 阶段二：前序 DFS，遇到空文件夹即记录并剪枝（根目录不参与清理）
    let mut empty = Vec::new();
    collect(nodes, &children, &counts, root, root, &mut empty);
    empty
}

fn count_subtree(
    nodes: &HashMap<NodeId, Node>,
    children: &HashMap<NodeId, Vec<NodeId>>,
    id: NodeId,
    counts: &mut HashMap<NodeId, usize>,
) -> usize {
    let node = match nodes.get(&id) {
        Some(n) if !n.deleted => n,
        _ => {
            counts.insert(id, 0);
            return 0;
        }
    };
    let mut sum = 0;
    if let Some(kids) = children.get(&id) {
        for &kid in kids {
            sum += count_subtree(nodes, children, kid, counts);
        }
    }
    if node.kind == NodeKind::Bookmark {
        sum = 1;
    }
    counts.insert(id, sum);
    sum
}

fn collect(
    nodes: &HashMap<NodeId, Node>,
    children: &HashMap<NodeId, Vec<NodeId>>,
    counts: &HashMap<NodeId, usize>,
    id: NodeId,
    root_id: NodeId,
    empty: &mut Vec<NodeId>,
) {
    let node = match nodes.get(&id) {
        Some(n) if !n.deleted => n,
        _ => return,
    };
    if node.kind == NodeKind::Folder {
        if counts.get(&id).copied().unwrap_or(0) == 0 {
            if id != root_id {
                empty.push(id);
                return; // 剪枝：子文件夹不再重复列出
            }
            // 根目录为空：不列入结果，但继续下钻找子级空文件夹
        }
        if let Some(kids) = children.get(&id) {
            for &kid in kids {
                collect(nodes, children, counts, kid, root_id, empty);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: NodeId, parent: NodeId, kind: NodeKind) -> Node {
        Node {
            id,
            parent_id: parent,
            kind,
            title: format!("n{id}"),
            url: None,
            add_date: None,
            icon: None,
            position: id,
            deleted: false,
        }
    }

    #[test]
    fn test_nested_empty_folders() {
        // root(1) -> A(2) -> B(3) -> bookmark(4)
        //          -> E(5) -> F(6)   // 全空
        let mut nodes = HashMap::new();
        for &(id, parent, kind) in &[
            (1, 1, NodeKind::Folder),
            (2, 1, NodeKind::Folder),
            (3, 2, NodeKind::Folder),
            (4, 3, NodeKind::Bookmark),
            (5, 1, NodeKind::Folder),
            (6, 5, NodeKind::Folder),
        ] {
            nodes.insert(id, node(id, parent, kind));
        }
        let empty = find_empty_folders(&nodes, 1);
        assert_eq!(empty, vec![5]);
    }

    #[test]
    fn test_all_empty() {
        let mut nodes = HashMap::new();
        nodes.insert(1, node(1, 1, NodeKind::Folder));
        nodes.insert(2, node(2, 1, NodeKind::Folder));
        nodes.insert(3, node(3, 2, NodeKind::Folder));
        let empty = find_empty_folders(&nodes, 1);
        assert_eq!(empty, vec![2]);
    }

    #[test]
    fn test_deleted_nodes_ignored() {
        let mut nodes = HashMap::new();
        nodes.insert(1, node(1, 1, NodeKind::Folder));
        let mut b = node(2, 1, NodeKind::Bookmark);
        b.deleted = true;
        nodes.insert(2, b);
        let empty = find_empty_folders(&nodes, 1);
        // 唯一书签已软删除 → 子树为空；根目录不参与清理 → 结果为空
        assert_eq!(empty, Vec::<NodeId>::new());
    }
}
