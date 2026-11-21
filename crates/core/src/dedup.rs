//! URL 精确去重（第 1 期）。
//!
//! 按“标准化后的 URL 哈希”分组，组内按 DFS（树形展示）顺序保留第一个，
//! 其余视为重复项。第 2 期模糊去重将在 [`crate::urlnorm::FuzzyOptions`]
//! 基础上扩展，此处仅预留。

use std::collections::HashMap;

use serde::Serialize;

use crate::urlnorm::FuzzyOptions;
use crate::NodeId;

/// 一个重复分组。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupGroup {
    /// 标准化后的 URL
    pub normalized_url: String,
    pub hash: String,
    /// 组内按树形顺序保留的第一个书签
    pub keep_node_id: NodeId,
    /// 其余重复项（按树形顺序）
    pub duplicate_node_ids: Vec<NodeId>,
}

/// 在 DFS 顺序的书签列表中查找重复分组。
///
/// `bookmarks` 必须按树形展示顺序给出 `(节点id, 原始URL)`，
/// 保证“保留第一个”的语义与 UI 中的顺序一致。
pub fn find_duplicate_groups(bookmarks: &[(NodeId, String)]) -> Vec<DupGroup> {
    find_duplicate_groups_with(bookmarks, FuzzyOptions::default())
}

/// 带模糊选项的分组（第 2 期接口预留）。
pub fn find_duplicate_groups_with(
    bookmarks: &[(NodeId, String)],
    opts: FuzzyOptions,
) -> Vec<DupGroup> {
    // hash -> (首个 node_id, 其余)
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, (NodeId, Vec<NodeId>)> = HashMap::new();

    for (id, url) in bookmarks {
        let hash = match url_hash_with(url, opts) {
            Some(h) => h,
            None => continue, // 无法标准化的 URL 不参与去重
        };
        match map.get_mut(&hash) {
            Some((_, dups)) => dups.push(*id),
            None => {
                order.push(hash.clone());
                map.insert(hash, (*id, Vec::new()));
            }
        }
    }

    order
        .into_iter()
        .filter_map(|h| {
            map.remove(&h).and_then(|(keep, dups)| {
                if dups.is_empty() {
                    None
                } else {
                    // 反查标准化 URL 用于展示
                    let normalized = first_normalized(bookmarks, &h, opts).unwrap_or_default();
                    Some(DupGroup {
                        normalized_url: normalized,
                        hash: h,
                        keep_node_id: keep,
                        duplicate_node_ids: dups,
                    })
                }
            })
        })
        .collect()
}

fn url_hash_with(url: &str, opts: FuzzyOptions) -> Option<String> {
    let normalized = crate::urlnorm::normalize_with(url, opts).ok()?;
    Some(crate::urlnorm::url_hash(&normalized))
}

fn first_normalized(bookmarks: &[(NodeId, String)], hash: &str, opts: FuzzyOptions) -> Option<String> {
    for (_, url) in bookmarks {
        if let Ok(norm) = crate::urlnorm::normalize_with(url, opts) {
            if crate::urlnorm::url_hash(&norm) == hash {
                return Some(norm);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keep_first_in_dfs_order() {
        let bms = vec![
            (1, "https://example.com/a".to_string()),
            (2, "HTTPS://EXAMPLE.COM/a".to_string()), // 与 1 重复（scheme/host 大小写）
            (3, "https://example.com/b".to_string()),
            (4, "https://example.com/a?x=1".to_string()), // query 不同，不重复
            (5, "https://example.com:443/a".to_string()), // 与 1 重复（默认端口）
        ];
        let groups = find_duplicate_groups(&bms);
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.keep_node_id, 1);
        assert_eq!(g.duplicate_node_ids, vec![2, 5]);
        assert_eq!(g.normalized_url, "https://example.com/a");
    }

    #[test]
    fn test_no_dups() {
        let bms = vec![(1, "https://a.com".to_string()), (2, "https://b.com".to_string())];
        assert!(find_duplicate_groups(&bms).is_empty());
    }
}
