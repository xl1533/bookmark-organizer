//! # bookmark-core
//!
//! 跨平台书签整理工具的核心逻辑 crate。
//!
//! - 不依赖任何平台 API（无 GUI、无文件对话框），可被 Tauri 2 桌面端与未来的 Android 端复用
//! - 提供书签树模型、Netscape 收藏夹 HTML 导入/导出、URL 标准化与精确去重、
//!   空文件夹清理、链接有效性检测（HTTP HEAD/GET）、SQLite 持久化、
//!   时间戳 HTML 备份与回收站（软删除/恢复）
//!
//! ## 非功能约束（由调用方与 crate 共同保证）
//! - 所有网络请求仅为 GET/HEAD，不提交任何数据，不执行任何网页 JS
//! - 本地书签数据不会发送到任何第三方

pub mod backup;
pub mod checker;
pub mod dedup;
pub mod empty;
pub mod model;
pub mod netscape;
pub mod store;
pub mod urlnorm;

pub use checker::{CheckOptions, CheckResult, CheckStatus, CheckTarget, LinkChecker};
pub use model::{NewNode, Node, NodeId, NodeKind, ROOT_ID};
pub use store::{Store, TrashEntry};

/// 统一错误类型。
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("URL 标准化失败: {0}")]
    UrlNorm(#[from] urlnorm::UrlNormError),

    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTML 导入失败: {0}")]
    Parse(String),

    #[error("{0}")]
    Other(String),
}

pub type CoreResult<T> = Result<T, CoreError>;

/// Unix 时间戳（秒）。
pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}
