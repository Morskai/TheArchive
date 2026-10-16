mod edit_clipboard;
pub mod diagnostics;
mod diagnostic_commands;
pub mod actors;
pub mod actor_tags;
pub mod image_session;
pub mod reset;
pub mod actor_management;
pub mod application_options;
pub mod window_lifecycle;
#[cfg(windows)]
pub mod single_instance;
mod artwork;
pub mod catalog;
pub mod covers;
pub mod directory_roles;
mod folder_groups;
pub mod inventory;
pub mod actor_info;
pub mod lifecycle;
pub mod display_preferences;
pub mod interface_language;
pub mod observation;
pub mod playback;
pub mod mpv;
pub mod embedded_player;
pub mod video_metadata;
pub mod problem_folders;
pub mod problems;
pub mod proposal;
pub mod source_scan;
pub mod scan_rules;
pub mod storage;
pub mod storage_session;
pub mod translation_job;
pub mod translation;
pub mod translation_provider;
pub mod translation_credentials;
mod translation_commands;
mod film_commands;

use storage_session::StorageSession;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

struct ReadyStorage<'a> {
    state: tauri::State<'a,StorageSession>,
    generation: u64,
}
impl std::ops::Deref for ReadyStorage<'_> {
    type Target = StorageSession;
    fn deref(&self) -> &StorageSession { &self.state }
}
impl ReadyStorage<'_> {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_,storage::Storage>, &'static str> {
        self.state.lock_at(self.generation)
    }
    fn exclusive(&self) -> Result<storage_session::ExclusiveStorage<'_>, &'static str> {
        self.state.exclusive_at(self.generation)
    }
}

fn ready_storage(
    app: &tauri::AppHandle,
) -> Result<ReadyStorage<'_>, &'static str> {
    let state = app.state::<StorageSession>();
    let generation;
    {
        let storage = state.lock()?;
        if reset::recovery_pending(storage.data_directory()) { return Err("reset_recovery_required"); }
        app.state::<lifecycle::StartupSession>()
            .initialize(&storage)
            .map_err(|_| "startup_failed")?;
        generation = state.generation();
    }
    Ok(ReadyStorage { state, generation })
}

// Keep each existing command's lock error while sharing the startup barrier.
fn ready_storage_with_lock_error<'a>(
    app: &'a tauri::AppHandle,
    lock_error: &'static str,
) -> Result<ReadyStorage<'a>, &'static str> {
    ready_storage(app).map_err(|error| {
        if error == "storage_unavailable" {
            lock_error
        } else {
            error
        }
    })
}

#[tauri::command]
fn window_titlebar_action(window: tauri::Window, action: String) -> Result<bool, String> {
    perform_titlebar_action(window, action)
}
pub fn perform_titlebar_action(window: tauri::Window, action: String) -> Result<bool, String> {
    let result = match action.as_str() {
        "state" => Ok(()),
        "minimize" => window.minimize(),
        "maximize" => {
            if window.is_maximized().map_err(|_| "window_failed")? {
                window.unmaximize()
            } else {
                window.maximize()
            }
        },
        "close" => { if window.label() == "player" { window.close() } else { window_lifecycle::request(window.app_handle(),false); Ok(()) } },
        "drag" => window.start_dragging(),
        _ => return Err("invalid_window_action".into()),
    };
    result.map_err(|_| "window_failed")?;
    window.is_maximized().map_err(|_| "window_failed".into())
}

#[tauri::command]
async fn initialize_library_session(
    app: tauri::AppHandle,
    diagnostic_operation_id: Option<String>,
) -> Result<lifecycle::StartupSummary, String> {
    crate::diagnostics::spawn_blocking("initialize_library_session", diagnostic_operation_id, move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        app.state::<lifecycle::StartupSession>()
            .initialize(&storage)
    })
    .await
    .map_err(|_| "startup_failed".to_owned())?
}

#[tauri::command]
async fn load_interface_preferences(
    app: tauri::AppHandle,
) -> Result<lifecycle::InterfacePreferences, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "preferences_load_failed")?;
        lifecycle::load_preferences(&storage)
    })
    .await
    .map_err(|_| "preferences_load_failed".to_owned())?
}

#[tauri::command]
async fn save_interface_preferences(
    app: tauri::AppHandle,
    preferences: lifecycle::InterfacePreferences,
    preserve_ratios: Option<bool>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "preferences_save_failed")?;
        if preserve_ratios.unwrap_or(false) { display_preferences::save_appearance(&storage, &preferences) }
        else { lifecycle::save_preferences(&storage, &preferences) }
    })
    .await
    .map_err(|_| "preferences_save_failed".to_owned())?
}

