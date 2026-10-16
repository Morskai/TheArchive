//! One application-owned player, shared by the main overlay and detached window.
use crate::{catalog, mpv::Mpv, storage::Storage};
use serde::{Deserialize, Serialize};
use std::{path::{Path, PathBuf}, sync::{Mutex, mpsc, atomic::{AtomicU64, Ordering}}};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Video { id: String, name: String }
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track { id: String, kind: String, title: String, selected: bool }
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub token: u64,
    pub window: String,
    pub film_id: String,
    pub video_id: String,
    pub title: String,
    pub videos: Vec<Video>,
    pub tracks: Vec<Track>,
    pub position: f64,
    pub duration: f64,
    pub seeking: bool,
    pub paused: bool,
    pub stopped: bool,
    pub volume: f64,
    pub speed: f64,
    pub fullscreen: bool,
    pub loading: bool,
    pub aspect: String,
    pub error: Option<String>,
}
#[derive(Default)]
struct Session {
    next_token: u64,
    view: Option<PlayerState>,
    engine: Option<Mpv>,
    surface: usize,
    path: PathBuf,
}
#[derive(Default)]
pub struct EmbeddedPlayer(Mutex<Session>, AtomicU64);

fn on_main<T: Send + 'static>(app: &tauri::AppHandle, action: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    let (send, receive) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || { let _ = send.send(action()); }).map_err(|_| "player_window_failed")?;
    receive.recv().map_err(|_| "player_window_failed".to_owned())?
}

