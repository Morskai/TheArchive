use local_film_library_preview_lib::{
    catalog,
    observation::{ScanResult, scan_into_library},
    problems,
    source_scan::SourceScanSession,
    storage::Storage,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn setup() -> (tempfile::TempDir, Storage, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    fs::create_dir_all(root.join("[TEST-01] 影片")).unwrap();
    fs::write(root.join("[TEST-01] 影片/movie.mp4"), []).unwrap();
    let s = Storage::open(&home.path().join("data")).unwrap();
    s.with_connection(|db| {
        db.execute(
            "INSERT INTO libraries(id,display_name,root_path) VALUES('source','虚构来源',?1)",
            [root.to_string_lossy().as_ref()],
        )?;
        Ok(())
    })
    .unwrap();
    (home, s, root)
}
fn scan(s: &Storage, root: &Path) {
    assert!(matches!(
        scan_into_library(s, "source", root).unwrap(),
        ScanResult::Complete(_)
    ));
}
fn picture(path: &Path) {
    image::RgbImage::new(12, 18).save(path).unwrap();
}

#[test]
fn corrupt_cover_stays_handled_until_usable_cover_returns_then_missing_reprompts() {
    let (home, s, root) = setup();
    let cover = root.join("[TEST-01] 影片/poster.jpg");
    fs::write(&cover, b"synthetic invalid image").unwrap();
    scan(&s, &root);
    let issue = problems::list(&s).unwrap().remove(0);
    assert_eq!(issue.kind, "film_missing_cover");
    assert!(issue.reason.contains("无法读取"));
    assert!(!catalog::list_films(&s).unwrap()[0].has_cover);
    assert_eq!(fs::read(&cover).unwrap(), b"synthetic invalid image");
    assert!(!s.data_directory().join("cover-cache").exists());
    problems::mark(&s, &[issue.id.clone()]).unwrap();
    drop(s);
    let s = Storage::open(&home.path().join("data")).unwrap();
    scan(&s, &root);
    assert!(problems::list(&s).unwrap()[0].handled);
    fs::write(&cover, b"another synthetic invalid image").unwrap();
    scan(&s, &root);
    let next = problems::list(&s).unwrap().remove(0);
    assert_eq!(issue.id, next.id);
    assert!(next.handled);
    picture(&cover);
    scan(&s, &root);
    assert!(problems::list(&s).unwrap().is_empty());
    assert!(catalog::list_films(&s).unwrap()[0].has_cover);
    fs::write(&cover, b"synthetic invalid image").unwrap();
    scan(&s, &root);
    assert!(!problems::list(&s).unwrap()[0].handled);
}

#[test]
fn empty_marked_work_folders_are_detected_and_nested_videos_are_independent() {
    let (_home, s, root) = setup();
    picture(&root.join("[TEST-01] 影片/poster.jpg"));
    for dir in [
        "[TEST-02] 空影片",
        "封面标记影片",
        "普通空目录",
        "多人",
        "演员甲",
        "[TEST-03] 分段/part",
    ] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    picture(&root.join("封面标记影片/poster.jpg"));
    picture(&root.join("多人/poster.jpg"));
    picture(&root.join("演员甲/actor.jpg"));
    fs::write(root.join("[TEST-03] 分段/part/short.mp4"), []).unwrap();
    scan(&s, &root);
    let rows = problems::list(&s).unwrap();
    let missing: Vec<_> = rows
        .iter()
        .filter(|p| p.kind == "film_missing_video")
        .collect();
    assert!(missing.is_empty());
    assert_eq!(catalog::list_films(&s).unwrap().len(), 2);
}

#[test]
fn previously_scanned_video_disappearance_creates_no_missing_history() {
    let (home, s, root) = setup();
    fs::create_dir_all(root.join("普通影片")).unwrap();
    let video = root.join("普通影片/first.mkv"); fs::write(&video, []).unwrap(); scan(&s, &root);
    fs::remove_file(&video).unwrap(); scan(&s, &root);
    assert!(!problems::list(&s).unwrap().iter().any(|p| p.kind == "film_missing_video"));
    drop(s); let s = Storage::open(&home.path().join("data")).unwrap();
    fs::write(&video, []).unwrap(); scan(&s, &root);
    fs::remove_file(&video).unwrap(); scan(&s, &root);
    assert!(!problems::list(&s).unwrap().iter().any(|p| p.kind == "film_missing_video"));
}

#[test]
fn new_directory_problem_write_failure_rolls_back_current_scan_and_issues() {
    let (_home, s, root) = setup();
    picture(&root.join("[TEST-01] 影片/poster.jpg"));
    scan(&s, &root);
    s.with_connection(|db|db.execute_batch("CREATE TRIGGER fail_missing BEFORE INSERT ON issue_records WHEN NEW.subject_type='film_missing_cover' BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    fs::create_dir(root.join("[TEST-02] 空影片")).unwrap();
    fs::write(root.join("[TEST-02] 空影片/new.mkv"), []).unwrap();
    assert!(scan_into_library(&s, "source", &root).is_err());
    assert_eq!(catalog::list_films(&s).unwrap()[0].videos.len(), 1);
    assert!(problems::list(&s).unwrap().is_empty());
}

#[test]
fn changing_same_named_cover_between_preview_and_confirm_rejects_stale_scan() {
    let (_home, s, root) = setup();
    let cover = root.join("[TEST-01] 影片/poster.jpg");
    picture(&cover);
    scan(&s, &root);
    let session = SourceScanSession::new();
    let preview = session.preview(&s, &root, Some("source")).unwrap();
    fs::write(&cover, b"synthetic corruption").unwrap();
    assert!(
        session
            .confirm(&s, preview.token.as_deref().unwrap())
            .is_err()
    );
    assert!(problems::list(&s).unwrap().is_empty());
}

#[test]
fn removed_empty_folder_then_recreated_creates_no_problems() {
    let (_home, s, root) = setup(); picture(&root.join("[TEST-01] 影片/poster.jpg"));
    let empty = root.join("[TEST-02] 空影片"); fs::create_dir(&empty).unwrap(); scan(&s, &root);
    assert!(problems::list(&s).unwrap().is_empty());
    fs::remove_dir(&empty).unwrap(); scan(&s, &root); fs::create_dir(&empty).unwrap(); scan(&s, &root);
    assert!(problems::list(&s).unwrap().is_empty());
}

#[test]
fn paused_source_hides_issues_and_resume_retains_handled_evidence() {
    let (_home, s, root) = setup();
    scan(&s, &root);
    let issue = problems::list(&s).unwrap().remove(0);
    problems::mark(&s, &[issue.id.clone()]).unwrap();
    local_film_library_preview_lib::source_scan::change_source_state(&s, "source", "pause")
        .unwrap();
    assert!(problems::list(&s).unwrap().is_empty());
    local_film_library_preview_lib::source_scan::change_source_state(&s, "source", "resume")
        .unwrap();
    let restored = problems::list(&s).unwrap().remove(0);
    assert_eq!(restored.id, issue.id);
    assert!(restored.handled);
}
