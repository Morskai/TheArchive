//! Small libmpv client. The handle is used only under the player session mutex.
//! Never load user scripts/config, write watch-later files, or forward media logs.
use libloading::Library;
use std::{ffi::{c_char, c_void, CStr, CString}, path::Path, time::{Duration, Instant}};

type Result<T> = std::result::Result<T, String>;
type Handle = *mut c_void;
#[repr(C)]
struct Event { id: i32, error: i32, reply: u64, data: *mut c_void }
#[repr(C)]
struct EndFile { reason: i32, error: i32 }
#[repr(C)]
struct ClientMessage { count: i32, args: *const *const c_char }
#[derive(Default)]
pub struct PlayerEvents { pub failed: bool, pub fullscreen_toggles: usize, pub escape: bool, pub back: bool }

pub struct Mpv {
    _library: Library,
    handle: Handle,
    destroy: unsafe extern "C" fn(Handle),
    command: unsafe extern "C" fn(Handle, *const *const c_char) -> i32,
    get: unsafe extern "C" fn(Handle, *const c_char) -> *mut c_char,
    free: unsafe extern "C" fn(*mut c_void),
    wait: unsafe extern "C" fn(Handle, f64) -> *const Event,
}
// libmpv permits calls from any thread; the owner serializes calls, including destruction.
unsafe impl Send for Mpv {}

impl Mpv {
    pub fn new(path: &Path, window: Option<usize>) -> Result<Self> {
        unsafe {
            let library = Library::new(path).map_err(|_| "internal_player_unavailable")?;
            let create = *library.get::<unsafe extern "C" fn() -> Handle>(b"mpv_create\0").map_err(|_| "internal_player_unavailable")?;
            let initialize = *library.get::<unsafe extern "C" fn(Handle) -> i32>(b"mpv_initialize\0").map_err(|_| "internal_player_unavailable")?;
            let option = *library.get::<unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> i32>(b"mpv_set_option_string\0").map_err(|_| "internal_player_unavailable")?;
            let destroy = *library.get(b"mpv_terminate_destroy\0").map_err(|_| "internal_player_unavailable")?;
            let command = *library.get(b"mpv_command\0").map_err(|_| "internal_player_unavailable")?;
            let get = *library.get(b"mpv_get_property_string\0").map_err(|_| "internal_player_unavailable")?;
            let free = *library.get(b"mpv_free\0").map_err(|_| "internal_player_unavailable")?;
            let wait = *library.get(b"mpv_wait_event\0").map_err(|_| "internal_player_unavailable")?;
            let handle = create();
            if handle.is_null() { return Err("internal_player_unavailable".into()); }
            let engine = Self { _library: library, handle, destroy, command, get, free, wait };
            let mut options = vec![
                ("config", "no".to_owned()), ("load-scripts", "no".into()),
                ("osc", "no".into()), ("input-default-bindings", "no".into()),
                ("input-vo-keyboard", "yes".into()), ("input-cursor", "yes".into()),
                ("input-doubleclick-time", windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime().to_string()),
                ("terminal", "no".into()), ("msg-level", "all=no".into()),
                ("ytdl", "no".into()), ("save-position-on-quit", "no".into()),
                ("sub-auto", "exact".into()), ("audio-file-auto", "no".into()),
                ("idle", "yes".into()), ("keep-open", "yes".into()),
                ("hwdec", "auto-safe".into()), ("volume-max", "100".into()),
            ];
            if let Some(window) = window {
                options.extend([("wid", window.to_string()), ("vo", "gpu".into()), ("gpu-context", "d3d11".into()), ("force-window", "yes".into())]);
            } else { options.extend([("vo", "null".into()), ("ao", "null".into())]); }
            for (key, value) in options {
                let key = CString::new(key).unwrap();
                let value = CString::new(value).unwrap();
                if option(handle, key.as_ptr(), value.as_ptr()) < 0 { return Err("internal_player_unavailable".into()); }
            }
            if initialize(handle) < 0 { return Err("internal_player_unavailable".into()); }
            // Enable only the fixed application shortcuts, never mpv's defaults.
            for (key, command) in [("SPACE", "cycle pause"), ("MBTN_LEFT_DBL", "cycle pause"),
                ("LEFT", "seek -20 relative+exact"), ("RIGHT", "seek 20 relative+exact"),
                ("ENTER", "script-message thearchive-fullscreen"), ("KP_ENTER", "script-message thearchive-fullscreen"),
                ("ESC", "script-message thearchive-escape"), ("BS", "script-message thearchive-back")] {
                engine.command(&["keybind", key, command])?;
            }
            Ok(engine)
        }
    }

