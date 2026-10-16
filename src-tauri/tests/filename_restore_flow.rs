use local_film_library_preview_lib::{
    catalog,
    observation::{ScanResult, scan_into_library},
    playback::{self, PlaybackSession},
    source_scan::{SourceScanSession, change_source_state},
    storage::Storage,
};
use std::{fs, path::Path};

fn touch(root: &Path, relative: &str) {
    let file = root.join(relative);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, []).unwrap();
}

fn add(storage: &Storage, root: &Path) -> String {
    let session = SourceScanSession::new();
    let preview = session.preview(storage, root, None).unwrap();
    session.confirm(storage, &preview.token.unwrap()).unwrap().library_id
}

fn scan(storage: &Storage, source: &str, root: &Path) {
    assert!(matches!(scan_into_library(storage, source, root).unwrap(), ScanResult::Complete(_)));
}

fn save_manual(storage: &Storage, film: &catalog::CatalogFilm) {
    catalog::set_favorite(storage, &film.id, true).unwrap();
    catalog::set_rating(storage, &film.id, Some(4)).unwrap();
    catalog::create_tag(storage, "虚构手工标签").unwrap();
    catalog::set_film_tags(storage, &film.id, &["虚构手工标签".into()]).unwrap();
    playback::set_preferred(storage, &film.id, &film.videos[0].id, true).unwrap();
    PlaybackSession::default().launch(storage, "synthetic-launch", &film.id, &film.videos[0].id, |_| Ok(())).unwrap();
}

fn assert_manual(storage: &Storage, old_id: &str) {
    let films = catalog::list_films(storage).unwrap();
    let film = films.iter().find(|film| film.id == old_id).unwrap();
    assert!(film.favorite);
    assert_eq!(film.rating, Some(4.0));
    assert_eq!(film.tags, ["虚构手工标签"]);
    assert!(film.last_launched_at.is_some());
    assert_eq!(playback::load_history(storage).unwrap().events.len(), 1);
    storage.with_connection(|db| {
        let retained: i64 = db.query_row("SELECT COUNT(*) FROM file_versions WHERE preferred=1", [], |r| r.get(0))?;
        assert_eq!(retained, 1);
        Ok(())
    }).unwrap();
}

fn film_ids(storage: &Storage) -> std::collections::BTreeSet<String> {
    storage.with_connection(|db| {
        let mut q = db.prepare("SELECT id FROM films")?;
        q.query_map([], |r| r.get(0))?.collect()
    }).unwrap()
}

#[test]
fn explicit_batch_reconciliation_restores_cross_source_move_in_either_scan_order() {
    for destination_first in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let old_root = home.path().join("synthetic-old");
        let new_root = home.path().join("synthetic-new");
        touch(&old_root, "[TEST-01] 原/movie.mp4");
        fs::create_dir_all(&new_root).unwrap();
        let storage = Storage::open(&home.path().join("data")).unwrap();
        let old_source = add(&storage, &old_root);
        let new_source = add(&storage, &new_root);
        let original = catalog::list_films(&storage).unwrap().remove(0);
        save_manual(&storage, &original);
        let before = film_ids(&storage);
        touch(&new_root, "[TEST-02] 新/movie.mp4");
        fs::remove_file(old_root.join("[TEST-01] 原/movie.mp4")).unwrap();
        if destination_first {
            scan(&storage, &new_source, &new_root);
            scan(&storage, &old_source, &old_root);
        } else {
            scan(&storage, &old_source, &old_root);
            scan(&storage, &new_source, &new_root);
        }
        let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
        assert_eq!(catalog::reconcile_filename_history(&storage, &created).unwrap(), usize::from(destination_first));
        assert_eq!(catalog::list_films(&storage).unwrap().len(), 1);
        assert_manual(&storage, &original.id);
    }
}

#[test]
fn explicit_batch_reconciliation_keeps_current_duplicates_and_new_manual_records_separate() {
    for save_new_manual in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let old_root = home.path().join("synthetic-old");
        let new_root = home.path().join("synthetic-new");
        touch(&old_root, "[TEST-01] 原/movie.mp4");
        fs::create_dir_all(&new_root).unwrap();
        let storage = Storage::open(&home.path().join("data")).unwrap();
        let old_source = add(&storage, &old_root);
        let new_source = add(&storage, &new_root);
        let old_id = catalog::list_films(&storage).unwrap().remove(0).id;
        let before = film_ids(&storage);
        touch(&new_root, "[TEST-02] 新/movie.mp4");
        scan(&storage, &new_source, &new_root);
        let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
        if save_new_manual {
            catalog::set_rating(&storage, &created[0], Some(3)).unwrap();
            fs::remove_file(old_root.join("[TEST-01] 原/movie.mp4")).unwrap();
            scan(&storage, &old_source, &old_root);
        }
        assert_eq!(catalog::reconcile_filename_history(&storage, &created).unwrap(), 0);
        let films = catalog::list_films(&storage).unwrap();
        assert_eq!(films.len(), if save_new_manual { 1 } else { 2 });
        assert!(films.iter().any(|f| f.id == created[0]));
        if save_new_manual {
            assert_eq!(films[0].rating, Some(3.0));
            assert_ne!(films[0].id, old_id);
        }
    }
}

