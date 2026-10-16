use local_film_library_preview_lib::{
    catalog, observation::scan_into_library, playback::*, storage::Storage,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

fn setup() -> (tempfile::TempDir, Storage, PathBuf, String, String) {
    let home = tempfile::tempdir().unwrap();
    let source = home.path().join("虚构 来源目录");
    let media = source.join("[TEST-01] 虚构 视频目录");
    fs::create_dir_all(&media).unwrap();
    fs::write(media.join("虚构 视频.mp4"), []).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO libraries(id,display_name,root_path) VALUES('source','虚构',?1)",
                [source.to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
        .unwrap();
    scan_into_library(&storage, "source", &source).unwrap();
    let film = catalog::list_films(&storage).unwrap().remove(0);
    let video = film.videos[0].id.clone();
    (home, storage, media, film.id, video)
}

#[test]
fn video_size_uses_saved_scan_metadata_and_obeys_id_and_source_access_checks() {
    let (_home, storage, media, film, video) = setup();
    fs::write(media.join("虚构 视频.mp4"), b"fictional").unwrap();
    assert_eq!(video_size(&storage, &film, &video).unwrap(), 0);
    scan_into_library(&storage, "source", media.parent().unwrap()).unwrap();
    assert_eq!(video_size(&storage, &film, &video).unwrap(), 9);
    assert!(video_size(&storage, "different-film", &video).is_err());
    storage
        .with_connection(|db| db.execute("UPDATE libraries SET enabled=0", []))
        .unwrap();
    assert!(video_size(&storage, &film, &video).is_err());
    assert_eq!(fs::read(media.join("虚构 视频.mp4")).unwrap(), b"fictional");
    assert!(load_history(&storage).unwrap().events.is_empty());
}

#[test]
fn settings_reopen_quotes_and_failed_save_preserve_previous_values() {
    let (home, storage, _, _, _) = setup();
    assert_eq!(load_settings(&storage).unwrap(), PlayerSettings::default());
    let program = home.path().join("虚构 程序.exe");
    fs::write(&program, []).unwrap();
    let settings = PlayerSettings {
        player_mode: "custom".into(),
        internal_display: "overlay".into(),
        player_path: program.to_string_lossy().into(),
        launch_arguments: r#"--title "中文 空格" "say \"hello\"" "" "D:\目录\\""#.into(),
    };
    assert_eq!(
        parse_arguments(&settings.launch_arguments).unwrap(),
        ["--title", "中文 空格", "say \"hello\"", "", "D:\\目录\\"]
    );
    save_settings(&storage, &settings).unwrap();
    let reopened = Storage::open(&home.path().join("data")).unwrap();
    assert_eq!(load_settings(&reopened).unwrap(), settings);
    assert_eq!(
        parse_arguments("\"未闭合").unwrap_err(),
        "invalid_arguments"
    );
    let invalid = PlayerSettings {
        player_path: home.path().join("absent.exe").to_string_lossy().into(),
        ..settings.clone()
    };
    assert!(save_settings(&reopened, &invalid).is_err());
    reopened.with_connection(|db| db.execute_batch("CREATE TRIGGER deny_settings BEFORE UPDATE ON player_settings BEGIN SELECT RAISE(ABORT,'test'); END")).unwrap();
    assert!(save_settings(&reopened, &PlayerSettings::default()).is_err());
    assert_eq!(load_settings(&reopened).unwrap(), settings);
}

#[test]
fn one_request_one_launch_history_reopen_clear_and_manual_data_survive() {
    let (home, storage, media, film, video) = setup();
    catalog::set_favorite(&storage, &film, true).unwrap();
    catalog::set_rating(&storage, &film, Some(4)).unwrap();
    catalog::create_tag(&storage, "虚构标签").unwrap();
    catalog::set_film_tags(&storage, &film, &["虚构标签".into()]).unwrap();
    set_preferred(&storage, &film, &video, true).unwrap();
    let session = PlaybackSession::default();
    let launches = AtomicUsize::new(0);
    let launch = |_: &LaunchTarget| {
        launches.fetch_add(1, Ordering::SeqCst);
        Ok(())
    };
    assert_eq!(
        session
            .launch(&storage, "request-0000000001", &film, &video, |_| Err(
                "launch_failed".into()
            ))
            .unwrap_err(),
        "launch_failed"
    );
    assert!(load_history(&storage).unwrap().events.is_empty());
    assert!(
        session
            .launch(&storage, "request-0000000002", &film, &video, launch)
            .unwrap()
            .history_saved
    );
    session
        .launch(&storage, "request-0000000002", &film, &video, launch)
        .unwrap();
    assert_eq!(launches.load(Ordering::SeqCst), 1);
    session
        .launch(&storage, "request-0000000003", &film, &video, launch)
        .unwrap();
    assert_eq!(launches.load(Ordering::SeqCst), 2);
    open_folder_with(&storage, &film, &video, |folder| {
        assert_eq!(folder, media.canonicalize().unwrap());
        Ok(())
    })
    .unwrap();
    assert_eq!(load_history(&storage).unwrap().events.len(), 2);
    let reopened = Storage::open(&home.path().join("data")).unwrap();
    let history = load_history(&reopened).unwrap();
    assert_eq!(history.events.len(), 2);
    reopened
        .with_connection(|db| db.execute_batch("UPDATE libraries SET enabled=0,removed=1"))
        .unwrap();
    assert_eq!(load_history(&reopened).unwrap().events.len(), 2);
    assert_eq!(load_history(&reopened).unwrap().films[0].id, film);
    reopened
        .with_connection(|db| db.execute_batch("UPDATE libraries SET enabled=1,removed=0"))
        .unwrap();
    assert!(history.events[0].launched_at.ends_with('Z'));
    assert!(history.films[0].last_launched_at.is_some());
    PlaybackSession::default()
        .launch(&reopened, "request-0000000002", &film, &video, |_| {
            panic!("persisted request must not launch")
        })
        .unwrap();
    scan_into_library(&reopened, "source", media.parent().unwrap()).unwrap();
    assert_eq!(
        catalog::list_films(&reopened).unwrap()[0]
            .preferred_video_id
            .as_deref(),
        Some(video.as_str())
    );
    reopened.with_connection(|db| db.execute_batch("CREATE TRIGGER deny_clear BEFORE DELETE ON player_launch_events BEGIN SELECT RAISE(ABORT,'test'); END")).unwrap();
    assert!(clear_history(&reopened).is_err());
    assert_eq!(load_history(&reopened).unwrap().events.len(), 2);
    reopened
        .with_connection(|db| db.execute_batch("DROP TRIGGER deny_clear"))
        .unwrap();
    clear_history(&reopened).unwrap();
    assert!(load_history(&reopened).unwrap().events.is_empty());
    let row = catalog::list_films(&reopened).unwrap().remove(0);
    assert!(row.last_launched_at.is_none());
    assert_eq!(row.id, film);
    assert!(row.favorite);
    assert_eq!(row.rating, Some(4.));
    assert_eq!(row.tags, ["虚构标签"]);
    assert_eq!(row.preferred_video_id, Some(video.clone()));
    session
        .launch(&reopened, "request-0000000002", &film, &video, |_| {
            panic!("clear must not relaunch")
        })
        .unwrap();
    set_preferred(&reopened, &film, &video, false).unwrap();
    assert!(
        catalog::list_films(&reopened).unwrap()[0]
            .preferred_video_id
            .is_none()
    );
    assert!(media.join("虚构 视频.mp4").exists());
}

#[test]
fn accepted_launch_but_history_failure_is_reported_without_retry() {
    let (_home, storage, _, film, video) = setup();
    storage.with_connection(|db| db.execute_batch("CREATE TRIGGER deny_event BEFORE INSERT ON player_launch_events BEGIN SELECT RAISE(ABORT,'test'); END")).unwrap();
    let session = PlaybackSession::default();
    let count = AtomicUsize::new(0);
    let launch = |_: &LaunchTarget| {
        count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    };
    assert!(
        !session
            .launch(&storage, "request-0000000004", &film, &video, launch)
            .unwrap()
            .history_saved
    );
    storage
        .with_connection(|db| db.execute_batch("DROP TRIGGER deny_event"))
        .unwrap();
    assert!(
        !session
            .launch(&storage, "request-0000000004", &film, &video, launch)
            .unwrap()
            .history_saved
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(load_history(&storage).unwrap().events.is_empty());
    assert!(
        session
            .launch(&storage, "request-0000000005", &film, &video, launch)
            .unwrap()
            .history_saved
    );
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn concurrent_replays_and_failed_clear_protection_preserve_history() {
    let (home, storage, _, film, video) = setup();
    let storage = std::sync::Arc::new(storage);
    let session = std::sync::Arc::new(PlaybackSession::default());
    let count = std::sync::Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let (storage, session, count, film, video) = (
                storage.clone(),
                session.clone(),
                count.clone(),
                film.clone(),
                video.clone(),
            );
            std::thread::spawn(move || {
                session
                    .launch(&storage, "request-concurrent01", &film, &video, |_| {
                        count.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        Ok(())
                    })
                    .unwrap()
            })
        })
        .collect();
    for handle in handles {
        assert!(handle.join().unwrap().history_saved);
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let protection = home.path().join("data/protections");
    fs::rename(&protection, home.path().join("data/retained-protections")).unwrap();
    fs::write(&protection, []).unwrap();
    assert_eq!(clear_history(&storage).unwrap_err(), "protection_failed");
    assert_eq!(load_history(&storage).unwrap().events.len(), 1);
    assert_eq!(catalog::list_films(&storage).unwrap()[0].id, film);
}

#[test]
fn validation_rejects_missing_inactive_unbound_and_out_of_range_files() {
    let (_home, storage, media, film, video) = setup();
    let check = |id: &str, expected: &str| {
        assert_eq!(
            PlaybackSession::default()
                .launch(&storage, id, &film, &video, |_| panic!(
                    "invalid file must not launch"
                ))
                .unwrap_err(),
            expected
        );
    };
    assert_eq!(
        resolve_video(&storage, "other-film", &video).unwrap_err(),
        "video_not_linked"
    );
    storage
        .with_connection(|db| db.execute_batch("UPDATE libraries SET enabled=0"))
        .unwrap();
    check("request-paused0001", "source_paused");
    storage
        .with_connection(|db| db.execute_batch("UPDATE libraries SET enabled=1,removed=1"))
        .unwrap();
    check("request-removed01", "source_removed");
    storage.with_connection(|db| db.execute_batch("UPDATE libraries SET removed=0; UPDATE file_observations SET observed_path='../outside.mp4'")).unwrap();
    check("request-outside01", "path_outside_source");
    storage
        .with_connection(|db| {
            db.execute(
                "UPDATE file_observations SET observed_path=?1",
                [format!(
                    "{}/虚构 视频.mp4",
                    media.file_name().unwrap().to_string_lossy()
                )],
            )
        })
        .unwrap();
    fs::remove_file(media.join("虚构 视频.mp4")).unwrap();
    check("request-missing01", "file_unavailable");
    fs::remove_dir(&media).unwrap();
    fs::remove_dir(media.parent().unwrap()).unwrap();
    check("request-offline01", "root_unavailable");
    assert!(load_history(&storage).unwrap().events.is_empty());
}

#[test]
fn preferred_change_rolls_back_and_keeps_preferred_binding() {
    let (_home, storage, _, film, video) = setup();
    storage.with_connection(|db| {
        db.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('other','source',?1,'other.mp4','video')",[&film])?;
        db.execute("INSERT INTO file_versions(observation_id,preferred) VALUES(?1,1)",[&video])?;
        db.execute_batch("CREATE TRIGGER deny_preferred BEFORE INSERT ON file_versions WHEN NEW.observation_id='other' BEGIN SELECT RAISE(ABORT,'test'); END")
    }).unwrap();
    assert!(set_preferred(&storage, &film, "other", true).is_err());
    assert!(set_preferred(&storage, "wrong-film", &video, false).is_err());
    assert_eq!(
        catalog::list_films(&storage).unwrap()[0].preferred_video_id,
        Some(video.clone())
    );
    storage
        .with_connection(|db| {
            let data: bool = db.query_row(
                "SELECT preferred FROM file_versions WHERE observation_id=?1",
                [&video],
                |r| r.get(0),
            )?;
            assert!(data);
            Ok(())
        })
        .unwrap();
}

#[test]
fn test_executable_receives_chinese_spaces_quotes_and_independent_video_argument() {
    let (home, storage, media, film, video) = setup();
    let source = home.path().join("helper.rs");
    let program = home.path().join("虚构 测试播放器.exe");
    let capture = home.path().join("captured.txt");
    fs::write(&source,r#"#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() { let args:Vec<String>=std::env::args().skip(1).collect(); std::fs::write(&args[0],args[1..].join("\n")).unwrap(); }"#).unwrap();
    let built = std::process::Command::new("rustc")
        .arg(&source)
        .arg("--crate-name")
        .arg("fictional_player")
        .arg("-o")
        .arg(&program)
        .output()
        .unwrap();
    assert!(built.status.success(), "test helper compilation failed");
    let options = PlayerSettings {
        player_mode: "custom".into(),
        internal_display: "overlay".into(),
        player_path: program.to_string_lossy().into(),
        launch_arguments: format!(
            r#""{}" "中文 空格" "say \"hello\"" "" "D:\尾\\""#,
            capture.display()
        ),
    };
    save_settings(&storage, &options).unwrap();
    PlaybackSession::default()
        .launch(
            &storage,
            "request-realtest01",
            &film,
            &video,
            launch_external,
        )
        .unwrap();
    for _ in 0..100 {
        if capture.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let captured = fs::read_to_string(&capture).unwrap();
    let expected = media.join("虚构 视频.mp4").canonicalize().unwrap();
    assert_eq!(
        captured.lines().collect::<Vec<_>>(),
        vec![
            "中文 空格",
            "say \"hello\"",
            "",
            "D:\\尾\\",
            expected.to_str().unwrap()
        ]
    );
    assert_eq!(load_history(&storage).unwrap().events.len(), 1);
    // Existing non-executable .exe passes path validation, but Windows rejects launch.
    let invalid = home.path().join("invalid.exe");
    fs::write(&invalid, b"not an executable").unwrap();
    save_settings(
        &storage,
        &PlayerSettings {
            player_path: invalid.to_string_lossy().into(),
            ..options
        },
    )
    .unwrap();
    assert!(
        PlaybackSession::default()
            .launch(
                &storage,
                "request-badexe001",
                &film,
                &video,
                launch_external
            )
            .is_err()
    );
    assert_eq!(load_history(&storage).unwrap().events.len(), 1);
}
