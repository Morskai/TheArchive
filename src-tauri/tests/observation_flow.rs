use local_film_library_preview_lib::observation::{
    ObservationImport, ScanResult, scan_into_library,
};
use local_film_library_preview_lib::storage::Storage;
use rusqlite::params;
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
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO libraries (id, display_name, root_path) VALUES ('test-library', '虚构测试片库', ?1)",
                [media.to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
        .unwrap();
    (home, storage)
}

fn complete(result: ScanResult) -> ObservationImport {
    match result {
        ScanResult::Complete(import) => import,
        other => panic!("expected complete scan, got {other:?}"),
    }
}

fn paths(storage: &Storage) -> Vec<(String, String, Option<String>)> {
    storage
        .with_connection(|db| {
            let mut statement = db.prepare(
                "SELECT observed_path, kind, film_id FROM file_observations ORDER BY observed_path",
            )?;
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect()
        })
        .unwrap()
}

fn observation_state(storage: &Storage) -> Vec<(String, String)> {
    storage.with_connection(|db| {
        let mut statement = db.prepare(
            "SELECT id, observed_path FROM file_observations ORDER BY observed_path, id",
        )?;
        statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect()
    }).unwrap()
}

#[test]
fn first_scan_records_assets_and_links_only_the_video_to_a_film() {
    let (home, storage) = setup();
    let media = home.path().join("media");
    touch(&media, "演员甲 [DEMO-101]/DEMO-101.mp4");
    touch(&media, "演员甲 [DEMO-101]/封面.webp");
    touch(&media, "演员甲 [DEMO-101]/字幕.srt");
    touch(&media, "演员甲 [DEMO-101]/readme.txt");

    let result = complete(scan_into_library(&storage, "test-library", &media).unwrap());
    assert_eq!(result.discovered, 3);
    assert_eq!(result.inserted, 3);
    assert!(result.missing.is_empty());
    let observed = paths(&storage);
    let film_id = observed[0].2.clone().expect("video must have a film entry");
    assert!(film_id.starts_with("auto-"));
    assert_eq!(
        observed,
        vec![
            (
                "演员甲 [DEMO-101]/DEMO-101.mp4".into(),
                "video".into(),
                Some(film_id)
            ),
            ("演员甲 [DEMO-101]/字幕.srt".into(), "subtitle".into(), None),
            ("演员甲 [DEMO-101]/封面.webp".into(), "image".into(), None),
        ]
    );
    storage
        .with_connection(|db| {
            let films: i64 = db.query_row("SELECT COUNT(*) FROM films", [], |row| row.get(0))?;
            let actors: i64 = db.query_row("SELECT COUNT(*) FROM actors", [], |row| row.get(0))?;
            assert_eq!((films, actors), (1, 0));
            Ok(())
        })
        .unwrap();
}

