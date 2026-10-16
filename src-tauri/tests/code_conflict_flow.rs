use local_film_library_preview_lib::{catalog, observation::scan_into_library, problems, storage::Storage};
use std::{fs, path::Path};

fn video(root: &Path, path: &str) {
    let file=root.join(path); fs::create_dir_all(file.parent().unwrap()).unwrap(); fs::write(file, []).unwrap();
}
fn bind(s: &Storage, root: &Path, id: &str) {
    s.with_connection(|db| {db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES(?1,?1,?2)",(id,root.to_string_lossy().as_ref()))?;Ok(())}).unwrap();
    scan_into_library(s,id,root).unwrap();
}
fn conflicts(s: &Storage) -> Vec<problems::Problem> {
    problems::list(s).unwrap().into_iter().filter(|p|p.kind=="film_code_conflict").collect()
}

#[test]
fn same_source_distinct_folders_conflict_but_versions_and_other_sources_do_not() {
    let home=tempfile::tempdir().unwrap(); let root=home.path().join("source");
    video(&root,"[TEST-01] 一/a.mp4"); video(&root,"[TEST-01] 一/b.mkv");
    video(&root,"[test-01] 二/c.mp4");
    let other=home.path().join("other");video(&other,"[TEST-01] 三/d.mp4");
    let s=Storage::open(&home.path().join("data")).unwrap();bind(&s,&root,"one");bind(&s,&other,"two");
    let rows=conflicts(&s);assert_eq!(rows.len(),1);
    assert_eq!(rows[0].directories.len(),2);assert_eq!(rows[0].videos.len(),3);assert_eq!(rows[0].sources.len(),1);
    assert_eq!(catalog::list_films(&s).unwrap().len(),3);
    fs::remove_file(root.join("[test-01] 二/c.mp4")).unwrap();fs::remove_dir(root.join("[test-01] 二")).unwrap();scan_into_library(&s,"one",&root).unwrap();
    assert!(conflicts(&s).is_empty());
}

#[test]
fn same_evidence_keeps_handled_reopen_changes_reprompt_and_scan_failure_rolls_back() {
    let home=tempfile::tempdir().unwrap();let root=home.path().join("source");
    for file in ["[TEST-01] 一/a.mp4","[TEST-01] 二/b.mp4"] {video(&root,file)}
    let data=home.path().join("data");let s=Storage::open(&data).unwrap();bind(&s,&root,"one");
    let p=conflicts(&s).remove(0);problems::mark(&s,&[p.id.clone()]).unwrap();drop(s);
    let s=Storage::open(&data).unwrap();scan_into_library(&s,"one",&root).unwrap();
    assert!(conflicts(&s)[0].handled);assert_eq!(conflicts(&s)[0].id,p.id);
    s.with_connection(|db|db.execute_batch("CREATE TRIGGER fail_conflict BEFORE INSERT ON issue_records WHEN NEW.subject_type='film_code_conflict' BEGIN SELECT RAISE(ABORT,'synthetic'); END;")).unwrap();
    video(&root,"[TEST-01] 三/c.mp4");assert!(scan_into_library(&s,"one",&root).is_err());
    assert_eq!(conflicts(&s)[0].id,p.id);assert!(conflicts(&s)[0].handled);
    s.with_connection(|db|db.execute_batch("DROP TRIGGER fail_conflict")).unwrap();scan_into_library(&s,"one",&root).unwrap();
    let new=conflicts(&s).remove(0);assert_ne!(new.id,p.id);assert!(!new.handled);assert_eq!(new.directories.len(),3);
}

#[test]
fn numbers_are_literal_folder_tokens_and_filenames_are_not_guessed() {
    let home=tempfile::tempdir().unwrap();let root=home.path().join("source");
    for file in ["[TEST-01] 一/OTHER-99.mp4","[TEST-02] 二/OTHER-99.mp4","无编号一/ABC-123.mp4","无编号二/ABC-123.mkv","数字片名一/2026.mp4","数字片名二/2026.mp4"] {video(&root,file)}
    let s=Storage::open(&home.path().join("data")).unwrap();bind(&s,&root,"one");
    assert!(conflicts(&s).is_empty());
    video(&root,"[自定编号] 一/unique-a.mp4");
    video(&root,"[自定编号] 二/unique-b.mp4");
    scan_into_library(&s,"one",&root).unwrap();
    let rows = conflicts(&s);
    assert_eq!(rows.len(),1);
    assert_eq!(rows[0].directories,["[自定编号] 一","[自定编号] 二"]);
}

#[test]
fn empty_numbered_folders_do_not_participate_or_create_fake_films() {
    let home=tempfile::tempdir().unwrap();let root=home.path().join("source");
    fs::create_dir_all(root.join("[TEST-01] 空一")).unwrap();fs::create_dir_all(root.join("[TEST-01] 空二")).unwrap();
    let s=Storage::open(&home.path().join("data")).unwrap();bind(&s,&root,"one");
    assert!(conflicts(&s).is_empty());assert!(catalog::list_films(&s).unwrap().is_empty());
    video(&root,"[TEST-01] 实际影片/a.mp4");scan_into_library(&s,"one",&root).unwrap();
    assert!(conflicts(&s).is_empty());assert_eq!(catalog::list_films(&s).unwrap().len(),1);
}
