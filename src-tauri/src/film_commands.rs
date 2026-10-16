use crate::ready_storage;
use tauri::Manager;

#[tauri::command]
pub async fn set_film_code(
    app: tauri::AppHandle,
    film_id: String,
    value: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let s = state.lock().map_err(|_| "storage_unavailable")?;
        crate::catalog::set_code(&s, &film_id, &value).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "film_save_failed".to_owned())?
}
#[tauri::command]
pub async fn load_version_cover_thumbnail(
    app: tauri::AppHandle,
    film_id: String,
    video_id: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let _image_read = state.begin_image_read()?;
        let s = state
            .lock()
            .map_err(|_| "cover_unavailable")?
            .clone();
        app.state::<crate::image_session::ImageSession>().read_owned(&film_id, &crate::covers::request_key(&s,&film_id,Some(&video_id)), || crate::covers::cached_version_thumbnail(&s, &film_id, &video_id, true))
            .map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|_| "cover_load_failed".to_owned())?
}
