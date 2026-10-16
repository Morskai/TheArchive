use crate::{ready_storage, storage::Storage, translation, translation_credentials as credentials,
    translation_job::{BatchRequests, TranslationJobStatus, TranslationSession},
    translation_provider::{HttpTranslationProvider, TranslateProvider}};
use std::collections::BTreeSet;
use tauri::Manager;

#[derive(serde::Serialize)]
pub struct SettingsState {
    settings: translation::TranslationSettings,
    key_configured: bool,
    stats: translation::TranslationStats,
}

async fn with_storage<T: Send + 'static>(app: tauri::AppHandle,
    action: impl FnOnce(&Storage) -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        if crate::reset::recovery_pending(storage.data_directory()) { return Err("reset_recovery_required".into()); }
        action(&storage)
    }).await.map_err(|_| "translation_failed".to_owned())?
}

fn settings_state(storage: &Storage) -> Result<SettingsState, String> {
    let settings = translation::load_settings(storage)?;
    let key_configured = credentials::has_key(&settings.provider)?;
    Ok(SettingsState { settings, key_configured, stats: translation::stats(storage)? })
}

#[tauri::command]
pub async fn load_translation_settings(app: tauri::AppHandle) -> Result<SettingsState, String> {
    with_storage(app, settings_state).await
}

#[tauri::command]
pub async fn save_translation_settings(app: tauri::AppHandle,
    settings: translation::TranslationSettings) -> Result<SettingsState, String> {
    with_storage(app, move |storage| {
        translation::save_settings(storage, &settings)?;
        settings_state(storage)
    }).await
}

#[tauri::command]
pub async fn load_translation_key(app: tauri::AppHandle, provider: String) -> Result<Option<String>, String> {
    // Explicit settings input only. Secrets remain outside the database, backups and logs.
    with_storage(app, move |_storage| credentials::read_key(&provider)).await
}

#[tauri::command]
pub async fn save_translation_key(app: tauri::AppHandle, provider: String, key: String) -> Result<(), String> {
    with_storage(app, move |_storage| credentials::save_key(&provider, &key)).await
}

#[tauri::command]
pub async fn delete_translation_key(app: tauri::AppHandle, provider: String) -> Result<(), String> {
    with_storage(app, move |_storage| credentials::delete_key(&provider)).await
}

#[tauri::command]
pub async fn test_translation_connection(app: tauri::AppHandle, provider: String,
    diagnostic_operation_id: Option<String>,
) -> Result<(), String> {
    crate::diagnostics::spawn_blocking("test_translation_connection", diagnostic_operation_id, move || {
        let settings = {
            let state = ready_storage(&app)?;
            let storage = state.lock().map_err(|_| "storage_unavailable")?;
            translation::load_settings(&storage)?
        };
        let key = credentials::read_key(&provider)?.ok_or("translation_key_missing")?;
        HttpTranslationProvider::with_settings(&translation::TranslationSettings { provider, ..settings }, key)?.test_connection()
    }).await.map_err(|_| "translation_failed".to_owned())?
}

#[tauri::command]
pub async fn preview_title_translations(app: tauri::AppHandle) -> Result<translation::TranslationPreview, String> {
    with_storage(app, translation::preview).await
}

#[tauri::command]
pub async fn edit_film_title(app: tauri::AppHandle,film_id:String,expected_original:String,expected_title:String,text:String)->Result<crate::catalog::CatalogFilm,String> {
    with_storage(app,move |storage| translation::edit_title(storage,&film_id,&expected_original,&expected_title,&text)).await
}

#[tauri::command]
pub async fn clear_translation_memory(app: tauri::AppHandle, expected_count: usize) -> Result<usize, String> {
    let handle = app.clone();
    with_storage(app, move |storage| {
        if translation::stats(storage)?.memory != expected_count { return Err("translation_memory_changed".into()); }
        handle.state::<TranslationSession>().cancel();
        translation::clear_memory_expected(storage, expected_count)
    }).await
}