    pub fn command(&self, args: &[&str]) -> Result<()> {
        let args: Vec<_> = args.iter().map(|s| CString::new(*s).map_err(|_| "invalid_player_action".to_owned())).collect::<Result<_>>()?;
        let mut pointers: Vec<_> = args.iter().map(|s| s.as_ptr()).collect();
        pointers.push(std::ptr::null());
        if unsafe { (self.command)(self.handle, pointers.as_ptr()) } < 0 { Err("player_action_failed".into()) } else { Ok(()) }
    }
    pub fn property(&self, name: &str) -> Option<String> {
        let name = CString::new(name).ok()?;
        unsafe {
            let value = (self.get)(self.handle, name.as_ptr());
            if value.is_null() { return None; }
            let result = CStr::from_ptr(value).to_string_lossy().into_owned();
            (self.free)(value.cast());
            Some(result)
        }
    }
    pub fn number(&self, name: &str) -> f64 { self.property(name).and_then(|v| v.parse().ok()).filter(|v: &f64| v.is_finite()).unwrap_or(0.0) }
    pub fn flag(&self, name: &str) -> bool { self.property(name).as_deref() == Some("yes") }

    pub fn load(&self, path: &Path) -> Result<()> {
        let path = path.to_str().ok_or("file_unavailable")?;
        // Argument arrays preserve spaces, Unicode, quotes and leading dashes.
        self.command(&["stop"])?;
        while unsafe { (*(self.wait)(self.handle, 0.0)).id } != 0 {}
        self.command(&["loadfile", path, "replace"])?;
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(12) {
            let event = unsafe { &*(self.wait)(self.handle, 0.05) };
            if event.id == 8 { self.command(&["set", "pause", "no"])?; return Ok(()); }
            if event.id == 7 && !event.data.is_null() {
                let end = unsafe { &*event.data.cast::<EndFile>() };
                if end.error < 0 || end.reason == 4 { return Err("media_unplayable".into()); }
            }
            if event.id == 1 { return Err("media_unplayable".into()); }
        }
        let _ = self.command(&["stop"]);
        Err("player_load_timeout".into())
    }
    pub fn events(&self) -> PlayerEvents {
        let mut result = PlayerEvents::default();
        loop {
            let event = unsafe { &*(self.wait)(self.handle, 0.0) };
            if event.id == 0 { break; }
            if event.id == 7 && !event.data.is_null() { result.failed |= unsafe { (*event.data.cast::<EndFile>()).error < 0 }; }
            if event.id == 16 && !event.data.is_null() {
                let message = unsafe { &*event.data.cast::<ClientMessage>() };
                if message.count == 1 && !message.args.is_null() {
                    let argument = unsafe { *message.args };
                    if !argument.is_null() && unsafe { CStr::from_ptr(argument) }.to_bytes() == b"thearchive-fullscreen" { result.fullscreen_toggles += 1; }
                    if !argument.is_null() && unsafe { CStr::from_ptr(argument) }.to_bytes() == b"thearchive-escape" { result.escape = true; }
                    if !argument.is_null() && unsafe { CStr::from_ptr(argument) }.to_bytes() == b"thearchive-back" { result.back = true; }
                }
            }
        }
        result
    }
}
impl Drop for Mpv { fn drop(&mut self) { unsafe { (self.destroy)(self.handle) }; } }