#[tauri::command]
async fn load_display_preferences(app: tauri::AppHandle) -> Result<display_preferences::DisplayPreferences,String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state=ready_storage(&app)?; let storage=state.lock().map_err(|_|"preferences_load_failed")?;
        display_preferences::load(&storage)
    }).await.map_err(|_|"preferences_load_failed".to_owned())?
}
#[tauri::command]
async fn save_film_cover_ratios(app: tauri::AppHandle, library_ids: Vec<String>, ratio: String) -> Result<(),String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state=ready_storage(&app)?; let storage=state.lock().map_err(|_|"preferences_save_failed")?;
        display_preferences::save_film_ratios(&storage,&library_ids,&ratio)
    }).await.map_err(|_|"preferences_save_failed".to_owned())?
}
#[tauri::command]
async fn save_actor_photo_ratio(app: tauri::AppHandle, ratio: String) -> Result<(),String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state=ready_storage(&app)?; let storage=state.lock().map_err(|_|"preferences_save_failed")?;
        display_preferences::save_actor_ratio(&storage,&ratio)
    }).await.map_err(|_|"preferences_save_failed".to_owned())?
}

#[tauri::command]
async fn clear_generated_cache(app: tauri::AppHandle,
    diagnostic_operation_id: Option<String>,
) -> Result<(), String> {
    crate::diagnostics::spawn_blocking("clear_generated_cache", diagnostic_operation_id, move || {
        let state = ready_storage(&app)?;
        let storage = state.exclusive().map_err(|_| "cache_clear_failed")?;
        lifecycle::clear_generated_cache(&storage)?;
        app.state::<image_session::ImageSession>().clear();
        Ok(())
    })
    .await
    .map_err(|_| "cache_clear_failed".to_owned())?
}

#[tauri::command]
async fn load_ignored_directory_names(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state=ready_storage(&app)?;
        let storage=state.lock().map_err(|_|"scan_rules_load_failed")?;
        scan_rules::load(&storage)
    }).await.map_err(|_|"scan_rules_load_failed".to_owned())?
}

#[tauri::command]
async fn save_ignored_directory_names(app: tauri::AppHandle, names: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state=ready_storage(&app)?;
        let storage=state.exclusive().map_err(|_|"scan_rules_save_failed")?;
        scan_rules::save(&storage,&names)
    }).await.map_err(|_|"scan_rules_save_failed".to_owned())?
}

#[tauri::command]
async fn clear_user_data(app: tauri::AppHandle,
    diagnostic_operation_id: Option<String>,
) -> Result<(), String> {
    crate::diagnostics::spawn_blocking("clear_user_data", diagnostic_operation_id, move || {
        let state = app.state::<StorageSession>();
        let storage = state.exclusive_for_reset().map_err(|_| "reset_recovery_required")?;
        app.state::<embedded_player::EmbeddedPlayer>().close(&app, None)?;
        app.state::<translation_job::TranslationSession>().cancel();
        let result = covers::with_generated_cache_lock(|| reset::restore_initial_state(&storage));
        let applied = result.is_ok() || matches!(result.as_ref(), Err(code) if code == "reset_cleanup_pending");
        if applied {
            state.reset_generation();
            interface_language::publish_stored(&app, &storage);
            app.state::<window_lifecycle::WindowLifecycle>().close_exits.store(false,std::sync::atomic::Ordering::Release);
            if let Some(tray)=app.tray_by_id("thearchive-tray") { let _=tray.set_visible(true); }
            app.state::<image_session::ImageSession>().clear();
            app.state::<lifecycle::StartupSession>().reset();
            app.state::<source_scan::SourceScanSession>().reset();
            app.state::<playback::PlaybackSession>().reset();
            app.state::<translation_job::TranslationSession>().reset();
        }
        drop(storage);
        if applied { let _ = sync_application_options(&app); }
        result
    })
    .await
    .map_err(|_| "reset_recovery_required".to_owned())?
}

#[tauri::command]
async fn load_library_problems(
    app: tauri::AppHandle,
) -> Result<Vec<problem_folders::ProblemWithDirectories>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "problems_load_failed")?;
        problem_folders::list(&storage)
    })
    .await
    .map_err(|_| "problems_load_failed".to_owned())?
}

#[tauri::command]
async fn open_problem_directory(
    app: tauri::AppHandle,
    problem_id: String,
    directory_index: usize,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        problem_folders::open_directory(&storage, &problem_id, directory_index)
    })
    .await
    .map_err(|_| "launch_failed".to_owned())?
}

#[tauri::command]
async fn mark_library_problems(
    app: tauri::AppHandle,
    issue_ids: Vec<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "problem_save_failed")?;
        problems::mark(&storage, &issue_ids)
    })
    .await
    .map_err(|_| "problem_save_failed".to_owned())?
}

#[tauri::command]
async fn restore_library_problems(app: tauri::AppHandle, issue_ids: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "problem_save_failed")?;
        problems::restore(&storage, &issue_ids)
    }).await.map_err(|_| "problem_save_failed".to_owned())?
}
#[tauri::command]
async fn clear_library_problems(app: tauri::AppHandle, issue_ids: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "problem_save_failed")?;
        problems::clear(&storage, &issue_ids)
    }).await.map_err(|_| "problem_save_failed".to_owned())?
}
#[derive(serde::Serialize)]
struct ActorCatalog {
    actors: Vec<actors::CatalogActor>,
    films: Vec<catalog::CatalogFilm>,
    tags: Vec<String>,
    #[serde(rename = "actorTags")]
    actor_tags: Vec<String>,
}

