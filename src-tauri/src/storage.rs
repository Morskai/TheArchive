use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior};
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
pub(crate) const SCHEMA_VERSION: i64 = 23;
const DATABASE_FILE: &str = "library.sqlite3";

#[derive(Clone)]
pub struct Storage {
    root: PathBuf,
}

#[derive(serde::Serialize)]
pub struct BackupInfo {
    pub id: String,
    pub created_at_unix_ms: u64,
}

impl Storage {
    pub fn data_directory(&self) -> &Path {
        &self.root
    }

    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let storage = Self {
            root: root.to_path_buf(),
        };
        storage.recover_interrupted_restore()?;
        crate::reset::recover(root)?;
        storage.ensure_schema().inspect_err(|error| crate::diagnostics::record_error("database.migration", error.as_ref()))?;
        Ok(storage)
    }

    pub fn schema_version(&self) -> Result<i64> {
        let db = self.connect()?;
        Ok(db.pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    pub fn with_connection<T>(
        &self,
        action: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> Result<T> {
        let db = self.connect().inspect_err(|error| crate::diagnostics::record_error("database.open", error.as_ref()))?;
        Ok(action(&db).inspect_err(|error| crate::diagnostics::record_error("database.read", error))?)
    }

    pub fn with_transaction<T>(
        &self,
        action: impl FnOnce(&Transaction<'_>) -> rusqlite::Result<T>,
    ) -> Result<T> {
        let mut db = self.connect().inspect_err(|error| crate::diagnostics::record_error("database.open", error.as_ref()))?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate).inspect_err(|error| crate::diagnostics::record_error("database.transaction", error))?;
        let result = action(&tx).inspect_err(|error| crate::diagnostics::record_error("database.transaction", error))?;
        tx.commit().inspect_err(|error| crate::diagnostics::record_error("database.transaction", error))?;
        Ok(result)
    }

    pub fn create_backup(&self) -> Result<String> {
        let id = self.write_snapshot(&self.root.join("backups"), "backup").inspect_err(|error| crate::diagnostics::record_error("database.backup", error.as_ref()))?;
        Ok(id)
    }

    pub fn create_protection(&self) -> Result<String> {
        self.write_snapshot(&self.root.join("protections"), "protection").inspect_err(|error| crate::diagnostics::record_error("database.backup", error.as_ref()))
    }

    pub fn delete_backup(&self, backup_id: &str) -> Result<()> {
        if !valid_id(backup_id) {
            return Err("invalid backup id".into());
        }
        let directory = self.root.join("backups");
        let expected_directory = self.root.canonicalize()?.join("backups");
        if directory.canonicalize()? != expected_directory {
            return Err("backup directory is redirected".into());
        }
        let backup = directory.join(format!("backup-{backup_id}.sqlite3"));
        if !fs::symlink_metadata(&backup)?.is_file()
            || backup.canonicalize()?
                != expected_directory.join(format!("backup-{backup_id}.sqlite3"))
        {
            return Err("backup is not a regular application file".into());
        }
        fs::remove_file(backup)?;
        Ok(())
    }

    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        let directory = self.root.join("backups");
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut backups = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(id) = name
                .to_str()
                .and_then(|name| name.strip_prefix("backup-"))
                .and_then(|name| name.strip_suffix(".sqlite3"))
            else {
                continue;
            };
            if !valid_id(id) || !entry.file_type()?.is_file() {
                continue;
            }
            let created_at_unix_ms = entry
                .metadata()?
                .modified()?
                .duration_since(UNIX_EPOCH)?
                .as_millis()
                .try_into()?;
            backups.push(BackupInfo {
                id: id.to_owned(),
                created_at_unix_ms,
            });
        }
        backups.sort_by(|a, b| {
            b.created_at_unix_ms
                .cmp(&a.created_at_unix_ms)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(backups)
    }

    pub fn list_protections(&self) -> Result<Vec<String>> {
        let directory = self.root.join("protections");
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("protection-") && name.ends_with(".sqlite3") {
                    names.push(name.to_owned());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    pub fn restore_backup(&self, backup_id: &str) -> Result<()> {
        if !valid_id(backup_id) {
            return Err("invalid backup id".into());
        }
        let backup = self
            .root
            .join("backups")
            .join(format!("backup-{backup_id}.sqlite3"));
        validate_database(&backup).inspect_err(|error| crate::diagnostics::record_error("database.restore", error.as_ref()))?;

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(id) = name
                .to_str()
                .and_then(|name| name.strip_prefix("rollback-"))
                .and_then(|name| name.strip_suffix(".tmp"))
            else {
                continue;
            };
            if valid_id(id) {
                return Err("an earlier restore has a pending rollback file".into());
            }
        }

        // No replacement begins until the current database has a durable snapshot.
        self.write_snapshot(&self.root.join("protections"), "protection")?;

        let stage = self.root.join(format!("restore-{backup_id}.tmp"));
        let rollback = self.root.join(format!("rollback-{backup_id}.tmp"));
        if stage.exists() || rollback.exists() {
            return Err("restore staging path already exists".into());
        }
        let result = (|| -> Result<()> {
            fs::copy(&backup, &stage)?;
            OpenOptions::new().write(true).open(&stage)?.sync_all()?;
            validate_database(&stage)?;
            migrate_v1_file(&stage)?;
            OpenOptions::new().write(true).open(&stage)?.sync_all()?;
            validate_database(&stage)?;
            let current = self.database_path();
            fs::rename(&current, &rollback)?;
            if let Err(replace_error) = fs::rename(&stage, &current) {
                fs::rename(&rollback, &current).map_err(|rollback_error| {
                    format!("restore failed: {replace_error}; rollback failed: {rollback_error}")
                })?;
                return Err(replace_error.into());
            }
            let _ = fs::remove_file(&rollback);
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&stage);
        }
        result.inspect_err(|error| crate::diagnostics::record_error("database.restore", error.as_ref()))
    }

    fn database_path(&self) -> PathBuf {
        self.root.join(DATABASE_FILE)
    }

    fn recover_interrupted_restore(&self) -> Result<()> {
        let current = self.database_path();
        if current.exists() {
            return Ok(());
        }
        let mut rollbacks = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(id) = name
                .strip_prefix("rollback-")
                .and_then(|s| s.strip_suffix(".tmp"))
            else {
                continue;
            };
            if valid_id(id) {
                rollbacks.push(entry.path());
            }
        }
        match rollbacks.as_slice() {
            [] => Ok(()),
            [rollback] => {
                validate_database(rollback)?;
                fs::rename(rollback, current)?;
                Ok(())
            }
            _ => Err("multiple interrupted restores require manual recovery".into()),
        }
    }

    fn connect(&self) -> Result<Connection> {
        let db =
            Connection::open_with_flags(self.database_path(), OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        Ok(db)
    }

    fn ensure_schema(&self) -> Result<()> {
        let mut db = if self.database_path().exists() {
            self.connect()?
        } else {
            Connection::open(self.database_path())?
        };
        let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(format!("unsupported database version {version}").into());
        }
        if (1..SCHEMA_VERSION).contains(&version) {
            let mut log = crate::diagnostics::Scope::start("database.migration", None);
            // The older database remains untouched if the application-data snapshot fails.
            self.write_snapshot(&self.root.join("protections"), "protection")?;
            migrate_schema_v1(&mut db)?;
            log.finish(None, std::collections::BTreeMap::from([("schema".into(), SCHEMA_VERSION as u64)]));
            return Ok(());
        }
        if version == SCHEMA_VERSION {
            return Ok(());
        }
        let existing_tables: i64 = db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        if existing_tables != 0 {
            return Err("unrecognized unversioned database".into());
        }
        let tx = db.transaction()?;
        tx.execute_batch(SCHEMA_V1)?;
        tx.execute_batch(SCHEMA_V2)?;
        tx.execute_batch(SCHEMA_V3)?;
        tx.execute_batch(SCHEMA_V4)?;
        tx.execute_batch(SCHEMA_V5)?;
        tx.execute_batch(SCHEMA_V6)?;
        tx.execute_batch(SCHEMA_V7)?;
        tx.execute_batch(SCHEMA_V8)?;
        tx.execute_batch(SCHEMA_V9)?;
        tx.execute_batch(SCHEMA_V10)?;
        tx.execute_batch(SCHEMA_V11)?;
        tx.execute_batch(SCHEMA_V12)?;
        drop_legacy_availability(&tx)?;
        ensure_tag_positions(&tx)?;
        tx.execute_batch(SCHEMA_V15)?;
        migrate_v16(&tx)?;
        tx.execute_batch(SCHEMA_V17)?;
        tx.execute_batch(SCHEMA_V18)?;
        tx.execute_batch(SCHEMA_V19)?;
        tx.execute_batch(SCHEMA_V20)?;
        tx.execute_batch(SCHEMA_V21)?;
        tx.execute_batch(SCHEMA_V22)?;
        tx.execute_batch(SCHEMA_V23)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
        Ok(())
    }

    fn write_snapshot(&self, directory: &Path, prefix: &str) -> Result<String> {
        fs::create_dir_all(directory)
            .map_err(|error| format!("create snapshot directory: {error}"))?;
        let db = self.connect()?;
        let id: String = db.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
        let temporary = directory.join(format!("snapshot-{id}.tmp"));
        let final_path = directory.join(format!("{prefix}-{id}.sqlite3"));
        let result = (|| -> Result<()> {
            db.execute("VACUUM INTO ?1", [temporary.to_string_lossy().as_ref()])
                .map_err(|error| format!("create snapshot: {error}"))?;
            OpenOptions::new()
                .write(true)
                .open(&temporary)
                .map_err(|error| format!("open snapshot for sync: {error}"))?
                .sync_all()
                .map_err(|error| format!("sync snapshot: {error}"))?;
            validate_database(&temporary)?;
            fs::rename(&temporary, &final_path)
                .map_err(|error| format!("publish snapshot: {error}"))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        Ok(id)
    }
}

fn valid_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn migrate_schema_v1(db: &mut Connection) -> Result<()> {
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let tx = db.transaction()?;
    if version == 1 {
        tx.execute_batch(SCHEMA_V2)?;
    }
    if version < 3 {
        tx.execute_batch(SCHEMA_V3)?;
    }
    if version < 4 {
        tx.execute_batch(SCHEMA_V4)?;
    }
    if version < 5 {
        tx.execute_batch(SCHEMA_V5)?;
    }
    if version < 6 {
        tx.execute_batch(SCHEMA_V6)?;
    }
    if version < 7 {
        tx.execute_batch(SCHEMA_V7)?;
    }
    if version < 8 {
        tx.execute_batch(SCHEMA_V8)?;
    }
    if version < 9 {
        tx.execute_batch(SCHEMA_V9)?;
    }
    if version < 10 {
        tx.execute_batch(SCHEMA_V10)?;
    }
    if version < 11 {
        tx.execute_batch(SCHEMA_V11)?;
    }
    if version < 12 { tx.execute_batch(SCHEMA_V12)?; }
    if version < 13 { drop_legacy_availability(&tx)?; }
    if version < 14 { ensure_tag_positions(&tx)?; }
    if version < 15 { tx.execute_batch(SCHEMA_V15)?; }
    if version < 16 { migrate_v16(&tx)?; }
    if version < 17 { tx.execute_batch(SCHEMA_V17)?; }
    if version < 18 { tx.execute_batch(SCHEMA_V18)?; }
    if version < 19 { tx.execute_batch(SCHEMA_V19)?; }
    if version < 20 { tx.execute_batch(SCHEMA_V20)?; }
    if version < 21 {
        // Preserve the effective system default for an existing installation.
        tx.execute("INSERT OR IGNORE INTO player_settings(singleton,mode,program,arguments) VALUES(1,'system','','')", [])?;
        tx.execute_batch(SCHEMA_V21)?;
    }
    if version < 22 { tx.execute_batch(SCHEMA_V22)?; }
    if version < 23 { tx.execute_batch(SCHEMA_V23)?; }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

// Older backup schemas retain historical DDL until this protected migration.
fn migrate_v16(tx:&Transaction<'_>)->rusqlite::Result<()> {
    for (table,column) in [("film_manual","notes"),("actor_manual","notes"),("file_versions","purpose"),("file_versions","notes")] {
        let present:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2)",[table,column],|r|r.get(0))?;
        if present {tx.execute_batch(&format!("ALTER TABLE {table} DROP COLUMN {column};"))?;}
    }
    for (table,column,definition) in [("film_manual","cast_override","INTEGER NOT NULL DEFAULT 0 CHECK(cast_override IN(0,1))"),("actor_manual","aliases","TEXT"),("libraries","scan_mode","TEXT NOT NULL DEFAULT 'actor' CHECK(scan_mode IN('general','actor'))")] {
        let present:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2)",[table,column],|r|r.get(0))?;
        if !present {tx.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition};"))?;}
    }
    tx.execute_batch("CREATE TABLE IF NOT EXISTS info_actor_links(external_id TEXT PRIMARY KEY,actor_id TEXT NOT NULL REFERENCES actors(id));")?;
    Ok(())
}

fn drop_legacy_availability(tx:&Transaction<'_>)->rusqlite::Result<()> {
    let present:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('file_observations') WHERE name='availability')",[],|r|r.get(0))?;
    if present {tx.execute_batch("ALTER TABLE file_observations DROP COLUMN availability;")?;}
    Ok(())
}

fn migrate_v1_file(path: &Path) -> Result<()> {
    let mut db = Connection::open(path)?;
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if (1..SCHEMA_VERSION).contains(&version) {
        migrate_schema_v1(&mut db)?;
    }
    Ok(())
}

fn validate_database(path: &Path) -> Result<()> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = db.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(format!("database integrity check failed: {integrity}").into());
    }
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if !(1..=SCHEMA_VERSION).contains(&version) {
        return Err(format!("unsupported backup version {version}").into());
    }
    for table in [
        "libraries",
        "films",
        "film_manual",
        "file_observations",
        "actors",
        "actor_manual",
        "film_actors",
        "tags",
        "film_tags",
        "file_versions",
        "issue_records",
        "player_launch_events",
    ] {
        let present: i64 = db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |row| row.get(0),
        )?;
        if present != 1 {
            return Err(format!("backup missing table {table}").into());
        }
    }
    if version >= 14 {
        let present: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('tags') WHERE name='position')", [], |r| r.get(0))?;
        if !present { return Err("backup missing tag order".into()); }
    }
    if version >= 2 {
        let removed: i64 = db.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('libraries') WHERE name='removed'",
            [],
            |row| row.get(0),
        )?;
        if removed != 1 {
            return Err("backup missing source state".into());
        }
    }
    if version >= 3 {
        let present: i64 = db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='player_settings'",
            [],
            |row| row.get(0),
        )?;
        if present != 1 {
            return Err("backup missing player settings".into());
        }
    }
    if version >= 6 {
        let present: i64 = db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='actor_directories'",
            [],
            |r| r.get(0),
        )?;
        if present != 1 {
            return Err("backup missing actor directories".into());
        }
        let columns: i64 = db.query_row("SELECT COUNT(*) FROM pragma_table_info('actor_directories') WHERE name IN ('actor_id','directory_key','marker_id','directory_name','directory_created_at')", [], |r| r.get(0))?;
        if columns != 5 {
            return Err("backup missing actor directory evidence".into());
        }
    }
    if version >= 7 {
        let columns: i64 = db.query_row("SELECT COUNT(*) FROM pragma_table_info('interface_preferences') WHERE name IN ('singleton','reduce_motion','thin_scrollbar')",[],|r|r.get(0))?;
        if columns != 3 {
            return Err("backup missing interface preferences".into());
        }
    }
    if version >= 10 {
        let columns: i64 = db.query_row("SELECT COUNT(*) FROM pragma_table_info('ignored_directory_names') WHERE name IN ('position','name','name_key')",[],|r|r.get(0))?;
        if columns != 3 { return Err("backup missing ignored directory settings".into()); }
    }
    if version >= 11 {
        for (table, expected, names) in [
            ("translation_memory", 12, "'id','identity_anchor','normalized_code','original_title','normalized_title','body','fingerprint','source_language','translated_title','provider','translated_at','last_matched_at'"),
            ("film_translations", 2, "'film_id','memory_id'"),
            ("translation_settings", 5, "'singleton','enabled','provider','deepl_plan','target_language'"),
        ] {
            let columns: i64 = db.query_row(&format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name IN ({names})"), [], |r| r.get(0))?;
            if columns != expected { return Err("backup missing translation data".into()); }
        }
    }
    if version >= 12 {
        let count: i64 = db.query_row("SELECT COUNT(*) FROM pragma_table_info('translation_settings') WHERE name IN ('azure_region','baidu_app_id')",[],|r|r.get(0))?;
        if count != 2 { return Err("backup missing provider configuration".into()); }
    }
    if version >= 15 {
        let columns: i64 = db.query_row("SELECT COUNT(*) FROM pragma_table_info('application_options') WHERE name IN ('singleton','close_window_exits','monitor_libraries')", [], |r| r.get(0))?;
        if columns != 3 { return Err("backup missing application options".into()); }
    }
    if version >= 16 {
        for (table,column) in [("film_manual","cast_override"),("actor_manual","aliases"),("libraries","scan_mode")] {
            let present:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2)",[table,column],|r|r.get(0))?;
            if !present {return Err("backup missing directory metadata".into())}
        }
        let columns:i64=db.query_row("SELECT COUNT(*) FROM pragma_table_info('info_actor_links') WHERE name IN ('external_id','actor_id')",[],|r|r.get(0))?;
        if columns!=2{return Err("backup missing actor mappings".into())}
    }
    if (17..19).contains(&version) {
        let columns:i64=db.query_row("SELECT COUNT(*) FROM pragma_table_info('actors') WHERE name='modified_at'",[],|r|r.get(0))?;
        if columns!=1 {return Err("backup missing actor record dates".into());}
        let count: i64 = db.query_row("SELECT COUNT(*) FROM pragma_table_info('actor_photos') WHERE name IN ('actor_id','jpeg','fingerprint')", [], |r| r.get(0))?;
        if count != 3 { return Err("backup missing managed actor data".into()); }
    }
    if version == 17 {
        let count: i64 = db.query_row("SELECT COUNT(*) FROM pragma_table_info('actor_import_sources') WHERE name IN ('directory_key','actor_id','fingerprint','external_id')", [], |r| r.get(0))?;
        if count != 4 { return Err("backup missing managed actor data".into()); }
    }
    if version >= 18 {
        let present: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='actor_import_sources')", [], |r| r.get(0))?;
        if present { return Err("backup retains retired actor scan mappings".into()); }
    }
    if version >= 19 {
        for (table, columns, expected) in [
            ("actor_image_sources", "'actor_id','path','fingerprint'", 3),
            ("actor_tags", "'name','position'", 2),
            ("actor_tag_links", "'actor_id','tag'", 2),
        ] {
            let count: i64 = db.query_row(&format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name IN ({columns})"), [], |r| r.get(0))?;
            if count != expected { return Err("backup missing actor image paths or tags".into()); }
        }
        let photos: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='actor_photos')", [], |r| r.get(0))?;
        if photos { return Err("backup retains retired actor photo contents".into()); }
    }
    if version >= 8 {
        if version >= 22 {
            let columns:i64=db.query_row("SELECT COUNT(*) FROM pragma_table_info('library_display_preferences') WHERE name IN ('library_id','cover_ratio')",[],|r|r.get(0))?;
            if columns!=2 {return Err("backup missing library display preferences".into());}
        }
        let columns: i64 = db.query_row("SELECT COUNT(*) FROM pragma_table_info('interface_preferences') WHERE name IN ('film_cover_ratio','actor_photo_ratio','film_cover_fit','actor_photo_fit')",[],|r|r.get(0))?;
        if columns != 4 {
            return Err("backup missing image preferences".into());
        }
    }
    if version >= 5 {
        let present: i64 = db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='film_aliases'",
            [],
            |r| r.get(0),
        )?;
        if present != 1 {
            return Err("backup missing film identities".into());
        }
    }
    if version >= 4 {
        let present: i64 = db.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('libraries') WHERE name='record_hidden'",
            [],
            |r| r.get(0),
        )?;
        if present != 1 {
            return Err("backup missing directory record state".into());
        }
    }
    if version >= 21 {
        let present: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('player_settings') WHERE name='internal_display')", [], |r| r.get(0))?;
        if !present { return Err("backup missing internal player settings".into()); }
    }
    if version >= 23 {
        let columns: i64 = db.query_row("SELECT COUNT(*) FROM pragma_table_info('interface_language') WHERE name IN ('singleton','language')", [], |r| r.get(0))?;
        if columns != 2 { return Err("backup missing interface language".into()); }
        let valid: bool = db.query_row("SELECT COUNT(*)=1 AND MIN(singleton)=1 AND MIN(language) IN ('zh-CN','zh-TW','en','ja') FROM interface_language", [], |r| r.get(0))?;
        if !valid { return Err("backup invalid interface language".into()); }
    }
    if db
        .prepare("PRAGMA foreign_key_check")?
        .query([])?
        .next()?
        .is_some()
    {
        return Err("backup has broken foreign key references".into());
    }
    Ok(())
}

