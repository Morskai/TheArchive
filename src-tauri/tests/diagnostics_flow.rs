use local_film_library_preview_lib::diagnostics::{Event, LogFiles, error_detail, safe_error_code};
use serde_json::json;
use std::fs;

#[test]
fn hardlinked_log_cannot_modify_an_external_file() {
    let home = tempfile::tempdir().unwrap();
    let outside = home.path().join("outside.log");
    fs::write(&outside, "keep original").unwrap();
    let directory = home.path().join("logs");
    fs::create_dir(&directory).unwrap();
    fs::hard_link(&outside, directory.join("runtime.log")).unwrap();
    let mut logs = LogFiles::open(&directory, 800, 5).unwrap();
    assert!(logs.write(event(1)).is_err());
    assert!(logs.snapshot().is_err());
    assert_eq!(fs::read_to_string(outside).unwrap(), "keep original");
}

fn event(n: u64) -> Event {
    Event::new(
        "backend",
        "confirm_library_source",
        "complete",
        Some(format!("op-{n}")),
    )
}

#[test]
fn five_bounded_files_retain_recent_valid_events_across_reopen() {
    let home = tempfile::tempdir().unwrap();
    let mut logs = LogFiles::open(home.path(), 800, 5).unwrap();
    for n in 0..100 {
        logs.write(event(n)).unwrap();
    }
    let text = logs.snapshot().unwrap();
    assert!(text.contains("op-99"));
    assert!(!text.contains("\"op-0\""));
    assert!(fs::read_dir(home.path()).unwrap().count() <= 5);
    for file in fs::read_dir(home.path()).unwrap() {
        assert!(file.unwrap().metadata().unwrap().len() <= 800);
    }
    let mut reopened = LogFiles::open(home.path(), 800, 5).unwrap();
    reopened.write(event(100)).unwrap();
    assert!(reopened.snapshot().unwrap().contains("op-100"));
    for line in reopened.snapshot().unwrap().lines() {
        serde_json::from_str::<Event>(line).unwrap();
    }
}

#[test]
fn arbitrary_values_and_unknown_payloads_never_enter_logs_or_exports() {
    let home = tempfile::tempdir().unwrap();
    let mut logs = LogFiles::open(home.path(), 5000, 5).unwrap();
    let mut unsafe_event = event(1);
    unsafe_event.operation_id = Some("SECRET-KEY D:\\Private\\Actor".into());
    unsafe_event.error_code = Some("Bearer SECRET-KEY".into());
    unsafe_event.counts.insert("SECRET-KEY".into(), 8);
    unsafe_event.counts.insert("discovered".into(), 8);
    logs.write(unsafe_event).unwrap();
    let raw = fs::read_to_string(home.path().join("runtime.log")).unwrap();
    assert!(!raw.contains("SECRET-KEY"));
    assert!(!raw.contains("Private"));
    assert!(raw.contains("discovered"));
    // An unrelated/modified file cannot smuggle arbitrary text into an exported report.
    fs::write(
        home.path().join("runtime.1.log"),
        format!("{}\nSECRET-KEY\n", json!({"key": "SECRET-KEY"})),
    )
    .unwrap();
    assert!(!logs.snapshot().unwrap().contains("SECRET-KEY"));
    assert_eq!(safe_error_code("translation_auth"), "translation_auth");
    assert_eq!(
        safe_error_code("translation_auth SECRET-KEY"),
        "unknown_error"
    );
}

#[test]
fn low_level_error_preserves_numeric_cause_without_paths_or_sql() {
    let io = std::io::Error::from_raw_os_error(5);
    let detail = error_detail(&io);
    assert_eq!(detail.os_code, Some(5));
    let sql = rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(2067),
        Some("SECRET-KEY table/private/path".into()),
    );
    let detail = error_detail(&sql);
    assert_eq!(detail.sqlite_code, Some(2067));
    assert!(
        !serde_json::to_string(&detail)
            .unwrap()
            .contains("SECRET-KEY")
    );
}

#[test]
fn unavailable_logging_and_overlarge_events_are_reported_without_panics() {
    let home = tempfile::tempdir().unwrap();
    let file = home.path().join("file");
    fs::write(&file, "keep").unwrap();
    assert!(LogFiles::open(&file, 800, 5).is_err());
    assert_eq!(fs::read_to_string(&file).unwrap(), "keep");
    let mut logs = LogFiles::open(&home.path().join("logs"), 80, 5).unwrap();
    assert!(logs.write(event(1)).is_err());
    assert!(logs.snapshot().unwrap().is_empty());
}

#[test]
fn redirected_log_file_is_never_read_written_or_rotated() {
    let home = tempfile::tempdir().unwrap();
    let outside = home.path().join("outside.log");
    fs::write(&outside, "SECRET-KEY").unwrap();
    let directory = home.path().join("logs");
    fs::create_dir(&directory).unwrap();
    let mut logs = LogFiles::open(&directory, 800, 5).unwrap();
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_file(&outside, directory.join("runtime.log")).is_err() {
            return;
        }
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, directory.join("runtime.log")).unwrap();
    assert!(logs.write(event(1)).is_err());
    assert!(logs.snapshot().is_err());
    assert_eq!(fs::read_to_string(outside).unwrap(), "SECRET-KEY");
}
