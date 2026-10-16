use local_film_library_preview_lib::{
    observation::{scan_into_library, ScanResult},
    problem_folders,
    storage::Storage,
};
use std::fs;

fn setup() -> (tempfile::TempDir, Storage, std::path::PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-source");
    for directory in [
        "synthetic-actor/[TEST-01] first",
        "[TEST-01] second",
        "[EMPTY] synthetic-empty",
    ] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    image::RgbImage::from_pixel(8, 8, image::Rgb([10, 20, 30]))
        .save(root.join("synthetic-actor/actor.jpg"))
        .unwrap();
    for directory in ["synthetic-actor/[TEST-01] first", "[TEST-01] second"] {
        fs::write(root.join(directory).join("synthetic.mp4"), []).unwrap();
    }
    let storage = Storage::open(&home.path().join("software")).unwrap();
    storage.with_connection(|db|db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('synthetic','隔离来源',?1)",[root.to_string_lossy().as_ref()])).unwrap();
    assert!(matches!(
        scan_into_library(&storage, "synthetic", &root).unwrap(),
        ScanResult::Complete(_)
    ));
    storage.with_connection(|db| { db.execute("INSERT INTO actors(id,observed_name) VALUES('manual-no-photo','合成演员')",[])?; Ok(()) }).unwrap();
    fs::remove_file(root.join("synthetic-actor/actor.jpg")).unwrap();
    assert!(matches!(
        scan_into_library(&storage, "synthetic", &root).unwrap(),
        ScanResult::Complete(_)
    ));
    (home, storage, root)
}
#[test]
fn film_problems_offer_saved_directories_while_managed_actor_has_no_media_target() {
    let (_home, storage, root) = setup();
    let rows = problem_folders::list(&storage).unwrap();
    for kind in [
        "film_missing_cover",
        "film_code_conflict",
    ] {
        let row = rows.iter().find(|row| row.problem.kind == kind).unwrap();
        assert!(!row.directory_options.is_empty(), "{kind}");
        if kind == "film_code_conflict" {
            assert_eq!(row.directory_options.len(), 2);
        }
        for (index, relative) in row.directory_options.iter().enumerate() {
            assert_eq!(row.directory_targets[index].source_id, "synthetic");
            assert_eq!(row.directory_targets[index].relative, *relative);
            assert_eq!(row.directory_targets[index].label, *relative);
            problem_folders::open_directory_with(&storage, &row.problem.id, index, |path| {
                assert_eq!(path, root.join(relative).canonicalize().unwrap());
                Ok(())
            })
            .unwrap();
        }
    }
    assert!(!rows.iter().any(|row|row.problem.kind=="actor_missing_photo"));

}
#[test]
fn invalid_stale_missing_and_redirected_targets_never_call_opener_or_mark_handled() {
    let (home, storage, root) = setup();
    let rows = problem_folders::list(&storage).unwrap();
    let row = rows
        .iter()
        .find(|r| r.problem.kind == "film_missing_cover")
        .unwrap();
    let id = &row.problem.id;
    assert!(
        problem_folders::open_directory_with(&storage, id, 99, |_| panic!("unexpected open"))
            .is_err()
    );
    assert!(
        problem_folders::open_directory_with(&storage, "unknown", 0, |_| panic!("unexpected open"))
            .is_err()
    );
    let folder = root.join(&row.directory_options[0]);
    fs::remove_file(folder.join("synthetic.mp4")).unwrap();
    fs::remove_dir(&folder).unwrap();
    assert_eq!(
        problem_folders::open_directory_with(&storage, id, 0, |_| panic!("unexpected open"))
            .unwrap_err(),
        "directory_unavailable"
    );
    let outside = home.path().join("synthetic-outside");
    fs::create_dir(&outside).unwrap();
    #[cfg(windows)]
    {
        let result = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&folder)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(result.status.success());
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &folder).unwrap();
    assert_eq!(
        problem_folders::open_directory_with(&storage, id, 0, |_| panic!("unexpected open"))
            .unwrap_err(),
        "path_outside_source"
    );
    assert!(
        !problem_folders::list(&storage)
            .unwrap()
            .into_iter()
            .find(|r| r.problem.id == *id)
            .unwrap()
            .problem
            .handled
    );
    storage
        .with_connection(|db| db.execute("UPDATE libraries SET enabled=0", []))
        .unwrap();
    assert!(
        problem_folders::open_directory_with(&storage, id, 0, |_| panic!("unexpected open"))
            .is_err()
    );
    #[cfg(windows)]
    fs::remove_dir(&folder).unwrap();
}
