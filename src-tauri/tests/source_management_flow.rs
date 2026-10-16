use local_film_library_preview_lib::catalog::list_films;
use local_film_library_preview_lib::source_scan::{
    SourceScanSession, change_source_state, list_sources,
};
use local_film_library_preview_lib::storage::Storage;
use std::fs::{self, File};
use std::path::Path;

fn touch(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    File::create(path).unwrap();
}

fn add(storage: &Storage, root: &Path) -> String {
    let session = SourceScanSession::new();
    let preview = session.preview(storage, root, None).unwrap();
    session
        .confirm(storage, &preview.token.unwrap())
        .unwrap()
        .library_id
}

#[test]
fn direct_source_removal_preserves_manual_data_and_fresh_readd() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("media");
    touch(&root, "[TEST-01] 旧片/old.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = add(&storage, &root);
    let original = list_films(&storage).unwrap().remove(0);
    let video = &original.videos[0].id;
    local_film_library_preview_lib::catalog::set_favorite(&storage, &original.id, true).unwrap();
    local_film_library_preview_lib::catalog::set_rating(&storage, &original.id, Some(4)).unwrap();
    local_film_library_preview_lib::catalog::create_tag(&storage, "人工标签").unwrap();
    local_film_library_preview_lib::catalog::set_film_tags(
        &storage,
        &original.id,
        &["人工标签".into()],
    )
    .unwrap();
    local_film_library_preview_lib::playback::set_preferred(&storage, &original.id, video, true)
        .unwrap();
    local_film_library_preview_lib::playback::PlaybackSession::default()
        .launch(
            &storage,
            "delete-test-request-1",
            &original.id,
            video,
            |_| Ok(()),
        )
        .unwrap();
    change_source_state(&storage, &source, "remove").unwrap();
    assert!(list_sources(&storage).unwrap().is_empty());
    assert!(change_source_state(&storage, &source, "remove").is_err());
    drop(storage);
    let reopened = Storage::open(&home.path().join("data")).unwrap();
    assert!(list_sources(&reopened).unwrap().is_empty());
    assert_ne!(add(&reopened, &root), source);
    let restored = list_films(&reopened).unwrap().remove(0);
    assert_eq!(restored.id, original.id);
    assert_ne!(restored.videos[0].id, *video);
    assert!(restored.favorite);
    assert_eq!(restored.rating, Some(4.0));
    assert_eq!(restored.tags, ["人工标签"]);
    assert_eq!(restored.preferred_video_id.as_deref(), Some(restored.videos[0].id.as_str()));
    assert_eq!(list_sources(&reopened).unwrap().len(), 1);
    assert_eq!(
        local_film_library_preview_lib::playback::load_history(&reopened)
            .unwrap()
            .events
            .len(),
        1
    );
    assert!(root.join("[TEST-01] 旧片/old.mp4").is_file());
}