fn actor_catalog_on(storage: &storage::Storage) -> Result<ActorCatalog, String> {
    catalog::ensure_film_entries(storage).map_err(|error| {
        if error.to_string() == "protection_failed" { "protection_failed" } else { "actors_load_failed" }
    })?;
    let films = catalog::list_films(storage).map_err(|_| "actors_load_failed")?;
    let actors = actors::list_actors_with_films(storage, &films)?;
    Ok(ActorCatalog {
        actors,
        films,
        tags: catalog::list_tags(storage).map_err(|_| "actors_load_failed")?,
        actor_tags: actor_tags::list(storage)?,
    })
}

#[tauri::command]
async fn load_actor_catalog(app: tauri::AppHandle,
    diagnostic_operation_id: Option<String>,
) -> Result<ActorCatalog, String> {
    crate::diagnostics::spawn_blocking("load_actor_catalog", diagnostic_operation_id, move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "actors_load_failed")?;
        actor_catalog_on(&storage)
    })
    .await
    .map_err(|_| "actors_load_failed".to_owned())?
}

#[tauri::command]
async fn list_actor_tags(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?; let storage = state.lock().map_err(|_| "storage_unavailable")?;
        actor_tags::list(&storage)
    }).await.map_err(|_| "actor_tags_load_failed".to_owned())?
}
#[tauri::command]
async fn create_actor_tag(app: tauri::AppHandle, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?; let storage = state.lock().map_err(|_| "storage_unavailable")?;
        actor_tags::create(&storage, &name)
    }).await.map_err(|_| "actor_tags_save_failed".to_owned())?
}
#[tauri::command]
async fn rename_actor_tag(app: tauri::AppHandle, name: String, new_name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?; let storage = state.lock().map_err(|_| "storage_unavailable")?;
        actor_tags::rename(&storage, &name, &new_name)
    }).await.map_err(|_| "actor_tags_save_failed".to_owned())?
}
#[tauri::command]
async fn delete_actor_tag(app: tauri::AppHandle, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?; let storage = state.lock().map_err(|_| "storage_unavailable")?;
        actor_tags::delete(&storage, &name)
    }).await.map_err(|_| "actor_tags_save_failed".to_owned())?
}
#[tauri::command]
async fn reorder_actor_tags(app: tauri::AppHandle, names: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?; let storage = state.lock().map_err(|_| "storage_unavailable")?;
        actor_tags::reorder(&storage, &names)
    }).await.map_err(|_| "actor_tags_save_failed".to_owned())?
}
#[tauri::command]
async fn apply_actor_bulk_tag(app: tauri::AppHandle, actor_ids: Vec<String>, name: String, add: bool) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?; let storage = state.lock().map_err(|_| "storage_unavailable")?;
        actor_tags::apply_bulk_tag(&storage, &actor_ids, &name, add)
    }).await.map_err(|_| "actor_tags_save_failed".to_owned())?
}
#[tauri::command]
async fn delete_actors(app: tauri::AppHandle, actor_ids: Vec<String>) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?; let storage = state.exclusive().map_err(|_| "storage_unavailable")?;
        let count = actors::delete_actors(&storage, &actor_ids)?;
        app.state::<image_session::ImageSession>().release(&actor_ids.iter().map(|id| format!("actor:{id}")).collect::<Vec<_>>());
        Ok(count)
    }).await.map_err(|_| "actor_delete_failed".to_owned())?
}

#[cfg(test)]
mod shared_catalog_tests {
    #[test]
    fn shared_catalog_keeps_protection_failure_and_unlinked_observations() {
        let home = tempfile::tempdir().unwrap();
        let storage = crate::storage::Storage::open(&home.path().join("software")).unwrap();
        storage.with_connection(|db| {
            db.execute("INSERT INTO libraries(id,display_name) VALUES('synthetic','合成来源')", [])?;
            db.execute("INSERT INTO file_observations(id,library_id,observed_path,kind) VALUES('unlinked','synthetic','movie.mp4','video')", [])?;
            Ok(())
        }).unwrap();
        // Block only this test's independent protection directory.
        std::fs::write(storage.data_directory().join("protections"), b"synthetic blocker").unwrap();
        assert!(matches!(super::actor_catalog_on(&storage), Err(error) if error == "protection_failed"));
        storage.with_connection(|db| {
            assert_eq!(db.query_row::<Option<String>,_,_>("SELECT film_id FROM file_observations WHERE id='unlinked'", [], |row| row.get(0))?, None);
            assert_eq!(db.query_row::<usize,_,_>("SELECT COUNT(*) FROM films", [], |row| row.get(0))?, 0);
            Ok(())
        }).unwrap();
    }
}

#[tauri::command]
async fn set_actor_favorite(app: tauri::AppHandle, actor_id: String, favorite: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "actor_save_failed")?;
        actors::set_favorite(&storage, &actor_id, favorite)
    }).await.map_err(|_| "actor_save_failed".to_owned())?
}

#[tauri::command]
async fn list_cast_candidates(app: tauri::AppHandle, film_id: String) -> Result<Vec<actors::CastCandidate>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "cast_load_failed")?;
        actors::list_cast_candidates(&storage, &film_id)
    }).await.map_err(|_| "cast_load_failed".to_owned())?
}

