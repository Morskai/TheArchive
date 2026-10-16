use local_film_library_preview_lib::{diagnostics, storage::Storage};

#[test]
fn startup_failure_is_flushed_before_process_exit() {
    if let Some(directory) = std::env::var_os("THEARCHIVE_TEST_STARTUP_LOG") {
        let root = std::path::PathBuf::from(directory);
        diagnostics::initialize(&root.join("logs")).unwrap();
        std::fs::create_dir(root.join("data")).unwrap();
        std::fs::write(
            root.join("data/library.sqlite3"),
            "broken synthetic database",
        )
        .unwrap();
        let error = match Storage::open(&root.join("data")) {
            Ok(_) => panic!("expected broken database"),
            Err(error) => error,
        };
        diagnostics::startup_failure(error.as_ref());
        std::process::exit(17);
    }
    let home = tempfile::tempdir().unwrap();
    let exit = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "startup_failure_is_flushed_before_process_exit"])
        .env("THEARCHIVE_TEST_STARTUP_LOG", home.path())
        .status()
        .unwrap();
    assert_eq!(exit.code(), Some(17));
    let logs = std::fs::read_to_string(home.path().join("logs/runtime.log")).unwrap();
    assert!(logs.contains("database.open"));
    assert!(logs.contains("sqliteCode"));
}
