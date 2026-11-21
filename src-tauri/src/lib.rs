mod commands;
mod state;

use state::AppState;
use std::sync::{Arc, Mutex};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app
                .path()
                .app_data_dir()
                .expect("无法解析应用数据目录");
            std::fs::create_dir_all(&dir)?;
            let store = bookmark_core::Store::open(&dir.join("bookmarks.db"))?;
            app.manage(AppState {
                store: Arc::new(Mutex::new(store)),
                data_dir: dir,
                run: Arc::new(Mutex::new(None)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_tree,
            commands::get_stats,
            commands::get_db_path,
            commands::import_html,
            commands::export_html,
            commands::move_nodes,
            commands::create_folder,
            commands::find_empty_folders,
            commands::delete_empty_folders,
            commands::dedup_preview,
            commands::dedup_apply,
            commands::start_check,
            commands::cancel_check,
            commands::list_dead,
            commands::delete_dead,
            commands::trash_list,
            commands::trash_restore,
            commands::trash_purge,
            commands::get_proxy,
            commands::set_proxy,
            commands::test_proxy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
