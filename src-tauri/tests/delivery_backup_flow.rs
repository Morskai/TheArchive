use local_film_library_preview_lib::{
    actor_management::{self, SaveActorInput}, actors, catalog,
    lifecycle::{self, InterfacePreferences},
    playback::{self, PlaybackSession, PlayerSettings},
    scan_rules,
    source_scan::SourceScanSession,
    storage::Storage,
};
use rusqlite::{types::Value, Connection, OpenFlags};
use std::{fs, path::Path};

// Use the existing public readers, rather than an IPC success message, as the
// restored application's view of its saved data.
fn public_values(storage: &Storage) -> serde_json::Value {
    serde_json::json!({
        "films": catalog::list_films(storage).unwrap(),
        "actors": actors::list_actors(storage).unwrap(),
        "tags": catalog::list_tags(storage).unwrap(),
        "history": playback::load_history(storage).unwrap(),
        "interface": lifecycle::load_preferences(storage).unwrap(),
        "scan": scan_rules::load(storage).unwrap(),
        "player": playback::load_settings(storage).unwrap(),
    })
}

// Open the synthetic database independently and read only. This also checks
// manual fields that do not currently have a public editing/reading interface.
fn database_values(path: &Path) -> Vec<Vec<Vec<Value>>> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    [
        "SELECT * FROM libraries ORDER BY id",
        "SELECT * FROM films ORDER BY id",
        "SELECT * FROM file_observations ORDER BY id",
        "SELECT * FROM film_manual ORDER BY film_id",
        "SELECT * FROM actors ORDER BY id",
        "SELECT * FROM actor_directories ORDER BY actor_id",
        "SELECT * FROM actor_manual ORDER BY actor_id",
        "SELECT * FROM actor_image_sources ORDER BY actor_id",
        "SELECT * FROM actor_tags ORDER BY position,name",
        "SELECT * FROM actor_tag_links ORDER BY actor_id,tag",
        "SELECT * FROM info_actor_links ORDER BY external_id",
        "SELECT * FROM film_actors ORDER BY film_id,actor_id,source",
        "SELECT * FROM tags ORDER BY id",
        "SELECT * FROM film_tags ORDER BY film_id,tag_id",
        "SELECT * FROM file_versions ORDER BY observation_id",
        "SELECT * FROM player_launch_events ORDER BY id",
        "SELECT * FROM interface_preferences ORDER BY singleton",
        "SELECT * FROM ignored_directory_names ORDER BY position",
        "SELECT * FROM player_settings ORDER BY singleton",
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

#[test]
fn delivery_backup_restores_saved_library_manual_data_and_settings_after_changes() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-source");
    let folder = root.join("虚构演员/[TEST-01] 虚构影片");
    fs::create_dir_all(&folder).unwrap();
    for name in ["first.mp4", "second.mkv"] {
        fs::write(folder.join(name), b"synthetic video fixture").unwrap();
    }
    for path in [root.join("虚构演员/actor.jpg"), folder.join("poster.jpg")] {
        image::RgbImage::from_pixel(24, 32, image::Rgb([80, 110, 140]))
            .save(path)
            .unwrap();
    }
    let media_paths = [
        folder.join("first.mp4"),
        folder.join("second.mkv"),
        folder.join("poster.jpg"),
        root.join("虚构演员/actor.jpg"),
    ];
    let original_bytes: Vec<_> = media_paths
        .iter()
        .map(|path| fs::read(path).unwrap())
        .collect();
    let data = home.path().join("isolated-software-data");
    let storage = Storage::open(&data).unwrap();
    let scans = SourceScanSession::new();
    let preview = scans.preview(&storage, &root, None).unwrap();
    scans
        .confirm(&storage, preview.token.as_deref().unwrap())
        .unwrap();
    let film = catalog::list_films(&storage).unwrap().remove(0);
    assert_eq!(film.videos.len(), 2);
    let first = film
        .videos
        .iter()
        .find(|video| video.observed_path.ends_with("first.mp4"))
        .unwrap()
        .id
        .clone();
    let second = film
        .videos
        .iter()
        .find(|video| video.observed_path.ends_with("second.mkv"))
        .unwrap()
        .id
        .clone();
    let actor_id = actor_management::save(&storage, &SaveActorInput {
        actor_id: None, name: "虚构演员".into(), aliases: vec![],
        profile: actors::Profile { birth_month: "".into(), height: "".into(), cup: "".into(), bust: "".into(), waist: "".into(), hips: "".into() },
        photo_path: Some(root.join("虚构演员/actor.jpg").to_string_lossy().into_owned()), remove_photo: false, film_ids: vec![film.id.clone()],
    }).unwrap();
    let actor = actors::list_actors(&storage).unwrap().into_iter().find(|actor| actor.id == actor_id).unwrap();
    let saved_photo = actors::thumbnail(&storage, &actor.id, false).unwrap();

    catalog::set_favorite(&storage, &film.id, true).unwrap();
    catalog::set_rating(&storage, &film.id, Some(4)).unwrap();
    catalog::create_tag(&storage, "交付保留标签").unwrap();
    catalog::set_film_tags(&storage, &film.id, &["交付保留标签".into()]).unwrap();
    actors::set_favorite(&storage, &actor.id, true).unwrap();
    actors::save_profile(
        &storage,
        &actor.id,
        &actors::Profile {
            birth_month: "1990-04".into(),
            height: "165".into(),
            cup: "c".into(),
            bust: "85".into(),
            waist: "60".into(),
            hips: "88".into(),
        },
    )
    .unwrap();
    storage.with_transaction(|db| {
        db.execute("UPDATE actor_manual SET aliases='交付演员别名' WHERE actor_id=?1", [&actor.id])?;
        db.execute("INSERT INTO info_actor_links(external_id,actor_id) VALUES('synthetic-film-reference',?1)", [&actor.id])?;
        db.execute("INSERT INTO file_versions(observation_id,preferred) VALUES(?1,0)", [&first])?;
        Ok(())
    }).unwrap();
    playback::set_preferred(&storage, &film.id, &first, true).unwrap();
    // The injected launcher accepts only a synthetic target and starts no process.
    assert!(
        PlaybackSession::default()
            .launch(
                &storage,
                "delivery-backup-before",
                &film.id,
                &first,
                |_| Ok(())
            )
            .unwrap()
            .history_saved
    );
    let preferences = InterfacePreferences {
        reduce_motion: true,
        thin_scrollbar: false,
        film_cover_ratio: "5:7".into(),
        actor_photo_ratio: "3:4".into(),
        film_cover_fit: "contain".into(),
        actor_photo_fit: "contain".into(),
    };
    lifecycle::save_preferences(&storage, &preferences).unwrap();
    scan_rules::save(&storage, &["写真".into(), "Photos".into()]).unwrap();
    let player = PlayerSettings {
        launch_arguments: "--synthetic-delivery".into(),
        ..PlayerSettings::default()
    };
    playback::save_settings(&storage, &player).unwrap();

    let saved = public_values(&storage);
    assert_eq!(saved["films"][0]["favorite"], true);
    assert_eq!(saved["films"][0]["rating"], 4.0);
    assert_eq!(
        saved["films"][0]["tags"],
        serde_json::json!(["交付保留标签"])
    );
    assert_eq!(saved["films"][0]["preferred_video_id"], first);
    assert_eq!(saved["actors"][0]["birthMonth"], "1990-04");
    assert_eq!(saved["actors"][0]["cup"], "C");
    assert_eq!(saved["history"]["events"].as_array().unwrap().len(), 1);
    let saved_database = database_values(&data.join("library.sqlite3"));
    let backup = storage.create_backup().unwrap();
    let backup_path = data
        .join("backups")
        .join(format!("backup-{backup}.sqlite3"));
    assert_eq!(database_values(&backup_path), saved_database);

    catalog::set_favorite(&storage, &film.id, false).unwrap();
    catalog::set_rating(&storage, &film.id, Some(1)).unwrap();
    catalog::create_tag(&storage, "修改后标签").unwrap();
    catalog::set_film_tags(&storage, &film.id, &["修改后标签".into()]).unwrap();
    actors::set_favorite(&storage, &actor.id, false).unwrap();
    actors::save_profile(
        &storage,
        &actor.id,
        &actors::Profile {
            birth_month: "1991-05".into(),
            height: "170".into(),
            cup: "d".into(),
            bust: "90".into(),
            waist: "65".into(),
            hips: "92".into(),
        },
    )
    .unwrap();
    let changed_photo = home.path().join("changed-photo.png");
    image::RgbImage::from_pixel(24, 32, image::Rgb([200, 30, 40])).save(&changed_photo).unwrap();
    actor_management::save(&storage, &SaveActorInput {
        actor_id: Some(actor.id.clone()), name: "虚构演员".into(), aliases: vec!["修改后别名".into()],
        profile: actors::Profile { birth_month: "1991-05".into(), height: "170".into(), cup: "d".into(), bust: "90".into(), waist: "65".into(), hips: "92".into() },
        photo_path: Some(changed_photo.to_string_lossy().into_owned()), remove_photo: false, film_ids: vec![film.id.clone()],
    }).unwrap();
    assert_ne!(actors::thumbnail(&storage, &actor.id, false).unwrap(), saved_photo);
    playback::set_preferred(&storage, &film.id, &second, true).unwrap();
    playback::clear_history(&storage).unwrap();
    assert!(
        PlaybackSession::default()
            .launch(
                &storage,
                "delivery-backup-after",
                &film.id,
                &second,
                |_| Ok(())
            )
            .unwrap()
            .history_saved
    );
    lifecycle::save_preferences(&storage, &InterfacePreferences::default()).unwrap();
    scan_rules::save(&storage, &["修改后忽略目录".into()]).unwrap();
    playback::save_settings(&storage, &PlayerSettings::default()).unwrap();
    storage.with_transaction(|db| {
        db.execute("UPDATE actor_manual SET aliases='修改后别名' WHERE actor_id=?1", [&actor.id])?;
        db.execute("UPDATE file_versions SET preferred=0 WHERE observation_id=?1", [&first])?;
        db.execute("DELETE FROM film_actors WHERE source='manual'", [])?;
        Ok(())
    }).unwrap();
    let changed = public_values(&storage);
    assert_eq!(changed["films"][0]["favorite"], false);
    assert_eq!(changed["films"][0]["rating"], 1.0);
    assert_eq!(
        changed["films"][0]["tags"],
        serde_json::json!(["修改后标签"])
    );
    assert_eq!(changed["films"][0]["preferred_video_id"], second);
    assert_eq!(changed["actors"][0]["birthMonth"], "1991-05");
    assert_eq!(
        changed["history"]["events"][0]["id"],
        "delivery-backup-after"
    );
    assert_eq!(
        lifecycle::load_preferences(&storage).unwrap(),
        InterfacePreferences::default()
    );
    assert_eq!(scan_rules::load(&storage).unwrap(), ["修改后忽略目录"]);
    assert_ne!(
        database_values(&data.join("library.sqlite3")),
        saved_database
    );

    let protections_before = storage.list_protections().unwrap().len();
    storage.restore_backup(&backup).unwrap();
    assert!(storage.list_protections().unwrap().len() > protections_before);
    assert_eq!(public_values(&storage), saved);
    assert_eq!(actors::thumbnail(&storage, &actor.id, false).unwrap(), saved_photo);
    assert_eq!(
        database_values(&data.join("library.sqlite3")),
        saved_database
    );
    drop(storage);
    let reopened = Storage::open(&data).unwrap();
    assert_eq!(public_values(&reopened), saved);
    assert_eq!(actors::thumbnail(&reopened, &actor.id, false).unwrap(), saved_photo);
    assert_eq!(
        database_values(&data.join("library.sqlite3")),
        saved_database
    );
    assert_eq!(reopened.list_backups().unwrap()[0].id, backup);
    for (path, original) in media_paths.iter().zip(original_bytes) {
        assert_eq!(fs::read(path).unwrap(), original);
    }
}
