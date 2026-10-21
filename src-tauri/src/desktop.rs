//! Desktop integration: tray icon, close-to-tray, the mini window and start-at-login.

use std::sync::Mutex;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder, Wry};
use tauri_plugin_autostart::ManagerExt;

pub const MAIN: &str = "main";
pub const MINI: &str = "mini";

/// Handles kept so the tray can be updated after setup.
pub struct Tray {
    icon: TrayIcon,
    autostart_item: CheckMenuItem<Wry>,
    base: Image<'static>,
    /// Last (running, needs_you) shown, to skip redundant icon rebuilds.
    last: Mutex<Option<(u32, u32)>>,
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let base = app.default_window_icon().cloned().expect("bundle icon").to_owned();
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);

    let show = MenuItem::with_id(app, "show", "Show Subagents", true, None::<&str>)?;
    let mini = MenuItem::with_id(app, "mini", "Mini window", true, None::<&str>)?;
    let autostart_item = CheckMenuItem::with_id(app, "autostart", "Start at login", true, autostart_on, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Subagents", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&show, &mini, &PredefinedMenuItem::separator(app)?, &autostart_item, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    let icon = TrayIconBuilder::with_id("subagents")
        .icon(base.clone())
        .tooltip("Subagents")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "mini" => {
                let _ = toggle_mini(app);
            }
            "autostart" => {
                let enable = !app.autolaunch().is_enabled().unwrap_or(false);
                let _ = set_autostart(app, enable);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(Tray { icon, autostart_item, base, last: Mutex::new(None) });
    Ok(())
}

/// Tooltip with the counts, and an amber dot on the icon while something needs the user.
pub fn set_status(app: &AppHandle, running: u32, needs_you: u32) -> tauri::Result<()> {
    let tray = app.state::<Tray>();
    let mut last = tray.last.lock().unwrap();
    if *last == Some((running, needs_you)) {
        return Ok(());
    }
    let had_dot = last.is_some_and(|(_, n)| n > 0);
    *last = Some((running, needs_you));

    let mut tip = format!("Subagents · {running} running");
    if needs_you > 0 {
        tip.push_str(&format!(" · {needs_you} need{} you", if needs_you == 1 { "s" } else { "" }));
    }
    tray.icon.set_tooltip(Some(tip))?;
    if had_dot != (needs_you > 0) {
        let icon = if needs_you > 0 { with_dot(&tray.base) } else { tray.base.clone() };
        tray.icon.set_icon(Some(icon))?;
    }
    Ok(())
}

/// The app icon with an amber dot in the bottom-right corner.
fn with_dot(base: &Image<'_>) -> Image<'static> {
    let (w, h) = (base.width(), base.height());
    let mut rgba = base.rgba().to_vec();
    let r = (w.min(h) as f32) * 0.24;
    let (cx, cy) = (w as f32 - r - 1.0, h as f32 - r - 1.0);
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            if d <= r + 1.0 {
                let i = ((y * w + x) * 4) as usize;
                // Dark ring for contrast, amber fill.
                let (cr, cg, cb) = if d > r - 0.5 { (20, 20, 30) } else { (245, 158, 11) };
                rgba[i..i + 4].copy_from_slice(&[cr, cg, cb, 255]);
            }
        }
    }
    Image::new_owned(rgba, w, h)
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Brings the main window forward and asks it to open a node's detail panel.
pub fn focus_node(app: &AppHandle, node_id: String) -> tauri::Result<()> {
    show_main(app);
    app.emit_to(MAIN, "open-node", node_id)
}

/// Opens the compact always-on-top window, or closes it if open. Returns whether it's open.
pub fn toggle_mini(app: &AppHandle) -> tauri::Result<bool> {
    if let Some(w) = app.get_webview_window(MINI) {
        w.close()?;
        return Ok(false);
    }
    WebviewWindowBuilder::new(app, MINI, WebviewUrl::App("index.html?view=mini".into()))
        .title("Subagents mini")
        .inner_size(340.0, 460.0)
        .min_inner_size(260.0, 200.0)
        .always_on_top(true)
        .skip_taskbar(true)
        .build()?;
    Ok(true)
}

pub fn set_autostart(app: &AppHandle, enable: bool) -> Result<bool, String> {
    let launcher = app.autolaunch();
    if enable { launcher.enable() } else { launcher.disable() }.map_err(|e| e.to_string())?;
    let on = launcher.is_enabled().unwrap_or(enable);
    let _ = app.state::<Tray>().autostart_item.set_checked(on);
    Ok(on)
}

pub fn autostart_enabled(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_is_drawn_in_corner_only() {
        let base = Image::new_owned(vec![0u8; 32 * 32 * 4], 32, 32);
        let dotted = with_dot(&base);
        let px = |x: u32, y: u32| {
            let i = ((y * 32 + x) * 4) as usize;
            dotted.rgba()[i..i + 4].to_vec()
        };
        assert_eq!(px(2, 2), vec![0, 0, 0, 0], "top-left untouched");
        assert_eq!(px(24, 24), vec![245, 158, 11, 255], "amber in the corner");
    }
}
