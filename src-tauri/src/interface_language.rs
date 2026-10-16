//! The interface language is independent of appearance and title translation.
use crate::storage::Storage;
use std::sync::atomic::{AtomicU8, Ordering};
use tauri::{Emitter, Manager};

pub const DEFAULT_LANGUAGE: &str = "zh-CN";

fn language_index(language: &str) -> Option<u8> {
    match language {
        "zh-CN" => Some(0),
        "zh-TW" => Some(1),
        "en" => Some(2),
        "ja" => Some(3),
        _ => None,
    }
}

pub fn load(storage: &Storage) -> Result<String, String> {
    let language: String = storage
        .with_connection(|db| {
            db.query_row(
                "SELECT language FROM interface_language WHERE singleton=1",
                [],
                |row| row.get(0),
            )
        })
        .map_err(|_| "interface_language_load_failed")?;
    if language_index(&language).is_none() {
        return Err("interface_language_load_failed".into());
    }
    Ok(language)
}

pub fn save(storage: &Storage, language: &str) -> Result<(), String> {
    if language_index(language).is_none() {
        return Err("invalid_interface_language".into());
    }
    storage.with_transaction(|tx| {
        tx.execute("INSERT INTO interface_language(singleton,language) VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET language=excluded.language", [language])?;
        Ok(())
    }).map_err(|_| "interface_language_save_failed".into())
}

/// Managed by each app instance; synthetic tests never share a process-global locale.
pub struct NativeLanguage(AtomicU8);
impl NativeLanguage {
    pub fn new(language: &str) -> Self {
        Self(AtomicU8::new(language_index(language).unwrap_or(0)))
    }
    pub fn set(&self, language: &str) {
        if let Some(index) = language_index(language) {
            self.0.store(index, Ordering::Release);
        }
    }
    pub fn text(&self, key: NativeText) -> &'static str {
        let index = self.0.load(Ordering::Acquire) as usize;
        key.translations()[index]
    }
}

#[derive(Clone, Copy)]
pub enum NativeText {
    Open,
    Settings,
    Exit,
    MainTitle,
    PlayerTitle,
    ChooseLibraryRoot,
    ChoosePhoto,
    Photos,
    ChooseActorImportRoot,
    ChooseActorExportDestination,
    ChooseDiagnosticDestination,
    ChooseSubtitle,
    Subtitles,
    AudioTrack,
}
impl NativeText {
    fn translations(self) -> [&'static str; 4] {
        match self {
            Self::Open => ["打开", "開啟", "Open", "開く"],
            Self::Settings => ["设置", "設定", "Settings", "設定"],
            Self::Exit => ["退出", "結束", "Exit", "終了"],
            Self::MainTitle => [
                "THE ARCHIVE  私人影像收藏室",
                "THE ARCHIVE  私人影像收藏室",
                "THE ARCHIVE  Private film collection",
                "THE ARCHIVE  プライベート映像コレクション",
            ],
            Self::PlayerTitle => [
                "TheArchive · 播放器",
                "TheArchive · 播放器",
                "TheArchive · Player",
                "TheArchive · プレーヤー",
            ],
            Self::ChooseLibraryRoot => [
                "选择片库根目录",
                "選擇媒體庫根目錄",
                "Choose library root folder",
                "ライブラリーのルートフォルダーを選択",
            ],
            Self::ChoosePhoto => [
                "选择演员照片",
                "選擇演員照片",
                "Choose actor photo",
                "出演者の写真を選択",
            ],
            Self::Photos => ["照片", "照片", "Photos", "写真"],
            Self::ChooseActorImportRoot => [
                "选择演员资料根目录",
                "選擇演員資料根目錄",
                "Choose actor information root folder",
                "出演者情報のルートフォルダーを選択",
            ],
            Self::ChooseActorExportDestination => [
                "选择演员资料导出位置",
                "選擇演員資料匯出位置",
                "Choose actor information export folder",
                "出演者情報のエクスポート先を選択",
            ],
            Self::ChooseDiagnosticDestination => [
                "选择诊断日志保存目录",
                "選擇診斷記錄儲存目錄",
                "Choose diagnostic log folder",
                "診断ログの保存先を選択",
            ],
            Self::ChooseSubtitle => ["选择字幕", "選擇字幕", "Choose subtitles", "字幕を選択"],
            Self::Subtitles => ["字幕", "字幕", "Subtitles", "字幕"],
            Self::AudioTrack => ["音轨", "音軌", "Audio track", "音声トラック"],
        }
    }
}

pub(crate) fn text(app: &tauri::AppHandle, key: NativeText) -> &'static str {
    app.try_state::<NativeLanguage>()
        .map(|state| state.text(key))
        .unwrap_or_else(|| key.translations()[0])
}

/// Caller retains the storage mutex through publication so save/restore/reset
/// cannot publish an older language after a newer committed operation.
/// Native presentation errors never turn a committed database save into a failure.
pub(crate) fn publish_committed(app: &tauri::AppHandle, language: &str) {
    app.state::<NativeLanguage>().set(language);
    if let Err(error) = crate::window_lifecycle::sync_tray_language(app) {
        crate::diagnostics::record_error("window.tray", &error);
    }
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.set_title(text(app, NativeText::MainTitle)) {
            crate::diagnostics::record_error("window.title", &error);
        }
    }
    if let Some(window) = app.get_webview_window("player") {
        if let Err(error) = window.set_title(text(app, NativeText::PlayerTitle)) {
            crate::diagnostics::record_error("window.title", &error);
        }
    }
    if let Err(error) = app.emit("interface-language-changed", language) {
        crate::diagnostics::record_error("interface_language.event", &error);
    }
}

pub(crate) fn publish_stored(app: &tauri::AppHandle, storage: &Storage) {
    match load(storage) {
        Ok(language) => publish_committed(app, &language),
        Err(error) => crate::diagnostics::record_error(
            "interface_language.load",
            &std::io::Error::other(error),
        ),
    }
}
