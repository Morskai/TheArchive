#[path = "support/legacy_schema.rs"]
mod legacy_schema;
use local_film_library_preview_lib::storage::Storage;
use std::fs;

fn seed_v17_actor(storage: &Storage) {
    storage.with_transaction(|db| {
        legacy_schema::restore_actor_v17(db)?;
        db.execute_batch("INSERT INTO actors(id,observed_name,created_at) VALUES('software-actor','保存演员','2000-01-01');
            INSERT INTO actor_manual(actor_id,favorite,aliases,birth_month,height_cm,cup,bust_cm,waist_cm,hip_cm) VALUES('software-actor',1,'保存别名','1990-04',165,'C',85,60,88);
            INSERT INTO films(id) VALUES('software-film');
            INSERT INTO film_manual(film_id,title,favorite,cast_override) VALUES('software-film','保存片名',1,1);
            INSERT INTO film_actors(film_id,actor_id,source) VALUES('software-film','software-actor','manual');
            INSERT INTO info_actor_links(external_id,actor_id) VALUES('film-archival-id','software-actor');
            INSERT INTO actor_import_sources(directory_key,actor_id,fingerprint,external_id) VALUES('old-directory','software-actor','old-fingerprint','old-actor-id');")?;
        let mut jpeg = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(24, 32, image::Rgb([80, 110, 140]))
            .write_to(&mut jpeg, image::ImageFormat::Jpeg).unwrap();
        db.execute("INSERT INTO actor_photos(actor_id,jpeg,fingerprint) VALUES('software-actor',?1,'software-jpeg-fingerprint')", [jpeg.into_inner()])?;
        db.execute("UPDATE actors SET modified_at='2001-02-03' WHERE id='software-actor'", [])?;
        Ok(())
    }).unwrap();
}

fn personal_and_film_values(db: &rusqlite::Connection) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    [
        "SELECT * FROM actors ORDER BY id",
        "SELECT * FROM actor_manual ORDER BY actor_id",
        "SELECT * FROM films ORDER BY id",
        "SELECT * FROM film_manual ORDER BY film_id",
        "SELECT * FROM film_actors ORDER BY film_id,actor_id,source",
        "SELECT * FROM info_actor_links ORDER BY external_id",
    ]
    .into_iter()
    .map(|sql| {
        let mut query = db.prepare(sql).unwrap();
        let columns = query.column_count();
        query
            .query_map([], |row| {
                (0..columns).map(|column| row.get(column)).collect()
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    })
    .collect()
}

fn legacy_photo_values(db: &rusqlite::Connection) -> Vec<(String, Vec<u8>, String)> {
    db.prepare("SELECT actor_id,jpeg,fingerprint FROM actor_photos ORDER BY actor_id").unwrap()
        .query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
        .collect::<rusqlite::Result<_>>().unwrap()
}

fn assert_current_actor_schema(db: &rusqlite::Connection) {
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
            .unwrap(),
        23
    );
    assert!(!db.query_row::<bool,_,_>("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='actor_photos')",[],|r|r.get(0)).unwrap());
    assert_eq!(db.query_row::<i64,_,_>("SELECT COUNT(*) FROM actor_image_sources",[],|r|r.get(0)).unwrap(),0,"legacy photos must not produce invented paths");
    assert!(
        !db.query_row::<bool, _, _>(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='actor_import_sources')",
            [],
            |row| row.get(0)
        )
        .unwrap()
    );
    assert!(
        db.prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none()
    );
}

