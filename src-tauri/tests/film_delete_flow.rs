use local_film_library_preview_lib::{catalog, observation::scan_into_library, problems, storage::Storage};

#[test]
fn deleting_a_card_does_not_report_its_existing_directory_as_missing() {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    std::fs::create_dir_all(media.join("Film/Child")).unwrap();
    std::fs::write(media.join("Film/movie.mp4"), b"original").unwrap();
    std::fs::write(media.join("Film/Child/movie.mp4"), b"child-original").unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('source','test',?1)", [media.to_string_lossy().as_ref()])?;
        Ok(())
    }).unwrap();
    scan_into_library(&storage, "source", &media).unwrap();
    let films = catalog::list_films(&storage).unwrap();
    let film = films.iter().find(|film| film.videos[0].observed_path == "Film/movie.mp4").unwrap();
    catalog::delete_films(&storage, &[film.id.clone()]).unwrap();
    let current = problems::list(&storage).unwrap();
    assert!(!current.iter().any(|problem| problem.directories.contains(&"Film".into())), "deleted card must not become a missing directory problem");
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 1, "child card must remain");
    scan_into_library(&storage, "source", &media).unwrap();
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 2);
    let restored = catalog::list_films(&storage).unwrap().into_iter().find(|film| film.videos[0].observed_path == "Film/movie.mp4").unwrap();
    assert!(problems::list(&storage).unwrap().iter().any(|problem| problem.subject_id == restored.id && problem.kind == "film_missing_cover"));
}

#[test]
fn a_reused_historical_directory_keeps_the_other_current_card_role() {
    check_reused_historical_directory(false);
}

#[test]
fn a_paused_reused_historical_directory_keeps_its_saved_card_role() {
    check_reused_historical_directory(true);
}

fn check_reused_historical_directory(paused: bool) {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    for dir in ["日本", "B"] {
        std::fs::create_dir_all(media.join(dir)).unwrap();
        std::fs::write(media.join(dir).join("new.mp4"), b"original").unwrap();
    }
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('source','test',?1)", [media.join("日本").to_string_lossy().as_ref()])?;
        db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('other','test',?1)", [media.join("B").to_string_lossy().as_ref()])?;
        Ok(())
    }).unwrap();
    scan_into_library(&storage, "source", &media.join("日本")).unwrap();
    scan_into_library(&storage, "other", &media.join("B")).unwrap();
    let films = catalog::list_films(&storage).unwrap();
    let removed = films.iter().find(|film| film.videos[0].source_id == "other").unwrap();
    let kept = films.iter().find(|film| film.videos[0].source_id == "source").unwrap();
    storage.with_connection(|db| {
        // A retained old identity anchor is outside the current scan snapshot.
        db.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('historical','source',?1,'old.mp4','video')", [&removed.id])?;
        if paused { db.execute("UPDATE libraries SET enabled=0 WHERE id='source'", [])?; }
        Ok(())
    }).unwrap();
    catalog::delete_films(&storage, &[removed.id.clone()]).unwrap();
    storage.with_connection(|db| db.execute("UPDATE libraries SET enabled=1 WHERE id='source'", []).map(|_| ())).unwrap();
    let remaining = catalog::list_films(&storage).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, kept.id);
    assert_eq!(remaining[0].title, kept.title);
    storage.with_connection(|db| {
        let json: String = db.query_row("SELECT evidence FROM issue_records WHERE subject_type='library_scan_details' AND subject_id='source'", [], |r| r.get(0))?;
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value["roles"].as_array().unwrap().iter().any(|r| r["directory"] == "" && r["role"] == "film"));
        Ok(())
    }).unwrap();
}

#[test]
fn empty_selection_and_failed_protection_do_not_delete_records() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| db.execute_batch("INSERT INTO films(id) VALUES('film');")).unwrap();
    assert_eq!(catalog::delete_films(&storage, &[]).unwrap_err(), "no_films_selected");
    std::fs::write(storage.data_directory().join("protections"), b"synthetic blocker").unwrap();
    assert_eq!(catalog::delete_films(&storage, &["film".into()]).unwrap_err(), "protection_failed");
    storage.with_connection(|db| {
        assert_eq!(db.query_row::<i64,_,_>("SELECT COUNT(*) FROM films", [], |r| r.get(0))?, 1);
        Ok(())
    }).unwrap();
}

