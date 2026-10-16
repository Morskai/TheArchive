use local_film_library_preview_lib::source_scan::SourceScanSession;
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
    let media = home.path().join("media");
    fs::create_dir(&media).unwrap();
    touch(&media, "synthetic-film/old.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO libraries (id, display_name, root_path) VALUES ('existing', '虚构来源', ?1)", [media.to_string_lossy().as_ref()])?;
        db.execute("INSERT INTO films (id) VALUES ('manual-film')", [])?;
        db.execute("INSERT INTO film_manual (film_id, title, rating) VALUES ('manual-film', '人工标题', 8)", [])?;
        db.execute("INSERT INTO file_observations (id, library_id, film_id, observed_path, kind) VALUES ('old-id', 'existing', 'manual-film', 'synthetic-film/old.mp4', 'video')", [])?;
        Ok(())
    }).unwrap();
    (home, storage)
}

fn unchanged(storage: &Storage) {
    storage
        .with_connection(|db| {
            let rows: i64 = db.query_row("SELECT COUNT(*) FROM file_observations", [], |row| {
                row.get(0)
            })?;
            let old: (String, String) = db.query_row(
            "SELECT id, film_id FROM file_observations WHERE observed_path='synthetic-film/old.mp4'",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
            let manual: (String, i64) = db.query_row(
                "SELECT title, rating FROM film_manual WHERE film_id='manual-film'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            assert_eq!(rows, 1);
            assert_eq!(
                old,
                ("old-id".into(), "manual-film".into())
            );
            assert_eq!(manual, ("人工标题".into(), 8));
            Ok(())
        })
        .unwrap();
}

#[test]
fn wrong_selected_root_cannot_replace_bound_observations() {
    let (home, storage) = setup();
    let wrong = home.path().join("wrong");
    fs::create_dir(&wrong).unwrap();
    let session = SourceScanSession::new();
    assert!(session.preview(&storage, &wrong, Some("existing")).is_err());
    assert!(
        session
            .preview(&storage, &home.path().join("media"), None)
            .is_err()
    );
    unchanged(&storage);
}

#[test]
fn incomplete_traversal_blocks_confirmation_and_preserves_records() {
    let (home, storage) = setup();
    let media = home.path().join("media");
    let outside = home.path().join("outside");
    fs::create_dir(&outside).unwrap();
    touch(&media, "synthetic-film/new.mp4");
    let link = media.join("linked");
    #[cfg(windows)]
    {
        let result = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(result.status.success());
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let session = SourceScanSession::new();
    let preview = session.preview(&storage, &media, Some("existing")).unwrap();
    assert!(!preview.complete);
    assert!(preview.issue_count > 0);
    assert_eq!(preview.unreadable_directory_count, 0);
    assert!(preview.token.is_none());
    unchanged(&storage);
}

#[test]
fn cancelling_confirmation_writes_nothing() {
    let (home, storage) = setup();
    let media = home.path().join("media");
    touch(&media, "synthetic-film/new.mp4");
    let session = SourceScanSession::new();
    let token = session
        .preview(&storage, &media, Some("existing"))
        .unwrap()
        .token
        .unwrap();
    session.cancel(&token);
    assert!(session.confirm(&storage, &token).is_err());
    unchanged(&storage);
}

#[test]
fn failed_protection_stops_before_first_real_observation_write() {
    let (home, storage) = setup();
    let media = home.path().join("media");
    touch(&media, "synthetic-film/new.mp4");
    let session = SourceScanSession::new();
    let token = session
        .preview(&storage, &media, Some("existing"))
        .unwrap()
        .token
        .unwrap();
    File::create(home.path().join("data/protections")).unwrap();
    assert!(session.confirm(&storage, &token).is_err());
    unchanged(&storage);
}

#[test]
fn failed_import_rolls_back_all_new_observations_and_keeps_manual_data() {
    let (home, storage) = setup();
    let media = home.path().join("media");
    touch(&media, "synthetic-film/a.mp4");
    touch(&media, "synthetic-film/z.mp4");
    let session = SourceScanSession::new();
    let token = session
        .preview(&storage, &media, Some("existing"))
        .unwrap()
        .token
        .unwrap();
    storage.with_connection(|db| {
        db.execute_batch("CREATE TRIGGER reject_z BEFORE INSERT ON file_observations WHEN NEW.observed_path='synthetic-film/z.mp4' BEGIN SELECT RAISE(FAIL, 'injected failure'); END;")?;
        Ok(())
    }).unwrap();
    assert!(session.confirm(&storage, &token).is_err());
    unchanged(&storage);
    assert_eq!(storage.list_protections().unwrap().len(), 1);
}

#[test]
fn confirmed_new_source_binds_root_and_links_video_observation() {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("fictional-media");
    touch(&media, "[DEMO-001]/sample.mp4");
    touch(&media, "[DEMO-001]/cover.webp");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let session = SourceScanSession::new();
    let preview = session.preview(&storage, &media, None).unwrap();
    assert!(preview.complete);
    assert_eq!(preview.video_count, 1);
    let committed = session.confirm(&storage, &preview.token.unwrap()).unwrap();
    assert_eq!(
        (
            committed.discovered,
            committed.inserted,
            committed.missing_count
        ),
        (2, 2, 0)
    );
    assert_eq!(storage.list_protections().unwrap().len(), 1);
    storage
        .with_connection(|db| {
            let films: i64 = db.query_row("SELECT COUNT(*) FROM films", [], |row| row.get(0))?;
            let actors: i64 = db.query_row("SELECT COUNT(*) FROM actors", [], |row| row.get(0))?;
            let bound: String = db.query_row(
                "SELECT root_path FROM libraries WHERE id=?1",
                [&committed.library_id],
                |row| row.get(0),
            )?;
            let observations: i64 = db.query_row(
                "SELECT COUNT(*) FROM file_observations WHERE library_id=?1 AND film_id IS NULL",
                [&committed.library_id],
                |row| row.get(0),
            )?;
            let named: i64 = db.query_row(
                "SELECT COUNT(*) FROM file_observations WHERE library_id=?1 AND file_name IS NOT NULL",
                [&committed.library_id],
                |row| row.get(0),
            )?;
            assert_eq!((films, actors, observations), (1, 0, 1));
            assert_eq!(named, 0);
            assert_eq!(
                Path::new(&bound).canonicalize().unwrap(),
                media.canonicalize().unwrap()
            );
            Ok(())
        })
        .unwrap();
}
