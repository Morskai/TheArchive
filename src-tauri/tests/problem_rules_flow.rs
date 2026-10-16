use local_film_library_preview_lib::{catalog, observation::scan_into_library, problems, storage::Storage};
use std::{fs, path::Path};

fn video(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, []).unwrap();
}
fn picture(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::new(12, 18).save(path).unwrap();
}
fn bind(storage: &Storage, root: &Path, source: &str) {
    storage.with_connection(|db| {
        db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES(?1,?1,?2)", (source, root.to_string_lossy().as_ref()))?;
        Ok(())
    }).unwrap();
    scan_into_library(storage, source, root).unwrap();
}

#[test]
fn usable_cover_fallback_and_single_video_image_do_not_report_missing() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source");
    video(&root,"single/main.mp4"); picture(&root,"single/main.png");
    video(&root,"multi/a.mp4"); video(&root,"multi/b.mp4");
    fs::write(root.join("multi/poster.jpg"), b"bad image").unwrap();
    picture(&root,"multi/poster.png"); fs::write(root.join("multi/a.jpg"), b"bad version image").unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap(); bind(&storage,&root,"source");
    assert!(catalog::list_films(&storage).unwrap().iter().all(|film| film.has_cover));
    assert!(problems::list(&storage).unwrap().is_empty());
}

#[test]
fn empty_markers_and_saved_actors_never_create_problems() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source");
    fs::create_dir_all(root.join("[SAME] 空一")).unwrap(); fs::create_dir_all(root.join("[SAME] 空二")).unwrap();
    picture(&root,"合集/poster.jpg"); video(&root,"合集/子影片/main.mp4"); picture(&root,"合集/子影片/poster.png");
    let storage = Storage::open(&home.path().join("data")).unwrap(); bind(&storage,&root,"source");
    storage.with_connection(|db| db.execute("INSERT INTO actors(id,observed_name) VALUES('actor','无照片演员')",[])).unwrap();
    assert_eq!(catalog::list_films(&storage).unwrap().len(),1);
    assert!(problems::list(&storage).unwrap().is_empty());
    fs::remove_file(root.join("合集/子影片/main.mp4")).unwrap();
    scan_into_library(&storage,"source",&root).unwrap();
    assert!(problems::list(&storage).unwrap().is_empty());
    storage.with_connection(|db| {
        assert_eq!(db.query_row("SELECT COUNT(*) FROM issue_records WHERE subject_type IN ('actor_missing_photo','film_missing_video') OR id LIKE 'problem-current:film_missing_video:%'",[],|r|r.get::<_,i64>(0))?,0);
        Ok(())
    }).unwrap();
}

#[test]
fn legacy_empty_directory_duplicate_with_no_roles_cannot_stay_current() {
    let home=tempfile::tempdir().unwrap();let root=home.path().join("source");fs::create_dir(&root).unwrap();
    let storage=Storage::open(&home.path().join("data")).unwrap();bind(&storage,&root,"source");
    let evidence=serde_json::json!({"rule":"film_code_conflict_v1","missing":true,"episode":0,"videos":[],"sources":[{"id":"source","name":"source"}],"directories":["[SAME] empty1","[SAME] empty2"],"artwork":[],"code":"SAME"}).to_string();
    storage.with_connection(|db| {
        db.execute("UPDATE issue_records SET evidence=json_remove(evidence,'$.roles') WHERE subject_type='library_scan_details'",[])?;
        db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('old-code','film_code_conflict','legacy-code','legacy',?1)",[&evidence])?;
        db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('problem-current:film_code_conflict:legacy-code','library_problem_current','legacy-code','legacy',?1)",[&evidence])?;
        Ok(())
    }).unwrap();
    assert!(problems::list(&storage).unwrap().is_empty());
}