#[tauri::command]
pub fn translation_job_status(session: tauri::State<'_, TranslationSession>) -> TranslationJobStatus { session.status() }

#[tauri::command]
pub fn cancel_title_translation(session: tauri::State<'_, TranslationSession>) -> TranslationJobStatus { session.cancel() }

#[tauri::command]
pub async fn start_title_translation(app: tauri::AppHandle, film_ids: Vec<String>, force: bool, diagnostic_operation_id: Option<String>) -> Result<TranslationJobStatus, String> {
    let ids = film_ids.into_iter().collect::<BTreeSet<_>>();
    if ids.is_empty() || ids.len() > 10_000 { return Err("translation_invalid_selection".into()); }
    let starting = app.clone();
    let total = ids.len();
    let (settings, token, report, initial) = with_storage(app.clone(), move |storage| {
        let settings = translation::load_settings(storage)?;
        if !settings.enabled { return Err("translation_disabled".into()); }
        let session = starting.state::<TranslationSession>();
        let (token, report) = session.begin_report(total)?;
        Ok((settings, token, report, session.status()))
    }).await?;
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || run_batch(handle, token, ids, force, settings, report, diagnostic_operation_id));
    Ok(initial)
}

// Each database phase uses the existing application lock. The external request never holds it.
fn run_batch(app: tauri::AppHandle, token: u64, ids: BTreeSet<String>, force: bool,
    settings: translation::TranslationSettings, report: crate::translation_job::TranslationReport, diagnostic_operation_id: Option<String>) {
    let session = app.state::<TranslationSession>();
    struct Finish<'a> { session: &'a TranslationSession, token: u64, report: crate::translation_job::TranslationReport, log: crate::diagnostics::Scope }
    impl Drop for Finish<'_> {
        fn drop(&mut self) {
            let status = self.session.finish_report(self.token, &self.report);
            let s = &status;
            let counts = std::collections::BTreeMap::from([
                ("total".into(), s.total as u64), ("completed".into(), s.completed as u64),
                ("succeeded".into(), s.succeeded as u64), ("failed".into(), s.failed as u64),
                ("reused".into(), s.reused as u64), ("api_requests".into(), s.api_requests as u64),
                ("cancelled".into(), u64::from(s.cancelled)),
            ]);
            let error = if std::thread::panicking() { Some("rust_panic") }
                else if status.cancelled { Some("translation_cancelled") }
                else { status.last_error.as_deref() };
            self.log.finish(error, counts);
        }
    }
    let _finish = Finish { session: &session, token, report, log: crate::diagnostics::Scope::start("translation.batch", diagnostic_operation_id) };
    let mut provider: Option<HttpTranslationProvider> = None;
    let mut cache = BatchRequests::default();
    for id in ids {
        if !session.active(token) { break; }
        let prepared = (|| {
            let state = ready_storage(&app)?;
            let storage = state.lock().map_err(|_| "storage_unavailable")?;
            if !session.active(token) { return Err("translation_cancelled".into()); }
            translation::prepare_translation(&storage, &id, force)
        })();
        let prepared = match prepared {
            Ok(Some(prepared)) => prepared,
            Ok(None) => { session.record(token, true, true, None); continue; }
            Err(error) => { session.record(token, false, false, Some(error)); continue; }
        };
        let (result, reused) = cache.translate(&prepared.body, || {
                if provider.is_none() {
                    let key = credentials::read_key(&settings.provider)?.ok_or("translation_key_missing")?;
                    provider = Some(HttpTranslationProvider::with_settings(&settings, key)?);
                }
                if !session.active(token) { return Err("translation_cancelled".into()); }
                session.requested(token);
                provider.as_ref().ok_or("translation_failed")?.translate(&prepared.body)
        });
        // Restore and both clear actions cancel under the same storage lock before modifying data.
        let result = result.and_then(|translated| {
            let state = ready_storage(&app)?;
            let storage = state.lock().map_err(|_| "storage_unavailable")?;
            session.with_active(token, || translation::save_translation(&storage, &prepared, &settings.provider,
                &translated.text, translated.source_language.as_deref().unwrap_or("")))
        });
        let stop = matches!(result.as_ref().err().map(String::as_str), Some(
            "translation_auth" | "translation_quota" | "translation_rate_limit" |
            "translation_network" | "translation_timeout" | "translation_server" |
            "translation_key_missing" | "translation_credentials_error" | "translation_credentials_unsupported"));
        session.record(token, result.is_ok(), result.is_ok() && reused, result.err());
        if stop { break; }
    }
}
