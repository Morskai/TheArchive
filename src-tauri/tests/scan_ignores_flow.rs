#[path="support/legacy_schema.rs"] mod legacy_schema;
#[path="support/empty_credentials.rs"] mod empty_credentials;
use local_film_library_preview_lib::{
    catalog, lifecycle,
    observation::{ScanResult, scan_into_library},
    problem_folders, problems, reset, scan_rules,
    storage::Storage,
};
use std::{fs, path::Path};

fn file(root: &Path, name: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, []).unwrap();
}
fn photo(root: &Path, name: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::new(12, 18).save(path).unwrap();
}
fn bind(storage: &Storage, root: &Path) {
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO libraries(id,display_name,root_path) VALUES('source','虚构来源',?1)",
                [root.to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
        .unwrap();
}
fn scan(storage: &Storage, root: &Path) {
    assert!(matches!(
        scan_into_library(storage, "source", root).unwrap(),
        ScanResult::Complete(_)
    ));
}

#[test]
fn empty_folder_with_ignored_videos_never_creates_directory_problems() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source"); let data = home.path().join("data");
    file(&root,"[EMPTY] empty/photos/hidden.mp4");
    let storage=Storage::open(&data).unwrap(); bind(&storage,&root); scan_rules::save(&storage,&["photos".into()]).unwrap(); scan(&storage,&root);
    assert!(problem_folders::list(&storage).unwrap().is_empty()); assert!(catalog::list_films(&storage).unwrap().is_empty());
    drop(storage); let storage=Storage::open(&data).unwrap(); scan(&storage,&root);
    assert!(problems::list(&storage).unwrap().is_empty());
}

#[test]
fn ignore_names_apply_at_scan_boundaries_preserving_history_and_manual_data() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("source");
    photo(&root, "Actor/actor.jpg");
    file(&root, "Actor/Photos/first.mp4");
    file(&root, "Actor/Photos-extra/main.mp4");
    fs::create_dir_all(root.join("Actor/empty")).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    bind(&storage, &root);
    scan(&storage, &root);
    let film = catalog::list_films(&storage)
        .unwrap()
        .into_iter()
        .find(|f| f.videos.iter().any(|v| v.observed_path.contains("Photos/")))
        .unwrap();
    catalog::set_favorite(&storage, &film.id, true).unwrap();
    scan_rules::save(&storage, &[" photos ".into()]).unwrap();
    assert_eq!(
        catalog::list_films(&storage).unwrap().len(),
        2,
        "saved settings do not scan"
    );
    scan(&storage, &root);
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 1);
    assert!(
        !problems::list(&storage)
            .unwrap()
            .iter()
            .any(|p| p.directories.iter().any(|d| d.ends_with("Photos")))
    );
    storage
        .with_connection(|db| {
            assert!(db.query_row(
                "SELECT favorite FROM film_manual WHERE film_id=?1",
                [&film.id],
                |r| r.get::<_, bool>(0)
            )?);
            Ok(())
        })
        .unwrap();
    scan_rules::save(&storage, &[]).unwrap();
    scan(&storage, &root);
    assert!(
        catalog::list_films(&storage)
            .unwrap()
            .iter()
            .any(|f| f.id == film.id && f.favorite)
    );
    assert!(root.join("Actor/Photos/first.mp4").exists());
}

#[test]
fn ignore_settings_validation_failure_reset_defaults_and_explicit_backup_restore() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("data");
    let storage = Storage::open(&data).unwrap();
    scan_rules::save(&storage, &["写真".into(), "Photos".into()]).unwrap();
    for names in [
        vec!["../photos"],
        vec!["photos", "PHOTOS"],
        vec![""],
        vec!["poster.jpg/child"],
        vec!["a*"],
        vec!["CON"],
        vec!["name."],
    ] {
        assert!(
            scan_rules::save(
                &storage,
                &names.into_iter().map(String::from).collect::<Vec<_>>()
            )
            .is_err()
        );
    }
    let backup = storage.create_backup().unwrap();
    storage.with_connection(|db| { db.execute_batch("CREATE TRIGGER reject_ignore BEFORE INSERT ON ignored_directory_names WHEN NEW.name='blocked' BEGIN SELECT RAISE(ABORT,'synthetic'); END")?; Ok(()) }).unwrap();
    assert!(scan_rules::save(&storage, &["new".into(), "blocked".into()]).is_err());
    assert_eq!(scan_rules::load(&storage).unwrap(), vec!["写真", "Photos"]);
    reset::restore_initial_state_with(&storage, &mut empty_credentials::EmptyCredentials::default()).unwrap();
    assert!(scan_rules::load(&storage).unwrap().is_empty());
    drop(storage);
    let storage = Storage::open(&data).unwrap();
    assert!(scan_rules::load(&storage).unwrap().is_empty());
    storage.restore_backup(&backup).unwrap();
    assert_eq!(scan_rules::load(&storage).unwrap(), vec!["写真", "Photos"]);
}

