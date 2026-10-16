use local_film_library_preview_lib::{
    catalog,
    inventory::inventory_with_rules,
    lifecycle,
    playback::{self, PlayerSettings},
    source_scan::{self, SourceScanSession},
    storage::Storage,
    storage_session::StorageSession,
};
use std::{
    fs,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

fn fixture() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    Arc<StorageSession>,
    Arc<SourceScanSession>,
    String,
    String,
    String,
) {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-media");
    fs::create_dir_all(root.join("film")).unwrap();
    fs::write(root.join("film/movie.mp4"), []).unwrap();
    let storage = Storage::open(&home.path().join("software")).unwrap();
    let scan = SourceScanSession::new();
    let preview = scan.preview(&storage, &root, None).unwrap();
    let source = scan
        .confirm(&storage, &preview.token.unwrap())
        .unwrap()
        .library_id;
    let film = catalog::list_films(&storage).unwrap().remove(0);
    catalog::set_favorite(&storage, &film.id, true).unwrap();
    (
        home,
        root,
        Arc::new(StorageSession::new(storage)),
        Arc::new(scan),
        source,
        film.id,
        film.videos[0].id.clone(),
    )
}

#[test]
fn filesystem_collection_allows_settings_playback_and_directory_before_scan_finishes() {
    let (_home, root, state, scan, source, film, video) = fixture();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let scanner = {
        let state = state.clone();
        let scan = scan.clone();
        let root = root.clone();
        let source = source.clone();
        thread::spawn(move || {
            scan.preview_on_with(&state, &root, Some(&source), |root, known, ignored| {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                inventory_with_rules(root, known, ignored)
            })
            .unwrap()
        })
    };
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let mut workers = vec![];
    for command in ["settings", "playback", "directory"] {
        let state = state.clone();
        let done_tx = done_tx.clone();
        let source = source.clone();
        let film = film.clone();
        let video = video.clone();
        workers.push(thread::spawn(move || {
            let storage = state.lock().unwrap();
            match command {
                "settings" => playback::save_settings(
                    &storage,
                    &PlayerSettings {
                        launch_arguments: "saved-during-scan".into(),
                        ..Default::default()
                    },
                )
                .unwrap(),
                "playback" => {
                    assert!(
                        playback::PlaybackSession::default()
                            .launch(
                                &storage,
                                "concurrent-play-request-1",
                                &film,
                                &video,
                                |_| Ok(())
                            )
                            .unwrap()
                            .history_saved
                    );
                }
                _ => {
                    source_scan::open_source_directory_with(&storage, &source, |_| Ok(())).unwrap()
                }
            }
            done_tx.send(command).unwrap();
        }));
    }
    let completed: Vec<_> = (0..3)
        .filter_map(|_| done_rx.recv_timeout(Duration::from_secs(1)).ok())
        .collect();
    release_tx.send(()).unwrap();
    assert_eq!(scanner.join().unwrap().video_count, 1);
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(
        completed.len(),
        3,
        "normal requests were blocked by filesystem collection: {completed:?}"
    );
    let storage = state.lock().unwrap();
    assert_eq!(
        playback::load_settings(&storage).unwrap().launch_arguments,
        "saved-during-scan"
    );
    assert_eq!(playback::load_history(&storage).unwrap().events.len(), 1);
    assert!(catalog::list_films(&storage).unwrap()[0].favorite);
}

#[test]
fn restore_waits_for_scan_lease_then_restores_actual_backup_values() {
    let (_home, root, state, scan, source, film, _video) = fixture();
    let backup = {
        let storage = state.lock().unwrap();
        storage.create_backup().unwrap()
    };
    catalog::set_rating(&state.lock().unwrap(), &film, Some(5)).unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let scanner = {
        let state = state.clone();
        let scan = scan.clone();
        thread::spawn(move || {
            scan.preview_on_with(&state, &root, Some(&source), |root, known, ignored| {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                inventory_with_rules(root, known, ignored)
            })
            .unwrap()
        })
    };
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (restored_tx, restored_rx) = mpsc::channel();
    let (started_tx, started_rx) = mpsc::channel();
    let restorer = {
        let state = state.clone();
        thread::spawn(move || {
            started_tx.send(()).unwrap();
            state.exclusive().unwrap().restore_backup(&backup).unwrap();
            restored_tx.send(()).unwrap();
        })
    };
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let premature = restored_rx.recv_timeout(Duration::from_millis(100)).is_ok();
    release_tx.send(()).unwrap();
    scanner.join().unwrap();
    restorer.join().unwrap();
    assert!(
        !premature,
        "restore replaced the database during a scan lease"
    );
    let films = catalog::list_films(&state.lock().unwrap()).unwrap();
    assert_eq!(films[0].id, film);
    assert!(films[0].favorite);
    assert_eq!(films[0].rating, None);
}