#[test]
fn failed_explicit_batch_reconciliation_retains_only_its_intent_and_retries_after_restart() {
    let home = tempfile::tempdir().unwrap();
    let old_root = home.path().join("synthetic-old");
    let new_root = home.path().join("synthetic-new");
    touch(&old_root, "[TEST-01] 原/movie.mp4");
    fs::create_dir_all(&new_root).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let old_source = add(&storage, &old_root);
    let new_source = add(&storage, &new_root);
    let original = catalog::list_films(&storage).unwrap().remove(0);
    save_manual(&storage, &original);
    let before = film_ids(&storage);
    touch(&new_root, "[TEST-02] 新/movie.mp4");
    fs::remove_file(old_root.join("[TEST-01] 原/movie.mp4")).unwrap();
    scan(&storage, &new_source, &new_root);
    scan(&storage, &old_source, &old_root);
    let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
    let before_problem_ids = storage.with_connection(|db| {
        let mut q = db.prepare("SELECT id FROM issue_records WHERE subject_type='library_problem_current' ORDER BY id")?;
        q.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()
    }).unwrap();
    storage.with_connection(|db| db.execute_batch("CREATE TRIGGER reject_restore_problem BEFORE INSERT ON issue_records WHEN NEW.subject_type='library_problem_current' BEGIN SELECT RAISE(ABORT,'synthetic reconciliation failure'); END")).unwrap();
    assert!(catalog::reconcile_filename_history(&storage, &created).is_err());
    assert_eq!(catalog::list_films(&storage).unwrap()[0].id, created[0]);
    storage.with_connection(|db| {
        let aliases: i64 = db.query_row("SELECT COUNT(*) FROM film_aliases", [], |r| r.get(0))?;
        assert_eq!(aliases, 0);
        let marker: String = db.query_row("SELECT evidence FROM issue_records WHERE subject_type='catalog_restore_scope'", [], |r| r.get(0))?;
        assert_eq!(serde_json::from_str::<std::collections::BTreeMap<String, String>>(&marker).unwrap(), std::collections::BTreeMap::from([(created[0].clone(), original.id.clone())]));
        let mut q = db.prepare("SELECT id FROM issue_records WHERE subject_type='library_problem_current' ORDER BY id")?;
        assert_eq!(q.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?, before_problem_ids);
        Ok(())
    }).unwrap();
    drop(storage);
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| db.execute_batch("DROP TRIGGER reject_restore_problem")).unwrap();
    assert_eq!(catalog::reconcile_filename_history(&storage, &[]).unwrap(), 1);
    assert_manual(&storage, &original.id);
    storage.with_connection(|db| {
        let pending: i64 = db.query_row("SELECT COUNT(*) FROM issue_records WHERE subject_type='catalog_restore_scope'", [], |r| r.get(0))?;
        assert_eq!(pending, 0);
        let alias: String = db.query_row("SELECT film_id FROM film_aliases WHERE id=?1", [&created[0]], |r| r.get(0))?;
        assert_eq!(alias, original.id);
        Ok(())
    }).unwrap();
    assert_eq!(catalog::reconcile_filename_history(&storage, &[]).unwrap(), 0);
}

#[test]
fn reconciliation_protection_failure_writes_no_retry_intent() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    touch(&root, "[TEST-01] 影片/movie.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let new_root = home.path().join("synthetic-new");
    fs::create_dir_all(&new_root).unwrap();
    let new_source = add(&storage, &new_root);
    let before = film_ids(&storage);
    touch(&new_root, "[TEST-02] 新/movie.mp4");
    fs::remove_file(root.join("[TEST-01] 影片/movie.mp4")).unwrap();
    scan(&storage, &new_source, &new_root);
    scan(&storage, &source, &root);
    let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
    let protections = home.path().join("data/protections");
    fs::remove_dir_all(&protections).unwrap();
    fs::write(&protections, []).unwrap();
    assert!(catalog::reconcile_filename_history(&storage, &created).is_err());
    storage.with_connection(|db| {
        let markers: i64 = db.query_row("SELECT COUNT(*) FROM issue_records WHERE subject_type='catalog_restore_scope'", [], |r| r.get(0))?;
        assert_eq!(markers, 0);
        Ok(())
    }).unwrap();
    assert_eq!(catalog::list_films(&storage).unwrap()[0].id, created[0]);
}

