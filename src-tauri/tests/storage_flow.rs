#[path="support/legacy_schema.rs"] mod legacy_schema;
use local_film_library_preview_lib::storage::Storage;
use rusqlite::{params, Connection};

#[test]
fn empty_start_and_restart_preserve_manual_data_without_merging_same_codes() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    storage.with_connection(|db| {
        db.execute("INSERT INTO libraries (id, display_name) VALUES (?1, ?2)", params!["lib-a", "虚构片库"])?;
        db.execute("INSERT INTO films (id) VALUES (?1), (?2)", params!["film-a", "film-b"])?;
        db.execute("INSERT INTO film_manual (film_id, title, rating, favorite) VALUES (?1, ?2, ?3, ?4)", params!["film-a", "手工标题", 8, 1])?;
        db.execute("INSERT INTO file_observations (id, library_id, film_id, observed_path, code_candidate, kind) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params!["file-a", "lib-a", "film-a", "虚构/A.mp4", "ABC-123", "undetermined"])?;
        db.execute("INSERT INTO file_observations (id, library_id, film_id, observed_path, code_candidate, kind) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params!["file-b", "lib-a", "film-b", "虚构/B.mp4", "ABC-123", "undetermined"])?;
        Ok(())
    }).unwrap();
    drop(storage);

    let reopened = Storage::open(dir.path()).unwrap();
    reopened
        .with_connection(|db| {
            let films: i64 = db.query_row("SELECT COUNT(*) FROM films", [], |row| row.get(0))?;
            let title: String = db.query_row(
                "SELECT title FROM film_manual WHERE film_id = 'film-a'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(films, 2);
            assert_eq!(title, "手工标题");
            Ok(())
        })
        .unwrap();
}

#[test]
fn backup_then_restore_recovers_prior_manual_data_and_keeps_pre_restore_protection() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).unwrap();
    storage
        .with_connection(|db| {
            db.execute("INSERT INTO films (id) VALUES ('film-a')", [])?;
            db.execute(
                "INSERT INTO film_manual (film_id, title) VALUES ('film-a', '最初')",
                [],
            )?;
            db.execute("INSERT INTO actors (id) VALUES ('actor-a')", [])?;
            db.execute(
                "INSERT INTO actor_manual (actor_id, aliases) VALUES ('actor-a', '虚构资料原值')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let backup = storage.create_backup().unwrap();
    storage
        .with_connection(|db| {
            db.execute(
                "UPDATE film_manual SET title = '改后' WHERE film_id = 'film-a'",
                [],
            )?;
            db.execute(
                "UPDATE actor_manual SET aliases = '虚构资料改后' WHERE actor_id = 'actor-a'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    storage.restore_backup(&backup).unwrap();
    storage
        .with_connection(|db| {
            let title: String = db.query_row(
                "SELECT title FROM film_manual WHERE film_id = 'film-a'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(title, "最初");
            let actor_aliases: String = db.query_row(
                "SELECT aliases FROM actor_manual WHERE actor_id='actor-a'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(actor_aliases, "虚构资料原值");
            Ok(())
        })
        .unwrap();
    let protections = storage.list_protections().unwrap();
    assert_eq!(protections.len(), 1);
    let protected = Connection::open(dir.path().join("protections").join(&protections[0])).unwrap();
    let before_restore: String = protected
        .query_row(
            "SELECT title FROM film_manual WHERE film_id='film-a'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before_restore, "改后");
    drop(storage);
    let reopened = Storage::open(dir.path()).unwrap();
    assert_eq!(reopened.schema_version().unwrap(), 23);
}

#[test]
fn invalid_backup_and_failed_protection_leave_current_data_openable() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).unwrap();
    storage
        .with_connection(|db| {
            db.execute("INSERT INTO films (id) VALUES ('film-a')", [])?;
            db.execute(
                "INSERT INTO film_manual (film_id, title) VALUES ('film-a', '安全')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(storage.restore_backup("../outside").is_err());
    let backup = storage.create_backup().unwrap();
    let backup_file = dir
        .path()
        .join("backups")
        .join(format!("backup-{backup}.sqlite3"));
    std::fs::write(&backup_file, b"invalid backup").unwrap();
    assert!(storage.restore_backup(&backup).is_err());
    std::fs::remove_file(&backup_file).unwrap();
    let backup = storage.create_backup().unwrap();
    std::fs::write(dir.path().join("protections"), b"blocks writes").unwrap();
    assert!(storage.restore_backup(&backup).is_err());
    drop(storage);
    let reopened = Storage::open(dir.path()).unwrap();
    reopened
        .with_connection(|db| {
            let title: String = db.query_row(
                "SELECT title FROM film_manual WHERE film_id = 'film-a'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(title, "安全");
            Ok(())
        })
        .unwrap();
}

#[test]
fn interrupted_restore_recovers_previous_database_on_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).unwrap();
    storage
        .with_connection(|db| {
            db.execute("INSERT INTO films (id) VALUES ('film-a')", [])?;
            Ok(())
        })
        .unwrap();
    drop(storage);
    std::fs::rename(
        dir.path().join("library.sqlite3"),
        dir.path()
            .join("rollback-0123456789abcdef0123456789abcdef.tmp"),
    )
    .unwrap();

    let reopened = Storage::open(dir.path()).unwrap();
    reopened
        .with_connection(|db| {
            let films: i64 = db.query_row("SELECT COUNT(*) FROM films", [], |row| row.get(0))?;
            assert_eq!(films, 1);
            Ok(())
        })
        .unwrap();
}

#[test]
fn missing_actor_manual_table_is_rejected_without_changing_current_data() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).unwrap();
    storage
        .with_connection(|db| {
            db.execute("INSERT INTO films (id) VALUES ('film-a')", [])?;
            db.execute(
                "INSERT INTO film_manual (film_id, title) VALUES ('film-a', '当前资料')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let backup = storage.create_backup().unwrap();
    let backup_path = dir
        .path()
        .join("backups")
        .join(format!("backup-{backup}.sqlite3"));
    Connection::open(backup_path)
        .unwrap()
        .execute_batch("DROP TABLE actor_manual;")
        .unwrap();

    assert!(storage.restore_backup(&backup).is_err());
    assert!(storage.list_protections().unwrap().is_empty());
    drop(storage);
    let reopened = Storage::open(dir.path()).unwrap();
    reopened
        .with_connection(|db| {
            let title: String = db.query_row(
                "SELECT title FROM film_manual WHERE film_id='film-a'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(title, "当前资料");
            Ok(())
        })
        .unwrap();
}

#[test]
fn broken_foreign_key_in_backup_is_rejected_without_changing_current_data() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).unwrap();
    storage
        .with_connection(|db| {
            db.execute("INSERT INTO films (id) VALUES ('film-a')", [])?;
            db.execute(
                "INSERT INTO film_manual (film_id, title) VALUES ('film-a', '当前资料')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let backup = storage.create_backup().unwrap();
    let backup_path = dir
        .path()
        .join("backups")
        .join(format!("backup-{backup}.sqlite3"));
    let tampered = Connection::open(backup_path).unwrap();
    tampered.pragma_update(None, "foreign_keys", "OFF").unwrap();
    tampered
        .execute(
            "INSERT INTO actor_manual (actor_id, aliases) VALUES ('missing-actor', '虚构')",
            [],
        )
        .unwrap();

    assert!(storage.restore_backup(&backup).is_err());
    assert!(storage.list_protections().unwrap().is_empty());
    drop(storage);
    let reopened = Storage::open(dir.path()).unwrap();
    reopened
        .with_connection(|db| {
            let title: String = db.query_row(
                "SELECT title FROM film_manual WHERE film_id='film-a'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(title, "当前资料");
            Ok(())
        })
        .unwrap();
}

#[test]
fn missing_active_database_never_silently_creates_empty_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).unwrap();
    storage
        .with_connection(|db| {
            db.execute("INSERT INTO films (id) VALUES ('film-a')", [])?;
            Ok(())
        })
        .unwrap();
    let original = dir.path().join("library.sqlite3");
    let held = dir.path().join("held.sqlite3");
    std::fs::rename(&original, &held).unwrap();
    assert!(storage.schema_version().is_err());
    assert!(!original.exists());
    std::fs::rename(&held, &original).unwrap();
    drop(storage);
    let reopened = Storage::open(dir.path()).unwrap();
    reopened
        .with_connection(|db| {
            let count: i64 = db.query_row("SELECT COUNT(*) FROM films", [], |row| row.get(0))?;
            assert_eq!(count, 1);
            Ok(())
        })
        .unwrap();
}

#[test]
fn existing_rollback_file_blocks_another_restore_to_keep_recovery_unambiguous() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).unwrap();
    storage
        .with_connection(|db| {
            db.execute("INSERT INTO films (id) VALUES ('film-a')", [])?;
            db.execute(
                "INSERT INTO film_manual (film_id, title) VALUES ('film-a', '当前资料')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let backup = storage.create_backup().unwrap();
    std::fs::copy(
        dir.path().join("library.sqlite3"),
        dir.path()
            .join("rollback-0123456789abcdef0123456789abcdef.tmp"),
    )
    .unwrap();
    assert!(storage.restore_backup(&backup).is_err());
    storage
        .with_connection(|db| {
            let title: String = db.query_row(
                "SELECT title FROM film_manual WHERE film_id='film-a'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(title, "当前资料");
            Ok(())
        })
        .unwrap();
}

#[test]
fn backup_list_survives_restart_and_only_includes_completed_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).unwrap();
    assert!(storage.list_backups().unwrap().is_empty());
    let backup = storage.create_backup().unwrap();
    std::fs::write(
        dir.path().join("backups").join("snapshot-incomplete.tmp"),
        b"partial",
    )
    .unwrap();
    drop(storage);

    let reopened = Storage::open(dir.path()).unwrap();
    let backups = reopened.list_backups().unwrap();
    assert_eq!(backups.len(), 1);
    assert_eq!(backups[0].id, backup);
    assert!(backups[0].created_at_unix_ms > 0);
}

#[test]
fn schema12_status_removal_preserves_values_and_restores_old_backup() {
    let home=tempfile::tempdir().unwrap();let root=home.path().join("data");
    let storage=Storage::open(&root).unwrap();
    storage.with_transaction(|tx| {
        legacy_schema::remove_actor_v17(tx)?; tx.execute_batch("INSERT INTO libraries(id,display_name) VALUES('source','Synthetic'); INSERT INTO films(id) VALUES('film'); INSERT INTO film_manual(film_id,rating,favorite) VALUES('film',9,1); INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('video','source','film','synthetic.mp4','video'); ALTER TABLE file_observations ADD COLUMN availability TEXT NOT NULL DEFAULT 'unverified'; UPDATE file_observations SET availability='available'; PRAGMA user_version=12;")?;
        Ok(())
    }).unwrap();
    let backup=storage.create_backup().unwrap();drop(storage);
    let reopened=Storage::open(&root).unwrap();
    let verify=|storage:&Storage| {storage.with_connection(|db| {
        assert_eq!(db.query_row("SELECT count(*) FROM pragma_table_info('file_observations') WHERE name='availability'",[],|r|r.get::<_,i64>(0))?,0);
        assert_eq!(db.query_row("SELECT rating,favorite FROM film_manual WHERE film_id='film'",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?,(9,1));
        assert_eq!(db.query_row("SELECT observed_path FROM file_observations WHERE id='video'",[],|r|r.get::<_,String>(0))?,"synthetic.mp4");
        Ok(())
    }).unwrap();};
    verify(&reopened);
    reopened.with_connection(|db|db.execute("UPDATE film_manual SET rating=1",[])).unwrap();
    reopened.restore_backup(&backup).unwrap();verify(&reopened);
    assert_eq!(reopened.schema_version().unwrap(), 23);
}
