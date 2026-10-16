use local_film_library_preview_lib::{actors, storage::Storage};

fn legacy() -> (tempfile::TempDir, Storage) {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_transaction(|tx| tx.execute_batch("
        DROP TABLE IF EXISTS actor_tag_links; DROP TABLE IF EXISTS actor_tags;
        DROP TABLE IF EXISTS actor_image_sources;
        CREATE TABLE IF NOT EXISTS actor_photos(actor_id TEXT PRIMARY KEY REFERENCES actors(id) ON DELETE CASCADE,jpeg BLOB NOT NULL,fingerprint TEXT NOT NULL);
        INSERT INTO actors(id,observed_name) VALUES('legacy','演员甲');
        INSERT INTO actor_manual(actor_id,favorite,aliases,birth_month) VALUES('legacy',1,'别名','1990');
        INSERT INTO films(id) VALUES('film');
        INSERT INTO film_actors(film_id,actor_id,source) VALUES('film','legacy','manual');
        INSERT INTO actor_photos(actor_id,jpeg,fingerprint) VALUES('legacy',X'010203','legacy-jpeg');
        PRAGMA user_version=18;
    ")).unwrap();
    (home, storage)
}

#[test]
fn protected_upgrade_discards_old_photo_content_and_keeps_actor_identity_and_manual_relations() {
    let (home, old) = legacy();
    drop(old);
    let storage = Storage::open(&home.path().join("data")).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    assert!(!storage.list_protections().unwrap().is_empty());
    storage.with_connection(|db| {
        assert!(!db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='actor_photos')", [], |r| r.get::<_,bool>(0))?);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM actor_image_sources", [], |r| r.get::<_,i64>(0))?,0);
        assert_eq!(db.query_row("SELECT observed_name FROM actors WHERE id='legacy'", [], |r| r.get::<_,String>(0))?,"演员甲");
        assert_eq!(db.query_row("SELECT favorite||'/'||aliases||'/'||birth_month FROM actor_manual WHERE actor_id='legacy'", [], |r| r.get::<_,String>(0))?,"1/别名/1990");
        assert_eq!(db.query_row("SELECT COUNT(*) FROM film_actors WHERE actor_id='legacy'", [], |r| r.get::<_,i64>(0))?,1);
        Ok(())
    }).unwrap();
    assert_eq!(actors::thumbnail(&storage,"legacy",false).unwrap_err(),"cover_missing");
}

#[test]
fn protection_failure_does_not_drop_old_photo_content() {
    let (home, old) = legacy();
    std::fs::write(home.path().join("data/protections"), b"blocked").unwrap();
    assert!(Storage::open(&home.path().join("data")).is_err());
    assert_eq!(old.schema_version().unwrap(),18);
    assert_eq!(old.with_connection(|db| db.query_row("SELECT jpeg FROM actor_photos", [], |r| r.get::<_,Vec<u8>>(0))).unwrap(),vec![1,2,3]);
}