#[test]
fn confirmed_current_duplicate_is_not_reinterpreted_as_a_later_move() {
    let home = tempfile::tempdir().unwrap();
    let old_root = home.path().join("synthetic-old");
    let new_root = home.path().join("synthetic-new");
    touch(&old_root, "[TEST-01] 原/movie.mp4");
    fs::create_dir_all(&new_root).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let old_source = add(&storage, &old_root);
    let new_source = add(&storage, &new_root);
    let original = catalog::list_films(&storage).unwrap().remove(0);
    catalog::set_favorite(&storage, &original.id, true).unwrap();
    let before = film_ids(&storage);
    touch(&new_root, "[TEST-02] 独立/movie.mp4");
    scan(&storage, &new_source, &new_root);
    scan(&storage, &old_source, &old_root);
    let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
    assert_eq!(catalog::reconcile_filename_history(&storage, &created).unwrap(), 0);
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 2);
    fs::remove_file(old_root.join("[TEST-01] 原/movie.mp4")).unwrap();
    scan(&storage, &old_source, &old_root);
    scan(&storage, &new_source, &new_root);
    assert_eq!(catalog::reconcile_filename_history(&storage, &[]).unwrap(), 0);
    let film = catalog::list_films(&storage).unwrap().remove(0);
    assert_eq!(film.id, created[0]);
    assert!(!film.favorite);
    assert_ne!(film.id, original.id);
}

#[test]
fn recovery_source_candidates_only_refresh_the_unique_related_source() {
    for moved in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let old_root = home.path().join("synthetic-old");
        let new_root = home.path().join("synthetic-new");
        let unrelated_root = home.path().join("synthetic-unrelated");
        touch(&old_root, "[TEST-01] 原/movie.mp4");
        fs::create_dir_all(&new_root).unwrap();
        touch(&unrelated_root, "[TEST-03] 无关/unrelated.mp4");
        let storage = Storage::open(&home.path().join("data")).unwrap();
        let old_source = add(&storage, &old_root);
        let new_source = add(&storage, &new_root);
        let unrelated_source = add(&storage, &unrelated_root);
        let original = catalog::list_films(&storage).unwrap().into_iter().find(|film| film.videos[0].source_id == old_source).unwrap();
        catalog::set_favorite(&storage, &original.id, true).unwrap();
        let before = film_ids(&storage);
        touch(&new_root, "[TEST-02] 新/movie.mp4");
        touch(&new_root, "[TEST-02] 新/new-version.mkv");
        if moved { fs::remove_file(old_root.join("[TEST-01] 原/movie.mp4")).unwrap(); }
        scan(&storage, &new_source, &new_root);
        let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
        let candidates = catalog::filename_recovery_sources(&storage, &created).unwrap();
        assert_eq!(candidates, [old_source.clone()]);
        assert!(!candidates.contains(&new_source));
        assert!(!candidates.contains(&unrelated_source));
        scan(&storage, &old_source, &old_root);
        assert_eq!(catalog::reconcile_filename_history(&storage, &created).unwrap(), usize::from(moved));
        let film = catalog::list_films(&storage).unwrap().into_iter().find(|film| film.videos[0].source_id == new_source).unwrap();
        if moved {
            assert_eq!(film.id, original.id);
            assert!(film.favorite);
        } else {
            assert_eq!(film.id, created[0]);
            assert!(!film.favorite);
        }
    }
}

#[test]
fn recovery_source_candidates_reject_ambiguous_history_or_competing_new_groups() {
    for ambiguous_history in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let old_root = home.path().join("synthetic-old");
        let new_root = home.path().join("synthetic-new");
        touch(&old_root, "[TEST-01] 原/movie.mp4");
        if ambiguous_history { touch(&old_root, "[TEST-02] 另一影片/movie.mp4"); }
        fs::create_dir_all(&new_root).unwrap();
        let storage = Storage::open(&home.path().join("data")).unwrap();
        add(&storage, &old_root);
        let new_source = add(&storage, &new_root);
        let before = film_ids(&storage);
        touch(&new_root, "[TEST-03] 新/movie.mp4");
        if !ambiguous_history { touch(&new_root, "[TEST-04] 另一新影片/movie.mp4"); }
        scan(&storage, &new_source, &new_root);
        let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
        assert!(catalog::filename_recovery_sources(&storage, &created).unwrap().is_empty());
    }
}

#[test]
fn recovery_source_candidates_exclude_manual_identity_same_source_and_nonactive_sources() {
    for case in ["manual", "same_source", "paused", "removed"] {
        let home = tempfile::tempdir().unwrap();
        let old_root = home.path().join("synthetic-old");
        let new_root = home.path().join("synthetic-new");
        touch(&old_root, "[TEST-01] 原/movie.mp4");
        fs::create_dir_all(&new_root).unwrap();
        let storage = Storage::open(&home.path().join("data")).unwrap();
        let old_source = add(&storage, &old_root);
        let new_source = add(&storage, &new_root);
        let before = film_ids(&storage);
        if case == "same_source" {
            touch(&old_root, "[TEST-02] 新/movie.mp4");
            scan(&storage, &old_source, &old_root);
        } else {
            touch(&new_root, "[TEST-02] 新/movie.mp4");
            scan(&storage, &new_source, &new_root);
        }
        let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
        match case {
            "manual" => catalog::set_favorite(&storage, &created[0], true).unwrap(),
            "paused" => change_source_state(&storage, &old_source, "pause").unwrap(),
            "removed" => change_source_state(&storage, &old_source, "remove").unwrap(),
            _ => {},
        }
        assert!(catalog::filename_recovery_sources(&storage, &created).unwrap().is_empty());
        assert!(catalog::filename_recovery_sources(&storage, &[]).unwrap().is_empty());
    }
}