fn surface(app: &tauri::AppHandle, label: &str) -> Result<usize> {
    let handle = app.clone();
    let label = label.to_owned();
    on_main(app, move || {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let window = handle.get_webview_window(&label).ok_or("player_window_failed")?;
        let parent = window.hwnd().map_err(|_| "player_window_failed")?.0;
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let child = unsafe { CreateWindowExW(WS_EX_NOACTIVATE, class.as_ptr(), std::ptr::null(), WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN, 0, 0, 1, 1, parent.cast(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null()) };
        if child.is_null() { Err("player_window_failed".into()) } else { Ok(child as usize) }
    })
}
fn destroy_surface(app: &tauri::AppHandle, surface: usize) {
    if surface != 0 { let _ = on_main(app, move || { unsafe { windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(surface as _); } Ok(()) }); }
}
fn announce(app: &tauri::AppHandle) { let _ = app.emit("embedded-player-changed", ()); }
fn authorize(view: &PlayerState, token: u64, window: &str) -> Result<()> {
    if view.token != token || view.window != window { Err("player_replaced".into()) } else { Ok(()) }
}

impl EmbeddedPlayer {
    // Native close notifications run on the UI thread. Read the owner without
    // taking a mutex which a worker may hold while waiting for that same thread.
    pub fn detached_owner(&self) -> Option<u64> {
        match self.1.load(Ordering::Acquire) { 0 => None, token => Some(token) }
    }
    pub fn open(&self, app: &tauri::AppHandle, storage: &Storage, film: &str, video: &str, path: &Path, display: &str, expected_owner: Option<(u64, &str)>) -> Result<()> {
        let film = catalog::player_film(storage, film).map_err(|_| "catalog_failed")?.ok_or("video_not_linked")?;
        let videos = film.videos.into_iter().map(|v| Video { id: v.id, name: v.observed_path.rsplit(['/', '\\']).next().unwrap_or("").to_owned() }).collect();
        let mut session = self.0.lock().map_err(|_| "player_unavailable")?;
        if let Some((token, window)) = expected_owner { authorize(session.view.as_ref().ok_or("player_replaced")?, token, window)?; }
        // An active player keeps its presentation until it is closed.
        let label = session.view.as_ref().map(|v| v.window.clone()).unwrap_or_else(|| if display == "window" { "player" } else { "main" }.into());
        if label == "player" && app.get_webview_window("player").is_none() {
            let mut builder = tauri::WebviewWindowBuilder::new(app, "player", tauri::WebviewUrl::App("index.html?player=1".into()))
                .title(crate::interface_language::text(app, crate::interface_language::NativeText::PlayerTitle)).inner_size(1100.0, 740.0).min_inner_size(640.0, 460.0)
                .decorations(false).visible(false).theme(Some(tauri::Theme::Dark));
            if let Some(directory) = app.config().app.windows.first().and_then(|w| w.data_directory.clone()) { builder = builder.data_directory(directory); }
            builder.build().map_err(|_| "player_window_failed")?;
        }
        session.next_token += 1;
        let aspect = session.view.as_ref().map(|v| v.aspect.clone()).unwrap_or_else(|| "original".into());
        session.view = Some(PlayerState { token: session.next_token, window: label.clone(), film_id: film.id, video_id: video.into(), title: film.title, videos, volume: 100.0, speed: 1.0, paused: true, loading: true, aspect, ..Default::default() });
        self.1.store(if label == "player" { session.next_token } else { 0 }, Ordering::Release);
        session.path = path.into();
        // Publish a renderable presentation before DLL initialization/demux.
        // The frontend uses this snapshot without blocking on the session mutex.
        let _ = app.emit("embedded-player-opening", session.view.as_ref().unwrap());
        let result = (|| {
          if session.engine.is_none() {
            let child = surface(app, &label)?;
            let resource = app.path().resource_dir().map_err(|_| "internal_player_unavailable")?.join("player/libmpv-2.dll");
            // Dev uses the same pinned binary verified by prepare-player.mjs.
            let resource = if cfg!(debug_assertions) { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-tools/mpv/libmpv-2.dll") } else { resource };
            match Mpv::new(&resource, Some(child)) {
                Ok(engine) => { session.engine = Some(engine); session.surface = child; }
                Err(error) => { destroy_surface(app, child); return Err(error); }
            }
          }
          session.engine.as_ref().unwrap().load(path)
        })();
        session.view.as_mut().unwrap().loading = false;
        if let Err(error) = &result { let v = session.view.as_mut().unwrap(); v.error = Some(error.clone()); v.stopped = true; }
        drop(session);
        if let Some(window) = app.get_webview_window(&label) { let _ = window.show(); let _ = window.unminimize(); let _ = window.set_focus(); }
        announce(app);
        result
    }

    pub fn active_display(&self, token: u64, window: &str) -> Result<String> {
        let session = self.0.lock().map_err(|_| "player_unavailable")?;
        let view = session.view.as_ref().ok_or("player_replaced")?;
        authorize(view, token, window)?;
        Ok(if view.window == "player" { "window" } else { "overlay" }.into())
    }
    fn state(&self, app: &tauri::AppHandle) -> Result<Option<PlayerState>> {
        let mut session = self.0.lock().map_err(|_| "player_unavailable")?;
        let Some(mut view) = session.view.clone() else { return Ok(None); };
        if let Some(engine) = &session.engine {
            let events = engine.events();
            if events.escape { let _ = app.emit_to(view.window.as_str(), "embedded-player-escape", view.token); }
            if events.back { let _ = app.emit_to(view.window.as_str(), "embedded-player-back", view.token); }
            if events.failed { view.error = Some("media_unplayable".into()); view.stopped = true; }
            if let Some(window) = app.get_webview_window(&view.window) {
                let fullscreen = window.is_fullscreen().map_err(|_| "player_window_failed")?;
                if events.fullscreen_toggles % 2 == 1 { window.set_fullscreen(!fullscreen).map_err(|_| "player_window_failed")?; }
                view.fullscreen = window.is_fullscreen().unwrap_or(fullscreen);
            }
            view.position = engine.number("time-pos");
            view.duration = engine.number("duration");
            view.seeking = engine.flag("seeking");
            view.paused = engine.flag("pause") || engine.flag("eof-reached");
            view.volume = engine.number("volume").clamp(0.0, 100.0);
            view.speed = engine.number("speed");
            view.tracks = (0..(engine.number("track-list/count") as usize).min(128)).filter_map(|i| {
                let base = format!("track-list/{i}");
                let kind = engine.property(&format!("{base}/type"))?;
                if kind != "audio" && kind != "sub" { return None; }
                let id = engine.property(&format!("{base}/id"))?;
                let title = [engine.property(&format!("{base}/title")), engine.property(&format!("{base}/lang"))].into_iter().flatten().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
                Some(Track { title: if title.is_empty() { format!("{} {id}", crate::interface_language::text(app, if kind == "sub" { crate::interface_language::NativeText::Subtitles } else { crate::interface_language::NativeText::AudioTrack })) } else { title }, id, kind, selected: engine.flag(&format!("{base}/selected")) })
            }).collect();
        }
        session.view = Some(view.clone());
        Ok(Some(view))
    }
    pub fn close(&self, app: &tauri::AppHandle, owner: Option<(u64, &str)>) -> Result<()> {
        let mut session = self.0.lock().map_err(|_| "player_unavailable")?;
        if let Some((token, window)) = owner {
            authorize(session.view.as_ref().ok_or("player_replaced")?, token, window)?;
        }
        if let Some(engine) = &session.engine { let _ = engine.command(&["stop"]); }
        let surface = session.surface;
        let label = session.view.as_ref().map(|v| v.window.clone());
        let handle = app.clone();
        // Remove native video and restore the owning window before publishing
        // a closed state. DLL teardown must not keep the library behind a modal.
        on_main(app, move || {
            if surface != 0 { unsafe { windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(surface as _, windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE); } }
            if let Some(label) = label { if let Some(window) = handle.get_webview_window(&label) {
                let _ = window.set_fullscreen(false);
                if label == "player" { let _ = window.hide(); }
            } }
            Ok(())
        })?;
        session.view.take();
        self.1.store(0, Ordering::Release);
        let engine = session.engine.take();
        let surface = std::mem::take(&mut session.surface);
        session.path.clear();
        drop(session);
        announce(app);
        drop(engine);
        destroy_surface(app, surface);
        Ok(())
    }
}

#[tauri::command]
pub async fn embedded_player_state(app: tauri::AppHandle) -> Result<Option<PlayerState>> {
    tauri::async_runtime::spawn_blocking(move || app.state::<EmbeddedPlayer>().state(&app)).await.map_err(|_| "player_unavailable".to_owned())?
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect { x: f64, y: f64, width: f64, height: f64 }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds { x: f64, y: f64, width: f64, height: f64, visible: bool, cutout: Option<Rect> }
#[tauri::command]
pub async fn embedded_player_layout(app: tauri::AppHandle, window: tauri::WebviewWindow, token: u64, bounds: Bounds) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        if [bounds.x, bounds.y, bounds.width, bounds.height].iter().any(|v| !v.is_finite() || v.abs() > 100000.0) || bounds.width < 0.0 || bounds.height < 0.0 { return Err("invalid_player_action".into()); }
        if bounds.cutout.as_ref().is_some_and(|r| [r.x, r.y, r.width, r.height].iter().any(|v| !v.is_finite() || v.abs() > 100000.0) || r.width < 0.0 || r.height < 0.0) { return Err("invalid_player_action".into()); }
        let session = app.state::<EmbeddedPlayer>();
        let session = session.0.lock().map_err(|_| "player_unavailable")?;
        authorize(session.view.as_ref().ok_or("player_replaced")?, token, window.label())?;
        let surface = session.surface;
        let scale = window.scale_factor().map_err(|_| "player_window_failed")?;
        on_main(&app, move || {
            unsafe {
                SetWindowPos(surface as _, HWND_TOP, (bounds.x * scale).round() as i32, (bounds.y * scale).round() as i32, (bounds.width * scale).round() as i32, (bounds.height * scale).round() as i32, SWP_NOACTIVATE);
                // Cut the HTML volume popup out of the native video region.
                use windows_sys::Win32::Graphics::Gdi::*;
                if let Some(r) = bounds.cutout {
                    let region = CreateRectRgn(0, 0, (bounds.width * scale).round() as i32, (bounds.height * scale).round() as i32);
                    let hole = CreateRectRgn((r.x * scale).floor() as i32, (r.y * scale).floor() as i32, ((r.x + r.width) * scale).ceil() as i32, ((r.y + r.height) * scale).ceil() as i32);
                    if !region.is_null() && !hole.is_null() && CombineRgn(region, region, hole, RGN_DIFF) != 0 {
                        if SetWindowRgn(surface as _, region, 1) == 0 { DeleteObject(region); }
                    } else if !region.is_null() { DeleteObject(region); }
                    if !hole.is_null() { DeleteObject(hole); }
                } else { SetWindowRgn(surface as _, std::ptr::null_mut(), 1); }
                ShowWindow(surface as _, if bounds.visible && bounds.width >= 1.0 && bounds.height >= 1.0 { SW_SHOWNA } else { SW_HIDE });
            }
            Ok(())
        })
    }).await.map_err(|_| "player_window_failed".to_owned())?
}