const SCHEMA_V22: &str = "
UPDATE interface_preferences SET film_cover_ratio='original' WHERE film_cover_ratio='3:2';
CREATE TABLE IF NOT EXISTS library_display_preferences(library_id TEXT PRIMARY KEY REFERENCES libraries(id) ON DELETE CASCADE,cover_ratio TEXT NOT NULL CHECK(cover_ratio IN('original','16:9','5:7')));
INSERT OR IGNORE INTO library_display_preferences(library_id,cover_ratio) SELECT id,COALESCE((SELECT film_cover_ratio FROM interface_preferences WHERE singleton=1),'original') FROM libraries;
";

const SCHEMA_V21: &str = "
CREATE TABLE player_settings_v21(singleton INTEGER PRIMARY KEY CHECK(singleton=1),mode TEXT NOT NULL CHECK(mode IN('system','custom','internal')),program TEXT NOT NULL,arguments TEXT NOT NULL,internal_display TEXT NOT NULL DEFAULT 'overlay' CHECK(internal_display IN('overlay','window')));
INSERT INTO player_settings_v21(singleton,mode,program,arguments) SELECT singleton,mode,program,arguments FROM player_settings;
DROP TABLE player_settings;
ALTER TABLE player_settings_v21 RENAME TO player_settings;
";