#[test]
fn source_removal_protection_and_write_failures_keep_list_and_manual_data() {
    for fail_protection in [true, false] {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("media");
        touch(&root, "[TEST-01] 旧片/old.mp4");
        let storage = Storage::open(&home.path().join("data")).unwrap();
        let source = add(&storage, &root);
        let original = list_films(&storage).unwrap().remove(0);
        local_film_library_preview_lib::catalog::set_favorite(&storage, &original.id, true)
            .unwrap();
        if fail_protection {
            let protections = home.path().join("data/protections");
            // This is an isolated application-data directory, never user media.
            fs::remove_dir_all(&protections).unwrap();
            File::create(protections).unwrap();
        } else {
            storage.with_connection(|db| db.execute_batch("CREATE TRIGGER reject_hidden BEFORE DELETE ON libraries BEGIN SELECT RAISE(ABORT,'test'); END")).unwrap();
        }
        assert_eq!(
            change_source_state(&storage, &source, "remove")
                .unwrap_err()
                .to_string(),
            if fail_protection {
                "protection_failed"
            } else {
                "state_failed"
            }
        );
        assert_eq!(list_sources(&storage).unwrap()[0].id, source);
        storage.with_connection(|db| {
            let state:(String,i64,i64)=db.query_row("SELECT o.film_id,m.favorite,l.record_hidden FROM file_observations o JOIN film_manual m ON m.film_id=o.film_id JOIN libraries l ON l.id=o.library_id", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            assert_eq!(state,(original.id,1,0));
            Ok(())
        }).unwrap();
        assert!(root.join("[TEST-01] 旧片/old.mp4").is_file());
    }
}

#[test]
fn source_recursively_reads_root_actor_and_nested_video_directories() {
    let home = tempfile::tempdir().unwrap();
    let region = home.path().join("日本");
    touch(&region, "演员/actor.jpg");
    touch(&region, "演员/作品/poster.jpg");
    touch(&region, "演员/作品/part.mp4");
    touch(&region, "演员/作品/second.mp4");
    touch(&region, "[TEST-02] 多人作品/poster.jpg");
    touch(&region, "[TEST-02] 多人作品/无编号.mp4");
    touch(&region, "[TEST-02] 多人作品/two.mp4");
    touch(&region, "根部忽略.mp4");
    touch(&region, "演员/演员根部忽略.mp4");
    touch(&region, "[TEST-02] 多人作品/花絮/子目录忽略.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let session = SourceScanSession::new();
    let preview = session.preview(&storage, &region, None).unwrap();
    assert_eq!(preview.video_count, 7);
    assert_eq!(preview.video_directory_count, 5);
    assert_eq!(preview.cover_hint_count, 2);
    session.confirm(&storage, &preview.token.unwrap()).unwrap();
    assert_eq!(list_films(&storage).unwrap().len(), 5);
    storage
        .with_connection(|db| {
            let actors: i64 = db.query_row("SELECT COUNT(*) FROM actors", [], |row| row.get(0))?;
            assert_eq!(actors, 0);
            Ok(())
        })
        .unwrap();

    let home2 = tempfile::tempdir().unwrap();
    let category = home2.path().join("日本/多人");
    touch(&category, "无编号.mp4");
    touch(&category, "作品/one.mp4");
    let storage2 = Storage::open(&home2.path().join("data")).unwrap();
    let preview2 = SourceScanSession::new()
        .preview(&storage2, &category, None)
        .unwrap();
    assert_eq!(preview2.video_count, 2);
    assert_eq!(preview2.video_directory_count, 2);
}

#[test]
fn parent_child_overlap_blocks_new_source_without_changing_existing_records() {
    let home = tempfile::tempdir().unwrap();
    let parent = home.path().join("region");
    let child = parent.join("category");
    touch(&child, "actor.jpg");
    touch(&child, "[TEST-01] 作品/one.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let id = add(&storage, &child);
    let session = SourceScanSession::new();
    assert_eq!(
        session
            .preview(&storage, &parent, None)
            .err()
            .unwrap()
            .to_string(),
        "range_overlap"
    );
    assert_eq!(
        session
            .preview(&storage, &child, None)
            .err()
            .unwrap()
            .to_string(),
        "already_bound"
    );
    assert_eq!(list_sources(&storage).unwrap()[0].id, id);
    assert_eq!(list_films(&storage).unwrap().len(), 1);
    assert_eq!(list_sources(&storage).unwrap()[0].id, id);
}

#[test]
fn pause_resume_remove_and_readd_keep_ids_and_manual_relationships() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("media");
    touch(&root, "[TEST-01] 旧片/old.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let id = add(&storage, &root);
    let film = list_films(&storage).unwrap().remove(0);
    assert_eq!(list_sources(&storage).unwrap()[0].film_count, 1);
    let video_id = film.videos[0].id.clone();
    storage.with_connection(|db| {
        db.execute("INSERT INTO film_manual (film_id, title, rating, favorite) VALUES (?1, '人工标题', 8, 1)", [&film.id])?;
        db.execute("INSERT INTO actors (id, observed_name) VALUES ('person', '人工关联')", [])?;
        db.execute("INSERT INTO film_actors (film_id, actor_id, source) VALUES (?1, 'person', 'manual')", [&film.id])?;
        Ok(())
    }).unwrap();
    change_source_state(&storage, &id, "pause").unwrap();
    assert!(list_films(&storage).unwrap().is_empty());
    assert!(!list_sources(&storage).unwrap()[0].enabled);
    change_source_state(&storage, &id, "resume").unwrap();
    assert_eq!(list_films(&storage).unwrap()[0].id, film.id);
    change_source_state(&storage, &id, "remove").unwrap();
    assert!(list_films(&storage).unwrap().is_empty());
    assert!(list_sources(&storage).unwrap().is_empty());
    touch(&root, "[TEST-02] 新片/new.mp4");
    let session = SourceScanSession::new();
    let preview = session.preview(&storage, &root, None).unwrap();
    assert_eq!(preview.video_count, 2);
    let restored = session.confirm(&storage, &preview.token.unwrap()).unwrap();
    assert_ne!(restored.library_id, id);
    assert!(restored.inserted > 0);
    assert_eq!(list_films(&storage).unwrap().len(), 2);
    let scanned = session.preview(&storage, &root, Some(&restored.library_id)).unwrap();
    assert_eq!(scanned.video_count, 2);
    session.confirm(&storage, &scanned.token.unwrap()).unwrap();
    let films = list_films(&storage).unwrap();
    assert_eq!(films.len(), 2);
    let old = films.iter().find(|item| item.id == film.id).unwrap();
    assert_ne!(old.videos[0].id, video_id);
    assert_eq!(
        (old.title.as_str(), old.rating, old.favorite),
        (film.title.as_str(), Some(4.0), true)
    );
    storage
        .with_connection(|db| {
            let links: i64 = db.query_row(
                "SELECT COUNT(*) FROM film_actors WHERE film_id=?1",
                [&film.id],
                |row| row.get(0),
            )?;
            assert_eq!(links, 1);
            Ok(())
        })
        .unwrap();
    drop(storage);
    let reopened = Storage::open(&home.path().join("data")).unwrap();
    assert_eq!(list_films(&reopened).unwrap().len(), 2);
}

#[test]
fn removed_child_can_be_readded_from_parent_without_duplicate_film() {
    let home = tempfile::tempdir().unwrap();
    let parent = home.path().join("region");
    let child = parent.join("category");
    touch(&child, "actor.jpg");
    touch(&child, "[TEST-01] 旧片/old.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let original_source = add(&storage, &child);
    let original = list_films(&storage).unwrap().remove(0);
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO film_manual (film_id, favorite) VALUES (?1, 1)",
                [&original.id],
            )?;
            Ok(())
        })
        .unwrap();
    change_source_state(&storage, &original_source, "remove").unwrap();
    touch(&parent, "[TEST-02] 新片/new.mp4");
    let replacement = add(&storage, &parent);
    assert_ne!(replacement, original_source);
    let films = list_films(&storage).unwrap();
    assert_eq!(films.len(), 2);
    let preserved = films.iter().find(|film| film.id == original.id).unwrap();
    assert!(preserved.favorite);
    assert_ne!(preserved.videos[0].id, original.videos[0].id);
    assert_eq!(preserved.videos[0].source_id, replacement);
    assert_eq!(preserved.videos[0].observed_path, "category/[TEST-01] 旧片/old.mp4");
    assert_eq!(
        list_sources(&storage)
            .unwrap()
            .iter()
            .filter(|source| source.enabled)
            .count(),
        1
    );
}

#[test]
fn removed_parent_can_be_readded_from_child_while_outside_records_remain() {
    let home = tempfile::tempdir().unwrap();
    let parent = home.path().join("region");
    touch(&parent, "category/actor.jpg");
    touch(&parent, "category/[TEST-01] 保留/kept.mp4");
    touch(&parent, "[TEST-02] 外侧/outside.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let original_source = add(&storage, &parent);
    let films = list_films(&storage).unwrap();
    let kept = films
        .iter()
        .find(|film| film.videos[0].observed_path == "category/[TEST-01] 保留/kept.mp4")
        .unwrap();
    let kept_id = kept.id.clone();
    let kept_video_id = kept.videos[0].id.clone();
    change_source_state(&storage, &original_source, "remove").unwrap();
    let replacement = add(&storage, &parent.join("category"));
    let visible = list_films(&storage).unwrap();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, kept_id);
    assert_ne!(visible[0].videos[0].id, kept_video_id);
    assert_eq!(visible[0].videos[0].observed_path, "[TEST-01] 保留/kept.mp4");
    storage.with_connection(|db| {
        let old_count: i64 = db.query_row("SELECT COUNT(*) FROM libraries WHERE id=?1", [&original_source], |r| r.get(0))?;
        assert_eq!(old_count, 0);
        let history_count: i64 = db.query_row("SELECT COUNT(*) FROM file_observations WHERE library_id='software-manual-history' AND kind='video'", [], |r| r.get(0))?;
        assert_eq!(history_count, 2);
        Ok(())
    }).unwrap();
    assert_ne!(replacement, original_source);
    change_source_state(&storage, &replacement, "remove").unwrap();
    let session = SourceScanSession::new();
    let preview = session
        .preview(&storage, &parent, None)
        .unwrap();
    assert_ne!(
        session
            .confirm(&storage, &preview.token.unwrap())
            .unwrap()
            .library_id,
        original_source
    );
    let restored = list_films(&storage).unwrap();
    assert_eq!(restored.len(), 2);
    assert_ne!(
        restored
            .iter()
            .find(|film| film.id == kept_id)
            .unwrap()
            .videos[0]
            .id,
        kept_video_id
    );
}

#[test]
fn protection_incomplete_and_write_failures_leave_source_and_manual_data_unchanged() {
    let home = tempfile::tempdir().unwrap();
    let parent = home.path().join("region");
    let child = parent.join("category");
    touch(&child, "actor.jpg");
    touch(&child, "[TEST-01] 旧片/old.mp4");
    touch(&parent, "[TEST-02] 新片/new.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let id = add(&storage, &child);
    let old = list_films(&storage).unwrap().remove(0);
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO film_manual (film_id, title) VALUES (?1, '原资料')",
                [&old.id],
            )?;
            Ok(())
        })
        .unwrap();
    change_source_state(&storage, &id, "remove").unwrap();
    let session = SourceScanSession::new();
    let token = session
        .preview(&storage, &parent, None)
        .unwrap()
        .token
        .unwrap();
    storage.with_connection(|db| {
        db.execute_batch("CREATE TRIGGER reject_rehome BEFORE INSERT ON file_observations BEGIN SELECT RAISE(FAIL, 'injected'); END;")?;
        Ok(())
    }).unwrap();
    assert!(session.confirm(&storage, &token).is_err());
    storage
        .with_connection(|db| {
            let count: i64 = db.query_row(
                "SELECT COUNT(*) FROM libraries WHERE id=?1",
                [&id],
                |row| row.get(0),
            )?;
            let path: String = db.query_row(
                "SELECT observed_path FROM file_observations WHERE id=?1",
                [&old.videos[0].id],
                |row| row.get(0),
            )?;
            let owner: String = db.query_row(
                "SELECT library_id FROM file_observations WHERE id=?1",
                [&old.videos[0].id],
                |row| row.get(0),
            )?;
            let manual_title: String = db.query_row(
                "SELECT title FROM film_manual WHERE film_id=?1",
                [&old.id],
                |row| row.get(0),
            )?;
            assert_eq!(count, 0);
            assert_eq!(path, "old.mp4");
            assert_eq!(owner, "software-manual-history");
            assert_eq!(manual_title, "原资料");
            Ok(())
        })
        .unwrap();
    assert!(list_films(&storage).unwrap().is_empty());
    assert!(list_sources(&storage).unwrap().is_empty());

    let home2 = tempfile::tempdir().unwrap();
    let root2 = home2.path().join("media");
    touch(&root2, "[TEST-01] 作品/file.mp4");
    let storage2 = Storage::open(&home2.path().join("data")).unwrap();
    storage2
        .with_connection(|db| {
            db.execute(
                "INSERT INTO libraries (id, display_name, root_path) VALUES ('source', '虚构', ?1)",
                [root2.to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
        .unwrap();
    File::create(home2.path().join("data/protections")).unwrap();
    assert_eq!(
        change_source_state(&storage2, "source", "pause")
            .err()
            .unwrap()
            .to_string(),
        "protection_failed"
    );
    assert!(list_sources(&storage2).unwrap()[0].enabled);
}

#[test]
fn offline_and_linked_sources_allow_saved_state_changes_but_failed_writes_roll_back() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("media");
    touch(&root, "[TEST-01] 作品/one.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let id = add(&storage, &root);
    let original = list_films(&storage).unwrap().remove(0);
    local_film_library_preview_lib::catalog::set_favorite(&storage, &original.id, true).unwrap();
    fs::rename(&root, home.path().join("temporarily-offline")).unwrap();
    change_source_state(&storage, &id, "pause").unwrap();
    assert!(!list_sources(&storage).unwrap()[0].enabled);
    assert!(list_films(&storage).unwrap().is_empty());
    change_source_state(&storage, &id, "resume").unwrap();
    assert!(list_sources(&storage).unwrap()[0].enabled);
    assert_eq!(list_films(&storage).unwrap()[0].id, original.id);
    assert!(list_films(&storage).unwrap()[0].favorite);
    change_source_state(&storage, &id, "remove").unwrap();
    let session = SourceScanSession::new();
    assert!(session.preview(&storage, &root, None).is_err());
    fs::rename(home.path().join("temporarily-offline"), &root).unwrap();
    let id = add(&storage, &root);
    let outside = home.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let link = root.join("linked");
    #[cfg(windows)]
    {
        let result = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(result.status.success());
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    change_source_state(&storage, &id, "pause").unwrap();
    change_source_state(&storage, &id, "resume").unwrap();
    change_source_state(&storage, &id, "remove").unwrap();
    let incomplete = session.preview(&storage, &root, None).unwrap();
    assert!(!incomplete.complete);
    assert!(incomplete.token.is_none());
    assert!(incomplete.issue_count > 0);
    assert!(list_films(&storage).unwrap().is_empty());
    #[cfg(windows)]
    fs::remove_dir(&link).unwrap();
    #[cfg(unix)]
    fs::remove_file(&link).unwrap();
    let id = add(&storage, &root);
    storage.with_connection(|db| {
        db.execute_batch("CREATE TRIGGER reject_pause BEFORE UPDATE OF enabled ON libraries WHEN NEW.id<>'' BEGIN SELECT RAISE(FAIL, 'injected'); END;")?;
        Ok(())
    }).unwrap();
    assert_eq!(
        change_source_state(&storage, &id, "pause")
            .err()
            .unwrap()
            .to_string(),
        "state_failed"
    );
    assert!(list_sources(&storage).unwrap()[0].enabled);
    assert_eq!(list_films(&storage).unwrap().len(), 1);
}

#[test]
fn legacy_removed_records_are_purged_but_manual_problem_history_is_retained() {
    let home=tempfile::tempdir().unwrap();let root=home.path().join("synthetic-source");touch(&root,"film/movie.mp4");
    let s=Storage::open(&home.path().join("data")).unwrap();let source=add(&s,&root);
    let problem=local_film_library_preview_lib::problems::list(&s).unwrap().remove(0);
    local_film_library_preview_lib::problems::mark(&s,&[problem.id.clone()]).unwrap();
    s.with_connection(|db|{db.execute("UPDATE libraries SET enabled=0,removed=1,record_hidden=1 WHERE id=?1",[&source])?;Ok(())}).unwrap();
    assert!(list_sources(&s).unwrap().is_empty());
    s.with_connection(|db|{
        assert_eq!(db.query_row("SELECT COUNT(*) FROM libraries WHERE id=?1",[&source],|r|r.get::<_,i64>(0))?,0);
        assert!(db.query_row("SELECT handled_at IS NOT NULL FROM issue_records WHERE id=?1",[&problem.id],|r|r.get::<_,bool>(0))?);
        Ok(())
    }).unwrap();
    assert!(local_film_library_preview_lib::problems::list(&s).unwrap().is_empty());
    assert_ne!(add(&s,&root),source);
}
