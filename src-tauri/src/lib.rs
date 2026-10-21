mod open;
mod parser;
mod pty;
mod state;
mod stats;
mod transcript;
mod types;
mod watcher;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::ipc::Channel;
use tauri::{Emitter, Manager, RunEvent};

use pty::{PtyEvent, PtyManager, SpawnRequest};
use state::GraphStore;
use types::GraphState;

type SharedStore = Arc<Mutex<GraphStore>>;

/// Root of the transcripts the UI may read (`<claude dir>/projects`).
struct ProjectsDir(PathBuf);

#[tauri::command]
fn snapshot(store: tauri::State<'_, SharedStore>) -> GraphState {
    store.lock().unwrap().snapshot()
}

/// Loads a transcript for the detail view. Only `.jsonl` files under the projects
/// directory are readable, so the webview cannot use this to read arbitrary files.
#[tauri::command]
fn load_transcript(
    path: String,
    projects: tauri::State<'_, ProjectsDir>,
) -> Result<transcript::TranscriptView, String> {
    let requested = Path::new(&path).canonicalize().map_err(|e| e.to_string())?;
    let root = projects.0.canonicalize().map_err(|e| e.to_string())?;
    if !requested.starts_with(&root) || requested.extension().is_none_or(|e| e != "jsonl") {
        return Err("not a transcript".into());
    }
    transcript::load(&requested).map_err(|e| e.to_string())
}

#[tauri::command]
fn pty_spawn(
    app: tauri::AppHandle,
    request: SpawnRequest,
    cols: u16,
    rows: u16,
    on_event: Channel<PtyEvent>,
) -> Result<u32, String> {
    app.state::<PtyManager>().spawn(app.clone(), request, cols, rows, on_event)
}

#[tauri::command]
fn pty_write(id: u32, data: String, ptys: tauri::State<'_, PtyManager>) -> Result<(), String> {
    ptys.write(id, &data)
}

#[tauri::command]
fn pty_resize(id: u32, cols: u16, rows: u16, ptys: tauri::State<'_, PtyManager>) -> Result<(), String> {
    ptys.resize(id, cols, rows)
}

#[tauri::command]
fn open_file(app: tauri::AppHandle, path: String) -> Result<open::OpenOutcome, String> {
    open::open_file(&app, &path)
}

#[tauri::command]
fn pty_kill(id: u32, ptys: tauri::State<'_, PtyManager>) -> Result<(), String> {
    ptys.kill(id)
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_opener::init())
        .manage(PtyManager::default())
        .setup(|app| {
            let store = SharedStore::default();
            app.manage(store.clone());
            let dirs = watcher::ClaudeDirs::new(&claude_dir(app)?);
            app.manage(ProjectsDir(dirs.projects.clone()));

            let handle = app.handle().clone();
            watcher::spawn(dirs, store, move |event| {
                let _ = handle.emit("graph-event", event);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            load_transcript,
            pty_spawn,
            pty_write,
            pty_resize,
            pty_kill,
            open_file
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Sessions started in the app live in its terminals; don't leave them orphaned.
            if let RunEvent::Exit = event {
                app.state::<PtyManager>().kill_all();
            }
        });
}