#[tauri::command]
async fn add_film_cast_actor(app: tauri::AppHandle, film_id: String, actor_id: String) -> Result<Vec<catalog::CatalogActorLink>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "cast_save_failed")?;
        actors::add_film_cast_actor(&storage, &film_id, &actor_id)
    }).await.map_err(|_| "cast_save_failed".to_owned())?
}

#[tauri::command]
async fn remove_film_cast_actor(app: tauri::AppHandle, film_id: String, actor_id: String) -> Result<Vec<catalog::CatalogActorLink>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "cast_save_failed")?;
        actors::remove_film_cast_actor(&storage, &film_id, &actor_id)
    }).await.map_err(|_| "cast_save_failed".to_owned())?
}

#[tauri::command]
async fn open_library_source_directory(app: tauri::AppHandle, library_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        source_scan::open_source_directory(&storage, &library_id).map_err(|error| error.to_string())
    }).await.map_err(|_| "open_failed".to_owned())?
}

#[tauri::command]
async fn save_actor_profile(
    app: tauri::AppHandle,
    actor_id: String,
    profile: actors::Profile,
    aliases: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage_with_lock_error(&app, "actor_save_failed")?;
        let storage = state.lock().map_err(|_| "actor_save_failed")?;
        actors::save_details(&storage, &actor_id, &profile, aliases.as_deref())
    })
    .await
    .map_err(|_| "actor_save_failed".to_owned())?
}

#[tauri::command]
async fn load_actor_thumbnail(
    app: tauri::AppHandle,
    actor_id: String,
    large: bool,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let _image_read = state.begin_image_read()?;
        let storage = state
            .lock()
            .map_err(|_| "cover_unavailable")?
            .clone();
        app.state::<image_session::ImageSession>().read_owned(&format!("actor:{actor_id}"), &actors::request_key(&storage,&actor_id), || actors::thumbnail(&storage, &actor_id, large)).map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|_| "cover_load_failed".to_owned())?
}

#[tauri::command]
async fn load_film_actor_thumbnail(
    app: tauri::AppHandle,
    actor_id: String,
    film_id: String,
    video_id: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let _image_read = state.begin_image_read()?;
        let storage = state
            .lock()
            .map_err(|_| "cover_unavailable")?
            .clone();
        app.state::<image_session::ImageSession>().read_owned(&format!("actor:{actor_id}"), &actors::request_key(&storage,&actor_id), || actors::film_thumbnail(&storage, &actor_id, &film_id, &video_id))
            .map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|_| "cover_load_failed".to_owned())?
}

#[tauri::command]
async fn load_cover_thumbnail(
    app: tauri::AppHandle,
    film_id: String,
    large: bool,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let _image_read = state.begin_image_read()?;
        let storage = state
            .lock()
            .map_err(|_| "storage_unavailable")?
            .clone();
        app.state::<image_session::ImageSession>().read_owned(&film_id, &covers::request_key(&storage,&film_id,None), || covers::cached_thumbnail(&storage, &film_id, large))
            .map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|_| "cover_load_failed".to_owned())?
}


#[tauri::command]
async fn load_video_size(
    app: tauri::AppHandle,
    film_id: String,
    video_id: String,
) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "file_unavailable")?;
        playback::video_size(&storage, &film_id, &video_id)
    })
    .await
    .map_err(|_| "file_unavailable".to_owned())?
}

#[tauri::command]
async fn load_video_resolution(app:tauri::AppHandle,film_id:String,video_id:String)->Result<Option<video_metadata::VideoResolution>,String> {
    tauri::async_runtime::spawn_blocking(move || {
        let file={
            let state=ready_storage(&app)?;
            let storage=state.lock().map_err(|_|"file_unavailable")?;
            playback::current_video_path(&storage,&film_id,&video_id)?
        };
        // Property handlers may be slow: never hold the software database lock while reading them.
        Ok(video_metadata::dimensions(&file))
    }).await.map_err(|_|"file_unavailable".to_owned())?
}

#[tauri::command]
async fn load_directory_facts(
    app: tauri::AppHandle,
    film_id: String,
    video_id: String,
) -> Result<playback::DirectoryFacts, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "directory_unavailable")?;
        playback::directory_facts(&storage, &film_id, &video_id)
    })
    .await
    .map_err(|_| "directory_unavailable".to_owned())?
}

#[tauri::command]
async fn application_data_directory(
    app: tauri::AppHandle,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage_with_lock_error(&app, "settings_load_failed")?;
        let storage = state.lock().map_err(|_| "settings_load_failed")?;
        Ok(storage.data_directory().to_string_lossy().into_owned())
    })
    .await
    .map_err(|_| "settings_load_failed".to_owned())?
}

#[tauri::command]
async fn open_application_data_directory(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage_with_lock_error(&app, "directory_unavailable")?;
        let storage = state.lock().map_err(|_| "directory_unavailable")?;
        playback::open_data_directory(&storage)
    }).await.map_err(|_| "directory_open_failed".to_owned())?
}

#[tauri::command]
async fn load_player_settings(app: tauri::AppHandle) -> Result<playback::PlayerSettings, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "settings_load_failed")?;
        playback::load_settings(&storage)
    })
    .await
    .map_err(|_| "settings_load_failed".to_owned())?
}

