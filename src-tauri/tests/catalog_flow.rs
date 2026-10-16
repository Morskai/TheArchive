use local_film_library_preview_lib::catalog::{ensure_film_entries, list_films};
use local_film_library_preview_lib::observation::{ScanResult, scan_into_library};
use local_film_library_preview_lib::storage::Storage;
use std::fs::{self, File};
use std::path::Path;

fn touch(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    File::create(path).unwrap();
}

fn setup() -> (tempfile::TempDir, Storage) {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("fictional-media");
    fs::create_dir(&media).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO libraries (id, display_name, root_path) VALUES ('source', '虚构来源', ?1)", [media.to_string_lossy().as_ref()])?;
        Ok(())
    }).unwrap();
    (home, storage)
}

fn insert_observation(storage: &Storage, id: &str, path: &str, kind: &str) {
    storage.with_connection(|db| {
        db.execute("INSERT INTO file_observations (id, library_id, observed_path, kind) VALUES (?1, 'source', ?2, ?3)", (id, path, kind))?;
        Ok(())
    }).unwrap();
}

fn film_id(storage: &Storage, observation_id: &str) -> Option<String> {
    storage
        .with_connection(|db| {
            db.query_row(
                "SELECT film_id FROM file_observations WHERE id=?1",
                [observation_id],
                |row| row.get(0),
            )
        })
        .unwrap()
}

#[test]
fn existing_observations_gain_stable_film_ids_after_restart() {
    let (home, storage) = setup();
    insert_observation(&storage, "a", "A [SAME-01]/one.mp4", "video");
    insert_observation(&storage, "b", "B [SAME-01]/two.mp4", "video");
    insert_observation(&storage, "image", "A [SAME-01]/cover.webp", "image");
    assert_eq!(ensure_film_entries(&storage).unwrap(), 2);
    let first = film_id(&storage, "a").unwrap();
    let second = film_id(&storage, "b").unwrap();
    assert_ne!(first, second);
    assert_eq!(film_id(&storage, "image"), None);
    assert_eq!(storage.list_protections().unwrap().len(), 1);
    drop(storage);
    let reopened = Storage::open(&home.path().join("data")).unwrap();
    assert_eq!(ensure_film_entries(&reopened).unwrap(), 0);
    assert_eq!(film_id(&reopened, "a"), Some(first));
    assert_eq!(film_id(&reopened, "b"), Some(second));
    assert_eq!(list_films(&reopened).unwrap().len(), 2);
}

