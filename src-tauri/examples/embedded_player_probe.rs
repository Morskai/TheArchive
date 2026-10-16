//! Isolated real Tauri + libmpv probe. Only reads generated media under output/.
use local_film_library_preview_lib as archive;
use tauri::{Manager, Emitter};
use std::{collections::HashMap, sync::{Mutex, mpsc, atomic::{AtomicU64, Ordering}}};
#[derive(Default)]
struct Replies { next: AtomicU64, pending: Mutex<HashMap<u64, mpsc::SyncSender<serde_json::Value>>> }
#[derive(Default)]
struct WindowActions(Mutex<Vec<String>>);
#[tauri::command]
fn probe_window_actions(app: tauri::AppHandle) -> Vec<String> { app.state::<WindowActions>().0.lock().unwrap().clone() }
#[tauri::command]
fn probe_report(app: tauri::AppHandle, id: u64, result: serde_json::Value) { if let Some(sender) = app.state::<Replies>().pending.lock().unwrap().remove(&id) { let _ = sender.send(result); } }
#[tauri::command]
async fn probe_eval(app: tauri::AppHandle, label: String, script: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let replies = app.state::<Replies>();
        let id = replies.next.fetch_add(1, Ordering::SeqCst);
        let (send, receive) = mpsc::sync_channel(1);
        replies.pending.lock().unwrap().insert(id, send);
        app.get_webview_window(&label).ok_or("missing_probe_window")?.eval(format!("Promise.resolve().then(()=>({script})).then(result=>window.__TAURI_INTERNALS__.invoke('probe_report',{{id:{id},result:result??null}})).catch(error=>window.__TAURI_INTERNALS__.invoke('probe_report',{{id:{id},result:{{error:String(error)}}}}))")).map_err(|_| "probe_eval_failed")?;
        let result = receive.recv_timeout(std::time::Duration::from_secs(5)).map_err(|_| "probe_eval_timeout".to_owned());
        replies.pending.lock().unwrap().remove(&id);
        result
    }).await.map_err(|_| "probe_eval_failed".to_owned())?
}

#[tauri::command]
fn probe_catalog(app: tauri::AppHandle) -> Vec<archive::catalog::CatalogFilm> {
    let storage = app.state::<archive::storage_session::StorageSession>();
    archive::catalog::list_films(&storage.lock().unwrap()).unwrap()
}
#[tauri::command]
fn probe_display(app: tauri::AppHandle, display: String) {
    let storage = app.state::<archive::storage_session::StorageSession>();
    archive::playback::save_settings(&storage.lock().unwrap(), &archive::playback::PlayerSettings { internal_display: display, ..Default::default() }).unwrap();
}
#[tauri::command]
fn load_interface_preferences() -> archive::lifecycle::InterfacePreferences { Default::default() }
#[tauri::command]
fn probe_windows(app: tauri::AppHandle) -> Vec<(String, String)> { app.webview_windows().into_iter().map(|(label,w)| (label, w.url().map(|u| u.to_string()).unwrap_or_default())).collect() }
#[tauri::command]
fn probe_hwnd(app: tauri::AppHandle, label: String) -> usize { app.get_webview_window(&label).unwrap().hwnd().unwrap().0 as usize }
#[tauri::command]
fn window_titlebar_action(window: tauri::Window, action: String) -> Result<bool, String> {
    let result = archive::perform_titlebar_action(window.clone(), action.clone());
    if result.is_ok() { window.app_handle().state::<WindowActions>().0.lock().unwrap().push(action); }
    result
}
#[tauri::command]
async fn launch_film_video(app: tauri::AppHandle, window: tauri::WebviewWindow, request_id: String, film_id: String, video_id: String, player_token: Option<u64>) -> Result<archive::playback::LaunchOutcome, String> { archive::perform_launch_film_video(app, window, request_id, film_id, video_id, player_token).await }
#[tauri::command]
async fn probe_exit(app: tauri::AppHandle) {
    tauri::async_runtime::spawn_blocking(move || { let _ = app.state::<archive::embedded_player::EmbeddedPlayer>().close(&app, None); app.exit(0); }).await.unwrap();
}
fn main() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../output/embedded-player");
    let home = tempfile::Builder::new().prefix("native-probe-").tempdir_in(&root).unwrap();
    let storage = archive::storage::Storage::open(&home.path().join("data")).unwrap();
    let media = root.join(if std::env::var("PLAYER_PROBE_LONG").as_deref() == Ok("1") { "long-media" } else { "media" }).canonicalize().unwrap();
    storage.with_connection(|db| { db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('probe','合成片库',?1)", [media.to_string_lossy().as_ref()])?; Ok(()) }).unwrap();
    archive::observation::scan_into_library(&storage, "probe", &media).unwrap();
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "com.thearchive.syntheticplayerprobe".into();
    context.config_mut().build.dev_url = Some("http://127.0.0.1:15173".parse().unwrap());
    context.config_mut().app.windows[0].url = tauri::WebviewUrl::External("http://127.0.0.1:15173/__native-player-probe".parse().unwrap());
    context.config_mut().app.windows[0].data_directory = Some(home.path().join("webview"));
    let startup_guard = archive::single_instance::StartupGuard::acquire(&context.config().identifier).unwrap();
    let app = tauri::Builder::default()
        .plugin(archive::single_instance::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(archive::storage_session::StorageSession::new(storage))
        .manage(archive::lifecycle::StartupSession::default())
        .manage(archive::playback::PlaybackSession::default())
        .manage(archive::embedded_player::EmbeddedPlayer::default())
        .manage(archive::window_lifecycle::WindowLifecycle::default())
        .manage(Replies::default())
        .manage(WindowActions::default())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "player" { api.prevent_close(); let app = window.app_handle().clone(); let owner = app.state::<archive::embedded_player::EmbeddedPlayer>().detached_owner(); if let Some(token) = owner { tauri::async_runtime::spawn_blocking(move || { let _ = app.state::<archive::embedded_player::EmbeddedPlayer>().close(&app, Some((token, "player"))); }); } }
            }
        })
        .invoke_handler(tauri::generate_handler![probe_catalog, probe_display, probe_exit, probe_windows, probe_hwnd, probe_eval, probe_report, probe_window_actions, load_interface_preferences,
            launch_film_video, window_titlebar_action, archive::window_lifecycle::restart_application,
            archive::embedded_player::embedded_player_state, archive::embedded_player::embedded_player_layout,
            archive::embedded_player::embedded_player_control, archive::embedded_player::embedded_player_close,
            archive::embedded_player::embedded_player_choose_subtitle])
        .setup(|app| {
            if let Ok(path) = std::env::var("PLAYER_PROBE_RESTART_MARKER") {
                use std::io::Write;
                writeln!(std::fs::OpenOptions::new().create(true).append(true).open(path)?, "{}", std::process::id())?;
            }
            let _ = app.emit("probe-ready", ()); Ok(())
        })
        .build(context).unwrap();
    drop(startup_guard);
    app.run(|_, _| {});
    drop(home);
}
