//! 时间戳 HTML 备份与工具函数。
//!
//! 在任何破坏性操作（一键删除失效项等）前调用 [`create_backup`]，
//! 自动生成形如 `bookmarks_backup_20261121_153000.html` 的 Netscape 格式备份，
//! 可直接重新导入本工具或浏览器。

use std::path::{Path, PathBuf};

use crate::netscape;
use crate::store::Store;
use crate::CoreResult;

/// 生成当前书签库（未删除部分）的 HTML 备份。
/// `dir` 不存在时会自动创建。返回备份文件绝对路径。
pub fn create_backup(store: &Store, dir: &Path) -> CoreResult<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let tree = store.export_tree()?;
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let file_name = format!("bookmarks_backup_{stamp}.html");
    let path = dir.join(file_name);
    let html = netscape::export(&tree, &tree.title);
    std::fs::write(&path, html)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NewNode;
    use crate::store::{ImportMode, Store};

    #[test]
    fn test_backup_creates_importable_file() {
        let mut store = Store::open_in_memory().unwrap();
        let tree = vec![NewNode::bookmark("A", "https://a.com/", None, None)];
        store.import_children(&tree, ImportMode::Replace).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = create_backup(&store, dir.path()).unwrap();
        assert!(path.exists());
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with("bookmarks_backup_") && name.ends_with(".html"));

        let html = std::fs::read_to_string(&path).unwrap();
        let parsed = crate::netscape::parse(&html).unwrap();
        assert_eq!(parsed.children.len(), 1);
    }
}