#[tauri::command]
async fn save_player_settings(
    app: tauri::AppHandle,
    settings: playback::PlayerSettings,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "settings_save_failed")?;
        playback::save_settings(&storage, &settings)
    })
    .await
    .map_err(|_| "settings_save_failed".to_owned())?
}

#[tauri::command]
async fn launch_film_video(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    request_id: String,
    film_id: String,
    video_id: String,
    player_token: Option<u64>,
) -> Result<playback::LaunchOutcome, String> {
    perform_launch_film_video(app, window, request_id, film_id, video_id, player_token).await
}
pub async fn perform_launch_film_video(app: tauri::AppHandle, window: tauri::WebviewWindow, request_id: String, film_id: String, video_id: String, player_token: Option<u64>) -> Result<playback::LaunchOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "launch_failed")?;
        let display = player_token.map(|token| app.state::<embedded_player::EmbeddedPlayer>().active_display(token, window.label())).transpose()?;
        app.state::<playback::PlaybackSession>().launch_with_display(
            &storage,
            &request_id,
            &film_id,
            &video_id,
            display.as_deref(),
            |target| match target {
                playback::LaunchTarget::Internal { video, display } => {
                    if let Some(token) = player_token { app.state::<embedded_player::EmbeddedPlayer>().active_display(token, window.label())?; }
                    app.state::<embedded_player::EmbeddedPlayer>().open(&app, &storage, &film_id, &video_id, video, display, player_token.map(|token| (token, window.label())))
                },
                _ => playback::launch_external(target),
            },
        )
    })
    .await
    .map_err(|_| "launch_result_unknown".to_owned())?
}

#[tauri::command]
async fn open_film_folder(
    app: tauri::AppHandle,
    film_id: String,
    video_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "launch_failed")?;
        playback::open_folder(&storage, &film_id, &video_id)
    })
    .await
    .map_err(|_| "launch_failed".to_owned())?
}

#[tauri::command]
async fn set_preferred_video(
    app: tauri::AppHandle,
    film_id: String,
    video_id: String,
    preferred: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "preferred_save_failed")?;
        playback::set_preferred(&storage, &film_id, &video_id, preferred)
    })
    .await
    .map_err(|_| "preferred_save_failed".to_owned())?
}

#[tauri::command]
async fn load_launch_history(app: tauri::AppHandle) -> Result<playback::History, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "history_load_failed")?;
        playback::load_history(&storage)
    })
    .await
    .map_err(|_| "history_load_failed".to_owned())?
}

#[tauri::command]
async fn clear_launch_history(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "history_clear_failed")?;
        playback::clear_history(&storage)
    })
    .await
    .map_err(|_| "history_clear_failed".to_owned())?
}

fn scan_error(error: impl std::fmt::Display) -> String {
    let code = error.to_string();
    match code.as_str() {
        "root_unavailable" | "wrong_root" | "already_bound" | "range_overlap" | "not_parent"
        | "source_paused" | "source_inactive" | "source_missing" | "source_changed"
        | "preview_expired" | "incomplete" | "scan_changed" | "protection_failed"
        | "import_failed" | "state_failed" => code,
        _ => "scan_failed".into(),
    }
}

#[tauri::command]
async fn load_real_films(app: tauri::AppHandle,
    diagnostic_operation_id: Option<String>,
) -> Result<Vec<catalog::CatalogFilm>, String> {
    crate::diagnostics::spawn_blocking("load_real_films", diagnostic_operation_id, move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "catalog_failed")?;
        catalog::ensure_film_entries(&storage).map_err(|error| {
            if error.to_string() == "protection_failed" {
                "protection_failed"
            } else {
                "catalog_failed"
            }
        })?;
        catalog::list_films(&storage).map_err(|_| "catalog_failed")
    })
    .await
    .map_err(|_| "catalog_failed".to_owned())?
    .map_err(str::to_owned)
}

#[tauri::command]
async fn list_real_tags(
    app: tauri::AppHandle,
) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage_with_lock_error(&app, "catalog_unavailable")?;
        let storage = state.lock().map_err(|_| "catalog_unavailable")?;
        catalog::list_tags(&storage).map_err(|_| "catalog_failed".into())
    })
    .await
    .map_err(|_| "catalog_failed".to_owned())?
}

#[tauri::command]
async fn create_real_tag(
    app: tauri::AppHandle,
    name: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage_with_lock_error(&app, "catalog_unavailable")?;
        let storage = state.lock().map_err(|_| "catalog_unavailable")?;
        catalog::create_tag(&storage, &name).map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "catalog_failed".to_owned())?
}

#[tauri::command]
async fn reorder_real_tags(app: tauri::AppHandle, names: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "catalog_unavailable")?;
        catalog::reorder_tags(&storage, &names).map_err(|error| error.to_string())
    }).await.map_err(|_| "catalog_failed".to_owned())?
}

#[tauri::command]
async fn delete_real_tag(app: tauri::AppHandle, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "catalog_unavailable")?;
        catalog::delete_tag(&storage, &name).map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "catalog_failed".to_owned())?
}

#[tauri::command]
async fn rename_real_tag(app: tauri::AppHandle, name: String, new_name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "catalog_unavailable")?;
        catalog::rename_tag(&storage, &name, &new_name).map_err(|error| error.to_string())
    }).await.map_err(|_| "catalog_failed".to_owned())?
}

