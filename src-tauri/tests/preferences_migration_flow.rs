#[path="support/legacy_schema.rs"] mod legacy_schema;
use local_film_library_preview_lib::storage::Storage;

#[test]
fn fresh_database_has_persistent_preferences_and_nullable_actor_marker() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    storage.with_connection(|db| {
        let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='interface_preferences')",[],|r|r.get(0))?;
        assert!(exists);
        let required: bool = db.query_row("SELECT \"notnull\" FROM pragma_table_info('actor_directories') WHERE name='marker_id'",[],|r|r.get(0))?;
        assert!(!required);
        Ok(())
    }).unwrap();
}

#[test]
fn version_six_upgrade_preserves_manual_data_and_protects_before_migration() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    storage.with_connection(|db| {
        legacy_schema::remove_actor_v17(db)?; db.execute_batch("DROP TABLE film_translations; DROP TABLE translation_memory; DROP TABLE translation_settings; DROP TABLE ignored_directory_names; DROP TABLE IF EXISTS interface_preferences; PRAGMA user_version=6; ALTER TABLE actor_manual ADD COLUMN notes TEXT; INSERT INTO actors(id,observed_name) VALUES('a','虚构演员'); INSERT INTO actor_manual(actor_id,favorite,notes) VALUES('a',1,'虚构人工备注');")?;
        Ok(())
    }).unwrap();
    drop(storage);
    let storage = Storage::open(home.path()).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    assert_eq!(storage.list_protections().unwrap().len(), 1);
    storage
        .with_connection(|db| {
            let values: (bool, Option<i64>) = db.query_row(
                "SELECT favorite,height_cm FROM actor_manual WHERE actor_id='a'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            assert_eq!(values, (true, None));
            Ok(())
        })
        .unwrap();
}