// Retire only the two removed problem types, including handled history,
// current evidence anchors and clear suppressions. Shared scan facts remain.
const SCHEMA_V20: &str = "
DELETE FROM issue_records WHERE subject_type IN ('film_missing_video','actor_missing_photo')
 OR (subject_type='film_missing_cover' AND subject_id GLOB 'directory:*')
 OR (subject_type='library_problem_current' AND (id GLOB 'problem-current:film_missing_video:*' OR id GLOB 'problem-current:actor_missing_photo:*'))
 OR (subject_type='library_problem_current' AND id GLOB 'problem-current:film_missing_cover:directory:*')
 OR (subject_type='library_problem_cleared' AND (subject_id GLOB 'film_missing_video:*' OR subject_id GLOB 'actor_missing_photo:*' OR subject_id GLOB 'film_missing_cover:directory:*'));
";

// Historical scan-directory/external identities must not survive the current
// actor.info contract. Personal actor records and film info mappings remain.
const SCHEMA_V18: &str = "DROP TABLE actor_import_sources;";

// The user explicitly approved discarding untraceable legacy JPEGs. No guessed
// paths or permanent photo copies are created; actor records and cast remain.
const SCHEMA_V19: &str = "
PRAGMA secure_delete=ON;
DROP TABLE actor_photos;
CREATE TABLE actor_image_sources(actor_id TEXT PRIMARY KEY REFERENCES actors(id) ON DELETE CASCADE,path TEXT NOT NULL,fingerprint TEXT NOT NULL);
CREATE TRIGGER actor_image_insert_time AFTER INSERT ON actor_image_sources BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.actor_id; END;
CREATE TRIGGER actor_image_update_time AFTER UPDATE ON actor_image_sources WHEN OLD.path IS NOT NEW.path OR OLD.fingerprint IS NOT NEW.fingerprint BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.actor_id; END;
CREATE TRIGGER actor_image_delete_time AFTER DELETE ON actor_image_sources BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=OLD.actor_id; END;
CREATE TABLE actor_tags(name TEXT PRIMARY KEY COLLATE NOCASE,position INTEGER NOT NULL CHECK(position>=0));
CREATE TABLE actor_tag_links(actor_id TEXT NOT NULL REFERENCES actors(id) ON DELETE CASCADE,tag TEXT NOT NULL REFERENCES actor_tags(name) ON UPDATE CASCADE ON DELETE CASCADE,PRIMARY KEY(actor_id,tag));
";

