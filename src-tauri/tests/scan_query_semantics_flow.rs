use local_film_library_preview_lib::{catalog, covers, source_scan, storage::Storage};

#[test]
fn prefetched_history_and_cover_ownership_keep_retained_versions_aliases_and_date_ties() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("software")).unwrap();
    storage.with_transaction(|tx| {
        tx.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('source','合成来源',?1)", [home.path().to_string_lossy().as_ref()])?;
        tx.execute_batch("INSERT INTO films(id) VALUES('film-a'),('film-b'),('alias-a');
            INSERT INTO film_aliases(id,film_id) VALUES('alias-a','film-a');
            INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES
            ('a1','source','film-a','clips/a.mp4','video'),('a2','source','film-a','clips/b.mp4','video'),
            ('b1','source','film-b','clips/c.mp4','video'),('unknown','source',NULL,'clips/unknown.mp4','video'),
            ('a0-retained','source','film-a','retained/missing.mp4','video'),('a-alias','source','alias-a','old/alias.mp4','video'),
            ('cover','source',NULL,'clips/poster.jpg','image');
            INSERT INTO file_versions(observation_id,preferred) VALUES('a2',1),('a0-retained',1),('a-alias',1);
            INSERT INTO player_launch_events(id,film_id,observation_id,launched_at) VALUES
            ('event-a','film-a','a1','2026-10-10T00:00:00Z'),
            ('event-z','alias-a','a-alias','2026-10-10T08:00:00+08:00');
            INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence)
            VALUES('scan','cover_scan','source','current','[\"a1\",\"a2\",\"b1\",\"unknown\",\"cover\"]');")?;
        Ok(())
    }).unwrap();
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 2);
    let a = films.iter().find(|film| film.id == "film-a").unwrap();
    assert_eq!(
        a.videos.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(),
        ["a1", "a2"]
    );
    assert_eq!(a.preferred_video_id.as_deref(), Some("a0-retained"));
    assert_eq!(
        a.last_launched_at.as_deref(),
        Some("2026-10-10T08:00:00+08:00")
    );
    for film in &films {
        let cover = covers::info(&storage, &film.id).unwrap();
        assert!(cover.needs_review);
        assert_eq!(cover.candidates.len(), 1);
        assert_eq!(cover.candidates[0].id, "cover");
        assert!(!film.has_cover);
    }
}

#[test]
fn source_summary_counts_current_evidence_and_keeps_legacy_and_disabled_sources() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("software")).unwrap();
    storage.with_transaction(|tx| {
        tx.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('current','当前合成来源',?1)", [home.path().join("current").to_string_lossy().as_ref()])?;
        tx.execute("INSERT INTO libraries(id,display_name,root_path,enabled) VALUES('legacy','旧合成来源',?1,0)", [home.path().join("legacy").to_string_lossy().as_ref()])?;
        tx.execute_batch("INSERT INTO films(id) VALUES('a'),('b');
            INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES
            ('live','current','a','film/live.mp4','video'),('missing','current','b','film/missing.mp4','video'),
            ('image','current',NULL,'film/poster.jpg','image'),
            ('old1','legacy','a','film/old.mp4','video'),('old2','legacy',NULL,'film/text.srt','subtitle');
            INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence)
            VALUES('scan','cover_scan','current','current','[\"live\",\"image\"]');")?;
        Ok(())
    }).unwrap();
    let sources = source_scan::list_sources(&storage).unwrap();
    let current = sources.iter().find(|s| s.id == "current").unwrap();
    assert_eq!((current.observation_count, current.film_count), (2, 1));
    let legacy = sources.iter().find(|s| s.id == "legacy").unwrap();
    assert_eq!((legacy.observation_count, legacy.film_count), (2, 1));
    assert!(!legacy.enabled);
    assert_eq!(legacy.video_bytes, None);
    assert_eq!(legacy.last_scanned_at, None);
}
