use crate::diagnostics;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub fn write_diagnostic_event(event: diagnostics::Event) {
    // Backend origin and typed details are reserved for trusted Rust call sites.
    let mut event = event;
    event.origin = "frontend".into();
    event.detail = None;
    diagnostics::record(event);
}
#[tauri::command]
pub fn diagnostic_log_status() -> diagnostics::LogStatus {
    diagnostics::status()
}

#[tauri::command]
pub async fn open_diagnostic_directory() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let path = diagnostics::log_directory().ok_or("diagnostic_unavailable")?;
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            std::process::Command::new("explorer.exe")
                .arg(path)
                .creation_flags(0x08000000)
                .spawn()
                .map_err(|_| "diagnostic_open_failed")?;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Err("diagnostic_open_unsupported")
        }
    })
    .await
    .map_err(|_| "diagnostic_open_failed".to_owned())?
    .map_err(str::to_owned)
}

#[tauri::command]
pub async fn export_diagnostic_logs(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(directory) = app
            .dialog()
            .file()
            .set_title(crate::interface_language::text(&app, crate::interface_language::NativeText::ChooseDiagnosticDestination))
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let directory = directory
            .into_path()
            .map_err(|_| "diagnostic_export_failed")?;
        let destination =
            diagnostics::export_to(&directory).map_err(|_| "diagnostic_export_failed")?;
        Ok(Some(destination.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|_| "diagnostic_export_failed".to_owned())?
    .map_err(str::to_owned)
}