#[tauri::command]
async fn set_real_favorite(
    app: tauri::AppHandle,
    film_id: String,
    favorite: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "catalog_unavailable")?;
        catalog::set_favorite(&storage, &film_id, favorite).map_err(|_| "catalog_failed".to_owned())
    })
    .await
    .map_err(|_| "catalog_failed".to_owned())?
}

#[tauri::command]
async fn set_real_rating(app: tauri::AppHandle, film_id: String, stars: Option<u8>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "catalog_failed")?;
        catalog::set_rating(&storage, &film_id, stars).map_err(|_| "catalog_failed".into())
    }).await.map_err(|_| "catalog_failed".to_owned())?
}

#[tauri::command]
async fn set_real_film_tags(
    app: tauri::AppHandle,
    film_id: String,
    names: Vec<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage_with_lock_error(&app, "catalog_unavailable")?;
        let storage = state.lock().map_err(|_| "catalog_unavailable")?;
        catalog::set_film_tags(&storage, &film_id, &names).map_err(|_| "catalog_failed".into())
    })
    .await
    .map_err(|_| "catalog_failed".to_owned())?
}

#[tauri::command]
async fn apply_real_bulk_tag(
    app: tauri::AppHandle,
    film_ids: Vec<String>,
    name: String,
    add: bool,
) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage_with_lock_error(&app, "catalog_unavailable")?;
        let storage = state.lock().map_err(|_| "catalog_unavailable")?;
        catalog::apply_bulk_tag(&storage, &film_ids, &name, add).map_err(|_| "catalog_failed".into())
    })
    .await
    .map_err(|_| "catalog_failed".to_owned())?
}

#[tauri::command]
async fn delete_real_films(app: tauri::AppHandle, film_ids: Vec<String>) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.exclusive().map_err(|_| "storage_unavailable")?;
        let count = catalog::delete_films(&storage, &film_ids)?;
        app.state::<image_session::ImageSession>().release(&film_ids);
        Ok(count)
    }).await.map_err(|_| "film_delete_failed".to_owned())?
}

#[tauri::command]
async fn create_data_backup(
    app: tauri::AppHandle,
    diagnostic_operation_id: Option<String>,
) -> Result<String, String> {
    crate::diagnostics::spawn_blocking("create_data_backup", diagnostic_operation_id, move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        storage.create_backup().map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "storage_unavailable".to_owned())?
}

#[tauri::command]
async fn list_data_backups(
    app: tauri::AppHandle,
) -> Result<Vec<storage::BackupInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        storage.list_backups().map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "storage_unavailable".to_owned())?
}

#[tauri::command]
async fn restore_data_backup(
    app: tauri::AppHandle,
    backup_id: String,
    diagnostic_operation_id: Option<String>,
) -> Result<(), String> {
    crate::diagnostics::spawn_blocking("restore_data_backup", diagnostic_operation_id, move || {
        let state = ready_storage(&app)?;
        let storage = state.exclusive().map_err(|_| "storage_unavailable")?;
        app.state::<translation_job::TranslationSession>().cancel();
        storage.restore_backup(&backup_id).map_err(|e| e.to_string())?;
        interface_language::publish_stored(&app, &storage);
        app.state::<image_session::ImageSession>().clear();
        app.state::<lifecycle::StartupSession>().reset();
        app.state::<source_scan::SourceScanSession>().reset();
        app.state::<playback::PlaybackSession>().reset();
        drop(storage);
        if let Err(error) = sync_application_options(&app) {
            diagnostics::record_error("application_options.restore", &std::io::Error::other(error));
        }
        Ok(())
    })
    .await
    .map_err(|_| "storage_unavailable".to_owned())?
}

#[tauri::command]
async fn delete_data_backup(app: tauri::AppHandle, backup_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "backup_delete_failed")?;
        storage
            .delete_backup(&backup_id)
            .map_err(|_| "backup_delete_failed")
    })
    .await
    .map_err(|_| "backup_delete_failed".to_owned())?
    .map_err(str::to_owned)
}

#[tauri::command]
async fn list_library_sources(
    app: tauri::AppHandle,
) -> Result<Vec<source_scan::LibrarySource>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        source_scan::list_sources(&storage).map_err(|_| "source_list_failed".into())
    })
    .await
    .map_err(|_| "source_list_failed".to_owned())?
}

#[tauri::command]
async fn choose_library_root(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(interface_language::text(&app, interface_language::NativeText::ChooseLibraryRoot))
            .blocking_pick_folder()
            .map(|folder| {
                folder
                    .into_path()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .transpose()
            .map_err(|_| "folder_selection_failed".to_owned())
    })
    .await
    .map_err(|_| "folder_selection_failed".to_owned())?
}

#[tauri::command]
async fn preview_library_source(
    app: tauri::AppHandle,
    root: String,
    library_id: Option<String>,
    diagnostic_operation_id: Option<String>,
) -> Result<source_scan::PreviewSummary, String> {
    crate::diagnostics::spawn_blocking("preview_library_source", diagnostic_operation_id, move || {
        let storage = ready_storage(&app)?;
        let session = app.state::<source_scan::SourceScanSession>();
        session
            .preview_on_at(&storage, storage.generation, std::path::Path::new(&root), library_id.as_deref(), inventory::inventory_with_rules)
            .map_err(scan_error)
    })
    .await
    .map_err(|_| "preview_failed".to_owned())?
}