const SCHEMA_V17: &str = "
ALTER TABLE actors ADD COLUMN modified_at TEXT NOT NULL DEFAULT '';
UPDATE actors SET modified_at=created_at;
CREATE TABLE actor_photos(actor_id TEXT PRIMARY KEY REFERENCES actors(id) ON DELETE CASCADE,jpeg BLOB NOT NULL,fingerprint TEXT NOT NULL);
CREATE TABLE actor_import_sources(directory_key TEXT PRIMARY KEY,actor_id TEXT NOT NULL REFERENCES actors(id) ON DELETE CASCADE,fingerprint TEXT NOT NULL,external_id TEXT);
DELETE FROM film_actors WHERE source='observation';
CREATE TRIGGER actor_created_time AFTER INSERT ON actors BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.id; END;
CREATE TRIGGER actor_name_time AFTER UPDATE OF observed_name ON actors WHEN OLD.observed_name IS NOT NEW.observed_name BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.id; END;
CREATE TRIGGER actor_manual_insert_time AFTER INSERT ON actor_manual BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.actor_id; END;
CREATE TRIGGER actor_manual_update_time AFTER UPDATE ON actor_manual WHEN OLD.favorite IS NOT NEW.favorite OR OLD.birth_month IS NOT NEW.birth_month OR OLD.height_cm IS NOT NEW.height_cm OR OLD.cup IS NOT NEW.cup OR OLD.bust_cm IS NOT NEW.bust_cm OR OLD.waist_cm IS NOT NEW.waist_cm OR OLD.hip_cm IS NOT NEW.hip_cm OR OLD.aliases IS NOT NEW.aliases BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.actor_id; END;
CREATE TRIGGER actor_manual_delete_time AFTER DELETE ON actor_manual BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=OLD.actor_id; END;
CREATE TRIGGER actor_photo_insert_time AFTER INSERT ON actor_photos BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.actor_id; END;
CREATE TRIGGER actor_photo_update_time AFTER UPDATE ON actor_photos WHEN OLD.fingerprint IS NOT NEW.fingerprint BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.actor_id; END;
CREATE TRIGGER actor_photo_delete_time AFTER DELETE ON actor_photos BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=OLD.actor_id; END;
";