#[test]
fn repeat_scan_keeps_existing_observation_and_manual_data_then_adds_new_file() {
    let (home, storage) = setup();
    let media = home.path().join("media");
    touch(&media, "同号甲/DEMO-101.mp4");
    storage.with_connection(|db| {
        db.execute("INSERT INTO films (id) VALUES ('manual-film')", [])?;
        db.execute("INSERT INTO film_manual (film_id, title, rating, favorite) VALUES ('manual-film', '人工标题', 9, 1)", [])?;
        db.execute("INSERT INTO actors (id, observed_name) VALUES ('manual-actor', '虚构演员')", [])?;
        db.execute("INSERT INTO actor_manual (actor_id, aliases) VALUES ('manual-actor', '人工别名')", [])?;
        db.execute("INSERT INTO issue_records (id, subject_type, subject_id, evidence_key, evidence, resolution) VALUES ('issue-1', 'film', 'manual-film', 'evidence-1', '虚构证据', '已处理')", [])?;
        Ok(())
    }).unwrap();

    assert_eq!(
        complete(scan_into_library(&storage, "test-library", &media).unwrap()).inserted,
        1
    );
    let first_id: String = storage
        .with_connection(|db| {
            db.query_row(
                "SELECT id FROM file_observations WHERE observed_path='同号甲/DEMO-101.mp4'",
                [],
                |row| row.get(0),
            )
        })
        .unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO file_versions (observation_id, preferred) VALUES (?1, 1)", [&first_id])?;
        db.execute("INSERT INTO player_launch_events (id, film_id, observation_id) VALUES ('launch-1', 'manual-film', ?1)", [&first_id])?;
        Ok(())
    }).unwrap();
    assert_eq!(
        complete(scan_into_library(&storage, "test-library", &media).unwrap()).inserted,
        0
    );
    touch(&media, "同号乙/DEMO-101.mp4");
    assert_eq!(
        complete(scan_into_library(&storage, "test-library", &media).unwrap()).inserted,
        1
    );
    fs::remove_file(media.join("同号甲/DEMO-101.mp4")).unwrap();
    let absent = complete(scan_into_library(&storage, "test-library", &media).unwrap());
    assert_eq!(absent.inserted, 0);
    assert_eq!(absent.missing.len(), 1);
    assert_eq!(absent.missing[0].observation_id, first_id);
    assert_eq!(absent.missing[0].observed_path, "同号甲/DEMO-101.mp4");
    touch(&media, "同号甲/DEMO-101.mp4");
    let returned = complete(scan_into_library(&storage, "test-library", &media).unwrap());
    assert_eq!(returned.inserted, 0);
    assert!(returned.missing.is_empty());
    assert_eq!(paths(&storage).len(), 2);
    storage
        .with_connection(|db| {
            let id: String = db.query_row(
                "SELECT id FROM file_observations WHERE observed_path='同号甲/DEMO-101.mp4'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(id, first_id);
            let manual: (String, i64, i64) = db.query_row(
                "SELECT title, rating, favorite FROM film_manual WHERE film_id='manual-film'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            assert_eq!(manual, ("人工标题".into(), 9, 1));
            let aliases: String = db.query_row(
                "SELECT aliases FROM actor_manual WHERE actor_id='manual-actor'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(aliases, "人工别名");
            let resolution: String = db.query_row(
                "SELECT resolution FROM issue_records WHERE id='issue-1'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(resolution, "已处理");
            let version: i64 = db.query_row(
                "SELECT preferred FROM file_versions WHERE observation_id=?1",
                [&first_id],
                |row| row.get(0),
            )?;
            assert_eq!(version, 1);
            let launches: i64 = db.query_row(
                "SELECT COUNT(*) FROM player_launch_events WHERE id='launch-1'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(launches, 1);
            Ok(())
        })
        .unwrap();
}

#[test]
fn disconnected_bound_root_leaves_existing_rows_unchanged() {
    let (home, storage) = setup();
    let media = home.path().join("media");
    touch(&media, "synthetic-film/existing.mp4");
    scan_into_library(&storage, "test-library", &media).unwrap();
    let before = observation_state(&storage);
    fs::rename(&media, home.path().join("disconnected-temp-media")).unwrap();
    assert!(matches!(
        scan_into_library(&storage, "test-library", &media).unwrap(),
        ScanResult::RootUnavailable { .. }
    ));
    assert_eq!(observation_state(&storage), before);
}

#[test]
fn incomplete_tree_with_a_skipped_link_does_not_commit_discovered_files() {
    let (home, storage) = setup();
    let media = home.path().join("media");
    let outside = home.path().join("outside");
    fs::create_dir(&outside).unwrap();
    touch(&media, "synthetic-film/existing.mp4");
    scan_into_library(&storage, "test-library", &media).unwrap();
    let before = observation_state(&storage);
    touch(&media, "synthetic-film/new.mp4");
    let link = media.join("linked");
    #[cfg(windows)]
    {
        let created = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let result = scan_into_library(&storage, "test-library", &media).unwrap();
    match result {
        ScanResult::Incomplete { issues } => {
            assert!(issues.iter().any(|issue| issue.relative_path == "linked"));
        }
        other => panic!("expected incomplete scan, got {other:?}"),
    }
    assert_eq!(observation_state(&storage), before);
}

#[test]
fn wrong_empty_root_with_same_name_cannot_produce_missing_evidence() {
    let (home, storage) = setup();
    let media = home.path().join("media");
    touch(&media, "synthetic-film/existing.mp4");
    complete(scan_into_library(&storage, "test-library", &media).unwrap());
    let before = observation_state(&storage);
    let other = home.path().join("other").join("media");
    fs::create_dir_all(&other).unwrap();
    assert!(matches!(
        scan_into_library(&storage, "test-library", &other).unwrap(),
        ScanResult::WrongRoot
    ));
    assert_eq!(observation_state(&storage), before);
}

#[test]
fn database_failure_rolls_back_all_new_observations() {
    let (home, storage) = setup();
    let media = home.path().join("media");
    touch(&media, "synthetic-film/existing.mp4");
    scan_into_library(&storage, "test-library", &media).unwrap();
    let before = paths(&storage);
    touch(&media, "synthetic-film/a.mp4");
    touch(&media, "synthetic-film/z.mp4");
    storage.with_connection(|db| {
        db.execute_batch("CREATE TRIGGER reject_z BEFORE INSERT ON file_observations WHEN NEW.observed_path='synthetic-film/z.mp4' BEGIN SELECT RAISE(FAIL, 'injected write failure'); END;")?;
        Ok(())
    }).unwrap();
    assert!(scan_into_library(&storage, "test-library", &media).is_err());
    assert_eq!(paths(&storage), before);
    storage
        .with_connection(|db| {
            let count: i64 = db.query_row(
                "SELECT COUNT(*) FROM file_observations WHERE library_id=?1",
                params!["test-library"],
                |row| row.get(0),
            )?;
            assert_eq!(count, 1);
            Ok(())
        })
        .unwrap();
}