#[test]
fn related_source_unavailable_preserves_its_snapshot_and_prevents_unproven_recovery() {
    let home = tempfile::tempdir().unwrap();
    let old_root = home.path().join("synthetic-old");
    let new_root = home.path().join("synthetic-new");
    touch(&old_root, "[TEST-01] 原/movie.mp4");
    fs::create_dir_all(&new_root).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let old_source = add(&storage, &old_root);
    let new_source = add(&storage, &new_root);
    let bound: String = storage.with_connection(|db| db.query_row("SELECT root_path FROM libraries WHERE id=?1", [&old_source], |r| r.get(0))).unwrap();
    let original = catalog::list_films(&storage).unwrap().remove(0);
    catalog::set_favorite(&storage, &original.id, true).unwrap();
    let before = film_ids(&storage);
    touch(&new_root, "[TEST-02] 新/movie.mp4");
    fs::rename(&old_root, home.path().join("synthetic-offline")).unwrap();
    scan(&storage, &new_source, &new_root);
    let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
    assert_eq!(catalog::filename_recovery_sources(&storage, &created).unwrap(), [old_source.clone()]);
    assert!(matches!(scan_into_library(&storage, &old_source, Path::new(&bound)).unwrap(), ScanResult::RootUnavailable { .. }));
    assert_eq!(catalog::reconcile_filename_history(&storage, &created).unwrap(), 0);
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 2);
    assert!(films.iter().find(|f| f.id == original.id).unwrap().favorite);
    assert!(!films.iter().find(|f| f.id == created[0]).unwrap().favorite);
}

#[test]
fn mixed_batch_failure_retries_only_the_proven_move_and_never_the_rejected_duplicate() {
    let home = tempfile::tempdir().unwrap();
    let old_root = home.path().join("synthetic-old");
    let new_root = home.path().join("synthetic-new");
    touch(&old_root, "[TEST-01] 真实移动/a.mp4");
    touch(&old_root, "[TEST-02] 保持并存/b.mp4");
    fs::create_dir_all(&new_root).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let old_source = add(&storage, &old_root);
    let new_source = add(&storage, &new_root);
    let old = catalog::list_films(&storage).unwrap();
    let old_a = old.iter().find(|film| film.videos[0].observed_path.ends_with("a.mp4")).unwrap().id.clone();
    let old_b = old.iter().find(|film| film.videos[0].observed_path.ends_with("b.mp4")).unwrap().id.clone();
    catalog::set_favorite(&storage, &old_a, true).unwrap();
    catalog::set_favorite(&storage, &old_b, true).unwrap();
    let before = film_ids(&storage);
    touch(&new_root, "[TEST-03] 移动目标/a.mp4");
    touch(&new_root, "[TEST-04] 独立影片/b.mp4");
    fs::remove_file(old_root.join("[TEST-01] 真实移动/a.mp4")).unwrap();
    scan(&storage, &new_source, &new_root);
    scan(&storage, &old_source, &old_root);
    let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
    let new_b = catalog::list_films(&storage).unwrap().into_iter().find(|film| film.videos[0].source_id == new_source && film.videos[0].observed_path.ends_with("b.mp4")).unwrap().id;
    storage.with_connection(|db| db.execute_batch("CREATE TRIGGER reject_move_alias BEFORE INSERT ON film_aliases BEGIN SELECT RAISE(ABORT,'synthetic alias failure'); END")).unwrap();
    assert!(catalog::reconcile_filename_history(&storage, &created).is_err());
    fs::remove_file(old_root.join("[TEST-02] 保持并存/b.mp4")).unwrap();
    scan(&storage, &old_source, &old_root);
    drop(storage);
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| db.execute_batch("DROP TRIGGER reject_move_alias")).unwrap();
    assert_eq!(catalog::reconcile_filename_history(&storage, &[]).unwrap(), 1);
    let films = catalog::list_films(&storage).unwrap();
    assert!(films.iter().any(|film| film.id == old_a && film.favorite));
    assert!(films.iter().any(|film| film.id == new_b && !film.favorite));
    assert!(films.iter().all(|film| film.id != old_b));
}

