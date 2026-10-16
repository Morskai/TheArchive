use local_film_library_preview_lib::{catalog, observation::scan_into_library, storage::Storage};
use std::fs;

fn setup() -> (tempfile::TempDir, Storage) {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("media");
    fs::create_dir_all(root.join("[DEMO-001] 虚构片名【标签】")).unwrap();
    fs::write(root.join("[DEMO-001] 虚构片名【标签】/a.mp4"), []).unwrap();
    fs::write(root.join("[DEMO-001] 虚构片名【标签】/b.mp4"), []).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('source','虚构分类',?1)", [root.to_string_lossy().as_ref()])?;
        for (film, video, file) in [("auto-a","a","a.mp4"),("auto-b","b","b.mp4")] {
            db.execute("INSERT INTO films(id) VALUES(?1)",[film])?;
            db.execute("INSERT INTO file_observations(id,film_id,library_id,observed_path,kind) VALUES(?1,?2,'source',?3,'video')",rusqlite::params![video,film,format!("[DEMO-001] 虚构片名【标签】/{file}")])?;
        }
        db.execute("INSERT INTO film_manual(film_id,rating) VALUES('auto-a',2),('auto-b',8)",[])?;
        db.execute("INSERT INTO player_launch_events(id,film_id,observation_id) VALUES('old-event','auto-b','b')",[])?;
        Ok(())
    }).unwrap();
    (home, storage)
}

#[test]
fn existing_folder_entries_become_one_film_and_keep_versions_after_reopen_and_rescan() {
    let (home, storage) = setup();
    catalog::ensure_film_entries(&storage).unwrap();
    let film = catalog::list_films(&storage).unwrap().remove(0);
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 1);
    assert_eq!(film.id, "auto-a");
    assert_eq!(film.title, "虚构片名");
    assert_eq!(film.videos.len(), 2);
    assert_eq!(film.rating, Some(1.0));
    let original_ids = film.videos.iter().map(|v| v.id.clone()).collect::<Vec<_>>();
    local_film_library_preview_lib::playback::set_preferred(&storage, &film.id, "b", true).unwrap();
    catalog::set_rating(&storage, &film.id, Some(5)).unwrap();
    catalog::set_favorite(&storage, &film.id, true).unwrap();
    assert_eq!(
        local_film_library_preview_lib::playback::load_history(&storage)
            .unwrap()
            .events[0]
            .film_id,
        film.id
    );
    drop(storage);
    let storage = Storage::open(&home.path().join("data")).unwrap();
    assert_eq!(catalog::ensure_film_entries(&storage).unwrap(), 0);
    let root = home.path().join("media");
    fs::write(root.join("[DEMO-001] 虚构片名【标签】/c.MKV"), []).unwrap();
    scan_into_library(&storage, "source", &root).unwrap();
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 1);
    assert_eq!(films[0].id, film.id);
    assert_eq!(films[0].videos.len(), 3);
    assert!(original_ids
        .iter()
        .all(|id| films[0].videos.iter().any(|v| &v.id == id)));
    assert_eq!(films[0].preferred_video_id.as_deref(), Some("b"));
    assert_eq!(films[0].rating, Some(5.0));
    assert!(films[0].favorite);
}

#[test]
fn folder_upgrade_protection_or_write_failure_keeps_old_links_and_history() {
    for protection_failure in [true, false] {
        let (home, storage) = setup();
        if protection_failure {
            fs::write(home.path().join("data/protections"), []).unwrap();
        } else {
            storage.with_connection(|db|db.execute_batch("CREATE TRIGGER deny_group BEFORE UPDATE OF film_id ON file_observations WHEN NEW.id='b' BEGIN SELECT RAISE(ABORT,'test'); END")).unwrap();
        }
        assert!(catalog::ensure_film_entries(&storage).is_err());
        storage
            .with_connection(|db| {
                let film: String = db.query_row(
                    "SELECT film_id FROM file_observations WHERE id='b'",
                    [],
                    |r| r.get(0),
                )?;
                assert_eq!(film, "auto-b");
                let event: String = db.query_row(
                    "SELECT film_id FROM player_launch_events WHERE id='old-event'",
                    [],
                    |r| r.get(0),
                )?;
                assert_eq!(event, "auto-b");
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn same_number_different_directories_and_source_root_work_stay_separate() {
    let (home, storage) = setup();
    let root = home.path().join("media");
    fs::create_dir_all(root.join("other/[DEMO-001] 另一片名")).unwrap();
    image::RgbImage::from_pixel(24, 32, image::Rgb([80, 40, 20]))
        .save_with_format(root.join("other/actor.jpg"), image::ImageFormat::Png)
        .unwrap();
    fs::write(root.join("other/[DEMO-001] 另一片名/c.mp4"), []).unwrap();
    fs::write(root.join("direct-one.mp4"), []).unwrap();
    fs::write(root.join("direct-two.mp4"), []).unwrap();
    scan_into_library(&storage, "source", &root).unwrap();
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 3);
    assert_eq!(films.iter().map(|f| f.videos.len()).sum::<usize>(), 5);
    assert_ne!(films[0].id, films[1].id);
    storage.with_connection(|db| {
        assert_eq!(db.query_row("SELECT COUNT(*) FROM file_observations WHERE observed_path IN ('direct-one.mp4','direct-two.mp4')", [], |r| r.get::<_, i64>(0))?, 2);
        Ok(())
    }).unwrap();
}
