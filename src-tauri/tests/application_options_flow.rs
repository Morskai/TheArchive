#[path = "support/legacy_schema.rs"]
mod legacy_schema;
use local_film_library_preview_lib::{application_options::{self, ApplicationOptions}, storage::Storage};

#[test]
fn legacy_monitor_flag_is_inert_when_loading_saving_and_restoring() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("software");
    let storage = Storage::open(&data).unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO application_options(singleton,close_window_exits,monitor_libraries) VALUES(1,0,1)", [])?;
        Ok(())
    }).unwrap();
    let backup = storage.create_backup().unwrap();
    let loaded = application_options::load(&storage).unwrap();
    assert!(!loaded.close_window_exits, "the ordinary close preference must survive");
    assert!(!loaded.monitor_libraries, "an old database must not enable removed monitoring");
    let legacy: ApplicationOptions = serde_json::from_value(serde_json::json!({"closeWindowExits": true, "monitorLibraries": true})).unwrap();
    assert!(!legacy.monitor_libraries, "a stale client flag must be ignored");
    application_options::save(&storage, &ApplicationOptions { close_window_exits: true, monitor_libraries: true }).unwrap();
    storage.with_connection(|db| {
        assert_eq!(db.query_row::<bool, _, _>("SELECT monitor_libraries FROM application_options", [], |row| row.get(0))?, false);
        Ok(())
    }).unwrap();
    assert_eq!(serde_json::to_value(application_options::load(&storage).unwrap()).unwrap(), serde_json::json!({"closeWindowExits": true}));
    storage.restore_backup(&backup).unwrap();
    drop(storage);
    let reopened = Storage::open(&data).unwrap();
    let restored = application_options::load(&reopened).unwrap();
    assert!(!restored.close_window_exits);
    assert!(!restored.monitor_libraries, "restoring an old backup must not revive monitoring");
}

#[test]
fn options_defaults_and_schema14_protection_are_preserved() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("software");
    let storage = Storage::open(&data).unwrap();
    assert_eq!(application_options::load(&storage).unwrap(), ApplicationOptions::default());
    assert!(!application_options::load(&storage).unwrap().close_window_exits);
    let current_schema = storage.schema_version().unwrap();
    storage.with_connection(|db| {
        legacy_schema::remove_actor_v17(db)?;
        db.execute_batch("DROP TABLE application_options; PRAGMA user_version=14;")
    }).unwrap();
    let protections = storage.list_protections().unwrap().len();
    drop(storage);
    let migrated = Storage::open(&data).unwrap();
    assert_eq!(migrated.schema_version().unwrap(), current_schema);
    assert_eq!(migrated.list_protections().unwrap().len(), protections + 1);
    assert_eq!(application_options::load(&migrated).unwrap(), ApplicationOptions::default());
}

#[test]
fn startup_source_reads_and_reconnects_keep_saved_cards_until_a_manual_scan() {
    use local_film_library_preview_lib::{catalog, lifecycle::StartupSession, source_scan::{self, SourceScanSession}};
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-library");
    std::fs::create_dir_all(root.join("saved-film")).unwrap();
    std::fs::write(root.join("saved-film/movie.mp4"), []).unwrap();
    let storage = Storage::open(&home.path().join("software")).unwrap();
    let scan = SourceScanSession::new();
    let token = scan.preview(&storage, &root, None).unwrap().token.unwrap();
    let source = scan.confirm(&storage, &token).unwrap().library_id;
    storage.with_connection(|db| {
        db.execute("INSERT INTO application_options(singleton,close_window_exits,monitor_libraries) VALUES(1,0,1)", [])?;
        Ok(())
    }).unwrap();
    std::fs::create_dir_all(root.join("new-film")).unwrap();
    std::fs::write(root.join("new-film/movie.mp4"), []).unwrap();
    for _ in 0..2 {
        StartupSession::default().initialize(&storage).unwrap();
        source_scan::list_sources(&storage).unwrap();
        assert_eq!(catalog::list_films(&storage).unwrap().len(), 1);
    }
    let offline = home.path().join("synthetic-offline");
    std::fs::rename(&root, &offline).unwrap();
    source_scan::list_sources(&storage).unwrap();
    std::fs::rename(&offline, &root).unwrap();
    StartupSession::default().initialize(&storage).unwrap();
    source_scan::list_sources(&storage).unwrap();
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 1);
    let token = scan.preview(&storage, &root, Some(&source)).unwrap().token.unwrap();
    scan.confirm(&storage, &token).unwrap();
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 2);
}