#[test]
fn rejected_duplicate_never_creates_retry_scope_even_when_scope_delete_would_fail() {
    let home = tempfile::tempdir().unwrap();
    let old_root = home.path().join("synthetic-old");
    let new_root = home.path().join("synthetic-new");
    touch(&old_root, "[TEST-01] 原/movie.mp4");
    fs::create_dir_all(&new_root).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let old_source = add(&storage, &old_root);
    let new_source = add(&storage, &new_root);
    let before = film_ids(&storage);
    touch(&new_root, "[TEST-02] 独立/movie.mp4");
    scan(&storage, &new_source, &new_root);
    scan(&storage, &old_source, &old_root);
    let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
    storage.with_connection(|db| db.execute_batch("CREATE TRIGGER reject_scope_delete BEFORE DELETE ON issue_records WHEN OLD.subject_type='catalog_restore_scope' BEGIN SELECT RAISE(ABORT,'synthetic intent delete failure'); END")).unwrap();
    assert_eq!(catalog::reconcile_filename_history(&storage, &created).unwrap(), 0);
    fs::remove_file(old_root.join("[TEST-01] 原/movie.mp4")).unwrap();
    scan(&storage, &old_source, &old_root);
    assert_eq!(catalog::reconcile_filename_history(&storage, &[]).unwrap(), 0);
    assert_eq!(catalog::list_films(&storage).unwrap()[0].id, created[0]);
    storage.with_connection(|db| {
        let count: i64 = db.query_row("SELECT COUNT(*) FROM issue_records WHERE subject_type='catalog_restore_scope'", [], |r| r.get(0))?;
        assert_eq!(count, 0);
        Ok(())
    }).unwrap();
}

#[test]
fn cancelled_proven_intent_is_empty_before_cleanup_failure_and_cannot_revive_later() {
    let home = tempfile::tempdir().unwrap();
    let old_root = home.path().join("synthetic-old");
    let new_root = home.path().join("synthetic-new");
    touch(&old_root, "[TEST-01] 原/movie.mp4");
    fs::create_dir_all(&new_root).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let old_source = add(&storage, &old_root);
    let new_source = add(&storage, &new_root);
    let before = film_ids(&storage);
    touch(&new_root, "[TEST-02] 新/movie.mp4");
    fs::remove_file(old_root.join("[TEST-01] 原/movie.mp4")).unwrap();
    scan(&storage, &new_source, &new_root);
    scan(&storage, &old_source, &old_root);
    let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
    storage.with_connection(|db| db.execute_batch("CREATE TRIGGER reject_move_alias BEFORE INSERT ON film_aliases BEGIN SELECT RAISE(ABORT,'synthetic alias failure'); END")).unwrap();
    assert!(catalog::reconcile_filename_history(&storage, &created).is_err());
    storage.with_connection(|db| db.execute_batch("DROP TRIGGER reject_move_alias; CREATE TRIGGER reject_scope_delete BEFORE DELETE ON issue_records WHEN OLD.subject_type='catalog_restore_scope' BEGIN SELECT RAISE(ABORT,'synthetic intent delete failure'); END")).unwrap();
    touch(&old_root, "[TEST-01] 原/movie.mp4");
    scan(&storage, &old_source, &old_root);
    assert!(catalog::reconcile_filename_history(&storage, &[]).is_err());
    storage.with_connection(|db| {
        let json: String = db.query_row("SELECT evidence FROM issue_records WHERE subject_type='catalog_restore_scope'", [], |r| r.get(0))?;
        assert_eq!(serde_json::from_str::<std::collections::BTreeMap<String, String>>(&json).unwrap().len(), 0);
        Ok(())
    }).unwrap();
    fs::remove_file(old_root.join("[TEST-01] 原/movie.mp4")).unwrap();
    scan(&storage, &old_source, &old_root);
    assert_eq!(catalog::reconcile_filename_history(&storage, &[]).unwrap(), 0);
    assert_eq!(catalog::list_films(&storage).unwrap()[0].id, created[0]);
}

fn assert_current_part_b_record(storage: &Storage, old_event_observation: &str) -> String {
    let film = catalog::list_films(storage).unwrap().remove(0);
    let video = film.videos.iter().find(|v| v.observed_path.ends_with("part-b.mkv")).unwrap();
    assert_eq!(film.preferred_video_id.as_deref(), Some(video.id.as_str()));
    storage.with_connection(|db| {
        let version:i64=db.query_row("SELECT preferred FROM file_versions WHERE observation_id=?1",[&video.id],|r|r.get(0))?;assert_eq!(version,1);
        let count: i64 = db.query_row("SELECT COUNT(*) FROM file_versions", [], |r| r.get(0))?;
        assert_eq!(count, 1);
        let event_observation: String = db.query_row("SELECT observation_id FROM player_launch_events", [], |r| r.get(0))?;
        assert_eq!(event_observation, old_event_observation);
        Ok(())
    }).unwrap();
    video.id.clone()
}