#[test]
fn confirmation_recheck_allows_settings_and_rejects_changed_files_without_losing_manual_data() {
    let (_home, root, state, scan, source, film, _video) = fixture();
    let token = scan
        .preview_on(&state, &root, Some(&source))
        .unwrap()
        .token
        .unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let scanner = {
        let state = state.clone();
        let scan = scan.clone();
        thread::spawn(move || {
            lifecycle::confirm_source_refresh_on_with(
                &state,
                &scan,
                &token,
                |root, known, ignored| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    inventory_with_rules(root, known, ignored)
                },
            )
        })
    };
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (saved_tx, saved_rx) = mpsc::channel();
    let saver = {
        let state = state.clone();
        thread::spawn(move || {
            playback::save_settings(&state.lock().unwrap(), &PlayerSettings::default()).unwrap();
            saved_tx.send(()).unwrap();
        })
    };
    let saved_before_finish = saved_rx.recv_timeout(Duration::from_secs(1)).is_ok();
    fs::write(root.join("film/movie.mp4"), [1, 2]).unwrap();
    release_tx.send(()).unwrap();
    assert_eq!(
        scanner.join().unwrap().err().unwrap().to_string(),
        "scan_changed"
    );
    saver.join().unwrap();
    assert!(
        saved_before_finish,
        "settings were blocked by confirmation filesystem recheck"
    );
    let films = catalog::list_films(&state.lock().unwrap()).unwrap();
    assert_eq!(films[0].id, film);
    assert_eq!(films[0].videos.len(), 1);
    assert!(films[0].favorite);
}

#[test]
fn related_source_collection_remains_responsive_and_preserves_manual_history_after_reopen() {
    let (home, old_root, state, scan, _source, film, _video) = fixture();
    let old_canonical = old_root.canonicalize().unwrap();
    let new_root = home.path().join("synthetic-destination");
    fs::create_dir_all(&new_root).unwrap();
    let token = scan
        .preview_on(&state, &new_root, None)
        .unwrap()
        .token
        .unwrap();
    let destination = lifecycle::confirm_source_refresh_on(&state, &scan, &token)
        .unwrap()
        .source
        .library_id;
    fs::create_dir_all(new_root.join("new-film")).unwrap();
    fs::write(new_root.join("new-film/movie.mp4"), []).unwrap();
    fs::remove_file(old_root.join("film/movie.mp4")).unwrap();
    let token = scan
        .preview_on(&state, &new_root, Some(&destination))
        .unwrap()
        .token
        .unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let scanner = {
        let state = state.clone();
        let scan = scan.clone();
        thread::spawn(move || {
            lifecycle::confirm_source_refresh_on_with(
                &state,
                &scan,
                &token,
                |root, known, ignored| {
                    if root == old_canonical {
                        entered_tx.send(()).unwrap();
                        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    }
                    inventory_with_rules(root, known, ignored)
                },
            )
            .unwrap()
        })
    };
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (saved_tx, saved_rx) = mpsc::channel();
    let saver = {
        let state = state.clone();
        let film = film.clone();
        thread::spawn(move || {
            let storage = state.lock().unwrap();
            playback::save_settings(
                &storage,
                &PlayerSettings {
                    launch_arguments: "during-related-scan".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            catalog::set_rating(&storage, &film, Some(4)).unwrap();
            saved_tx.send(()).unwrap();
        })
    };
    let saved_before_finish = saved_rx.recv_timeout(Duration::from_secs(1)).is_ok();
    release_tx.send(()).unwrap();
    let result = scanner.join().unwrap();
    saver.join().unwrap();
    assert!(
        saved_before_finish,
        "settings were blocked by related-source enumeration"
    );
    assert_eq!(result.related_retained, 0);
    assert!(!result.recovery_failed);
    let reopened = Storage::open(&home.path().join("software")).unwrap();
    let films = catalog::list_films(&reopened).unwrap();
    assert_eq!(films.len(), 1);
    assert_eq!(films[0].id, film);
    assert!(films[0].favorite);
    assert_eq!(films[0].rating, Some(4.0));
    assert_eq!(films[0].videos[0].observed_path, "new-film/movie.mp4");
    assert_eq!(
        playback::load_settings(&reopened).unwrap().launch_arguments,
        "during-related-scan"
    );
}
