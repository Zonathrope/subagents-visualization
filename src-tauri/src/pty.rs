//! Hosts Claude Code sessions in pseudo-terminals so the UI can show and drive them.
//!
//! The webview can only ask for the two launches in [`SpawnRequest`]; it never supplies
//! a program or raw arguments.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::thread;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SpawnRequest {
    /// `claude --session-id <id>` in `cwd`. The caller picks the id so it can link the
    /// terminal to the session's transcript and graph node before Claude writes anything.
    New { cwd: String, session_id: String },
    /// `claude attach <id>` for a background session.
    Attach { id: String },
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PtyEvent {
    Output { data: String },
    Exit { code: Option<u32> },
}

struct Pty {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Default)]
pub struct PtyManager {
    next_id: AtomicU32,
    ptys: Mutex<HashMap<u32, Pty>>,
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn is_short_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn claude_command(args: &[&str]) -> CommandBuilder {
    // npm installs `claude` as a .cmd shim on Windows, which CreateProcess can't run directly.
    #[cfg(windows)]
    let mut cmd = {
        let mut c = CommandBuilder::new("cmd.exe");
        c.args(["/d", "/c", "claude"]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = CommandBuilder::new("claude");
    cmd.args(args);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd
}

fn build_command(request: &SpawnRequest) -> Result<CommandBuilder, String> {
    match request {
        SpawnRequest::New { cwd, session_id } => {
            if !is_uuid(session_id) {
                return Err("session id must be a UUID".into());
            }
            if !Path::new(cwd).is_dir() {
                return Err(format!("not a directory: {cwd}"));
            }
            let mut cmd = claude_command(&["--session-id", session_id]);
            cmd.cwd(cwd);
            Ok(cmd)
        }
        SpawnRequest::Attach { id } => {
            if !is_short_id(id) {
                return Err("invalid background session id".into());
            }
            let mut cmd = claude_command(&["attach", id]);
            if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
                cmd.cwd(home);
            }
            Ok(cmd)
        }
    }
}

/// Length of the longest prefix that is complete UTF-8; a multi-byte character split
/// across reads is held back until the rest arrives.
fn utf8_complete_len(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        Err(_) => bytes.len(), // genuinely invalid bytes: let lossy decoding replace them
    }
}

impl PtyManager {
    pub fn spawn(
        &self,
        app: AppHandle,
        request: SpawnRequest,
        cols: u16,
        rows: u16,
        on_event: Channel<PtyEvent>,
    ) -> Result<u32, String> {
        let cmd = build_command(&request)?;
        let pair = native_pty_system()
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
        let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        drop(pair.slave);

        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let killer = child.clone_killer();
        self.ptys.lock().unwrap().insert(id, Pty { writer, master: pair.master, killer });

        // ConPTY keeps the output pipe open after the child exits, so the waiter closes the
        // pty once the process is gone; that ends the reader below.
        let waiter = thread::spawn(move || {
            let code = child.wait().ok().map(|s| s.exit_code());
            app.state::<PtyManager>().ptys.lock().unwrap().remove(&id);
            code
        });

        thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            let mut pending: Vec<u8> = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        pending.extend_from_slice(&buf[..n]);
                        let len = utf8_complete_len(&pending);
                        if len > 0 {
                            let data = String::from_utf8_lossy(&pending[..len]).into_owned();
                            pending.drain(..len);
                            if on_event.send(PtyEvent::Output { data }).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
            let code = waiter.join().ok().flatten();
            let _ = on_event.send(PtyEvent::Exit { code });
        });

        Ok(id)
    }

    pub fn write(&self, id: u32, data: &str) -> Result<(), String> {
        let mut ptys = self.ptys.lock().unwrap();
        let pty = ptys.get_mut(&id).ok_or("terminal has exited")?;
        pty.writer.write_all(data.as_bytes()).and_then(|_| pty.writer.flush()).map_err(|e| e.to_string())
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        let ptys = self.ptys.lock().unwrap();
        let pty = ptys.get(&id).ok_or("terminal has exited")?;
        pty.master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())
    }

    pub fn kill(&self, id: u32) -> Result<(), String> {
        let mut ptys = self.ptys.lock().unwrap();
        match ptys.get_mut(&id) {
            Some(pty) => pty.killer.kill().map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }

    pub fn kill_all(&self) {
        for pty in self.ptys.lock().unwrap().values_mut() {
            let _ = pty.killer.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_requests() {
        let uuid = "a6e4f7ef-31c9-47dc-973c-cd386996d4e1";
        let cwd = std::env::temp_dir().to_string_lossy().into_owned();
        assert!(build_command(&SpawnRequest::New { cwd: cwd.clone(), session_id: uuid.into() }).is_ok());
        assert!(build_command(&SpawnRequest::New { cwd: cwd.clone(), session_id: "x; rm -rf /".into() }).is_err());
        assert!(build_command(&SpawnRequest::New { cwd: "Z:/no/such/dir".into(), session_id: uuid.into() }).is_err());
        assert!(build_command(&SpawnRequest::Attach { id: "bc8cef12".into() }).is_ok());
        assert!(build_command(&SpawnRequest::Attach { id: "a & calc".into() }).is_err());
    }

    #[test]
    fn deserializes_camel_case_requests() {
        let r: SpawnRequest =
            serde_json::from_str(r#"{"kind":"new","cwd":"C:/x","sessionId":"s"}"#).unwrap();
        assert!(matches!(r, SpawnRequest::New { session_id, .. } if session_id == "s"));
    }

    /// Runs the real `claude` through the same command path and pty setup as the app.
    #[test]
    #[ignore = "needs Claude Code installed"]
    fn runs_claude_in_a_pty() {
        let pair = native_pty_system()
            .openpty(PtySize { rows: 24, cols: 100, pixel_width: 0, pixel_height: 0 })
            .unwrap();
        let mut child = pair.slave.spawn_command(claude_command(&["--version"])).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        let out = std::thread::spawn(move || {
            let mut all = Vec::new();
            let mut buf = [0u8; 4096];
            let mut answered = false;
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 { break; }
                all.extend_from_slice(&buf[..n]);
                // ConPTY blocks on a cursor-position query until the terminal replies;
                // xterm.js answers it in the app, so answer it here too.
                if !answered && all.windows(4).any(|w| w == b"\x1b[6n") {
                    let _ = writer.write_all(b"\x1b[1;1R");
                    answered = true;
                }
            }
            String::from_utf8_lossy(&all).into_owned()
        });
        let status = child.wait().unwrap();
        drop(pair.master);
        let text = out.join().unwrap();
        assert!(status.success(), "exit {status:?}, output: {text:?}");
        assert!(text.contains("Claude Code"), "output: {text:?}");
    }

    #[test]
    fn holds_back_split_utf8() {
        let bytes = "é".as_bytes();
        assert_eq!(utf8_complete_len(&[b'a', bytes[0]]), 1);
        assert_eq!(utf8_complete_len(&[b'a', bytes[0], bytes[1]]), 3);
    }
}
