use bookmark_core::Store;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

/// 全局应用状态。
pub struct AppState {
    /// SQLite 存储（快速同步操作，锁内无 await）
    pub store: Arc<Mutex<Store>>,
    /// 应用数据目录（数据库、备份所在）
    pub data_dir: PathBuf,
    /// 当前检测任务句柄（None = 空闲）
    pub run: Arc<Mutex<Option<CheckRunHandle>>>,
}

#[derive(Clone)]
pub struct CheckRunHandle {
    pub cancel: Arc<AtomicBool>,
}