#[test]
fn unrelated_video_title_and_source_label_changes_keep_handled_state() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source");
    video(&root,"[SAME] first/a.mp4"); video(&root,"[same] second/b.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap(); bind(&storage,&root,"source");
    let before = problems::list(&storage).unwrap();
    problems::mark(&storage,&before.iter().map(|p|p.id.clone()).collect::<Vec<_>>()).unwrap();
    video(&root,"[SAME] first/new-version.mkv");
    storage.with_connection(|db| { db.execute("UPDATE libraries SET display_name='新显示名称'",[])?; db.execute("INSERT INTO film_manual(film_id,title) SELECT id,'手动片名' FROM films",[])?; Ok(()) }).unwrap();
    scan_into_library(&storage,"source",&root).unwrap();
    let after = problems::list(&storage).unwrap();
    assert_eq!(after.len(),before.len());
    assert!(after.iter().all(|p|p.handled && before.iter().any(|old|old.id==p.id)));
    assert!(after.iter().all(|p|p.sources[0].name=="新显示名称"));
    let conflict = after.iter().find(|p|p.kind=="film_code_conflict").unwrap();
    assert_eq!(conflict.videos.len(),3);
    video(&root,"[SAME] third/c.mp4"); scan_into_library(&storage,"source",&root).unwrap();
    assert!(!problems::list(&storage).unwrap().iter().find(|p|p.kind=="film_code_conflict").unwrap().handled);
}

#[test]
fn directory_title_change_with_restored_card_ids_keeps_duplicate_handled() {
    let home=tempfile::tempdir().unwrap(); let root=home.path().join("source");
    video(&root,"[SAME] first/unique-first.mp4"); video(&root,"[SAME] second/unique-second.mp4");
    let storage=Storage::open(&home.path().join("data")).unwrap(); bind(&storage,&root,"source");
    let before=catalog::list_films(&storage).unwrap();
    let issue=problems::list(&storage).unwrap().into_iter().find(|p|p.kind=="film_code_conflict").unwrap();
    problems::mark(&storage,&[issue.id.clone()]).unwrap();
    fs::rename(root.join("[SAME] first"),root.join("[SAME] 新片名")).unwrap();
    scan_into_library(&storage,"source",&root).unwrap();
    let after=catalog::list_films(&storage).unwrap();
    assert!(after.iter().all(|film|before.iter().any(|old|old.id==film.id)));
    let current=problems::list(&storage).unwrap().into_iter().find(|p|p.kind=="film_code_conflict").unwrap();
    assert_eq!(current.id,issue.id); assert!(current.handled);
    assert!(current.directories.iter().any(|directory|directory.contains("新片名")));
}

#[test]
fn historical_multi_directory_card_uses_one_effective_number_and_never_self_duplicates() {
    let home=tempfile::tempdir().unwrap(); let root=home.path().join("source");
    video(&root,"[SAME] first/a.mp4"); video(&root,"[SAME] second/b.mp4");
    let storage=Storage::open(&home.path().join("data")).unwrap(); bind(&storage,&root,"source");
    let films=catalog::list_films(&storage).unwrap(); let first=films.iter().find(|f|f.videos[0].observed_path.ends_with("a.mp4")).unwrap();
    storage.with_connection(|db|db.execute("UPDATE file_observations SET film_id=?1 WHERE kind='video'",[&first.id])).unwrap();
    assert_eq!(catalog::list_films(&storage).unwrap().len(),1);
    assert!(!problems::list(&storage).unwrap().iter().any(|p|p.kind=="film_code_conflict"));
    video(&root,"[SAME] third/c.mp4");scan_into_library(&storage,"source",&root).unwrap();
    // Recreate the persisted legacy multi-directory card after the current
    // scan, which otherwise correctly regroups newly scanned directories.
    storage.with_connection(|db|db.execute("UPDATE file_observations SET film_id=?1,observed_path='[OTHER] second/b.mp4' WHERE observed_path='[SAME] second/b.mp4'",[&first.id])).unwrap();
    assert!(!problems::list(&storage).unwrap().iter().any(|p|p.kind=="film_code_conflict"));
    catalog::set_code(&storage,&first.id,"SAME").unwrap();
    assert_eq!(problems::list(&storage).unwrap().iter().filter(|p|p.kind=="film_code_conflict").count(),1);
}

