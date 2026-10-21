mod desktop;
mod open;
mod parser;
mod pty;
mod search;
mod state;
mod stats;
mod transcript;
mod types;
mod watcher;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::ipc::Channel;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};

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
// Commands that read transcripts are async and do the work on a blocking thread:
// synchronous commands run on the main thread and would freeze the window.

#[tauri::command]
async fn load_transcript(
    path: String,
    projects: tauri::State<'_, ProjectsDir>,
) -> Result<transcript::TranscriptView, String> {
    let requested = Path::new(&path).canonicalize().map_err(|e| e.to_string())?;
    let root = projects.0.canonicalize().map_err(|e| e.to_string())?;
    if !requested.starts_with(&root) || requested.extension().is_none_or(|e| e != "jsonl") {
        return Err("not a transcript".into());
    }
    tauri::async_runtime::spawn_blocking(move || transcript::load(&requested))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Every file edit in a session: its own and all its agents'. Paths come from the store,
/// never from the webview.
#[tauri::command]
async fn load_session_changes(
    session_id: String,
    store: tauri::State<'_, SharedStore>,
) -> Result<Vec<transcript::SessionChange>, String> {
    let sources = store.lock().unwrap().session_sources(&session_id);
    tauri::async_runtime::spawn_blocking(move || transcript::session_changes(&sources))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_transcripts(
    query: String,
    store: tauri::State<'_, SharedStore>,
) -> Result<Vec<search::SearchHit>, String> {
    let sources = store.lock().unwrap().search_sources();
    tauri::async_runtime::spawn_blocking(move || search::search(&sources, &query))
        .await
        .map_err(|e| e.to_string())
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

/// Tray tooltip/icon from the main window's counts.
#[tauri::command]
fn set_tray_status(app: tauri::AppHandle, running: u32, needs_you: u32) -> Result<(), String> {
    desktop::set_status(&app, running, needs_you).map_err(|e| e.to_string())
}

/// Shows the main window and opens a node's detail panel there (used by the mini window).
#[tauri::command]
fn focus_node(app: tauri::AppHandle, node_id: String) -> Result<(), String> {
    desktop::focus_node(&app, node_id).map_err(|e| e.to_string())
}

/// Async on purpose: creating a window from a synchronous command deadlocks on Windows,
/// because sync commands run on the main thread that WebView2 needs to build the window.
#[tauri::command]
async fn toggle_mini(app: tauri::AppHandle) -> Result<bool, String> {
    desktop::toggle_mini(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_autostart(app: tauri::AppHandle) -> bool {
    desktop::autostart_enabled(&app)
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enable: bool) -> Result<bool, String> {
    desktop::set_autostart(&app, enable)
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
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .manage(PtyManager::default())
        .on_window_event(|window, event| {
            // Closing the main window hides it to the tray: hosted sessions keep running and
            // alerts keep coming. Quit from the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == desktop::MAIN {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let store = SharedStore::default();
            app.manage(store.clone());
            let dirs = watcher::ClaudeDirs::new(&claude_dir(app)?);
            app.manage(ProjectsDir(dirs.projects.clone()));

            let handle = app.handle().clone();
            watcher::spawn(dirs, store, move |event| {
                let _ = handle.emit("graph-event", event);
            });
            desktop::setup_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            load_transcript,
            load_session_changes,
            search_transcripts,
            pty_spawn,
            pty_write,
            pty_resize,
            pty_kill,
            open_file,
            set_tray_status,
            focus_node,
            toggle_mini,
            get_autostart,
            set_autostart
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
