mod parser;
mod state;
mod types;
mod watcher;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager};

use state::GraphStore;
use types::GraphState;

type SharedStore = Arc<Mutex<GraphStore>>;

#[tauri::command]
fn snapshot(store: tauri::State<'_, SharedStore>) -> GraphState {
    store.lock().unwrap().snapshot()
}

/// Claude Code's data directory: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
fn claude_dir(app: &tauri::App) -> tauri::Result<PathBuf> {
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => Ok(app.path().home_dir()?.join(".claude")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            let store = SharedStore::default();
            app.manage(store.clone());

            let handle = app.handle().clone();
            watcher::spawn(watcher::ClaudeDirs::new(&claude_dir(app)?), store, move |event| {
                let _ = handle.emit("graph-event", event);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![snapshot])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