#[test]
fn v17_upgrade_discards_legacy_photo_and_scan_mapping_after_protecting_personal_and_film_data() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    seed_v17_actor(&storage);
    let legacy_photos = storage.with_connection(|db| Ok(legacy_photo_values(db))).unwrap();
    let expected = storage
        .with_connection(|db| Ok(personal_and_film_values(db)))
        .unwrap();
    drop(storage);
    let upgraded = Storage::open(home.path()).unwrap();
    assert_eq!(local_film_library_preview_lib::actors::thumbnail(&upgraded,"software-actor",false).unwrap_err(),"cover_missing");
    upgraded
        .with_connection(|db| {
            assert_current_actor_schema(db);
            assert_eq!(personal_and_film_values(db), expected);
            Ok(())
        })
        .unwrap();
    let protections = upgraded.list_protections().unwrap();
    assert_eq!(protections.len(), 1);
    let protected =
        rusqlite::Connection::open(home.path().join("protections").join(&protections[0])).unwrap();
    assert_eq!(
        protected
            .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
            .unwrap(),
        17
    );
    assert_eq!(
        protected
            .query_row::<String, _, _>("SELECT external_id FROM actor_import_sources", [], |row| {
                row.get(0)
            })
            .unwrap(),
        "old-actor-id"
    );
    assert_eq!(personal_and_film_values(&protected), expected);
    assert_eq!(legacy_photo_values(&protected), legacy_photos);
    let backup = upgraded.create_backup().unwrap();
    upgraded
        .with_transaction(|db| {
            db.execute("UPDATE actor_manual SET favorite=0,height_cm=170", [])?;
            Ok(())
        })
        .unwrap();
    upgraded.restore_backup(&backup).unwrap();
    upgraded
        .with_connection(|db| {
            assert_current_actor_schema(db);
            assert_eq!(personal_and_film_values(db), expected);
            Ok(())
        })
        .unwrap();
}

#[test]
fn v17_backup_restores_staged_migration_without_reintroducing_scan_identity() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    seed_v17_actor(&storage);
    let legacy_photos = storage.with_connection(|db| Ok(legacy_photo_values(db))).unwrap();
    let expected = storage
        .with_connection(|db| Ok(personal_and_film_values(db)))
        .unwrap();
    let backup = storage.create_backup().unwrap();
    drop(storage);
    let upgraded = Storage::open(home.path()).unwrap();
    upgraded
        .with_transaction(|db| {
            db.execute("UPDATE actor_manual SET favorite=0,height_cm=170", [])?;
            Ok(())
        })
        .unwrap();
    let before_restore = upgraded
        .with_connection(|db| Ok(personal_and_film_values(db)))
        .unwrap();
    upgraded.restore_backup(&backup).unwrap();
    assert_eq!(local_film_library_preview_lib::actors::thumbnail(&upgraded,"software-actor",false).unwrap_err(),"cover_missing");
    upgraded
        .with_connection(|db| {
            assert_current_actor_schema(db);
            assert_eq!(personal_and_film_values(db), expected);
            Ok(())
        })
        .unwrap();
    let protections = upgraded.list_protections().unwrap();
    assert_eq!(protections.len(), 2);
    assert!(protections.iter().any(|name| {
        let db = rusqlite::Connection::open(home.path().join("protections").join(name)).unwrap();
        personal_and_film_values(&db) == before_restore
    }));
    let original_backup = rusqlite::Connection::open(
        home.path()
            .join("backups")
            .join(format!("backup-{backup}.sqlite3")),
    )
    .unwrap();
    assert_eq!(legacy_photo_values(&original_backup), legacy_photos);
    assert_eq!(
        original_backup
            .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
            .unwrap(),
        17
    );
    assert_eq!(
        original_backup
            .query_row::<i64, _, _>("SELECT COUNT(*) FROM actor_import_sources", [], |row| row
                .get(0))
            .unwrap(),
        1
    );
    drop(upgraded);
    Storage::open(home.path())
        .unwrap()
        .with_connection(|db| {
            assert_current_actor_schema(db);
            assert_eq!(personal_and_film_values(db), expected);
            Ok(())
        })
        .unwrap();
}

#[test]
fn v17_upgrade_keeps_scan_mapping_and_all_data_when_protection_fails() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    seed_v17_actor(&storage);
    let legacy_photos = storage.with_connection(|db| Ok(legacy_photo_values(db))).unwrap();
    let expected = storage
        .with_connection(|db| Ok(personal_and_film_values(db)))
        .unwrap();
    drop(storage);
    fs::write(
        home.path().join("protections"),
        b"blocked synthetic protection",
    )
    .unwrap();
    assert!(Storage::open(home.path()).is_err());
    let db = rusqlite::Connection::open(home.path().join("library.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
            .unwrap(),
        17
    );
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT COUNT(*) FROM actor_import_sources", [], |row| row
            .get(0))
            .unwrap(),
        1
    );
    assert_eq!(personal_and_film_values(&db), expected);
    assert_eq!(legacy_photo_values(&db), legacy_photos);
}