#[tauri::command]
async fn change_library_source_state(
    app: tauri::AppHandle,
    library_id: String,
    action: String,
    diagnostic_operation_id: Option<String>,
) -> Result<(), String> {
    crate::diagnostics::spawn_blocking("change_library_source_state", diagnostic_operation_id, move || {
        let storage = ready_storage(&app)?;
        let storage = storage.exclusive().map_err(|_| "storage_unavailable")?;
        source_scan::change_source_state(&storage, &library_id, &action).map_err(scan_error)?;
        Ok(())
    })
    .await
    .map_err(|_| "state_failed".to_owned())?
}

#[tauri::command]
fn cancel_library_preview(
    session: tauri::State<'_, source_scan::SourceScanSession>,
    token: String,
) {
    session.cancel(&token);
}

#[tauri::command]
async fn confirm_library_source(
    app: tauri::AppHandle,
    token: String,
    diagnostic_operation_id: Option<String>,
) -> Result<lifecycle::SourceRefreshResult, String> {
    crate::diagnostics::spawn_blocking("confirm_library_source", diagnostic_operation_id, move || {
        let storage = ready_storage(&app)?;
        let session = app.state::<source_scan::SourceScanSession>();
        let result=lifecycle::confirm_source_refresh_on_at(&storage, storage.generation, &session, &token, inventory::inventory_with_rules).map_err(scan_error)?;
        app.state::<image_session::ImageSession>().release(&result.successful_artwork_keys);
        Ok(result)
    })
    .await
    .map_err(|_| "import_failed".to_owned())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diagnostics::install_panic_hook();
    let context = tauri::generate_context!();
    #[cfg(windows)]
    let startup_guard = single_instance::StartupGuard::acquire(&context.config().identifier)
        .expect("could not acquire application startup mutex");
    let builder = tauri::Builder::default();
    #[cfg(windows)]
    let builder = builder.plugin(single_instance::init());
    let app = builder
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window,event| {
            if let tauri::WindowEvent::CloseRequested { api,.. }=event {
                if window.label() == "player" {
                    api.prevent_close();
                    let handle = window.app_handle().clone();
                    let owner = handle.state::<embedded_player::EmbeddedPlayer>().detached_owner();
                    if let Some(token) = owner { tauri::async_runtime::spawn_blocking(move || { let _ = handle.state::<embedded_player::EmbeddedPlayer>().close(&handle, Some((token, "player"))); }); }
                    return;
                }
                if !window.app_handle().state::<window_lifecycle::WindowLifecycle>().closing.load(std::sync::atomic::Ordering::Acquire) {
                    api.prevent_close();window_lifecycle::request(window.app_handle(),false);
                }
            }
        })
        .setup(|app| {
            if let Ok(log_dir) = app.path().app_log_dir() { let _ = diagnostics::initialize(&log_dir); }
            let mut startup_log = diagnostics::Scope::start("startup", None);
            let data_dir = app.path().app_data_dir()?;
            let storage = storage::Storage::open(&data_dir)
                .map_err(|error| { diagnostics::startup_failure(error.as_ref()); std::io::Error::other(error.to_string()) })?;
            let initial_language = interface_language::load(&storage).unwrap_or_else(|error| {
                diagnostics::record_error("interface_language.load", &std::io::Error::other(error));
                interface_language::DEFAULT_LANGUAGE.into()
            });
            app.manage(StorageSession::new(storage));
            app.manage(lifecycle::StartupSession::default());
            app.manage(image_session::ImageSession::default());
            app.manage(source_scan::SourceScanSession::new());
            app.manage(playback::PlaybackSession::default());
            app.manage(embedded_player::EmbeddedPlayer::default());
            app.manage(translation_job::TranslationSession::default());
            app.manage(window_lifecycle::WindowLifecycle::default());
            app.manage(interface_language::NativeLanguage::new(&initial_language));
            if let Err(error) = window_lifecycle::setup_tray(app.handle()) { diagnostics::record_error("window.tray", &error); }
            interface_language::publish_committed(app.handle(), &initial_language);
            let handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let mut log = diagnostics::Scope::start("initialize_library_session", None);
                let _ = ready_storage(&handle);
                let result = sync_application_options(&handle);
                log.finish(result.as_ref().err().map(String::as_str), std::collections::BTreeMap::new());
            });
            startup_log.finish(None, std::collections::BTreeMap::from([("schema".into(), storage::SCHEMA_VERSION as u64)]));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            diagnostic_commands::write_diagnostic_event,
            diagnostic_commands::diagnostic_log_status,
            diagnostic_commands::open_diagnostic_directory,
            diagnostic_commands::export_diagnostic_logs,
            load_application_options, save_application_options, resolve_window_close,
            translation_commands::load_translation_settings,
            translation_commands::save_translation_settings,
            translation_commands::save_translation_key,
            translation_commands::load_translation_key,
            translation_commands::delete_translation_key,
            translation_commands::test_translation_connection,
            translation_commands::preview_title_translations,
            translation_commands::clear_translation_memory,
            translation_commands::edit_film_title,
            translation_commands::translation_job_status,
            translation_commands::cancel_title_translation,
            translation_commands::start_title_translation,
            window_titlebar_action,
            initialize_library_session,
            load_interface_preferences,
            load_interface_language, save_interface_language,
            load_display_preferences, save_film_cover_ratios, save_actor_photo_ratio,
            save_interface_preferences,
            load_ignored_directory_names,
            save_ignored_directory_names,
            clear_generated_cache,
            clear_user_data,
            load_library_problems,
            open_problem_directory,
            mark_library_problems,
            restore_library_problems,
            clear_library_problems,
            load_actor_catalog,
            list_actor_tags, create_actor_tag, rename_actor_tag, delete_actor_tag,
            reorder_actor_tags, apply_actor_bulk_tag, delete_actors,
            actor_management::choose_actor_photo,
            actor_management::choose_actor_import_root,
            actor_management::choose_actor_export_destination,
            actor_management::save_managed_actor,
            actor_management::preview_actor_import,
            actor_management::confirm_actor_scan_add,
            actor_management::preview_actor_export,
            actor_management::confirm_actor_export,
            set_actor_favorite,
            list_cast_candidates,
            add_film_cast_actor,
            remove_film_cast_actor,
            open_library_source_directory,
            save_actor_profile,
            film_commands::set_film_code,
            film_commands::load_version_cover_thumbnail,
            load_actor_thumbnail,
            load_film_actor_thumbnail,
            load_cover_thumbnail,
            load_video_size,
            load_video_resolution,
            load_directory_facts,
            load_player_settings,
            application_data_directory,
            open_application_data_directory,
            save_player_settings,
            launch_film_video,
            embedded_player::embedded_player_state,
            embedded_player::embedded_player_layout,
            embedded_player::embedded_player_control,
            embedded_player::embedded_player_close,
            embedded_player::embedded_player_choose_subtitle,
            window_lifecycle::restart_application,
            open_film_folder,
            set_preferred_video,
            load_launch_history,
            clear_launch_history,
            load_real_films,
            list_real_tags,
            edit_clipboard::read_edit_clipboard,
            edit_clipboard::write_edit_clipboard,
            create_real_tag,
            reorder_real_tags,
            delete_real_tag,
            rename_real_tag,
            set_real_favorite,
            set_real_rating,
            set_real_film_tags,
            apply_real_bulk_tag,
            delete_real_films,
            list_data_backups,
            create_data_backup,
            restore_data_backup,
            delete_data_backup,
            list_library_sources,
            choose_library_root,
            preview_library_source,
            change_library_source_state,
            cancel_library_preview,
            confirm_library_source
        ])
        .build(context)
        .inspect_err(|error| { diagnostics::record_error("startup", error); diagnostics::flush(); })
        .expect("error while building tauri application");
    // build initializes the single-instance notification window before any application setup.
    #[cfg(windows)]
    drop(startup_guard);
    app.run(|_, _| {});
}