#[test]
fn new_and_repeated_scans_keep_directory_versions_and_cross_directory_entries() {
    let (home, storage) = setup();
    let media = home.path().join("fictional-media");
    touch(&media, "A [SAME-01]/one.mp4");
    touch(&media, "B [SAME-01]/two.mp4");
    touch(&media, "C [OTHER-02]/part1.mp4");
    touch(&media, "C [OTHER-02]/part2.mp4");
    assert!(matches!(
        scan_into_library(&storage, "source", &media).unwrap(),
        ScanResult::Complete(_)
    ));
    let first = list_films(&storage).unwrap();
    assert_eq!(first.len(), 3);
    assert!(first.iter().all(|film| film.needs_organization));
    assert_eq!(first.iter().map(|film| film.videos.len()).sum::<usize>(), 4);
    assert_eq!(
        first.iter().filter(|film| film.videos.len() == 2).count(),
        1
    );
    assert_eq!(
        first
            .iter()
            .map(|film| &film.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );
    assert!(matches!(
        scan_into_library(&storage, "source", &media).unwrap(),
        ScanResult::Complete(_)
    ));
    let second = list_films(&storage).unwrap();
    assert_eq!(
        second
            .iter()
            .map(|film| &film.id)
            .collect::<std::collections::HashSet<_>>(),
        first.iter().map(|film| &film.id).collect()
    );
    touch(&media, "C [OTHER-02]/part3.mp4");
    scan_into_library(&storage, "source", &media).unwrap();
    assert_eq!(list_films(&storage).unwrap().len(), 3);
    assert_eq!(
        list_films(&storage)
            .unwrap()
            .iter()
            .map(|film| film.videos.len())
            .sum::<usize>(),
        5
    );
}

#[test]
fn film_link_failure_rolls_back_every_new_entry() {
    let (_home, storage) = setup();
    insert_observation(&storage, "a", "a.mp4", "video");
    insert_observation(&storage, "z", "z.mp4", "video");
    storage.with_connection(|db| {
        db.execute_batch("CREATE TRIGGER reject_z BEFORE UPDATE OF film_id ON file_observations WHEN NEW.id='z' BEGIN SELECT RAISE(FAIL, 'injected failure'); END;")?;
        Ok(())
    }).unwrap();
    assert!(ensure_film_entries(&storage).is_err());
    assert_eq!(film_id(&storage, "a"), None);
    assert_eq!(film_id(&storage, "z"), None);
    assert!(list_films(&storage).unwrap().is_empty());
    assert_eq!(storage.list_protections().unwrap().len(), 1);
}

#[test]
fn protection_failure_leaves_existing_observations_unlinked() {
    let (home, storage) = setup();
    insert_observation(&storage, "old", "old.mp4", "video");
    File::create(home.path().join("data/protections")).unwrap();
    assert!(ensure_film_entries(&storage).is_err());
    assert_eq!(film_id(&storage, "old"), None);
    assert!(list_films(&storage).unwrap().is_empty());
}

#[test]
fn manual_film_data_and_existing_links_survive_upgrade_and_absence() {
    let (home, storage) = setup();
    storage.with_connection(|db| {
        db.execute("INSERT INTO films (id) VALUES ('manual-film')", [])?;
        db.execute("INSERT INTO film_manual (film_id, title, rating, favorite) VALUES ('manual-film', '人工标题', 9, 1)", [])?;
        db.execute("INSERT INTO file_observations (id, library_id, film_id, observed_path, kind) VALUES ('linked', 'source', 'manual-film', 'linked.mp4', 'video')", [])?;
        Ok(())
    }).unwrap();
    insert_observation(&storage, "new", "new.mp4", "video");
    assert_eq!(ensure_film_entries(&storage).unwrap(), 1);
    assert_eq!(film_id(&storage, "linked").as_deref(), Some("manual-film"));
    assert_eq!(film_id(&storage, "new").as_deref(), Some("manual-film"));
    let media = home.path().join("fictional-media");
    fs::rename(&media, home.path().join("temporarily-offline")).unwrap();
    assert!(matches!(
        scan_into_library(&storage, "source", &media).unwrap(),
        ScanResult::RootUnavailable { .. }
    ));
    storage
        .with_connection(|db| {
            let manual: (String, i64, i64) = db.query_row(
                "SELECT title, rating, favorite FROM film_manual WHERE film_id='manual-film'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            assert_eq!(manual, ("人工标题".into(), 9, 1));
            Ok(())
        })
        .unwrap();
    let catalog = list_films(&storage).unwrap();
    assert_eq!(catalog.len(), 1);
    let manual = catalog
        .iter()
        .find(|film| film.id == "manual-film")
        .unwrap();
    assert_ne!(manual.title, "人工标题");
    assert!(!manual.needs_organization);
    assert_eq!(manual.videos[0].id, "linked");
}

#[test]
fn favorite_toggle_persists_after_reopen_and_keeps_other_manual_fields() {
    let (home, storage) = setup();
    storage.with_connection(|db| {
        db.execute("INSERT INTO films (id) VALUES ('favorite-film')", [])?;
        db.execute("INSERT INTO film_manual (film_id, title, rating) VALUES ('favorite-film', '虚构人工标题', 8)", [])?;
        Ok(())
    }).unwrap();
    local_film_library_preview_lib::catalog::set_favorite(&storage, "favorite-film", true).unwrap();
    local_film_library_preview_lib::catalog::set_favorite(&storage, "favorite-film", false).unwrap();
    drop(storage);
    let reopened = Storage::open(&home.path().join("data")).unwrap();
    reopened.with_connection(|db| {
        let row: (String, i64, bool) = db.query_row("SELECT title,rating,favorite FROM film_manual WHERE film_id='favorite-film'", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?;
        assert_eq!(row, ("虚构人工标题".into(), 8, false));
        Ok(())
    }).unwrap();
}

#[test]
fn favorite_failed_write_rolls_back_to_saved_state() {
    let (_home, storage) = setup();
    storage.with_connection(|db| {
        db.execute("INSERT INTO films (id) VALUES ('favorite-film')", [])?;
        db.execute("INSERT INTO film_manual (film_id,favorite) VALUES ('favorite-film',1)", [])?;
        db.execute_batch("CREATE TRIGGER reject_favorite AFTER UPDATE OF favorite ON film_manual BEGIN SELECT RAISE(ABORT, 'isolated failure'); END;")?;
        Ok(())
    }).unwrap();
    assert!(local_film_library_preview_lib::catalog::set_favorite(&storage, "favorite-film", false).is_err());
    storage.with_connection(|db| {
        assert!(db.query_row("SELECT favorite FROM film_manual WHERE film_id='favorite-film'", [], |row| row.get::<_, bool>(0))?);
        db.execute_batch("DROP TRIGGER reject_favorite;")?;
        Ok(())
    }).unwrap();
    local_film_library_preview_lib::catalog::set_favorite(&storage, "favorite-film", false).unwrap();
}