#[test]
fn deletion_is_atomic_preserves_originals_and_rescan_can_restore() {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    std::fs::create_dir_all(media.join("A")).unwrap();
    std::fs::create_dir_all(media.join("B")).unwrap();
    std::fs::write(media.join("A/movie.mp4"), b"original-a").unwrap();
    std::fs::write(media.join("B/movie.mp4"), b"original-b").unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('source','test',?1)", [media.to_string_lossy().as_ref()])?;
        Ok(())
    }).unwrap();
    scan_into_library(&storage, "source", &media).unwrap();
    catalog::ensure_film_entries(&storage).unwrap();
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 2);
    let id = films[0].id.clone();
    catalog::set_favorite(&storage, &id, true).unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO actors(id,observed_name) VALUES('actor','保留演员')", [])?;
        db.execute("INSERT INTO film_actors(film_id,actor_id,source) VALUES(?1,'actor','manual')", [&id])?;
        db.execute("INSERT INTO tags(id,name) VALUES('tag','保留标签')", [])?;
        db.execute("INSERT INTO film_tags(film_id,tag_id) VALUES(?1,'tag')", [&id])?;
        db.execute("INSERT INTO player_launch_events(id,film_id,observation_id) SELECT 'play',?1,id FROM file_observations WHERE film_id=?1 LIMIT 1", [&id])?;
        db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('issue','film_missing_cover',?1,'test','test')", [&id])?;
        db.execute("INSERT INTO films(id) VALUES('old-alias')", [])?;
        db.execute("INSERT INTO film_aliases(id,film_id) VALUES('old-alias',?1)", [&id])?;
        db.execute("INSERT INTO player_launch_events(id,film_id) VALUES('old-play','old-alias')", [])?;
        db.execute("INSERT INTO translation_memory(id,identity_anchor,normalized_code,original_title,normalized_title,body,fingerprint,translated_title,provider) VALUES('memory',?1,'code','original','original','body','fingerprint','翻译记忆','test')", [&id])?;
        db.execute("INSERT INTO film_translations(film_id,memory_id) VALUES(?1,'memory')", [&id])?;
        db.execute("INSERT INTO actor_directories(actor_id,directory_key,marker_id,directory_name) SELECT 'actor','legacy',id,'旧资料' FROM file_observations WHERE film_id=?1 LIMIT 1", [&id])?;
        db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('old-cleared','library_problem_cleared','film_missing_cover:old-alias','cleared','{}')", [])?;
        Ok(())
    }).unwrap();
    assert!(catalog::delete_films(&storage, &[id.clone(), "missing".into()]).is_err());
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 2);
    storage.with_connection(|db| db.execute_batch("CREATE TRIGGER reject_delete BEFORE DELETE ON films BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    assert!(catalog::delete_films(&storage, &[id.clone()]).is_err());
    storage.with_connection(|db| {
        assert_eq!(db.query_row::<i64,_,_>("SELECT COUNT(*) FROM player_launch_events", [], |r| r.get(0))?, 2);
        db.execute_batch("DROP TRIGGER reject_delete;")
    }).unwrap();
    assert_eq!(catalog::delete_films(&storage, &[id.clone(), id]).unwrap(), 1);
    catalog::ensure_film_entries(&storage).unwrap();
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 1);
    storage.with_connection(|db| {
        assert_eq!(db.query_row::<i64,_,_>("SELECT COUNT(*) FROM issue_records WHERE id='old-cleared' OR (subject_type='library_problem_current' AND subject_id=?1)", [&films[0].id], |r| r.get(0))?, 0);
        for table in ["film_manual", "film_actors", "film_tags", "player_launch_events", "issue_records", "film_translations", "film_aliases"] {
            // Scan evidence belongs to the source and survives; only the film issue is removed.
            let sql = if table == "issue_records" { "SELECT COUNT(*) FROM issue_records WHERE id='issue'".to_owned() } else { format!("SELECT COUNT(*) FROM {table}") };
            assert_eq!(db.query_row::<i64,_,_>(&sql, [], |r| r.get(0))?, 0);
        }
        assert_eq!(db.query_row::<i64,_,_>("SELECT COUNT(*) FROM actors", [], |r| r.get(0))?, 1);
        assert_eq!(db.query_row::<i64,_,_>("SELECT COUNT(*) FROM tags", [], |r| r.get(0))?, 1);
        assert_eq!(db.query_row::<String,_,_>("SELECT translated_title FROM translation_memory WHERE id='memory'", [], |r| r.get(0))?, "翻译记忆");
        assert_eq!(db.query_row::<Option<String>,_,_>("SELECT marker_id FROM actor_directories WHERE actor_id='actor'", [], |r| r.get(0))?, None);
        Ok(())
    }).unwrap();
    assert_eq!(std::fs::read(media.join("A/movie.mp4")).unwrap(), b"original-a");
    assert_eq!(std::fs::read(media.join("B/movie.mp4")).unwrap(), b"original-b");
    scan_into_library(&storage, "source", &media).unwrap();
    catalog::ensure_film_entries(&storage).unwrap();
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 2);
    assert!(catalog::list_films(&storage).unwrap().iter().all(|film| !film.favorite));
}
