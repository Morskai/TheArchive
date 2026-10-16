use local_film_library_preview_lib::{
    actors, catalog,
    observation::{ScanResult, scan_into_library},
    playback, problems,
    source_scan::{SourceScanSession, list_sources},
    storage::Storage,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn setup() -> (tempfile::TempDir, Storage, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    fs::create_dir_all(root.join("甲/[TEST-01] 影片")).unwrap();
    fs::write(root.join("甲/[TEST-01] 影片/movie.mp4"), []).unwrap();
    image::RgbImage::new(12, 18)
        .save(root.join("甲/actor.jpg"))
        .unwrap();
    let s = Storage::open(&home.path().join("data")).unwrap();
    s.with_connection(|db| {
        db.execute(
            "INSERT INTO libraries(id,display_name,root_path) VALUES('source','虚构来源',?1)",
            [root.to_string_lossy().as_ref()],
        )?;
        Ok(())
    })
    .unwrap();
    scan(&s, &root);
    let photo_path = root.join("甲/actor.jpg");
    let film = catalog::list_films(&s).unwrap().remove(0);
    s.with_transaction(|db| {
        db.execute("INSERT INTO actors(id,observed_name) VALUES('saved-actor','甲')",[])?;
        db.execute("INSERT INTO actor_image_sources(actor_id,path,fingerprint) VALUES('saved-actor',?1,'synthetic')",[photo_path.to_string_lossy().as_ref()])?;
        db.execute("INSERT INTO film_actors(film_id,actor_id,source) VALUES(?1,'saved-actor','manual')",[&film.id])?;
        Ok(())
    }).unwrap();
    (home, s, root)
}
fn scan(s: &Storage, root: &Path) {
    assert!(matches!(
        scan_into_library(s, "source", root).unwrap(),
        ScanResult::Complete(_)
    ));
}

#[test]
fn only_film_cover_problems_from_real_objects_and_no_other_image_fallback() {
    let (_home, s, root) = setup();
    assert_eq!(problems::list(&s).unwrap().len(), 1);
    fs::remove_file(root.join("甲/actor.jpg")).unwrap();
    image::RgbImage::new(12, 18)
        .save(root.join("甲/[TEST-01] 影片/random.jpg"))
        .unwrap();
    scan(&s, &root);
    assert_eq!(problems::list(&s).unwrap().len(), 1, "film scanning cannot remove a saved actor photo path");
    assert!(actors::thumbnail(&s,"saved-actor",false).is_err(), "an unavailable selected path must not fall back to another image");
    s.with_connection(|db| {db.execute("DELETE FROM actor_image_sources WHERE actor_id='saved-actor'",[])?;Ok(())}).unwrap();
    let rows = problems::list(&s).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows.iter().all(|p| !p.handled));
    assert!(rows.iter().any(|p| p.reason.contains("poster.jpg")));
    assert!(!rows.iter().any(|p| p.kind == "actor_missing_photo"));
    assert!(!actors::list_actors(&s).unwrap()[0].has_portrait);
}

#[test]
fn handled_reopen_rescan_and_artwork_return_then_missing_reprompts() {
    let (home, s, root) = setup();
    let old = problems::list(&s).unwrap().remove(0);
    problems::mark(&s, &[old.id.clone()]).unwrap();
    drop(s);
    let s = Storage::open(&home.path().join("data")).unwrap();
    image::RgbImage::new(12, 18)
        .save(root.join("甲/[TEST-01] 影片/random.jpg"))
        .unwrap();
    scan(&s, &root);
    let same = problems::list(&s).unwrap().remove(0);
    assert_eq!(same.id, old.id);
    assert!(same.handled);
    let cover = root.join("甲/[TEST-01] 影片/POSTER.JPG");
    image::RgbImage::new(12, 18).save(&cover).unwrap();
    scan(&s, &root);
    assert!(problems::list(&s).unwrap().is_empty());
    fs::remove_file(cover).unwrap();
    scan(&s, &root);
    let next = problems::list(&s).unwrap().remove(0);
    assert_ne!(next.id, old.id);
    assert!(!next.handled && next.reason.contains("证据已变化"));
    s.with_connection(|db| {
        assert!(db.query_row(
            "SELECT handled_at IS NOT NULL FROM issue_records WHERE id=?1",
            [&old.id],
            |r| r.get::<_, bool>(0)
        )?);
        Ok(())
    })
    .unwrap();
}