const SCHEMA_V10: &str = "CREATE TABLE ignored_directory_names(position INTEGER PRIMARY KEY CHECK(position>=0), name TEXT NOT NULL, name_key TEXT NOT NULL UNIQUE);";

// Memory is user data independent of current observations, sources, and films.
// The anchor is deliberately not a foreign key, so disappearing films never delete it.
const SCHEMA_V11: &str = r#"
CREATE TABLE translation_memory (
    id TEXT PRIMARY KEY,
    identity_anchor TEXT NOT NULL,
    normalized_code TEXT NOT NULL,
    original_title TEXT NOT NULL,
    normalized_title TEXT NOT NULL,
    body TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    source_language TEXT NOT NULL DEFAULT '',
    translated_title TEXT NOT NULL,
    provider TEXT NOT NULL,
    translated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_matched_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX translation_memory_match ON translation_memory(normalized_code,normalized_title,fingerprint);
CREATE INDEX translation_memory_identity ON translation_memory(identity_anchor);
CREATE TABLE film_translations (
    film_id TEXT PRIMARY KEY REFERENCES films(id) ON DELETE CASCADE,
    memory_id TEXT NOT NULL REFERENCES translation_memory(id) ON DELETE CASCADE
);
CREATE TABLE translation_settings (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0,1)),
    provider TEXT NOT NULL DEFAULT 'deepl' CHECK(provider IN ('deepl','google')),
    deepl_plan TEXT NOT NULL DEFAULT 'free' CHECK(deepl_plan IN ('free','pro')),
    target_language TEXT NOT NULL DEFAULT 'ZH-HANS' CHECK(target_language='ZH-HANS')
);
"#;

const SCHEMA_V12: &str = r#"
ALTER TABLE translation_settings RENAME TO translation_settings_old;
CREATE TABLE translation_settings (singleton INTEGER PRIMARY KEY CHECK(singleton=1),enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0,1)),provider TEXT NOT NULL DEFAULT 'deepl' CHECK(provider IN ('deepl','google','azure','baidu')),deepl_plan TEXT NOT NULL DEFAULT 'free' CHECK(deepl_plan IN ('free','pro')),target_language TEXT NOT NULL DEFAULT 'ZH-HANS' CHECK(target_language='ZH-HANS'),azure_region TEXT NOT NULL DEFAULT '',baidu_app_id TEXT NOT NULL DEFAULT '');
INSERT INTO translation_settings(singleton,enabled,provider,deepl_plan,target_language) SELECT singleton,enabled,provider,deepl_plan,target_language FROM translation_settings_old;
DROP TABLE translation_settings_old;
"#;

