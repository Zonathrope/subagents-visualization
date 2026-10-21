//! "Open file" for paths taken from transcripts.
//!
//! Opening with the system default app would *run* scripts and executables (a `.bat`,
//! `.ps1` or `.js` an agent wrote), so those are revealed in the file manager instead.

use std::path::Path;

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// Extensions whose default Windows handler executes the file.
const RUNNABLE: &[&str] = &[
    "exe", "com", "bat", "cmd", "ps1", "psm1", "psd1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta",
    "msi", "msp", "msc", "lnk", "scr", "pif", "reg", "cpl", "jar", "py", "pyw", "sh", "url",
    "appref-ms", "application", "scf", "inf",
];

fn is_runnable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| RUNNABLE.contains(&e.to_ascii_lowercase().as_str()))
}

/// What happened, so the UI can say "revealed" instead of "opened".
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpenOutcome {
    Opened,
    Revealed,
}

pub fn open_file(app: &AppHandle, path: &str) -> Result<OpenOutcome, String> {
    let path = Path::new(path);
    if !path.is_file() {
        return Err(format!("file not found: {}", path.display()));
    }
    if is_runnable(path) {
        app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())?;
        Ok(OpenOutcome::Revealed)
    } else {
        app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())?;
        Ok(OpenOutcome::Opened)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_are_not_opened() {
        assert!(is_runnable(Path::new("C:/x/run.BAT")));
        assert!(is_runnable(Path::new("tool.ps1")));
        assert!(is_runnable(Path::new("build.js")));
        assert!(!is_runnable(Path::new("src/main.rs")));
        assert!(!is_runnable(Path::new("README.md")));
        assert!(!is_runnable(Path::new("Makefile")));
    }
}