#[test]
fn preferred_part_follows_two_moves_without_rewriting_launch_events() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    let other = home.path().join("synthetic-other");
    touch(&root, "[TEST-01] 原/part-a.mp4");
    touch(&root, "[TEST-01] 原/part-b.mkv");
    fs::create_dir_all(&other).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let other_source = add(&storage, &other);
    let original = catalog::list_films(&storage).unwrap().remove(0);
    let old_b = original.videos.iter().find(|v| v.observed_path.ends_with("part-b.mkv")).unwrap().id.clone();
    storage.with_connection(|db| {
        db.execute("INSERT INTO file_versions(observation_id,preferred) VALUES(?1,1)", [&old_b])?;
        Ok(())
    }).unwrap();
    PlaybackSession::default().launch(&storage, "synthetic-part-launch", &original.id, &old_b, |_| Ok(())).unwrap();
    fs::rename(root.join("[TEST-01] 原"), root.join("[TEST-02] 第一次移动")).unwrap();
    scan(&storage, &source, &root);
    let first_b = assert_current_part_b_record(&storage, &old_b);
    assert_ne!(first_b, old_b);
    let before = film_ids(&storage);
    fs::rename(root.join("[TEST-02] 第一次移动"), other.join("[TEST-03] 第二次移动")).unwrap();
    scan(&storage, &other_source, &other);
    scan(&storage, &source, &root);
    let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
    assert_eq!(catalog::reconcile_filename_history(&storage, &created).unwrap(), 1);
    let second_b = assert_current_part_b_record(&storage, &old_b);
    assert_ne!(second_b, first_b);
    assert_eq!(catalog::list_films(&storage).unwrap()[0].id, original.id);
}

#[test]
fn file_version_transfer_failure_rolls_back_direct_and_coordinated_recovery() {
    for coordinated in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("synthetic");
        let other = home.path().join("synthetic-other");
        touch(&root, "[TEST-01] 原/part-b.mkv");
        fs::create_dir_all(&other).unwrap();
        let storage = Storage::open(&home.path().join("data")).unwrap();
        let source = add(&storage, &root);
        let other_source = add(&storage, &other);
        let original = catalog::list_films(&storage).unwrap().remove(0);
        let old_b = original.videos[0].id.clone();
        storage.with_connection(|db| {
            db.execute("INSERT INTO file_versions(observation_id,preferred) VALUES(?1,1)", [&old_b])?;
            Ok(())
        }).unwrap();
        PlaybackSession::default().launch(&storage, "synthetic-part-launch", &original.id, &old_b, |_| Ok(())).unwrap();
        let before = film_ids(&storage);
        if coordinated {
            fs::rename(root.join("[TEST-01] 原"), other.join("[TEST-02] 新")).unwrap();
            scan(&storage, &other_source, &other);
            scan(&storage, &source, &root);
        } else {
            fs::rename(root.join("[TEST-01] 原"), root.join("[TEST-02] 新")).unwrap();
        }
        let created = film_ids(&storage).difference(&before).cloned().collect::<Vec<_>>();
        storage.with_connection(|db| db.execute_batch("CREATE TRIGGER reject_version_move BEFORE UPDATE OF observation_id ON file_versions BEGIN SELECT RAISE(ABORT,'synthetic version transfer failure'); END")).unwrap();
        if coordinated {
            assert!(catalog::reconcile_filename_history(&storage, &created).is_err());
        } else {
            assert!(scan_into_library(&storage, &source, &root).is_err());
            assert_eq!(catalog::list_films(&storage).unwrap()[0].videos[0].id, old_b);
        }
        storage.with_connection(|db| {
            let retained: String = db.query_row("SELECT observation_id FROM file_versions", [], |r| r.get(0))?;
            assert_eq!(retained, old_b);
            let aliases: i64 = db.query_row("SELECT COUNT(*) FROM film_aliases", [], |r| r.get(0))?;
            assert_eq!(aliases, 0);
            db.execute_batch("DROP TRIGGER reject_version_move")?;
            Ok(())
        }).unwrap();
        if coordinated {
            assert_eq!(catalog::reconcile_filename_history(&storage, &[]).unwrap(), 1);
        } else {
            scan(&storage, &source, &root);
        }
        assert_current_part_b_record(&storage, &old_b);
        assert_eq!(catalog::list_films(&storage).unwrap()[0].id, original.id);
    }
}