#[tauri::command]
pub async fn embedded_player_control(app: tauri::AppHandle, window: tauri::WebviewWindow, token: u64, action: String, value: Option<String>) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let session = app.state::<EmbeddedPlayer>();
        let mut session = session.0.lock().map_err(|_| "player_unavailable")?;
        authorize(session.view.as_ref().ok_or("player_replaced")?, token, window.label())?;
        let engine = session.engine.as_ref().ok_or("player_unavailable")?;
        let value = value.as_deref().unwrap_or("");
        let number = || value.parse::<f64>().ok().filter(|n| n.is_finite()).ok_or("invalid_player_action".to_owned());
        match action.as_str() {
            "pause" => {
                if session.view.as_ref().unwrap().stopped { engine.load(&session.path)?; session.view.as_mut().unwrap().stopped = false; }
                else if engine.flag("eof-reached") { engine.command(&["seek", "0", "absolute"])?; engine.command(&["set", "pause", "no"])?; }
                else { engine.command(&["cycle", "pause"])?; }
            }
            "stop" => { engine.command(&["stop"])?; session.view.as_mut().unwrap().stopped = true; }
            "seek" => { let seconds = number()?.clamp(0.0, engine.number("duration")); engine.command(&["seek", &seconds.to_string(), "absolute+exact"])?; }
            "skip" => { let seconds = number()?.clamp(-600.0, 600.0); engine.command(&["seek", &seconds.to_string(), "relative+exact"])?; }
            "volume" => { let volume = number()?.clamp(0.0, 100.0); engine.command(&["set", "volume", &volume.to_string()])?; }
            "speed" => { if !["0.5", "0.75", "1", "1.25", "1.5", "2"].contains(&value) { return Err("invalid_player_action".into()); } engine.command(&["set", "speed", value])?; }
            "subtitle" | "audio" => {
                if value != "no" && !value.bytes().all(|c| c.is_ascii_digit()) { return Err("invalid_player_action".into()); }
                if value.len() > 10 || value.is_empty() { return Err("invalid_player_action".into()); }
                engine.command(&["set", if action == "subtitle" { "sid" } else { "aid" }, value])?;
            }
            "aspect" => {
                let ratio = match value { "original" => "-1", "16:9" => "16:9", "4:3" => "4:3", "stretch" => "-1", _ => return Err("invalid_player_action".into()) };
                engine.command(&["set", "video-aspect-override", ratio])?;
                engine.command(&["set", "keepaspect", if value == "stretch" { "no" } else { "yes" }])?;
                session.view.as_mut().unwrap().aspect = value.into();
            }
            "fullscreen" => {
                let fullscreen = window.is_fullscreen().map_err(|_| "player_window_failed")?;
                window.set_fullscreen(!fullscreen).map_err(|_| "player_window_failed")?;
            }
            _ => return Err("invalid_player_action".into()),
        }
        if let Some(view) = session.view.as_mut() { view.error = None; }
        Ok(())
    }).await.map_err(|_| "player_action_failed".to_owned())?
}
#[tauri::command]
pub async fn embedded_player_close(app: tauri::AppHandle, window: tauri::WebviewWindow, token: u64) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || app.state::<EmbeddedPlayer>().close(&app, Some((token, window.label())))).await.map_err(|_| "player_unavailable".to_owned())?
}
#[tauri::command]
pub async fn embedded_player_choose_subtitle(app: tauri::AppHandle, window: tauri::WebviewWindow, token: u64) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<EmbeddedPlayer>().active_display(token, window.label())?;
        let path = app.dialog().file().set_parent(&window).set_title(crate::interface_language::text(&app, crate::interface_language::NativeText::ChooseSubtitle)).add_filter(crate::interface_language::text(&app, crate::interface_language::NativeText::Subtitles), &["srt", "ass", "ssa", "vtt", "sub", "idx", "sup"]).blocking_pick_file();
        let Some(path) = path else { return Ok(()); };
        let path = path.into_path().map_err(|_| "subtitle_unavailable")?;
        if !path.is_file() || !path.extension().is_some_and(|e| ["srt", "ass", "ssa", "vtt", "sub", "idx", "sup"].iter().any(|s| e.eq_ignore_ascii_case(s))) { return Err("subtitle_unavailable".into()); }
        let session = app.state::<EmbeddedPlayer>();
        let session = session.0.lock().map_err(|_| "player_unavailable")?;
        authorize(session.view.as_ref().ok_or("player_replaced")?, token, window.label())?;
        session.engine.as_ref().ok_or("player_unavailable")?.command(&["sub-add", path.to_str().ok_or("subtitle_unavailable")?, "select"])
    }).await.map_err(|_| "subtitle_unavailable".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_controls_and_other_window_cannot_touch_replacement() {
        let view = PlayerState { token: 12, window: "player".into(), ..Default::default() };
        assert!(authorize(&view, 11, "player").is_err());
        assert!(authorize(&view, 12, "main").is_err());
        assert!(authorize(&view, 12, "player").is_ok());
    }
    #[test]
    fn queued_native_close_retains_the_original_owner_without_locking_the_session() {
        let player = EmbeddedPlayer::default();
        assert_eq!(player.detached_owner(), None);
        player.1.store(12, Ordering::Release);
        let queued = player.detached_owner().unwrap();
        player.1.store(13, Ordering::Release);
        let replacement = PlayerState { token: 13, window: "player".into(), ..Default::default() };
        assert!(authorize(&replacement, queued, "player").is_err());
        player.1.store(0, Ordering::Release);
        assert_eq!(player.detached_owner(), None);
    }
}
