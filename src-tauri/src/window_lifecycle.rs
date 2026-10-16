use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};
#[derive(Debug, PartialEq, Eq)]
pub enum CloseAction {
    Hide,
    Confirm,
    Exit,
}
pub fn close_action(exit: bool, force: bool, tray_available: bool) -> CloseAction {
    if exit || force {
        CloseAction::Exit
    } else if tray_available {
        CloseAction::Hide
    } else {
        CloseAction::Confirm
    }
}
pub struct WindowLifecycle {
    pub close_exits: AtomicBool,
    pub tray_available: AtomicBool,
    pub closing: AtomicBool,
}
impl Default for WindowLifecycle {
    fn default() -> Self {
        Self {
            close_exits: AtomicBool::new(false),
            tray_available: AtomicBool::new(false),
            closing: AtomicBool::new(false),
        }
    }
}
pub fn request(app: &tauri::AppHandle, force: bool) {
    let lifecycle = app.state::<WindowLifecycle>();
    let close_exits = lifecycle.close_exits.load(Ordering::Acquire);
    let tray_visible = lifecycle.tray_available.load(Ordering::Acquire)
        && (close_exits || force || app.tray_by_id("thearchive-tray").is_some_and(|tray| tray.set_visible(true).is_ok()));
    match close_action(
        close_exits,
        force,
        tray_visible,
    ) {
        CloseAction::Hide => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
        }
        CloseAction::Confirm => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            let _ = app.emit("window-close-confirm", "tray_unavailable");
        }
        CloseAction::Exit => exit(app),
    }
}
pub fn exit(app: &tauri::AppHandle) {
    if app
        .state::<WindowLifecycle>()
        .closing
        .swap(true, Ordering::AcqRel)
    {
        return;
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _ = handle.state::<crate::embedded_player::EmbeddedPlayer>().close(&handle, None);
        let state = handle.state::<crate::storage_session::StorageSession>();
        // Let an in-flight short database transaction finish; don't wait for scan I/O.
        if let Ok(_storage) = state.wait_for_exit() {
            crate::diagnostics::record(crate::diagnostics::Event::new("backend", "shutdown", "complete", None));
            crate::diagnostics::flush();
            handle.exit(0);
        } else {
            handle
                .state::<WindowLifecycle>()
                .closing
                .store(false, Ordering::Release);
        }
    });
}
#[tauri::command]
pub async fn restart_application(app: tauri::AppHandle) -> Result<(), String> {
    if app.state::<WindowLifecycle>().closing.swap(true, Ordering::AcqRel) { return Err("restart_in_progress".into()); }
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        handle.state::<crate::embedded_player::EmbeddedPlayer>().close(&handle, None)?;
        let state = handle.state::<crate::storage_session::StorageSession>();
        let _storage = state.wait_for_exit().map_err(|_| "restart_failed".to_owned())?;
        crate::diagnostics::flush();
        handle.request_restart();
        Ok(())
    }).await.map_err(|_| "restart_failed".to_owned()).and_then(|result| result);
    if result.is_err() { app.state::<WindowLifecycle>().closing.store(false, Ordering::Release); }
    result
}
pub fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::{
        tray::{TrayIconBuilder, TrayIconEvent, MouseButton},
    };
    let menu = tray_menu(app)?;
    let mut tray = TrayIconBuilder::with_id("thearchive-tray")
        .tooltip("TheArchive")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(event, TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }) {
                show_main(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "settings" => { show_main(app); let _ = app.emit("tray-open-settings", ()); }
            "exit" => request(app, true),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    let tray = tray.build(app)?;
    tray.set_visible(true)?;
    app.state::<WindowLifecycle>()
        .tray_available
        .store(true, Ordering::Release);
    Ok(())
}

fn tray_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem};
    use crate::interface_language::{text, NativeText};
    let open = MenuItem::with_id(app, "open", text(app, NativeText::Open), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", text(app, NativeText::Settings), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "exit", text(app, NativeText::Exit), true, None::<&str>)?;
    Menu::with_items(app, &[&open, &settings, &quit])
}

pub(crate) fn sync_tray_language(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id("thearchive-tray") { tray.set_menu(Some(tray_menu(app)?))?; }
    Ok(())
}

pub fn show_main<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
