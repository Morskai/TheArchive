use local_film_library_preview_lib::{display_preferences as display, lifecycle, storage::Storage};

#[test]
fn independent_ratios_reopen_backup_and_atomic_failure() {
    let home=tempfile::tempdir().unwrap();
    let storage=Storage::open(home.path()).unwrap();
    storage.with_connection(|db| db.execute_batch("INSERT INTO libraries(id,display_name) VALUES('a','A'),('b','B');")).unwrap();
    display::save_film_ratios(&storage, &["a".into()], "5:7").unwrap();
    display::save_film_ratios(&storage, &["b".into()], "16:9").unwrap();
    display::save_actor_ratio(&storage, "3:4").unwrap();
    let saved=display::load(&storage).unwrap();
    assert_eq!(saved.library_ratios["a"], "5:7"); assert_eq!(saved.library_ratios["b"], "16:9");
    assert_eq!(saved.actor_ratio, "3:4");
    assert!(display::save_film_ratios(&storage, &["a".into(),"missing".into()], "original").is_err());
    assert!(display::save_film_ratios(&storage, &["a".into()], "3:2").is_err());
    assert_eq!(display::load(&storage).unwrap(),saved);
    let backup=storage.create_backup().unwrap(); drop(storage);
    let storage=Storage::open(home.path()).unwrap(); assert_eq!(display::load(&storage).unwrap(),saved);
    display::save_film_ratios(&storage, &["a".into(),"b".into()], "original").unwrap();
    storage.restore_backup(&backup).unwrap(); assert_eq!(display::load(&storage).unwrap(),saved);
    // A settings-page appearance save must not revert ratios read before a library change.
    let appearance=lifecycle::InterfacePreferences::default();
    display::save_appearance(&storage,&appearance).unwrap();
    assert_eq!(display::load(&storage).unwrap(),saved);
}

#[test]
fn schema21_migration_seeds_each_library_and_normalizes_retired_ratio() {
    for (old, expected) in [("3:2","original"),("5:7","5:7")] {
        let home=tempfile::tempdir().unwrap(); let storage=Storage::open(home.path()).unwrap();
        storage.with_connection(|db| {
            db.execute_batch("DROP TABLE library_display_preferences; INSERT INTO libraries(id,display_name) VALUES('a','A'); PRAGMA user_version=21;")?;
            db.execute("INSERT INTO interface_preferences(singleton,reduce_motion,thin_scrollbar,film_cover_ratio,actor_photo_ratio,film_cover_fit,actor_photo_fit) VALUES(1,1,0,?1,'2:3','contain','contain')",[old])?;
            Ok(())
        }).unwrap();
        let backup=storage.create_backup().unwrap();drop(storage);
        let storage=Storage::open(home.path()).unwrap();
        assert_eq!(storage.schema_version().unwrap(),23);
        assert_eq!(display::load(&storage).unwrap().library_ratios["a"],expected);
        let prefs=lifecycle::load_preferences(&storage).unwrap();assert!(prefs.reduce_motion);assert_eq!(prefs.film_cover_fit,"contain");assert_eq!(prefs.actor_photo_ratio,"2:3");
        assert!(!storage.list_protections().unwrap().is_empty());
        storage.restore_backup(&backup).unwrap();assert_eq!(display::load(&storage).unwrap().library_ratios["a"],expected);
    }
}
