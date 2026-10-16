use local_film_library_preview_lib::{
    application_options, display_preferences, interface_language as language, lifecycle, reset,
    storage::Storage,
};

#[test]
fn fresh_database_seeds_simplified_chinese() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    let language: String = storage
        .with_connection(|db| {
            db.query_row(
                "SELECT language FROM interface_language WHERE singleton=1",
                [],
                |row| row.get(0),
            )
        })
        .unwrap();
    assert_eq!(language, "zh-CN");
}

#[test]
fn language_reopens_and_stale_appearance_and_options_do_not_overwrite_it() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    let appearance = lifecycle::InterfacePreferences::default();
    let options = application_options::ApplicationOptions::default();
    for code in ["ja", "en", "zh-TW", "zh-CN"] {
        language::save(&storage, code).unwrap();
        display_preferences::save_appearance(&storage, &appearance).unwrap();
        lifecycle::save_preferences(&storage, &appearance).unwrap();
        application_options::save(&storage, &options).unwrap();
        let reopened = Storage::open(home.path()).unwrap();
        assert_eq!(language::load(&reopened).unwrap(), code);
    }
}

#[test]
fn invalid_language_and_database_constraints_leave_the_saved_language_intact() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    language::save(&storage, "ja").unwrap();
    for invalid in ["", "fr", "en-US", "zh-cn", " ja", "ja "] {
        assert_eq!(
            language::save(&storage, invalid).unwrap_err(),
            "invalid_interface_language"
        );
        assert_eq!(language::load(&storage).unwrap(), "ja");
    }
    assert!(storage
        .with_connection(|db| db.execute("UPDATE interface_language SET language='fr'", []))
        .is_err());
    assert!(storage
        .with_connection(|db| db.execute(
            "INSERT INTO interface_language(singleton,language) VALUES(2,'en')",
            []
        ))
        .is_err());
    storage.with_connection(|db| db.execute_batch("CREATE TRIGGER refuse_language BEFORE UPDATE ON interface_language BEGIN SELECT RAISE(ABORT,'synthetic'); END;")).unwrap();
    assert_eq!(
        language::save(&storage, "en").unwrap_err(),
        "interface_language_save_failed"
    );
    assert_eq!(language::load(&storage).unwrap(), "ja");
}

#[test]
fn failed_read_reports_a_stable_code_without_silently_recreating_defaults() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    language::save(&storage, "ja").unwrap();
    storage
        .with_connection(|db| {
            db.execute_batch(
                "ALTER TABLE interface_language RENAME TO unavailable_interface_language;",
            )
        })
        .unwrap();
    assert_eq!(
        language::load(&storage).unwrap_err(),
        "interface_language_load_failed"
    );
    let saved: String = storage
        .with_connection(|db| {
            db.query_row(
                "SELECT language FROM unavailable_interface_language WHERE singleton=1",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(saved, "ja");
}

#[test]
fn schema22_upgrade_is_protected_and_old_backup_defaults_to_simplified_chinese() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    storage.with_connection(|db| db.execute_batch("DROP TABLE interface_language; PRAGMA user_version=22; INSERT INTO films(id) VALUES('preserved'); INSERT INTO film_manual(film_id,title) VALUES('preserved','中文 Japanese title');")).unwrap();
    let old_backup = storage.create_backup().unwrap();
    drop(storage);
    let storage = Storage::open(home.path()).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    assert_eq!(language::load(&storage).unwrap(), "zh-CN");
    assert_eq!(storage.list_protections().unwrap().len(), 1);
    language::save(&storage, "ja").unwrap();
    storage.restore_backup(&old_backup).unwrap();
    assert_eq!(language::load(&storage).unwrap(), "zh-CN");
    let title: String = storage
        .with_connection(|db| {
            db.query_row(
                "SELECT title FROM film_manual WHERE film_id='preserved'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(title, "中文 Japanese title");
}

#[test]
fn schema22_upgrade_leaves_original_untouched_when_protection_fails() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    storage
        .with_connection(|db| {
            db.execute_batch("DROP TABLE interface_language; PRAGMA user_version=22;")
        })
        .unwrap();
    drop(storage);
    std::fs::write(home.path().join("protections"), "blocks snapshot directory").unwrap();
    assert!(Storage::open(home.path()).is_err());
    let db = rusqlite::Connection::open(home.path().join("library.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        22
    );
    let table: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='interface_language'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(table, 0);
}

#[derive(Default)]
struct EmptyCredentials;
impl reset::Credentials for EmptyCredentials {
    fn read(&mut self, _: &str, _: bool) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn save(&mut self, _: &str, _: bool, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn delete(&mut self, _: &str, _: bool) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn backup_restores_its_language_and_reset_returns_to_initial_language() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(home.path()).unwrap();
    language::save(&storage, "zh-TW").unwrap();
    let backup = storage.create_backup().unwrap();
    language::save(&storage, "en").unwrap();
    storage.restore_backup(&backup).unwrap();
    assert_eq!(language::load(&storage).unwrap(), "zh-TW");
    reset::restore_initial_state_with(&storage, &mut EmptyCredentials).unwrap();
    assert_eq!(language::load(&storage).unwrap(), "zh-CN");
    let reopened = Storage::open(home.path()).unwrap();
    assert_eq!(language::load(&reopened).unwrap(), "zh-CN");
}

#[test]
fn native_labels_are_language_specific_and_per_app_state_is_independent() {
    use language::{NativeLanguage, NativeText};
    let first = NativeLanguage::new("ja");
    let second = NativeLanguage::new("en");
    assert_eq!(first.text(NativeText::Open), "開く");
    assert_eq!(second.text(NativeText::Open), "Open");
    first.set("zh-TW");
    assert_eq!(first.text(NativeText::PlayerTitle), "TheArchive · 播放器");
    assert_eq!(first.text(NativeText::ChooseSubtitle), "選擇字幕");
    assert_eq!(second.text(NativeText::ChooseSubtitle), "Choose subtitles");
    assert_eq!(
        language::NativeLanguage::new("zh-CN").text(NativeText::ChoosePhoto),
        "选择演员照片"
    );
}

#[test]
fn language_failures_remain_identifiable_in_private_safe_diagnostic_logs() {
    use local_film_library_preview_lib::diagnostics::{Event, LogFiles};
    let home = tempfile::tempdir().unwrap();
    let mut logs = LogFiles::open(home.path(), 16_384, 1).unwrap();
    let failures = [
        ("load_interface_language", "interface_language_load_failed"),
        ("save_interface_language", "interface_language_save_failed"),
        ("save_interface_language", "invalid_interface_language"),
        ("interface_language.load", "interface_language_load_failed"),
        ("interface_language.event", "unknown_error"),
        ("window.title", "unknown_error"),
        ("application_options.restore", "unknown_error"),
    ];
    for (operation, code) in failures {
        let mut event = Event::new("backend", operation, "failed", None);
        event.error_code = Some(code.into());
        logs.write(event).unwrap();
    }
    let text = std::fs::read_to_string(home.path().join("runtime.log")).unwrap();
    let events: Vec<Event> = text.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(events.len(), failures.len());
    for (event, (operation, code)) in events.iter().zip(failures) {
        assert_eq!(event.operation, operation);
        assert_eq!(event.error_code.as_deref(), Some(code));
    }
}
