use local_film_library_preview_lib::{actors, catalog, source_scan::SourceScanSession, storage::Storage, translation};

#[test]
fn folder_creation_times_are_saved_by_scan_and_survive_offline_reads() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    let actor = root.join("Synthetic Actor");
    let directory = actor.join("[TEST-123] Fictional title");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(actor.join("actor.jpg"), b"synthetic").unwrap();
    std::fs::write(directory.join("test.mp4"), b"synthetic").unwrap();
    #[cfg(windows)]
    {
        // Different folder/file/import times make the Windows semantics observable.
        let status = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "[IO.Directory]::SetCreationTimeUtc((Join-Path $env:SYNTHETIC_CREATION_ROOT 'synthetic/Synthetic Actor'), [datetime]'2001-01-01T01:02:03Z'); [IO.Directory]::SetCreationTimeUtc((Join-Path $env:SYNTHETIC_CREATION_ROOT 'synthetic/Synthetic Actor/[TEST-123] Fictional title'), [datetime]'2002-02-02T04:05:06Z'); [IO.File]::SetCreationTimeUtc((Join-Path $env:SYNTHETIC_CREATION_ROOT 'synthetic/Synthetic Actor/[TEST-123] Fictional title/test.mp4'), [datetime]'2003-03-03T07:08:09Z')"])
            .env("SYNTHETIC_CREATION_ROOT", home.path()).status().unwrap();
        assert!(status.success());
    }
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let timestamp = |path: &std::path::Path| {
        let ms = std::fs::metadata(path).unwrap().created().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        storage.with_connection(|db| db.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ',?1/1000.0,'unixepoch')", [ms], |r|r.get::<_,String>(0))).unwrap()
    };
    let film_created = timestamp(&directory);
    let session = SourceScanSession::new();
    let preview = session.preview(&storage, &root, None).unwrap();
    session.confirm(&storage, &preview.token.unwrap()).unwrap();
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(serde_json::to_value(&films[0]).unwrap()["created_at"], film_created);
    assert!(actors::list_actors(&storage).unwrap().is_empty());
    std::fs::rename(&root, home.path().join("offline")).unwrap();
    assert_eq!(serde_json::to_value(&catalog::list_films(&storage).unwrap()[0]).unwrap()["created_at"], film_created);
    assert!(actors::list_actors(&storage).unwrap().is_empty());
}

#[test]
fn legacy_manual_title_is_inactive_without_deleting_other_manual_data() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db|db.execute_batch("INSERT INTO libraries(id,display_name,root_path) VALUES('source','Synthetic','X:/synthetic'); INSERT INTO films(id) VALUES('film'); INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('video','source','film','[TEST-123] Fictional title/test.mp4','video'); INSERT INTO film_manual(film_id,title,rating,favorite) VALUES('film','Legacy manual',8,1);")).unwrap();
    let film = catalog::list_films(&storage).unwrap().remove(0);
    assert_ne!(film.title, "Legacy manual");
    assert_eq!(film.rating, Some(4.0));
    assert!(film.favorite);
    assert!(film.created_at.is_none());
    assert!(translation::prepare_translation(&storage, "film", false).unwrap().is_some());
    let serialized = serde_json::to_value(translation::preview(&storage).unwrap()).unwrap();
    assert_ne!(serialized["entries"][0]["status"], "manual");
    assert_eq!(serialized["entries"][0]["sources"][0]["id"], "source");
    assert_eq!(storage.with_connection(|db|db.query_row("SELECT title FROM film_manual",[],|r|r.get::<_,String>(0))).unwrap(), "Legacy manual");
}