#[test]
fn file_version_restore_refuses_ambiguous_history_and_never_overwrites_new_manual_record() {
    for new_has_manual in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("synthetic");
        touch(&root, "[TEST-01] 原/movie.mp4");
        let storage = Storage::open(&home.path().join("data")).unwrap();
        let source = add(&storage, &root);
        let original = catalog::list_films(&storage).unwrap().remove(0);
        let old_video = original.videos[0].id.clone();
        fs::remove_file(root.join("[TEST-01] 原/movie.mp4")).unwrap();
        touch(&root, "[TEST-02] 新/movie.mp4");
        storage.with_connection(|db| {
            db.execute("INSERT INTO file_versions(observation_id,preferred) VALUES(?1,1)", [&old_video])?;
            db.execute("INSERT INTO file_observations(id,library_id,observed_path,kind) VALUES('synthetic-new',?1,'[TEST-02] 新/movie.mp4','video')", [&source])?;
            if new_has_manual {
                db.execute("INSERT INTO file_versions(observation_id,preferred) VALUES('synthetic-new',0)", [])?;
            } else {
                db.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('synthetic-other-history',?1,?2,'历史/movie.mp4','video')", (&source, &original.id))?;
                db.execute("INSERT INTO file_versions(observation_id,preferred) VALUES('synthetic-other-history',0)", [])?;
            }
            db.execute("UPDATE issue_records SET evidence='[\"synthetic-new\"]' WHERE subject_type='cover_scan' AND subject_id=?1", [&source])?;
            Ok(())
        }).unwrap();
        catalog::ensure_film_entries(&storage).unwrap();
        assert_eq!(catalog::list_films(&storage).unwrap()[0].id, original.id);
        storage.with_connection(|db| {
            let old:i64=db.query_row("SELECT preferred FROM file_versions WHERE observation_id=?1",[&old_video],|r|r.get(0))?;assert_eq!(old,1);
            let new_count: i64 = db.query_row("SELECT COUNT(*) FROM file_versions WHERE observation_id='synthetic-new'", [], |r| r.get(0))?;
            assert_eq!(new_count, i64::from(new_has_manual));
            if new_has_manual {
                let new:i64=db.query_row("SELECT preferred FROM file_versions WHERE observation_id='synthetic-new'",[],|r|r.get(0))?;assert_eq!(new,0);
            }
            Ok(())
        }).unwrap();
    }
}

#[test]
fn removed_source_then_moved_root_restores_identity_manual_data_and_history_after_restart() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-original");
    touch(&root, "[TEST-01] 原目录/movie.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let original = catalog::list_films(&storage).unwrap().remove(0);
    save_manual(&storage, &original);
    change_source_state(&storage, &source, "remove").unwrap();
    let moved = home.path().join("synthetic-moved");
    fs::rename(&root, &moved).unwrap();
    drop(storage);
    let storage = Storage::open(&home.path().join("data")).unwrap();
    assert_ne!(add(&storage, &moved), source);
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 1);
    assert_manual(&storage, &original.id);
    storage.with_connection(|db| {
        let observations: i64 = db.query_row("SELECT COUNT(*) FROM file_observations WHERE kind='video'", [], |r| r.get(0))?;
        assert_eq!(observations, 2, "the old observation remains historical");
        Ok(())
    }).unwrap();
    assert_eq!(fs::metadata(moved.join("[TEST-01] 原目录/movie.mp4")).unwrap().len(), 0);
}

#[test]
fn missing_then_reappearing_in_another_folder_restores_the_original_film() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    touch(&root, "[TEST-01] 原目录/movie.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let original = catalog::list_films(&storage).unwrap().remove(0);
    save_manual(&storage, &original);
    fs::remove_file(root.join("[TEST-01] 原目录/movie.mp4")).unwrap();
    scan(&storage, &source, &root);
    assert!(catalog::list_films(&storage).unwrap().is_empty());
    touch(&root, "[TEST-02] 新目录/movie.mp4");
    scan(&storage, &source, &root);
    assert_manual(&storage, &original.id);
}

#[test]
fn move_in_one_scan_recovers_using_the_complete_new_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    touch(&root, "[TEST-01] 原目录/movie.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let original = catalog::list_films(&storage).unwrap().remove(0);
    fs::create_dir(root.join("[TEST-02] 新目录")).unwrap();
    fs::rename(root.join("[TEST-01] 原目录/movie.mp4"), root.join("[TEST-02] 新目录/movie.mp4")).unwrap();
    scan(&storage, &source, &root);
    assert_eq!(catalog::list_films(&storage).unwrap()[0].id, original.id);
}

#[test]
fn filename_match_ignores_case_but_requires_the_full_extension() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    touch(&root, "[TEST-01] 原目录/Épisode.MP4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let original = catalog::list_films(&storage).unwrap().remove(0);
    fs::remove_file(root.join("[TEST-01] 原目录/Épisode.MP4")).unwrap();
    touch(&root, "[TEST-02] 同扩展/éPISODE.mp4");
    touch(&root, "[TEST-03] 异扩展/éPISODE.mkv");
    scan(&storage, &source, &root);
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 2);
    assert_eq!(films.iter().find(|f| f.videos[0].observed_path.ends_with(".mp4")).unwrap().id, original.id);
    assert_ne!(films.iter().find(|f| f.videos[0].observed_path.ends_with(".mkv")).unwrap().id, original.id);
}

#[test]
fn paused_source_is_in_the_historical_filename_index() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-old");
    touch(&root, "[TEST-01] 原目录/movie.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let original = catalog::list_films(&storage).unwrap().remove(0);
    change_source_state(&storage, &source, "pause").unwrap();
    let moved = home.path().join("synthetic-new");
    fs::rename(&root, &moved).unwrap();
    add(&storage, &moved);
    assert_eq!(catalog::list_films(&storage).unwrap()[0].id, original.id);
}