#[tauri::command]
async fn load_interface_language(app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "interface_language_load_failed")?;
        interface_language::load(&storage)
    }).await.map_err(|_| "interface_language_load_failed".to_owned())?
}

#[tauri::command]
async fn save_interface_language(app: tauri::AppHandle, language: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "interface_language_save_failed")?;
        interface_language::save(&storage, &language)?;
        interface_language::publish_committed(&app, &language);
        Ok(())
    }).await.map_err(|_| "interface_language_save_failed".to_owned())?
}

fn sync_application_options(app:&tauri::AppHandle)->Result<(),String>{
 let state=ready_storage(app)?;
 let storage=state.lock().map_err(|_|"storage_unavailable")?;
 let options=application_options::load(&storage)?;
 app.state::<window_lifecycle::WindowLifecycle>().close_exits.store(options.close_window_exits,std::sync::atomic::Ordering::Release);
 if let Some(tray)=app.tray_by_id("thearchive-tray"){let _=tray.set_visible(!options.close_window_exits);}
 Ok(())
}
#[tauri::command]
async fn load_application_options(app:tauri::AppHandle)->Result<application_options::ApplicationOptions,String>{
 tauri::async_runtime::spawn_blocking(move||{let state=ready_storage(&app)?;let storage=state.lock().map_err(|_|"storage_unavailable")?;application_options::load(&storage)}).await.map_err(|_|"options_load_failed".to_owned())?
}
#[tauri::command]
async fn save_application_options(app:tauri::AppHandle,options:application_options::ApplicationOptions)->Result<(),String>{
 tauri::async_runtime::spawn_blocking(move||{let state=ready_storage(&app)?;let storage=state.exclusive().map_err(|_|"storage_unavailable")?;application_options::save(&storage,&options)?;drop(storage);sync_application_options(&app)}).await.map_err(|_|"options_save_failed".to_owned())?
}
#[tauri::command]
fn resolve_window_close(app:tauri::AppHandle,action:String)->Result<(),String>{
 match action.as_str(){"cancel"=>Ok(()),"exit"=>{window_lifecycle::exit(&app);Ok(())},_=>Err("invalid_action".into())}
}