fn legacy() -> (tempfile::TempDir, Storage) {
    let home = tempfile::tempdir().unwrap(); let storage=Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| db.execute_batch("
        INSERT INTO films(id) VALUES('film');
        INSERT INTO film_manual(film_id,title,favorite) VALUES('film','人工片名',1);
        INSERT INTO actors(id,observed_name) VALUES('actor','演员');
        INSERT INTO film_actors(film_id,actor_id,source) VALUES('film','actor','manual');
        INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence,handled_at) VALUES
        ('retired-video','film_missing_video','directory:s:d','k','{}','2026-10-01'),
        ('retired-actor','actor_missing_photo','actor','k','{}',NULL),
        ('problem-current:film_missing_video:directory:s:d','library_problem_current','directory:s:d','k','{}',NULL),
        ('problem-current:actor_missing_photo:actor','library_problem_current','actor','k','{}',NULL),
        ('cleared-video','library_problem_cleared','film_missing_video:directory:s:d','k','{}',NULL),
        ('cleared-actor','library_problem_cleared','actor_missing_photo:actor','k','{}',NULL),
        ('empty-cover','film_missing_cover','directory:s:empty','k','{}',NULL),
        ('problem-current:film_missing_cover:directory:s:empty','library_problem_current','directory:s:empty','k','{}',NULL),
        ('cleared-empty-cover','library_problem_cleared','film_missing_cover:directory:s:empty','k','{}',NULL),
        ('keep-cover','film_missing_cover','film','k','{}','2026-10-01');
        PRAGMA user_version=19;
    ")).unwrap(); (home,storage)
}

#[test]
fn protected_upgrade_and_old_backup_restore_remove_only_retired_problem_records() {
    let (home,old)=legacy(); let backup=old.create_backup().unwrap(); drop(old);
    let storage=Storage::open(&home.path().join("data")).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23); assert!(!storage.list_protections().unwrap().is_empty());
    for restoring in [false,true] {
        if restoring { storage.restore_backup(&backup).unwrap(); }
        storage.with_connection(|db| {
            assert_eq!(db.query_row("SELECT COUNT(*) FROM issue_records",[],|r|r.get::<_,i64>(0))?,1);
            assert_eq!(db.query_row("SELECT id FROM issue_records",[],|r|r.get::<_,String>(0))?,"keep-cover");
            assert_eq!(db.query_row("SELECT title FROM film_manual",[],|r|r.get::<_,String>(0))?,"人工片名");
            assert_eq!(db.query_row("SELECT COUNT(*) FROM actors",[],|r|r.get::<_,i64>(0))?,1);
            assert_eq!(db.query_row("SELECT COUNT(*) FROM film_actors",[],|r|r.get::<_,i64>(0))?,1); Ok(())
        }).unwrap();
    }
}

#[test]
fn protection_or_migration_failure_keeps_retired_history_and_schema() {
    for protection_failure in [true,false] {
        let(home,old)=legacy();
        if protection_failure { fs::write(home.path().join("data/protections"),b"blocked").unwrap(); }
        else { old.with_connection(|db|db.execute_batch("CREATE TRIGGER reject_cleanup BEFORE DELETE ON issue_records BEGIN SELECT RAISE(ABORT,'synthetic failure'); END")).unwrap(); }
        assert!(Storage::open(&home.path().join("data")).is_err());
        assert_eq!(old.schema_version().unwrap(),19);
        assert_eq!(old.with_connection(|db|db.query_row("SELECT COUNT(*) FROM issue_records",[],|r|r.get::<_,i64>(0))).unwrap(),10);
    }
}