#[test]
fn simultaneously_present_same_filename_in_distinct_folders_or_sources_never_merges() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-first");
    touch(&root, "[TEST-01] 第一/movie.mp4");
    touch(&root, "[TEST-02] 第二/movie.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let first = catalog::list_films(&storage).unwrap();
    assert_eq!(first.len(), 2);
    assert_ne!(first[0].id, first[1].id);
    let other = home.path().join("synthetic-second");
    touch(&other, "[TEST-03] 第三/movie.mp4");
    add(&storage, &other);
    scan(&storage, &source, &root);
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 3);
    assert_eq!(films.iter().map(|f| &f.id).collect::<std::collections::BTreeSet<_>>().len(), 3);
}

#[test]
fn ambiguous_absent_history_does_not_choose_or_merge_either_film() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-old");
    touch(&root, "[TEST-01] 第一/movie.mp4");
    touch(&root, "[TEST-02] 第二/movie.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let old = catalog::list_films(&storage).unwrap();
    for film in &old { catalog::set_favorite(&storage, &film.id, true).unwrap(); }
    change_source_state(&storage, &source, "remove").unwrap();
    let other = home.path().join("synthetic-new");
    touch(&other, "[TEST-03] 新/movie.mp4");
    add(&storage, &other);
    let restored = catalog::list_films(&storage).unwrap().remove(0);
    assert!(old.iter().all(|f| f.id != restored.id));
    assert!(!restored.favorite);
    storage.with_connection(|db| {
        let retained: i64 = db.query_row("SELECT COUNT(*) FROM film_manual WHERE favorite=1", [], |r| r.get(0))?;
        assert_eq!(retained, 2);
        Ok(())
    }).unwrap();
}

#[test]
fn two_new_groups_cannot_claim_one_absent_historical_film() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-old");
    touch(&root, "[TEST-01] 原/movie.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let original = catalog::list_films(&storage).unwrap().remove(0).id;
    change_source_state(&storage, &source, "remove").unwrap();
    let other = home.path().join("synthetic-new");
    touch(&other, "[TEST-02] 第一/movie.mp4");
    touch(&other, "[TEST-03] 第二/movie.mp4");
    add(&storage, &other);
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 2);
    assert_ne!(films[0].id, films[1].id);
    assert!(films.iter().all(|f| f.id != original));
}

#[test]
fn versions_are_planned_together_and_only_one_historical_film_is_restored() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-old");
    touch(&root, "[TEST-01] 原/part-a.mp4");
    touch(&root, "[TEST-01] 原/part-b.mkv");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let original = catalog::list_films(&storage).unwrap().remove(0);
    save_manual(&storage, &original);
    change_source_state(&storage, &source, "remove").unwrap();
    let other = home.path().join("synthetic-new");
    touch(&other, "[TEST-02] 新/brand-new.mp4");
    touch(&other, "[TEST-02] 新/part-b.mkv");
    add(&storage, &other);
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 1);
    assert_eq!(films[0].videos.len(), 2);
    assert_manual(&storage, &original.id);
}

#[test]
fn versions_pointing_to_different_history_identities_do_not_merge_manual_records() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-old");
    touch(&root, "[TEST-01] 第一/one.mp4");
    touch(&root, "[TEST-02] 第二/two.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let old = catalog::list_films(&storage).unwrap();
    change_source_state(&storage, &source, "remove").unwrap();
    let other = home.path().join("synthetic-new");
    touch(&other, "[TEST-03] 新/one.mp4");
    touch(&other, "[TEST-03] 新/two.mp4");
    add(&storage, &other);
    let film = catalog::list_films(&storage).unwrap().remove(0);
    assert_eq!(film.videos.len(), 2);
    assert!(old.iter().all(|f| f.id != film.id));
}

#[test]
fn failed_restoration_rolls_back_links_current_snapshot_and_preserves_history() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    touch(&root, "[TEST-01] 原/movie.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let original = catalog::list_films(&storage).unwrap().remove(0);
    save_manual(&storage, &original);
    fs::remove_file(root.join("[TEST-01] 原/movie.mp4")).unwrap();
    touch(&root, "[TEST-02] 新/movie.mp4");
    touch(&root, "[TEST-02] 新/new-version.mkv");
    storage.with_connection(|db| db.execute_batch("CREATE TRIGGER reject_restore BEFORE UPDATE OF film_id ON file_observations WHEN NEW.observed_path LIKE '[TEST-02]%' BEGIN SELECT RAISE(ABORT,'synthetic restoration failure'); END")).unwrap();
    assert!(scan_into_library(&storage, &source, &root).is_err());
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 1);
    assert_eq!(films[0].videos[0].observed_path, "[TEST-01] 原/movie.mp4");
    assert_manual(&storage, &original.id);
    storage.with_connection(|db| {
        let state: (i64, i64) = db.query_row("SELECT (SELECT COUNT(*) FROM films),(SELECT COUNT(*) FROM file_observations WHERE kind='video')", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        assert_eq!(state, (1, 1));
        db.execute_batch("DROP TRIGGER reject_restore")?;
        Ok(())
    }).unwrap();
    scan(&storage, &source, &root);
    assert_manual(&storage, &original.id);
    assert_eq!(catalog::list_films(&storage).unwrap()[0].videos.len(), 2);
}
