#[path="support/legacy_schema.rs"] mod legacy_schema;
use local_film_library_preview_lib::{
    actors, catalog,
    observation::{ScanResult, scan_into_library},
    storage::Storage,
};
use std::fs;

#[test]
fn removed_manual_cast_stays_removed_after_scan_reopen_and_backup() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("source");
    fs::create_dir_all(root.join("甲/[TEST-1] film")).unwrap();
    fs::write(root.join("甲/actor.jpg"), []).unwrap();
    fs::write(root.join("甲/[TEST-1] film/main.mp4"), []).unwrap();
    let data = home.path().join("data");
    let storage = Storage::open(&data).unwrap();
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO libraries(id,display_name,root_path) VALUES('s','合成',?1)",
                [root.to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(matches!(
        scan_into_library(&storage, "s", &root).unwrap(),
        ScanResult::Complete(_)
    ));
    let film = catalog::list_films(&storage).unwrap().remove(0);
    storage.with_connection(|db| {
        db.execute("INSERT INTO actors(id,observed_name) VALUES('saved-actor','甲')",[])?;
        db.execute("INSERT INTO film_actors(film_id,actor_id,source) VALUES(?1,'saved-actor','manual')",[&film.id])?;
        Ok(())
    }).unwrap();
    let actor = actors::list_actors(&storage).unwrap().remove(0);
    actors::remove_film_cast_actor(&storage, &film.id, &actor.id).unwrap();
    assert!(catalog::list_films(&storage).unwrap()[0].actors.is_empty());
    let backup = storage.create_backup().unwrap();
    scan_into_library(&storage, "s", &root).unwrap();
    assert!(catalog::list_films(&storage).unwrap()[0].actors.is_empty());
    storage.restore_backup(&backup).unwrap();
    drop(storage);
    let storage = Storage::open(&data).unwrap();
    assert!(catalog::list_films(&storage).unwrap()[0].actors.is_empty());
    assert_eq!(actors::list_actors(&storage).unwrap().len(), 1);
}

#[test]
fn new_schema_removes_only_unused_columns_and_preserves_preferred() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    storage.with_connection(|db|{
 for (table,column) in [("film_manual","notes"),("actor_manual","notes"),("file_versions","purpose"),("file_versions","notes")] {
 assert_eq!(db.query_row("SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name=?2",[table,column],|r|r.get::<_,i64>(0))?,0,"{table}.{column}");
 }
 assert_eq!(db.query_row("SELECT COUNT(*) FROM pragma_table_info('file_versions') WHERE name='preferred'",[],|r|r.get::<_,i64>(0))?,1);
 Ok(())}).unwrap();
}

#[test]
fn v15_upgrade_protects_removed_columns_and_preserves_other_manual_data() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("source");
    fs::create_dir_all(root.join("甲/[AB-1] Original title")).unwrap();
    fs::write(root.join("甲/actor.jpg"), []).unwrap();
    fs::write(root.join("甲/[AB-1] Original title/a.mp4"), []).unwrap();
    let data = home.path().join("data");
    let storage = Storage::open(&data).unwrap();
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO libraries(id,display_name,root_path) VALUES('s','合成',?1)",
                [root.to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
        .unwrap();
    scan_into_library(&storage, "s", &root).unwrap();
    let film = catalog::list_films(&storage).unwrap().remove(0);
    storage.with_connection(|db| {db.execute("INSERT INTO actors(id,observed_name) VALUES('saved-actor','甲')",[])?;Ok(())}).unwrap();
    let actor = actors::list_actors(&storage).unwrap().remove(0);
    storage.with_connection(|db|{
  legacy_schema::remove_actor_v17(db)?; db.execute_batch("ALTER TABLE film_manual ADD COLUMN notes TEXT; ALTER TABLE actor_manual ADD COLUMN notes TEXT; ALTER TABLE file_versions ADD COLUMN purpose TEXT NOT NULL DEFAULT 'undetermined' CHECK(purpose IN ('main','part','extra','undetermined')); ALTER TABLE file_versions ADD COLUMN notes TEXT; ALTER TABLE film_manual DROP COLUMN cast_override; ALTER TABLE actor_manual DROP COLUMN aliases; ALTER TABLE libraries DROP COLUMN scan_mode; DROP TABLE info_actor_links; PRAGMA user_version=15;")?;
  db.execute("INSERT INTO film_manual(film_id,code,rating,favorite,notes) VALUES(?1,'人工编号',9,1,'旧作品备注')",[&film.id])?;
  db.execute("INSERT INTO actor_manual(actor_id,height_cm,favorite,notes) VALUES(?1,165,1,'旧演员备注')",[&actor.id])?;
  db.execute("INSERT INTO file_versions(observation_id,preferred,purpose,notes) SELECT id,1,'main','旧视频备注' FROM file_observations WHERE kind='video'",[])?;
  Ok(())
 }).unwrap();
    let backup = storage.create_backup().unwrap();
    let earlier_protections = storage.list_protections().unwrap();
    drop(storage);
    let storage = Storage::open(&data).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    let protections = storage.list_protections().unwrap();
    assert_eq!(protections.len(), earlier_protections.len() + 1);
    let new_protection = protections
        .iter()
        .find(|name| !earlier_protections.contains(name))
        .unwrap();
    let old = rusqlite::Connection::open(data.join("protections").join(new_protection)).unwrap();
    assert_eq!(
        old.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        15
    );
    assert_eq!(
        old.query_row::<String, _, _>(
            "SELECT notes FROM film_manual WHERE film_id=?1",
            [&film.id],
            |r| r.get(0)
        )
        .unwrap(),
        "旧作品备注"
    );
    assert_eq!(
        old.query_row::<String, _, _>("SELECT purpose FROM file_versions LIMIT 1", [], |r| r
            .get(0))
            .unwrap(),
        "main"
    );
    storage.restore_backup(&backup).unwrap();
    let current = catalog::list_films(&storage).unwrap().remove(0);
    assert_eq!(current.id, film.id);
    assert_eq!(current.manual_code.as_deref(), Some("人工编号"));
    assert_eq!(current.rating, Some(4.5));
    assert!(current.favorite);
    let current_actor = actors::list_actors(&storage).unwrap().remove(0);
    assert_eq!(current_actor.id, actor.id);
    assert_eq!(current_actor.height, "165");
    assert!(current_actor.favorite);
    storage
        .with_connection(|db| {
            assert_eq!(
                db.query_row::<i64, _, _>(
                    "SELECT preferred FROM file_versions LIMIT 1",
                    [],
                    |r| r.get(0)
                )?,
                1
            );
            for (table, column) in [
                ("film_manual", "notes"),
                ("actor_manual", "notes"),
                ("file_versions", "purpose"),
                ("file_versions", "notes"),
            ] {
                assert_eq!(
                    db.query_row::<i64, _, _>(
                        "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name=?2",
                        [table, column],
                        |r| r.get(0)
                    )?,
                    0
                );
            }
            Ok(())
        })
        .unwrap();
}