#[test]
fn mark_failure_and_invalid_batch_roll_back_all_states() {
    let (_home, s, root) = setup();
    fs::remove_file(root.join("甲/actor.jpg")).unwrap();
    s.with_connection(|db| {db.execute("DELETE FROM actor_image_sources WHERE actor_id='saved-actor'",[])?;Ok(())}).unwrap();
    scan(&s, &root);
    let rows = problems::list(&s).unwrap();
    s.with_connection(|db|db.execute_batch("CREATE TRIGGER fail_mark BEFORE UPDATE OF handled_at ON issue_records BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    assert!(problems::mark(&s, &[rows[0].id.clone()]).is_err());
    assert!(problems::list(&s).unwrap().iter().all(|p| !p.handled));
    s.with_connection(|db| db.execute_batch("DROP TRIGGER fail_mark;"))
        .unwrap();
    assert!(problems::mark(&s, &[rows[0].id.clone(), "invalid".into()]).is_err());
    assert!(problems::list(&s).unwrap().iter().all(|p| !p.handled));
}

#[test]
fn issue_failure_rolls_back_current_snapshot_observations_and_preserves_actor_photo_path() {
    let (_home, s, root) = setup();
    let old = problems::list(&s).unwrap().remove(0);
    s.with_connection(|db|db.execute_batch("CREATE TRIGGER fail_problem BEFORE INSERT ON issue_records WHEN NEW.subject_type='film_missing_cover' BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    fs::remove_file(root.join("甲/actor.jpg")).unwrap();
    fs::write(root.join("甲/[TEST-01] 影片/new.mp4"), []).unwrap();
    fs::create_dir_all(root.join("新作品")).unwrap();
    fs::write(root.join("新作品/other.mp4"), []).unwrap();
    assert!(scan_into_library(&s, "source", &root).is_err());
    assert_eq!(problems::list(&s).unwrap()[0].id, old.id);
    assert_eq!(catalog::list_films(&s).unwrap()[0].videos.len(), 1);
    assert!(actors::list_actors(&s).unwrap()[0].has_portrait);
    s.with_connection(|db| {
        assert_eq!(db.query_row("SELECT path FROM actor_image_sources WHERE actor_id='saved-actor'",[],|r|r.get::<_,String>(0))?,root.join("甲/actor.jpg").to_string_lossy());
        Ok(())
    }).unwrap();
    s.with_connection(|db| {
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM file_observations WHERE observed_path LIKE '%new.mp4'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn full_refresh_replaces_current_versions_and_counts_preserving_manual_identity() {
    let (home, s, root) = setup();
    let old = catalog::list_films(&s).unwrap().remove(0);
    s.with_connection(|db| {db.execute("INSERT INTO player_launch_events(id,film_id,observation_id,launched_at) VALUES('synthetic-event',?1,?2,'2026-10-01T10:00:00Z')",(&old.id,&old.videos[0].id))?;Ok(())}).unwrap();
    catalog::set_favorite(&s, &old.id, true).unwrap();
    catalog::set_rating(&s, &old.id, Some(4)).unwrap();
    fs::remove_file(root.join("甲/[TEST-01] 影片/movie.mp4")).unwrap();
    fs::create_dir(root.join("甲/[TEST-02] 临时")).unwrap();
    fs::write(root.join("甲/[TEST-02] 临时/temp.mp4"), []).unwrap();
    let session = SourceScanSession::new();
    let preview = session.preview(&s, &root, Some("source")).unwrap();
    session
        .confirm(&s, preview.token.as_deref().unwrap())
        .unwrap();
    let current = catalog::list_films(&s).unwrap();
    assert_eq!(current.len(), 1);
    assert_ne!(current[0].id, old.id);
    assert_eq!(list_sources(&s).unwrap()[0].observation_count, 2);
    assert_eq!(playback::load_history(&s).unwrap().film_count, 1);
    fs::remove_file(root.join("甲/[TEST-02] 临时/temp.mp4")).unwrap();
    fs::remove_dir(root.join("甲/[TEST-02] 临时")).unwrap();
    fs::write(root.join("甲/[TEST-01] 影片/movie.mp4"), []).unwrap();
    scan(&s, &root);
    drop(s);
    let s = Storage::open(&home.path().join("data")).unwrap();
    let restored = catalog::list_films(&s).unwrap().remove(0);
    assert_eq!(restored.id, old.id);
    assert!(restored.favorite);
    assert_eq!(restored.rating, Some(4.0));
    fs::remove_file(root.join("甲/[TEST-01] 影片/movie.mp4")).unwrap();
    scan(&s, &root);
    assert!(catalog::list_films(&s).unwrap().is_empty());
    assert_eq!(list_sources(&s).unwrap()[0].film_count, 0);
    let issues = problems::list(&s).unwrap();
    assert!(issues.is_empty());
    let history = playback::load_history(&s).unwrap();
    assert_eq!(history.film_count, 0);
    assert_eq!(history.events.len(), 1);
    assert!(history.films.iter().any(|film| film.id == old.id));
}

#[test]
fn replacing_every_video_in_same_film_directory_keeps_identity_and_manual_values() {
    let (_home, s, root) = setup();
    let old = catalog::list_films(&s).unwrap().remove(0);
    catalog::set_favorite(&s, &old.id, true).unwrap();
    catalog::set_rating(&s, &old.id, Some(5)).unwrap();
    catalog::create_tag(&s, "合成标签").unwrap();
    catalog::set_film_tags(&s, &old.id, &["合成标签".into()]).unwrap();
    fs::remove_file(root.join("甲/[TEST-01] 影片/movie.mp4")).unwrap();
    fs::write(root.join("甲/[TEST-01] 影片/new.mkv"), []).unwrap();
    scan(&s, &root);
    let new = catalog::list_films(&s).unwrap().remove(0);
    assert_eq!(new.id, old.id);
    assert!(new.favorite);
    assert_eq!(new.rating, Some(5.0));
    assert_eq!(new.tags, ["合成标签"]);
    assert_eq!(new.videos.len(), 1);
    assert!(new.videos[0].observed_path.ends_with("new.mkv"));
}

#[test]
fn disconnected_scan_keeps_evidence_and_old_conflicts_are_history_only() {
    let (_home, s, root) = setup();
    let old = problems::list(&s).unwrap().remove(0);
    s.with_connection(|db|{db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('legacy','cover_filename_conflict','legacy','old','{}')",[])?;Ok(())}).unwrap();
    fs::rename(&root, root.with_extension("offline")).unwrap();
    assert!(matches!(
        scan_into_library(&s, "source", &root).unwrap(),
        ScanResult::RootUnavailable { .. }
    ));
    let rows = problems::list(&s).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, old.id);
}
