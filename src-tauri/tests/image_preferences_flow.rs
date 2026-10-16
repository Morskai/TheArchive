#[path="support/legacy_schema.rs"] mod legacy_schema;
use local_film_library_preview_lib::{
    lifecycle::{self, InterfacePreferences},
    storage::Storage,
};

#[test]
fn version_seven_preferences_upgrade_and_old_backup_restore_keep_settings() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    storage.with_connection(|db| {
        legacy_schema::remove_actor_v17(db)?; db.execute_batch("DROP TABLE film_translations; DROP TABLE translation_memory; DROP TABLE translation_settings; DROP TABLE ignored_directory_names; DROP TABLE interface_preferences; CREATE TABLE interface_preferences(singleton INTEGER PRIMARY KEY,reduce_motion INTEGER NOT NULL,thin_scrollbar INTEGER NOT NULL); INSERT INTO interface_preferences VALUES(1,1,0); PRAGMA user_version=7;")?;
        Ok(())
    }).unwrap();
    let backup = storage.create_backup().unwrap();
    drop(storage);
    let storage = Storage::open(home.path()).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    assert_eq!(storage.list_protections().unwrap().len(), 1);
    let values = serde_json::to_value(lifecycle::load_preferences(&storage).unwrap()).unwrap();
    assert_eq!(values["reduceMotion"], true);
    assert_eq!(values["thinScrollbar"], false);
    assert_eq!(values["filmCoverRatio"], "original");
    assert_eq!(values["actorPhotoRatio"], "original");
    assert_eq!(values["filmCoverFit"], "cover");
    storage.restore_backup(&backup).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    assert_eq!(
        serde_json::to_value(lifecycle::load_preferences(&storage).unwrap()).unwrap(),
        values
    );
}

#[test]
fn image_preferences_reopen_restore_and_failed_or_invalid_saves_preserve_all_values() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    let json = serde_json::json!({"reduceMotion":true,"thinScrollbar":false,"filmCoverRatio":"16:9","actorPhotoRatio":"3:4","filmCoverFit":"contain","actorPhotoFit":"contain"});
    let preferences: InterfacePreferences = serde_json::from_value(json.clone()).unwrap();
    lifecycle::save_preferences(&storage, &preferences).unwrap();
    let backup = storage.create_backup().unwrap();
    drop(storage);
    let storage = Storage::open(home.path()).unwrap();
    assert_eq!(
        serde_json::to_value(lifecycle::load_preferences(&storage).unwrap()).unwrap(),
        json
    );
    for field in [
        "filmCoverRatio",
        "actorPhotoRatio",
        "filmCoverFit",
        "actorPhotoFit",
    ] {
        let mut invalid = json.clone();
        invalid[field] = "invalid".into();
        let invalid: InterfacePreferences = serde_json::from_value(invalid).unwrap();
        assert!(lifecycle::save_preferences(&storage, &invalid).is_err());
        assert_eq!(lifecycle::load_preferences(&storage).unwrap(), preferences);
    }
    storage.with_connection(|db| db.execute_batch("CREATE TRIGGER reject_image_preferences BEFORE UPDATE ON interface_preferences BEGIN SELECT RAISE(ABORT,'synthetic'); END;")).unwrap();
    assert!(lifecycle::save_preferences(&storage, &InterfacePreferences::default()).is_err());
    assert_eq!(lifecycle::load_preferences(&storage).unwrap(), preferences);
    storage.restore_backup(&backup).unwrap();
    assert_eq!(lifecycle::load_preferences(&storage).unwrap(), preferences);
    let legacy: InterfacePreferences =
        serde_json::from_value(serde_json::json!({"reduceMotion":false,"thinScrollbar":true}))
            .unwrap();
    assert_eq!(legacy, InterfacePreferences::default());
}

#[test]
fn version_eight_upgrade_and_backup_allow_portrait_without_losing_manual_data() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    storage.with_connection(|db| {
        legacy_schema::remove_actor_v17(db)?; db.execute_batch("DROP TABLE film_translations; DROP TABLE translation_memory; DROP TABLE translation_settings; DROP TABLE ignored_directory_names; DROP TABLE interface_preferences;
            CREATE TABLE interface_preferences(singleton INTEGER PRIMARY KEY CHECK(singleton=1),reduce_motion INTEGER NOT NULL CHECK(reduce_motion IN(0,1)),thin_scrollbar INTEGER NOT NULL CHECK(thin_scrollbar IN(0,1)),film_cover_ratio TEXT NOT NULL DEFAULT 'original' CHECK(film_cover_ratio IN('original','16:9','3:2')),actor_photo_ratio TEXT NOT NULL DEFAULT 'original' CHECK(actor_photo_ratio IN('original','2:3','3:4')),film_cover_fit TEXT NOT NULL DEFAULT 'cover' CHECK(film_cover_fit IN('cover','contain')),actor_photo_fit TEXT NOT NULL DEFAULT 'cover' CHECK(actor_photo_fit IN('cover','contain')));
            INSERT INTO interface_preferences VALUES(1,1,0,'3:2','3:4','contain','contain');
            INSERT INTO films(id) VALUES('synthetic');
            INSERT INTO film_manual(film_id,title,rating,favorite) VALUES('synthetic','虚构人工标题',8,1);
            PRAGMA user_version=8;")?;
        Ok(())
    }).unwrap();
    let old_backup = storage.create_backup().unwrap();
    let mut old = lifecycle::load_preferences(&storage).unwrap();
    old.film_cover_ratio = "original".into();
    drop(storage);
    let storage = Storage::open(home.path()).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    assert_eq!(lifecycle::load_preferences(&storage).unwrap(), old);
    assert_eq!(storage.list_protections().unwrap().len(), 1);
    let portrait = InterfacePreferences {
        film_cover_ratio: "5:7".into(),
        ..old.clone()
    };
    lifecycle::save_preferences(&storage, &portrait).unwrap();
    let portrait_backup = storage.create_backup().unwrap();
    drop(storage);
    let storage = Storage::open(home.path()).unwrap();
    assert_eq!(lifecycle::load_preferences(&storage).unwrap(), portrait);
    storage.restore_backup(&old_backup).unwrap();
    assert_eq!(lifecycle::load_preferences(&storage).unwrap(), old);
    lifecycle::save_preferences(&storage, &portrait).unwrap();
    storage.restore_backup(&portrait_backup).unwrap();
    assert_eq!(lifecycle::load_preferences(&storage).unwrap(), portrait);
    storage.with_connection(|db| {
        let row: (String, i64, i64) = db.query_row("SELECT title,rating,favorite FROM film_manual WHERE film_id='synthetic'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        assert_eq!(row,("虚构人工标题".into(),8,1));
        assert!(db.execute("UPDATE interface_preferences SET film_cover_ratio='invalid'",[]).is_err());
        db.execute_batch("CREATE TRIGGER reject_portrait BEFORE UPDATE ON interface_preferences BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?;
        Ok(())
    }).unwrap();
    assert!(lifecycle::save_preferences(&storage, &old).is_err());
    assert_eq!(lifecycle::load_preferences(&storage).unwrap(), portrait);
}
