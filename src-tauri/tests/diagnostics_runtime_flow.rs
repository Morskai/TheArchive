use local_film_library_preview_lib::{
    diagnostics::{self, Event, Scope},
    storage::Storage,
};
use std::collections::BTreeMap;

#[test]
fn scoped_errors_concurrent_records_panic_and_backups_keep_their_boundaries() {
    let home = tempfile::tempdir().unwrap();
    let directory = home.path().join("logs");
    diagnostics::initialize(&directory).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let mut scope = Scope::start("confirm_library_source", Some("op-a-1".into()));
    assert!(
        storage
            .with_transaction::<()>(|db| {
                db.execute(
                    "INSERT INTO private_missing_table VALUES ('SECRET-KEY')",
                    [],
                )?;
                Ok(())
            })
            .is_err()
    );
    scope.finish(Some("import_failed"), BTreeMap::new());
    drop(scope);
    let workers = (0..4)
        .map(|n| {
            std::thread::spawn(move || {
                for i in 0..10 {
                    diagnostics::record(Event::new(
                        "backend",
                        "actor.import",
                        "complete",
                        Some(format!("op-{n}-{i}")),
                    ));
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
    for _ in 0..30 {
        let mut event = Event::new("frontend", "load_film_cover", "failed", None);
        event.error_code = Some("cover_unavailable".into());
        diagnostics::record(event);
    }
    for n in 0..12 {
        let mut task = Scope::start("confirm_library_source", Some(format!("op-task-{n}")));
        task.finish(
            Some("import_failed"),
            BTreeMap::from([("failed".into(), 1)]),
        );
    }
    diagnostics::flush();
    let original_hook = std::panic::take_hook();
    diagnostics::install_panic_hook();
    let caught = std::panic::catch_unwind(|| {
        let _scope = Scope::start("actor.export", Some("op-panic-1".into()));
        panic!("SECRET-KEY private file");
    });
    assert!(caught.is_err());
    std::panic::set_hook(original_hook);
    let text = diagnostics::snapshot().unwrap();
    let records = text
        .lines()
        .map(|line| serde_json::from_str::<Event>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(records.iter().any(|event| {
        event.operation == "database.transaction"
            && event.operation_id.as_deref() == Some("op-a-1")
            && event
                .detail
                .as_ref()
                .is_some_and(|d| d.sqlite_code.is_some())
    }));
    assert!(records.iter().any(|event| event.event == "panic"
        && event.operation_id.as_deref() == Some("op-panic-1")
        && event.line.is_some()));
    assert_eq!(
        records
            .iter()
            .filter(|event| event.operation == "load_film_cover")
            .count(),
        5
    );
    assert_eq!(
        records
            .iter()
            .filter(|event| event.operation == "actor.import")
            .count(),
        40
    );
    assert_eq!(
        records
            .iter()
            .filter(|event| event.operation == "confirm_library_source" && event.event == "failed")
            .count(),
        13
    );
    assert!(!text.contains("SECRET-KEY"));
    assert!(!text.contains("private_missing_table"));
    let exports = home.path().join("exports");
    std::fs::create_dir(&exports).unwrap();
    let untouched = exports.join("keep.log");
    std::fs::write(&untouched, "keep").unwrap();
    let first = diagnostics::export_to(&exports).unwrap();
    let second = diagnostics::export_to(&exports).unwrap();
    assert_ne!(first, second);
    assert!(
        !std::fs::read_to_string(first)
            .unwrap()
            .contains("SECRET-KEY")
    );
    assert_eq!(std::fs::read_to_string(untouched).unwrap(), "keep");
    assert!(diagnostics::export_to(&exports.join("missing")).is_err());
    let outside = home.path().join("external-directory");
    std::fs::create_dir(&outside).unwrap();
    let link = home.path().join("redirected-directory");
    #[cfg(windows)]
    let created = std::os::windows::fs::symlink_dir(&outside, &link).is_ok()
        || std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap()
            .status
            .success();
    #[cfg(unix)]
    let created = std::os::unix::fs::symlink(&outside, &link).is_ok();
    assert!(
        created,
        "temporary redirected-directory fixture must be available"
    );
    assert!(diagnostics::export_to(&link).is_err());
    assert_eq!(std::fs::read_dir(outside).unwrap().count(), 0);
    storage.create_backup().unwrap();
    assert!(!storage.data_directory().join("logs").exists());
    assert_eq!(
        std::fs::read_dir(storage.data_directory().join("backups"))
            .unwrap()
            .count(),
        1
    );
    // All paths are owned by this tempfile. A logger failure cannot turn a real SQLite commit into failure.
    std::fs::rename(&directory, home.path().join("logs-before-failure")).unwrap();
    std::fs::write(&directory, "unavailable log directory").unwrap();
    let mut task = Scope::start("confirm_library_source", Some("op-log-unavailable-1".into()));
    storage.with_transaction(|db| db.execute_batch("CREATE TABLE diagnostic_business_sentinel(value INTEGER); INSERT INTO diagnostic_business_sentinel VALUES (7)")).unwrap();
    task.finish(None, BTreeMap::new());
    diagnostics::flush();
    assert!(!diagnostics::status().available);
    assert_eq!(storage.with_connection(|db| db.query_row::<i64, _, _>("SELECT value FROM diagnostic_business_sentinel", [], |row| row.get(0))).unwrap(), 7);
}