#[test]
fn real_cover_problem_write_failure_keeps_previous_snapshot_and_processing_state() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("source");
    file(&root,"[EMPTY] empty/main.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    bind(&storage, &root);
    scan(&storage, &root);
    let old = problems::list(&storage).unwrap();
    problems::mark(
        &storage,
        &old.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
    )
    .unwrap();
    let snapshot = storage
        .with_connection(|db| {
            db.query_row(
                "SELECT evidence FROM issue_records WHERE subject_type='library_scan_details'",
                [],
                |r| r.get::<_, String>(0),
            )
        })
        .unwrap();
    fs::write(root.join("[EMPTY] empty/poster.jpg"), b"synthetic broken image").unwrap();
    file(&root,"[NEW] film/new.mp4");
    storage.with_connection(|db| { db.execute_batch("CREATE TRIGGER reject_empty_cover BEFORE INSERT ON issue_records WHEN NEW.subject_type='film_missing_cover' BEGIN SELECT RAISE(ABORT,'synthetic'); END")?; Ok(()) }).unwrap();
    assert!(scan_into_library(&storage, "source", &root).is_err());
    assert_eq!(
        storage
            .with_connection(|db| db.query_row(
                "SELECT evidence FROM issue_records WHERE subject_type='library_scan_details'",
                [],
                |r| r.get::<_, String>(0)
            ))
            .unwrap(),
        snapshot
    );
    // Remove the synthetic failure before reading, since list reconciles saved evidence.
    storage
        .with_connection(|db| db.execute_batch("DROP TRIGGER reject_empty_cover"))
        .unwrap();
    assert!(problems::list(&storage).unwrap().iter().all(|p| p.handled));
}

#[test]
fn v9_upgrade_and_old_backup_restore_protect_existing_preferences() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("data");
    let storage = Storage::open(&data).unwrap();
    let prefs = lifecycle::InterfacePreferences {
        film_cover_ratio: "5:7".into(),
        reduce_motion: true,
        ..Default::default()
    };
    lifecycle::save_preferences(&storage, &prefs).unwrap();
    storage
        .with_connection(|db| {
            legacy_schema::remove_actor_v17(db)?; db.execute_batch("DROP TABLE film_translations; DROP TABLE translation_memory; DROP TABLE translation_settings; DROP TABLE ignored_directory_names; PRAGMA user_version=9;")?;
            Ok(())
        })
        .unwrap();
    let backup = storage.create_backup().unwrap();
    drop(storage);
    let storage = Storage::open(&data).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    assert!(scan_rules::load(&storage).unwrap().is_empty());
    assert_eq!(lifecycle::load_preferences(&storage).unwrap(), prefs);
    assert!(!storage.list_protections().unwrap().is_empty());
    scan_rules::save(&storage, &["写真".into()]).unwrap();
    storage.restore_backup(&backup).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    assert!(scan_rules::load(&storage).unwrap().is_empty());
    assert_eq!(lifecycle::load_preferences(&storage).unwrap(), prefs);
}

#[test]
fn preview_confirm_rechecks_ignore_names_and_reads_unexcluded_category_descendants() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("source");
    photo(&root, "Actor/actor.jpg");
    file(&root, "Actor/film/main.mp4");
    fs::create_dir_all(root.join("Actor/写真")).unwrap();
    photo(&root, "Category/Other/actor.jpg");
    file(&root, "Category/Other/film/main.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let session = local_film_library_preview_lib::source_scan::SourceScanSession::new();
    let before = session.preview(&storage, &root, None).unwrap();
    scan_rules::save(&storage, &["写真".into()]).unwrap();
    assert_eq!(
        session
            .confirm(&storage, before.token.as_deref().unwrap())
            .err()
            .unwrap()
            .to_string(),
        "scan_changed"
    );
    let preview = session.preview(&storage, &root, None).unwrap();
    assert_eq!(preview.video_count, 2);
    session
        .confirm(&storage, preview.token.as_deref().unwrap())
        .unwrap();
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 2);
    let issues = problems::list(&storage).unwrap();
    assert!(!issues.iter().any(|p| {
        p.directories
            .iter()
            .any(|d| d.contains("写真"))
    }));
    assert_eq!(
        issues
            .iter()
            .filter(|p| p.directories == vec!["Category"])
            .count(),
        0
    );
}