#[test]
fn interrupted_v17_restore_recovers_then_protects_and_migrates_actor_data() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    seed_v17_actor(&storage);
    let expected = storage
        .with_connection(|db| Ok(personal_and_film_values(db)))
        .unwrap();
    drop(storage);
    let rollback = home
        .path()
        .join("rollback-33333333333333333333333333333333.tmp");
    fs::rename(home.path().join("library.sqlite3"), &rollback).unwrap();
    let recovered = Storage::open(home.path()).unwrap();
    recovered
        .with_connection(|db| {
            assert_current_actor_schema(db);
            assert_eq!(personal_and_film_values(db), expected);
            Ok(())
        })
        .unwrap();
    assert_eq!(recovered.list_protections().unwrap().len(), 1);
    assert!(!rollback.exists());
}

#[test]
fn v5_upgrade_protects_user_data_and_old_backup_restores_to_current_schema() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("data");
    let storage = Storage::open(&data).unwrap();
    storage.with_connection(|db| {
        db.execute_batch("ALTER TABLE actor_manual ADD COLUMN notes TEXT;")?;
        db.execute("INSERT INTO actors(id,observed_name) VALUES('stable','人工演员')",[])?;
        db.execute("INSERT INTO actor_manual(actor_id,favorite,notes,height_cm) VALUES('stable',1,'人工备注',165)",[])?;
        db.execute("INSERT INTO films(id) VALUES('saved-film')",[])?;
        db.execute("INSERT INTO film_manual(film_id,favorite) VALUES('saved-film',1)",[])?;
        db.execute("INSERT INTO film_actors(film_id,actor_id,source) VALUES('saved-film','stable','observation'),('saved-film','stable','manual')",[])?;
        legacy_schema::remove_actor_v17(db)?; db.execute_batch("DROP TABLE film_translations; DROP TABLE translation_memory; DROP TABLE translation_settings; DROP TABLE ignored_directory_names; DROP TABLE interface_preferences; DROP TABLE actor_directories; PRAGMA user_version=5;")?;
        Ok(())
    }).unwrap();
    let backup = storage.create_backup().unwrap();
    drop(storage);
    let storage = Storage::open(&data).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    assert_eq!(storage.list_protections().unwrap().len(), 1);
    storage.restore_backup(&backup).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    storage.with_connection(|db| {let row:(String,bool,i64)=db.query_row("SELECT a.id,m.favorite,m.height_cm FROM actors a JOIN actor_manual m ON m.actor_id=a.id",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?; assert_eq!(row,("stable".into(),true,165)); Ok(())}).unwrap();
    storage.with_connection(|db| {
        assert_eq!(db.query_row::<i64,_,_>("SELECT COUNT(*) FROM film_actors WHERE source='observation'",[],|r|r.get(0))?,0);
        assert_eq!(db.query_row::<i64,_,_>("SELECT COUNT(*) FROM film_actors WHERE source='manual'",[],|r|r.get(0))?,1);
        assert!(db.query_row::<bool,_,_>("SELECT favorite FROM film_manual WHERE film_id='saved-film'",[],|r|r.get(0))?);
        Ok(())
    }).unwrap();
}

#[test]
fn v5_upgrade_stops_before_mutation_if_protection_fails() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("data");
    let storage = Storage::open(&data).unwrap();
    let db_path = data.join("library.sqlite3");
    storage
        .with_connection(|db| {
            legacy_schema::remove_actor_v17(db)?; db.execute_batch("DROP TABLE film_translations; DROP TABLE translation_memory; DROP TABLE translation_settings; DROP TABLE ignored_directory_names; DROP TABLE interface_preferences; DROP TABLE actor_directories; PRAGMA user_version=5;")?;
            Ok(())
        })
        .unwrap();
    drop(storage);
    fs::write(data.join("protections"), b"blocked").unwrap();
    assert!(Storage::open(&data).is_err());
    let db = rusqlite::Connection::open(db_path).unwrap();
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        5
    );
    assert_eq!(
        db.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='actor_directories'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
}