#[test]
fn folder_modification_times_update_only_on_scan_and_remain_available_offline() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    let actor = root.join("Synthetic Actor");
    let directory = actor.join("[TEST-456] Fictional title");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(actor.join("actor.jpg"), b"synthetic").unwrap();
    std::fs::write(directory.join("test.mp4"), b"synthetic").unwrap();
    #[cfg(windows)]
    let set_times = |film: &str, actor: &str| {
        let status = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "[IO.Directory]::SetLastWriteTimeUtc((Join-Path $env:SYNTHETIC_MODIFIED_ROOT 'synthetic/Synthetic Actor'), [datetime]$env:SYNTHETIC_ACTOR_TIME); [IO.Directory]::SetLastWriteTimeUtc((Join-Path $env:SYNTHETIC_MODIFIED_ROOT 'synthetic/Synthetic Actor/[TEST-456] Fictional title'), [datetime]$env:SYNTHETIC_FILM_TIME); [IO.File]::SetLastWriteTimeUtc((Join-Path $env:SYNTHETIC_MODIFIED_ROOT 'synthetic/Synthetic Actor/[TEST-456] Fictional title/test.mp4'), [datetime]'2003-03-03T07:08:09Z')"])
            .env("SYNTHETIC_MODIFIED_ROOT", home.path()).env("SYNTHETIC_FILM_TIME", film).env("SYNTHETIC_ACTOR_TIME", actor).status().unwrap();
        assert!(status.success());
    };
    #[cfg(windows)]
    set_times("2002-02-02T04:05:06Z", "2001-01-01T01:02:03Z");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let timestamp = |path: &std::path::Path| {
        let ms = std::fs::metadata(path).unwrap().modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        storage.with_connection(|db| db.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ',?1/1000.0,'unixepoch')", [ms], |r|r.get::<_,String>(0))).unwrap()
    };
    let film_time = timestamp(&directory);
    let session = SourceScanSession::new();
    let preview = session.preview(&storage, &root, None).unwrap();
    let source = session.confirm(&storage, &preview.token.unwrap()).unwrap();
    let film_value = || serde_json::to_value(&catalog::list_films(&storage).unwrap()[0]).unwrap()["modified_at"].clone();
    assert_eq!(film_value(), film_time);
    #[cfg(windows)]
    {
        set_times("2005-05-05T05:05:05Z", "2004-04-04T04:04:04Z");
        assert_eq!(film_value(), film_time);
        let preview = session.preview(&storage, &root, Some(&source.library_id)).unwrap();
        session.confirm(&storage, &preview.token.unwrap()).unwrap();
        assert_eq!(film_value(), timestamp(&directory));
    }
    let saved_film = film_value();
    std::fs::rename(&root, home.path().join("offline")).unwrap();
    assert_eq!(film_value(), saved_film);
    // Older snapshots lack the optional time map; do not substitute import/video time.
    storage.with_connection(|db| db.execute("UPDATE issue_records SET evidence=json_remove(evidence,'$.directory_modified_at') WHERE subject_type='library_scan_details'", [])).unwrap();
    assert_eq!(film_value(), serde_json::Value::Null);
}


#[test]
fn film_folder_times_aggregate_earliest_creation_and_latest_modification() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db|db.execute_batch(r#"
        INSERT INTO libraries(id,display_name,root_path) VALUES('source','Synthetic','X:/synthetic');
        INSERT INTO films(id) VALUES('film');
        INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('one','source','film','First/one.mp4','video'),('two','source','film','Second/two.mp4','video');
        INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('details','library_scan_details','source','current','{"directories":["First","Second"],"cover_checks":[],"roles":[{"directory":"First","role":"film"},{"directory":"Second","role":"film"}],"directory_created_at":{"First":"2001-01-01T00:00:00.000Z","Second":"2002-01-01T00:00:00.000Z"},"directory_modified_at":{"First":"2004-01-01T00:00:00.000Z","Second":"2005-01-01T00:00:00.000Z"}}');
    "#)).unwrap();
    let film = catalog::list_films(&storage).unwrap().remove(0);
    assert_eq!(film.created_at.as_deref(), Some("2001-01-01T00:00:00.000Z"));
    assert_eq!(film.modified_at.as_deref(), Some("2005-01-01T00:00:00.000Z"));
}