const SCHEMA_V1: &str = r#"
CREATE TABLE libraries (
    id TEXT PRIMARY KEY, display_name TEXT NOT NULL, root_path TEXT,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE films (
    id TEXT PRIMARY KEY, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE film_manual (
    film_id TEXT PRIMARY KEY REFERENCES films(id) ON DELETE CASCADE,
    title TEXT, code TEXT, notes TEXT, rating INTEGER CHECK (rating BETWEEN 0 AND 10),
    favorite INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
    cover_choice TEXT
);
CREATE TABLE file_observations (
    id TEXT PRIMARY KEY, library_id TEXT NOT NULL REFERENCES libraries(id),
    film_id TEXT REFERENCES films(id), observed_path TEXT NOT NULL,
    file_name TEXT, code_candidate TEXT, created_at TEXT,
    kind TEXT NOT NULL DEFAULT 'undetermined' CHECK (kind IN ('video', 'subtitle', 'image', 'undetermined')),
    availability TEXT NOT NULL DEFAULT 'unverified' CHECK (availability IN ('unverified', 'available', 'unavailable')),
    observed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX file_observations_film_id ON file_observations(film_id);
CREATE INDEX file_observations_library_id ON file_observations(library_id);
CREATE TABLE actors (
    id TEXT PRIMARY KEY, observed_name TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE actor_manual (
    actor_id TEXT PRIMARY KEY REFERENCES actors(id) ON DELETE CASCADE,
    birth_month TEXT, height_cm INTEGER, cup TEXT,
    bust_cm INTEGER, waist_cm INTEGER, hip_cm INTEGER,
    notes TEXT, favorite INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1))
);
CREATE TABLE film_actors (
    film_id TEXT NOT NULL REFERENCES films(id) ON DELETE CASCADE,
    actor_id TEXT NOT NULL REFERENCES actors(id) ON DELETE CASCADE,
    source TEXT NOT NULL CHECK (source IN ('manual', 'observation')),
    PRIMARY KEY (film_id, actor_id, source)
);
CREATE TABLE tags (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE);
CREATE TABLE film_tags (
    film_id TEXT NOT NULL REFERENCES films(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (film_id, tag_id)
);
CREATE TABLE file_versions (
    observation_id TEXT PRIMARY KEY REFERENCES file_observations(id) ON DELETE CASCADE,
    purpose TEXT NOT NULL DEFAULT 'undetermined' CHECK (purpose IN ('main', 'part', 'extra', 'undetermined')),
    preferred INTEGER NOT NULL DEFAULT 0 CHECK (preferred IN (0, 1)),
    notes TEXT
);
CREATE TABLE issue_records (
    id TEXT PRIMARY KEY, subject_type TEXT NOT NULL, subject_id TEXT NOT NULL,
    evidence_key TEXT NOT NULL, evidence TEXT NOT NULL,
    resolution TEXT, handled_at TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE player_launch_events (
    id TEXT PRIMARY KEY, film_id TEXT NOT NULL REFERENCES films(id),
    observation_id TEXT REFERENCES file_observations(id),
    launched_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
"#;

const SCHEMA_V2: &str = "ALTER TABLE libraries ADD COLUMN removed INTEGER NOT NULL DEFAULT 0 CHECK (removed IN (0, 1));";

const SCHEMA_V3: &str = "CREATE TABLE player_settings (singleton INTEGER PRIMARY KEY CHECK(singleton=1), mode TEXT NOT NULL CHECK(mode IN ('system','custom')), program TEXT NOT NULL, arguments TEXT NOT NULL);";

const SCHEMA_V4: &str = "ALTER TABLE libraries ADD COLUMN record_hidden INTEGER NOT NULL DEFAULT 0 CHECK(record_hidden IN (0,1));";

const SCHEMA_V5: &str = "CREATE TABLE film_aliases(id TEXT PRIMARY KEY REFERENCES films(id), film_id TEXT NOT NULL REFERENCES films(id), CHECK(id<>film_id));";

const SCHEMA_V6: &str = "CREATE TABLE actor_directories(actor_id TEXT PRIMARY KEY REFERENCES actors(id), directory_key TEXT NOT NULL UNIQUE, marker_id TEXT NOT NULL REFERENCES file_observations(id), directory_name TEXT NOT NULL, directory_created_at TEXT);";

const SCHEMA_V7: &str = r#"
CREATE TABLE IF NOT EXISTS interface_preferences (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    reduce_motion INTEGER NOT NULL CHECK(reduce_motion IN (0,1)),
    thin_scrollbar INTEGER NOT NULL CHECK(thin_scrollbar IN (0,1))
);
ALTER TABLE actor_directories RENAME TO actor_directories_v6;
CREATE TABLE actor_directories (
    actor_id TEXT PRIMARY KEY REFERENCES actors(id),
    directory_key TEXT NOT NULL UNIQUE,
    marker_id TEXT REFERENCES file_observations(id),
    directory_name TEXT NOT NULL, directory_created_at TEXT
);
INSERT INTO actor_directories SELECT * FROM actor_directories_v6;
DROP TABLE actor_directories_v6;
"#;

const SCHEMA_V8: &str = r#"
ALTER TABLE interface_preferences ADD COLUMN film_cover_ratio TEXT NOT NULL DEFAULT 'original' CHECK(film_cover_ratio IN ('original','16:9','3:2'));
ALTER TABLE interface_preferences ADD COLUMN actor_photo_ratio TEXT NOT NULL DEFAULT 'original' CHECK(actor_photo_ratio IN ('original','2:3','3:4'));
ALTER TABLE interface_preferences ADD COLUMN film_cover_fit TEXT NOT NULL DEFAULT 'cover' CHECK(film_cover_fit IN ('cover','contain'));
ALTER TABLE interface_preferences ADD COLUMN actor_photo_fit TEXT NOT NULL DEFAULT 'cover' CHECK(actor_photo_fit IN ('cover','contain'));
"#;

const SCHEMA_V9: &str = r#"
CREATE TABLE interface_preferences_v9 (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    reduce_motion INTEGER NOT NULL CHECK(reduce_motion IN (0,1)),
    thin_scrollbar INTEGER NOT NULL CHECK(thin_scrollbar IN (0,1)),
    film_cover_ratio TEXT NOT NULL DEFAULT 'original' CHECK(film_cover_ratio IN ('original','16:9','3:2','5:7')),
    actor_photo_ratio TEXT NOT NULL DEFAULT 'original' CHECK(actor_photo_ratio IN ('original','2:3','3:4')),
    film_cover_fit TEXT NOT NULL DEFAULT 'cover' CHECK(film_cover_fit IN ('cover','contain')),
    actor_photo_fit TEXT NOT NULL DEFAULT 'cover' CHECK(actor_photo_fit IN ('cover','contain'))
);
INSERT INTO interface_preferences_v9 SELECT singleton,reduce_motion,thin_scrollbar,film_cover_ratio,actor_photo_ratio,film_cover_fit,actor_photo_fit FROM interface_preferences;
DROP TABLE interface_preferences;
ALTER TABLE interface_preferences_v9 RENAME TO interface_preferences;
"#;

#[cfg(test)]
mod migration_tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn schema11_adds_long_lived_memory_without_changing_manual_data() {
        let root = tempfile::tempdir().unwrap();
        old_database(&root.path().join(DATABASE_FILE));
        let storage = Storage::open(root.path()).unwrap();
        assert_eq!(storage.schema_version().unwrap(), SCHEMA_VERSION);
        storage.with_connection(|db| {
            let manual: String = db.query_row("SELECT title FROM film_manual WHERE film_id='film'", [], |r| r.get(0))?;
            assert_eq!(manual, "人工标题");
            assert_eq!(db.query_row("SELECT count(*) FROM translation_memory", [], |r| r.get::<_, i64>(0))?, 0);
            assert_eq!(db.query_row("SELECT count(*) FROM pragma_foreign_key_list('translation_memory')", [], |r| r.get::<_, i64>(0))?, 0);
            Ok(())
        }).unwrap();
    }

    #[test]
    fn schema12_upgrade_and_old_backup_preserve_manual_and_provider_data() {
        let root=tempfile::tempdir().unwrap();
        let path=root.path().join(DATABASE_FILE);
        old_database(&path);
        let db=Connection::open(&path).unwrap();
        for schema in [SCHEMA_V2,SCHEMA_V3,SCHEMA_V4,SCHEMA_V5,SCHEMA_V6,SCHEMA_V7,SCHEMA_V8,SCHEMA_V9,SCHEMA_V10,SCHEMA_V11] {db.execute_batch(schema).unwrap();}
        db.execute("INSERT INTO translation_settings(singleton,enabled,provider,deepl_plan,target_language) VALUES(1,1,'google','free','ZH-HANS')",[]).unwrap();
        db.execute("INSERT INTO translation_memory(id,identity_anchor,normalized_code,original_title,normalized_title,body,fingerprint,translated_title,provider) VALUES('memory','film','ABC001','ABC-001 fictional','fictional','fictional','fixture','saved translation','google')",[]).unwrap();
        db.execute("INSERT INTO film_translations(film_id,memory_id) VALUES('film','memory')",[]).unwrap();
        db.pragma_update(None,"user_version",11).unwrap(); drop(db);
        let old=Storage {root:root.path().to_path_buf()};
        let backup=old.create_backup().unwrap();
        let upgraded=Storage::open(root.path()).unwrap();
        assert_eq!(upgraded.schema_version().unwrap(),SCHEMA_VERSION);
        assert_eq!(upgraded.list_protections().unwrap().len(),1);
        for restored in [false,true] {
            if restored {upgraded.restore_backup(&backup).unwrap();}
            upgraded.with_connection(|db| {
                let manual:(String,i64,i64)=db.query_row("SELECT title,rating,favorite FROM film_manual WHERE film_id='film'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
                assert_eq!(manual,("人工标题".into(),8,1));
                let settings:(String,String,String)=db.query_row("SELECT provider,azure_region,baidu_app_id FROM translation_settings",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
                assert_eq!(settings,("google".into(),String::new(),String::new()));
                assert_eq!(db.query_row("SELECT translated_title FROM translation_memory WHERE id='memory'",[],|r|r.get::<_,String>(0))?,"saved translation");
                assert_eq!(db.query_row("SELECT memory_id FROM film_translations WHERE film_id='film'",[],|r|r.get::<_,String>(0))?,"memory");
                Ok(())
            }).unwrap();
        }
    }

    fn old_database(path: &Path) {
        let db = Connection::open(path).unwrap();
        db.execute_batch(SCHEMA_V1).unwrap();
        db.pragma_update(None, "user_version", 1).unwrap();
        db.execute("INSERT INTO libraries (id, display_name, root_path) VALUES ('source', '虚构', 'X:/fictional')", []).unwrap();
        db.execute("INSERT INTO films (id) VALUES ('film')", [])
            .unwrap();
        db.execute("INSERT INTO film_manual (film_id, title, rating, favorite) VALUES ('film', '人工标题', 8, 1)", []).unwrap();
        db.execute("INSERT INTO file_observations (id, library_id, film_id, observed_path, kind) VALUES ('observation', 'source', 'film', 'old.mp4', 'video')", []).unwrap();
    }

    fn v3_database(path: &Path) {
        old_database(path);
        let db = Connection::open(path).unwrap();
        db.execute_batch(SCHEMA_V2).unwrap();
        db.execute_batch(SCHEMA_V3).unwrap();
        db.pragma_update(None, "user_version", 3).unwrap();
        db.execute(
            "INSERT INTO player_settings(singleton,mode,program,arguments) VALUES(1,'system','','--fictional')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn v4_upgrade_protects_first_and_restores_old_backup_with_alias_table() {
        for fail_protection in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let data = home.path().join("data");
            fs::create_dir(&data).unwrap();
            v3_database(&data.join(DATABASE_FILE));
            let db = Connection::open(data.join(DATABASE_FILE)).unwrap();
            db.execute_batch(SCHEMA_V4).unwrap();
            db.pragma_update(None, "user_version", 4).unwrap();
            drop(db);
            if fail_protection {
                File::create(data.join("protections")).unwrap();
                assert!(Storage::open(&data).is_err());
                let db = Connection::open(data.join(DATABASE_FILE)).unwrap();
                assert_eq!(
                    db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                        .unwrap(),
                    4
                );
                continue;
            }
            let storage = Storage::open(&data).unwrap();
            assert_eq!(storage.schema_version().unwrap(), SCHEMA_VERSION);
            let protected = data
                .join("protections")
                .join(&storage.list_protections().unwrap()[0]);
            fs::create_dir_all(data.join("backups")).unwrap();
            let id = "33333333333333333333333333333333";
            fs::copy(protected, data.join(format!("backups/backup-{id}.sqlite3"))).unwrap();
            storage.restore_backup(id).unwrap();
            storage
                .with_connection(|db| {
                    let aliases: i64 =
                        db.query_row("SELECT COUNT(*) FROM film_aliases", [], |r| r.get(0))?;
                    assert_eq!(aliases, 0);
                    Ok(())
                })
                .unwrap();
        }
    }

    #[test]
    fn v3_upgrade_and_restore_protect_manual_identity_and_player_settings() {
        let home = tempfile::tempdir().unwrap();
        let data = home.path().join("data");
        fs::create_dir(&data).unwrap();
        v3_database(&data.join(DATABASE_FILE));
        let storage = Storage::open(&data).unwrap();
        assert_eq!(storage.schema_version().unwrap(), SCHEMA_VERSION);
        let protected = data
            .join("protections")
            .join(&storage.list_protections().unwrap()[0]);
        let old = Connection::open(&protected).unwrap();
        assert_eq!(
            old.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            3
        );
        drop(old);
        fs::create_dir_all(data.join("backups")).unwrap();
        let backup_id = "22222222222222222222222222222222";
        fs::copy(
            protected,
            data.join(format!("backups/backup-{backup_id}.sqlite3")),
        )
        .unwrap();
        storage.restore_backup(backup_id).unwrap();
        assert_eq!(storage.schema_version().unwrap(), SCHEMA_VERSION);
        storage.with_connection(|db| {
            let state:(String,i64,i64,String)=db.query_row("SELECT o.film_id,m.rating,l.record_hidden,p.arguments FROM file_observations o JOIN film_manual m ON m.film_id=o.film_id JOIN libraries l ON l.id=o.library_id CROSS JOIN player_settings p",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
            assert_eq!(state,("film".into(),8,0,"--fictional".into()));
            Ok(())
        }).unwrap();
    }

    #[test]
    fn v3_failed_protection_leaves_version_and_manual_data_unchanged() {
        let home = tempfile::tempdir().unwrap();
        let data = home.path().join("data");
        fs::create_dir(&data).unwrap();
        v3_database(&data.join(DATABASE_FILE));
        File::create(data.join("protections")).unwrap();
        assert!(Storage::open(&data).is_err());
        let db = Connection::open(data.join(DATABASE_FILE)).unwrap();
        assert_eq!(
            db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            3
        );
        let rating: i64 = db
            .query_row("SELECT rating FROM film_manual", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rating, 8);
        let hidden: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('libraries') WHERE name='record_hidden'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hidden, 0);
    }

    #[test]
    fn v2_upgrade_is_protected_and_restorable_with_stable_manual_identity() {
        let home = tempfile::tempdir().unwrap();
        let data = home.path().join("data");
        fs::create_dir(&data).unwrap();
        old_database(&data.join(DATABASE_FILE));
        let db = Connection::open(data.join(DATABASE_FILE)).unwrap();
        db.execute_batch(SCHEMA_V2).unwrap();
        db.pragma_update(None, "user_version", 2).unwrap();
        drop(db);
        let storage = Storage::open(&data).unwrap();
        assert_eq!(storage.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(storage.list_protections().unwrap().len(), 1);
        storage
            .with_connection(|db| {
                let state: (String, i64) =
                    db.query_row("SELECT film_id,rating FROM film_manual", [], |r| {
                        Ok((r.get(0)?, r.get(1)?))
                    })?;
                assert_eq!(state, ("film".into(), 8));
                Ok(())
            })
            .unwrap();
        let protected = data
            .join("protections")
            .join(&storage.list_protections().unwrap()[0]);
        let old = Connection::open(protected).unwrap();
        assert_eq!(
            old.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            2
        );
        drop(old);
    }

    #[test]
    fn v2_failed_protection_keeps_old_version_and_records() {
        let home = tempfile::tempdir().unwrap();
        let data = home.path().join("data");
        fs::create_dir(&data).unwrap();
        old_database(&data.join(DATABASE_FILE));
        let db = Connection::open(data.join(DATABASE_FILE)).unwrap();
        db.execute_batch(SCHEMA_V2).unwrap();
        db.pragma_update(None, "user_version", 2).unwrap();
        File::create(data.join("protections")).unwrap();
        assert!(Storage::open(&data).is_err());
        assert_eq!(
            db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            2
        );
        let score: i64 = db
            .query_row("SELECT rating FROM film_manual", [], |r| r.get(0))
            .unwrap();
        assert_eq!(score, 8);
    }

    #[test]
    fn v1_migration_protects_first_and_preserves_identity_and_manual_data() {
        let home = tempfile::tempdir().unwrap();
        let data = home.path().join("data");
        fs::create_dir(&data).unwrap();
        old_database(&data.join(DATABASE_FILE));
        let storage = Storage::open(&data).unwrap();
        assert_eq!(storage.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(storage.list_protections().unwrap().len(), 1);
        storage.with_connection(|db| {
            let state: (String, String, i64, i64, i64) = db.query_row(
                "SELECT file_observations.id, file_observations.film_id, film_manual.rating, film_manual.favorite, libraries.removed FROM file_observations JOIN film_manual ON film_manual.film_id=file_observations.film_id JOIN libraries ON libraries.id=file_observations.library_id",
                [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )?;
            assert_eq!(state, ("observation".into(), "film".into(), 8, 1, 0));
            Ok(())
        }).unwrap();
    }

    #[test]
    fn failed_protection_leaves_v1_database_unchanged() {
        let home = tempfile::tempdir().unwrap();
        let data = home.path().join("data");
        fs::create_dir(&data).unwrap();
        old_database(&data.join(DATABASE_FILE));
        File::create(data.join("protections")).unwrap();
        assert!(Storage::open(&data).is_err());
        let db = Connection::open(data.join(DATABASE_FILE)).unwrap();
        let version: i64 = db
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
        let title: String = db
            .query_row(
                "SELECT title FROM film_manual WHERE film_id='film'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(title, "人工标题");
    }

    #[test]
    fn restoring_old_backup_upgrades_staged_copy_and_keeps_current_protection() {
        let home = tempfile::tempdir().unwrap();
        let data = home.path().join("data");
        let storage = Storage::open(&data).unwrap();
        let old = home.path().join("old.sqlite3");
        old_database(&old);
        fs::create_dir_all(data.join("backups")).unwrap();
        let backup_id = "11111111111111111111111111111111";
        fs::copy(
            &old,
            data.join(format!("backups/backup-{backup_id}.sqlite3")),
        )
        .unwrap();
        storage.restore_backup(backup_id).unwrap();
        assert_eq!(storage.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(storage.list_protections().unwrap().len(), 1);
        storage
            .with_connection(|db| {
                let title: String = db.query_row(
                    "SELECT title FROM film_manual WHERE film_id='film'",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!(title, "人工标题");
                Ok(())
            })
            .unwrap();
    }
}

const SCHEMA_V14: &str = "ALTER TABLE tags ADD COLUMN position INTEGER NOT NULL DEFAULT 0; UPDATE tags SET position=(SELECT COUNT(*) FROM tags AS preceding WHERE preceding.name < tags.name OR (preceding.name=tags.name AND preceding.id < tags.id));";

const SCHEMA_V15: &str = "CREATE TABLE IF NOT EXISTS application_options(singleton INTEGER PRIMARY KEY CHECK(singleton=1),close_window_exits INTEGER NOT NULL DEFAULT 1 CHECK(close_window_exits IN (0,1)),monitor_libraries INTEGER NOT NULL DEFAULT 0 CHECK(monitor_libraries IN (0,1)));";

const SCHEMA_V23: &str = "
CREATE TABLE IF NOT EXISTS interface_language (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    language TEXT NOT NULL DEFAULT 'zh-CN' CHECK(language IN ('zh-CN','zh-TW','en','ja'))
);
INSERT OR IGNORE INTO interface_language(singleton,language) VALUES(1,'zh-CN');
";

fn ensure_tag_positions(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    let present: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('tags') WHERE name='position')", [], |r| r.get(0))?;
    if !present { tx.execute_batch(SCHEMA_V14)?; }
    Ok(())
}
