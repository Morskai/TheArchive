#![cfg(windows)]
use local_film_library_preview_lib::mpv::Mpv;
use std::{fs, path::PathBuf};

fn wave() -> Vec<u8> {
    let samples = 8000_u32 * 20;
    let mut data = Vec::new();
    data.extend_from_slice(b"RIFF"); data.extend_from_slice(&(36 + samples * 2).to_le_bytes());
    data.extend_from_slice(b"WAVEfmt "); data.extend_from_slice(&16_u32.to_le_bytes());
    data.extend_from_slice(&1_u16.to_le_bytes()); data.extend_from_slice(&1_u16.to_le_bytes());
    data.extend_from_slice(&8000_u32.to_le_bytes()); data.extend_from_slice(&16000_u32.to_le_bytes());
    data.extend_from_slice(&2_u16.to_le_bytes()); data.extend_from_slice(&16_u16.to_le_bytes());
    data.extend_from_slice(b"data"); data.extend_from_slice(&(samples * 2).to_le_bytes());
    data.resize(data.len() + samples as usize * 2, 0); data
}
#[test]
fn pinned_engine_loads_unicode_media_controls_and_releases_it() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-tools/mpv/libmpv-2.dll");
    let home = tempfile::tempdir().unwrap();
    let file = home.path().join("中文 空格 #1.wav");
    fs::write(&file, wave()).unwrap();
    let original = fs::read(&file).unwrap();
    let engine = Mpv::new(&path, None).unwrap();
    engine.load(&file.canonicalize().unwrap()).unwrap();
    assert!((engine.number("duration") - 20.0).abs() < 0.1);
    assert!(engine.number("track-list/count") >= 1.0);
    engine.command(&["set", "pause", "yes"]).unwrap();
    assert!(engine.flag("pause"));
    engine.command(&["set", "volume", "32"]).unwrap();
    assert!((engine.number("volume") - 32.0).abs() < 0.1);
    engine.command(&["keypress", "SPACE"]).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!engine.flag("pause"), "fixed Space binding must work on the native surface");
    engine.command(&["keypress", "MBTN_LEFT_DBL"]).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(engine.flag("pause"), "double click pauses without entering mpv fullscreen");
    engine.command(&["keypress", "ENTER"]).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(engine.events().fullscreen_toggles, 1);
    assert!(!engine.flag("fullscreen"), "fullscreen belongs to the host window");
    engine.command(&["keypress", "ESC"]).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(engine.events().escape, "native Escape returns to the token-owned HTML dialog");
    engine.command(&["keypress", "BS"]).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(engine.events().back, "native Backspace uses the application's return operation");
    assert_eq!(engine.number("options/input-doubleclick-time"), unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() } as f64);
    engine.command(&["seek", "8", "absolute+exact"]).unwrap();
    engine.command(&["stop"]).unwrap();
    engine.load(&file).unwrap();
    let invalid = home.path().join("坏文件.mp4");
    fs::write(&invalid, b"not media").unwrap();
    assert!(engine.load(&invalid).is_err());
    drop(engine);
    assert_eq!(fs::read(&file).unwrap(), original);
    fs::rename(&file, home.path().join("released.wav")).unwrap();
}
