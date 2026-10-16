use local_film_library_preview_lib::{playback::{self, PlayerSettings}, storage::Storage};

#[test]
fn configuration_directory_uses_the_active_storage_without_mutating_it() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    let sentinel = home.path().join("preserved.txt");
    std::fs::write(&sentinel, b"preserve").unwrap();
    playback::open_data_directory_with(&storage, |path| {
        assert_eq!(path, home.path().canonicalize().unwrap());
        assert!(path.is_dir());
        Ok(())
    }).unwrap();
    assert_eq!(playback::open_data_directory_with(&storage, |_| Err("directory_open_failed".into())).unwrap_err(), "directory_open_failed");
    assert_eq!(std::fs::read(sentinel).unwrap(), b"preserve");
}

#[test]
fn internal_settings_persist_and_invalid_display_preserves_saved_choice() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    let mut options = PlayerSettings::default();
    assert_eq!(options.player_mode, "internal");
    assert_eq!(options.internal_display, "overlay");
    options.internal_display = "window".into();
    playback::save_settings(&storage, &options).unwrap();
    let reopened = Storage::open(home.path()).unwrap();
    assert_eq!(playback::load_settings(&reopened).unwrap(), options);
    let mut invalid = options.clone();
    invalid.internal_display = "anything".into();
    assert!(playback::save_settings(&reopened, &invalid).is_err());
    assert_eq!(playback::load_settings(&reopened).unwrap(), options);
    let backup = reopened.create_backup().unwrap();
    options.player_mode = "system".into();
    playback::save_settings(&reopened, &options).unwrap();
    reopened.restore_backup(&backup).unwrap();
    assert_eq!(playback::load_settings(&reopened).unwrap().internal_display, "window");
}

#[test]
fn schema20_upgrade_preserves_external_settings_and_protects_original() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("library.sqlite3");
    let original = Storage::open(home.path()).unwrap();
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch("DROP TABLE player_settings; CREATE TABLE player_settings(singleton INTEGER PRIMARY KEY CHECK(singleton=1),mode TEXT NOT NULL CHECK(mode IN('system','custom')),program TEXT NOT NULL,arguments TEXT NOT NULL); INSERT INTO player_settings VALUES(1,'custom','D:/播放器.exe','--中文'); PRAGMA user_version=20;").unwrap();
    drop(old);
    let old_backup = original.create_backup().unwrap();
    let upgraded = Storage::open(home.path()).unwrap();
    let options = playback::load_settings(&upgraded).unwrap();
    assert_eq!(options.player_mode, "custom");
    assert_eq!(options.player_path, "D:/播放器.exe");
    assert_eq!(options.launch_arguments, "--中文");
    assert_eq!(options.internal_display, "overlay");
    assert!(std::fs::read_dir(home.path().join("protections")).unwrap().next().is_some());
    assert_eq!(upgraded.schema_version().unwrap(), 23);
    upgraded.restore_backup(&old_backup).unwrap();
    assert_eq!(playback::load_settings(&upgraded).unwrap(), options);
}

#[test]
fn failed_schema21_migration_rolls_back_and_keeps_the_old_settings() {
    let home = tempfile::tempdir().unwrap();
    drop(Storage::open(home.path()).unwrap());
    let path = home.path().join("library.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE player_settings; CREATE TABLE player_settings(singleton INTEGER PRIMARY KEY,mode TEXT NOT NULL,program TEXT NOT NULL,arguments TEXT NOT NULL); INSERT INTO player_settings VALUES(1,'system','','preserved'); CREATE VIEW player_settings_v21 AS SELECT 1; PRAGMA user_version=20;").unwrap();
    drop(db);
    assert!(Storage::open(home.path()).is_err());
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0)).unwrap(), 20);
    assert_eq!(db.query_row::<String, _, _>("SELECT arguments FROM player_settings", [], |r| r.get(0)).unwrap(), "preserved");
    assert!(fs_count(&home.path().join("protections")) > 0);
}
fn fs_count(path: &std::path::Path) -> usize { std::fs::read_dir(path).unwrap().count() }

#[test]
fn retired_system_choice_loads_as_internal_and_preserves_custom_program_fields() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    storage.with_connection(|db| db.execute("INSERT INTO player_settings(singleton,mode,program,arguments) VALUES(1,'system','D:/custom.exe','--preserved')", [])).unwrap();
    let options = playback::load_settings(&storage).unwrap();
    assert_eq!(options.player_mode, "internal");
    assert_eq!(options.player_path, "D:/custom.exe");
    assert_eq!(options.launch_arguments, "--preserved");
    assert_eq!(options.internal_display, "overlay");
    assert_eq!(storage.schema_version().unwrap(), 23);
}
