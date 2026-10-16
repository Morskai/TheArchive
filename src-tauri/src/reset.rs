//! Restore the application's initial state while retaining complete title memory.
use crate::{storage::Storage, translation_credentials};
use serde::{Deserialize, Serialize};
use std::{fs, fs::OpenOptions, io::Write, path::Path};

type Result<T> = std::result::Result<T, String>;
const PENDING: &str = ".reset-pending";
const PROVIDERS: [&str; 4] = ["deepl", "google", "azure", "baidu"];
const OWNED_DIRECTORIES: [&str; 2] = ["cover-cache", "protections"];

pub trait Credentials {
    fn read(&mut self, provider: &str, protected: bool) -> Result<Option<String>>;
    fn save(&mut self, provider: &str, protected: bool, key: &str) -> Result<()>;
    fn delete(&mut self, provider: &str, protected: bool) -> Result<()>;
}

struct WindowsCredentials;
impl Credentials for WindowsCredentials {
    fn read(&mut self, provider: &str, protected: bool) -> Result<Option<String>> {
        if protected {
            translation_credentials::read_reset_key(provider)
        } else {
            translation_credentials::read_key(provider)
        }
    }
    fn save(&mut self, provider: &str, protected: bool, key: &str) -> Result<()> {
        if protected {
            translation_credentials::save_reset_key(provider, key)
        } else {
            translation_credentials::save_key(provider, key)
        }
    }
    fn delete(&mut self, provider: &str, protected: bool) -> Result<()> {
        if protected {
            translation_credentials::delete_reset_key(provider)
        } else {
            translation_credentials::delete_key(provider)
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    keys_present: [bool; 4],
    directories: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Prepared,
    FilesStaged,
    DatabaseReplaced,
    CredentialsDeleted,
    Committed,
}

/// Caller holds the storage lifecycle lease and the generated-image writer lock.
pub fn restore_initial_state(storage: &Storage) -> Result<()> {
    restore_initial_state_with(storage, &mut WindowsCredentials)
}

/// Injectable credential access keeps synthetic tests away from the user's vault.
pub fn restore_initial_state_with(
    storage: &Storage,
    credentials: &mut impl Credentials,
) -> Result<()> {
    reset_on(storage, credentials, |_| Ok(()))
}

pub fn recovery_pending(root: &Path) -> bool {
    // A permission error must also block commands; exists() would silently ignore it.
    !matches!(fs::symlink_metadata(root.join(PENDING)), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

/// Called before Storage opens a database. Pending rollback/cleanup must finish first.
pub fn recover(root: &Path) -> Result<()> {
    recover_with(root, &mut WindowsCredentials)
}

pub fn recover_with(root: &Path, credentials: &mut impl Credentials) -> Result<()> {
    if !recovery_pending(root) {
        return Ok(());
    }
    check_tree_node(root).map_err(|_| "reset_recovery_required")?;
    let root = root.canonicalize().map_err(|_| "reset_recovery_required")?;
    let stage = root.join(PENDING);
    check_tree(&stage).map_err(|_| "reset_recovery_required")?;
    if stage.join("committed").exists() || stage.join("rolled-back").exists() {
        return finish_cleanup(&stage, credentials).map_err(|_| "reset_recovery_required".into());
    }
    let manifest_path = stage.join("manifest.json");
    if !manifest_path.exists() {
        // No original changes start before the immutable manifest is durably written.
        return finish_cleanup(&stage, credentials).map_err(|_| "reset_recovery_required".into());
    }
    let manifest = read_manifest(&stage)?;
    rollback(&root, &stage, &manifest, credentials).map_err(|_| "reset_recovery_required".into())
}

fn reset_on(
    storage: &Storage,
    credentials: &mut impl Credentials,
    mut after: impl FnMut(Step) -> Result<()>,
) -> Result<()> {
    if recovery_pending(storage.data_directory()) {
        return Err("reset_recovery_required".into());
    }
    check_tree_node(storage.data_directory()).map_err(|_| "reset_failed")?;
    let root = storage
        .data_directory()
        .canonicalize()
        .map_err(|_| "reset_failed")?;
    check_tree(&root.join("library.sqlite3")).map_err(|_| "reset_failed")?;
    let mut directories = Vec::new();
    for name in OWNED_DIRECTORIES {
        let path = root.join(name);
        if path.try_exists().map_err(|_| "reset_failed")? {
            check_owned_directory(&path, name).map_err(|_| "reset_failed")?;
            directories.push(name.to_owned());
        }
    }
    let mut keys = Vec::new();
    for provider in PROVIDERS {
        if credentials
            .read(provider, true)
            .map_err(|_| "reset_credentials_failed")?
            .is_some()
        {
            return Err("reset_credentials_failed".into());
        }
        keys.push(
            credentials
                .read(provider, false)
                .map_err(|_| "reset_credentials_failed")?,
        );
    }
    // The current database is protected before any replacement or credential deletion.
    let protection = storage
        .create_protection()
        .map_err(|_| "protection_failed")?;
    if !directories.iter().any(|name| name == "protections") {
        directories.push("protections".into());
    }
    let stage = root.join(PENDING);
    fs::create_dir(&stage).map_err(|_| "reset_failed")?;
    let manifest = Manifest {
        keys_present: std::array::from_fn(|i| keys[i].is_some()),
        directories,
    };
    let work = (|| -> Result<()> {
        write_synced(
            &stage.join("manifest.tmp"),
            &serde_json::to_vec(&manifest).map_err(|_| "reset_failed")?,
        )?;
        fs::rename(stage.join("manifest.tmp"), stage.join("manifest.json"))
            .map_err(|_| "reset_failed")?;
        for (provider, key) in PROVIDERS.iter().zip(&keys) {
            if let Some(key) = key {
                credentials
                    .save(provider, true, key)
                    .map_err(|_| "reset_credentials_failed")?;
                if credentials
                    .read(provider, true)
                    .map_err(|_| "reset_credentials_failed")?
                    .as_ref()
                    != Some(key)
                {
                    return Err("reset_credentials_failed".into());
                }
            }
        }
        let next = stage.join("next.sqlite3");
        fs::copy(
            root.join(format!("protections/protection-{protection}.sqlite3")),
            &next,
        )
        .map_err(|_| "reset_failed")?;
        clear_staged_database(&next)?;
        sync_file(&next)?;
        write_synced(&stage.join("prepared"), b"1")?;
        after(Step::Prepared)?;
        for name in &manifest.directories {
            fs::rename(root.join(name), stage.join(name)).map_err(|_| "reset_failed")?;
        }
        after(Step::FilesStaged)?;
        fs::rename(root.join("library.sqlite3"), stage.join("original.sqlite3"))
            .map_err(|_| "reset_failed")?;
        fs::rename(&next, root.join("library.sqlite3")).map_err(|_| "reset_failed")?;
        sync_file(&root.join("library.sqlite3"))?;
        after(Step::DatabaseReplaced)?;
        for provider in PROVIDERS {
            credentials
                .delete(provider, false)
                .map_err(|_| "reset_credentials_failed")?;
            if credentials
                .read(provider, false)
                .map_err(|_| "reset_credentials_failed")?
                .is_some()
            {
                return Err("reset_credentials_failed".into());
            }
        }
        after(Step::CredentialsDeleted)?;
        write_synced(&stage.join("committed"), b"1")?;
        after(Step::Committed)?;
        Ok(())
    })();
    // The durable commit marker is the boundary. After it, interrupted cleanup
    // resumes cleanup rather than restoring a partly deleted rollback snapshot.
    if stage.join("committed").exists() {
        if work.is_err() || finish_cleanup(&stage, credentials).is_err() {
            return Err("reset_cleanup_pending".into());
        }
        return Ok(());
    }
    if let Err(error) = work {
        if rollback(&root, &stage, &manifest, credentials).is_err() {
            return Err("reset_recovery_required".into());
        }
        return Err(if error == "reset_credentials_failed" {
            error
        } else {
            "reset_failed".into()
        });
    }
    Err("reset_recovery_required".into())
}

fn clear_staged_database(path: &Path) -> Result<()> {
    let mut db = rusqlite::Connection::open(path).map_err(|_| "reset_failed")?;
    // Disable FK deletion order constraints on this isolated staged copy only.
    // Every non-memory table is emptied, then singleton defaults are reseeded.
    db.pragma_update(None, "foreign_keys", "OFF")
        .map_err(|_| "reset_failed")?;
    db.pragma_update(None, "secure_delete", "ON")
        .map_err(|_| "reset_failed")?;
    let tx = db.transaction().map_err(|_| "reset_failed")?;
    let tables: Vec<String> = {
        let mut query = tx.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name<>'translation_memory'").map_err(|_| "reset_failed")?;
        query
            .query_map([], |row| row.get(0))
            .map_err(|_| "reset_failed")?
            .collect::<rusqlite::Result<_>>()
            .map_err(|_| "reset_failed")?
    };
    for table in tables {
        tx.execute(
            &format!("DELETE FROM \"{}\"", table.replace('"', "\"\"")),
            [],
        )
        .map_err(|_| "reset_failed")?;
    }
    tx.execute("INSERT INTO interface_language(singleton,language) VALUES(1,'zh-CN')", [])
        .map_err(|_| "reset_failed")?;
    tx.commit().map_err(|_| "reset_failed")?;
    // Old manual records/photos must not remain as reusable free pages in the
    // new database. The independent translation_memory rows are never rewritten.
    db.execute_batch("VACUUM").map_err(|_| "reset_failed")?;
    let integrity: String = db
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|_| "reset_failed")?;
    if integrity != "ok"
        || db
            .prepare("PRAGMA foreign_key_check")
            .map_err(|_| "reset_failed")?
            .query([])
            .map_err(|_| "reset_failed")?
            .next()
            .map_err(|_| "reset_failed")?
            .is_some()
    {
        return Err("reset_failed".into());
    }
    Ok(())
}

fn rollback(
    root: &Path,
    stage: &Path,
    manifest: &Manifest,
    credentials: &mut impl Credentials,
) -> Result<()> {
    check_tree(stage)?;
    if stage.join("prepared").exists() {
        for (provider, present) in PROVIDERS.iter().zip(manifest.keys_present) {
            if present {
                let key = credentials
                    .read(provider, true)?
                    .ok_or("reset_recovery_required")?;
                credentials.save(provider, false, &key)?;
                if credentials.read(provider, false)?.as_ref() != Some(&key) {
                    return Err("reset_recovery_required".into());
                }
            } else {
                credentials.delete(provider, false)?;
                if credentials.read(provider, false)?.is_some() {
                    return Err("reset_recovery_required".into());
                }
            }
        }
    }
    let original = stage.join("original.sqlite3");
    if original.exists() {
        let replacement = stage.join("recover.sqlite3");
        fs::copy(&original, &replacement).map_err(|_| "reset_recovery_required")?;
        sync_file(&replacement)?;
        let current = root.join("library.sqlite3");
        if current.exists() {
            check_tree(&current)?;
            fs::remove_file(&current).map_err(|_| "reset_recovery_required")?;
        }
        fs::rename(replacement, current).map_err(|_| "reset_recovery_required")?;
    }
    for name in &manifest.directories {
        let source = stage.join(name);
        let destination = root.join(name);
        if source.exists() {
            if destination.exists() {
                return Err("reset_recovery_required".into());
            }
            fs::rename(source, destination).map_err(|_| "reset_recovery_required")?;
        } else if !destination.exists() {
            return Err("reset_recovery_required".into());
        }
    }
    write_synced(&stage.join("rolled-back"), b"1")?;
    finish_cleanup(stage, credentials)
}

fn finish_cleanup(stage: &Path, credentials: &mut impl Credentials) -> Result<()> {
    check_tree(stage)?;
    for provider in PROVIDERS {
        credentials.delete(provider, true)?;
    }
    // Keep the state markers until the last data-bearing file has gone. A crash
    // during this loop can safely retry without guessing which data was deleted.
    for entry in fs::read_dir(stage).map_err(|_| "reset_recovery_required")? {
        let entry = entry.map_err(|_| "reset_recovery_required")?;
        if ["manifest.json", "prepared", "committed", "rolled-back"]
            .iter()
            .any(|name| entry.file_name() == *name)
        {
            continue;
        }
        if entry
            .file_type()
            .map_err(|_| "reset_recovery_required")?
            .is_dir()
        {
            fs::remove_dir_all(entry.path()).map_err(|_| "reset_recovery_required")?;
        } else {
            fs::remove_file(entry.path()).map_err(|_| "reset_recovery_required")?;
        }
    }
    for name in ["manifest.json", "prepared", "rolled-back", "committed"] {
        let path = stage.join(name);
        if path.exists() {
            fs::remove_file(path).map_err(|_| "reset_recovery_required")?;
        }
    }
    fs::remove_dir(stage).map_err(|_| "reset_recovery_required".into())
}

fn read_manifest(stage: &Path) -> Result<Manifest> {
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(stage.join("manifest.json")).map_err(|_| "reset_recovery_required")?,
    )
    .map_err(|_| "reset_recovery_required")?;
    if manifest
        .directories
        .iter()
        .any(|name| !OWNED_DIRECTORIES.contains(&name.as_str()))
        || manifest.directories.len() > 2
    {
        return Err("reset_recovery_required".into());
    }
    Ok(manifest)
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "reset_failed")?;
    file.write_all(bytes).map_err(|_| "reset_failed")?;
    file.sync_all().map_err(|_| "reset_failed".into())
}
fn sync_file(path: &Path) -> Result<()> {
    OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|_| "reset_failed")?
        .sync_all()
        .map_err(|_| "reset_failed".into())
}
fn check_tree_node(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "reset_failed")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("reset_failed".into());
        }
    }
    if metadata.file_type().is_symlink() || !(metadata.is_file() || metadata.is_dir()) {
        return Err("reset_failed".into());
    }
    Ok(())
}
fn check_tree(path: &Path) -> Result<()> {
    check_tree_node(path)?;
    if path.canonicalize().map_err(|_| "reset_failed")? != path {
        return Err("reset_failed".into());
    }
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|_| "reset_failed")? {
            check_tree(&entry.map_err(|_| "reset_failed")?.path())?;
        }
    }
    Ok(())
}
fn check_owned_directory(path: &Path, name: &str) -> Result<()> {
    check_tree(path)?;
    if !path.is_dir() {
        return Err("reset_failed".into());
    }
    for entry in fs::read_dir(path).map_err(|_| "reset_failed")? {
        let entry = entry.map_err(|_| "reset_failed")?;
        let filename = entry.file_name();
        let filename = filename.to_str().ok_or("reset_failed")?;
        let owned = if name == "protections" {
            [("protection-", ".sqlite3"), ("snapshot-", ".tmp")]
                .iter()
                .any(|(prefix, suffix)| {
                    filename
                        .strip_prefix(prefix)
                        .and_then(|body| body.strip_suffix(suffix))
                        .is_some_and(|id| {
                            id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
                        })
                })
        } else {
            crate::covers::generated_name(filename)
        };
        if !owned || !entry.file_type().map_err(|_| "reset_failed")?.is_file() {
            return Err("reset_failed".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{catalog, translation};
    use std::{collections::BTreeMap, fs};

    #[derive(Default)]
    struct FakeCredentials {
        keys: BTreeMap<(String, bool), String>,
        fail_delete: Option<(String, bool)>,
        fail_cleanup: Option<String>,
        fail_save: Option<(String, bool)>,
        fail_read: Option<(String, bool)>,
    }
    impl Credentials for FakeCredentials {
        fn read(&mut self, provider: &str, protected: bool) -> Result<Option<String>> {
            if self.fail_read.as_ref() == Some(&(provider.into(), protected)) {
                self.fail_read = None;
                return Err("synthetic_credentials_failure".into());
            }
            Ok(self.keys.get(&(provider.into(), protected)).cloned())
        }
        fn save(&mut self, provider: &str, protected: bool, key: &str) -> Result<()> {
            if self.fail_save.as_ref() == Some(&(provider.into(), protected)) {
                self.fail_save = None;
                return Err("synthetic_credentials_failure".into());
            }
            self.keys.insert((provider.into(), protected), key.into());
            Ok(())
        }
        fn delete(&mut self, provider: &str, protected: bool) -> Result<()> {
            if protected && self.fail_cleanup.as_deref() == Some(provider) {
                self.fail_cleanup = None;
                return Err("synthetic_credentials_failure".into());
            }
            if self.fail_delete.as_ref() == Some(&(provider.into(), protected)) {
                self.fail_delete = None;
                return Err("synthetic_credentials_failure".into());
            }
            self.keys.remove(&(provider.into(), protected));
            Ok(())
        }
    }

    fn fixture() -> (tempfile::TempDir, Storage, FakeCredentials, Vec<String>) {
        let home = tempfile::tempdir().unwrap();
        let storage = Storage::open(&home.path().join("software")).unwrap();
        storage.with_connection(|db| {
            db.execute_batch("INSERT INTO libraries(id,display_name,root_path) VALUES('source','合成片库','X:/synthetic');
                INSERT INTO films(id) VALUES('film');
                INSERT INTO films(id) VALUES('alias');
                INSERT INTO film_aliases(id,film_id) VALUES('alias','film');
                INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('video','source','film','[TEST-001] 放課後の先生/video.mp4','video');
                INSERT INTO film_manual(film_id,title,rating,favorite,cover_choice,cast_override) VALUES('film','人工片名',8,1,'legacy',1);
                INSERT INTO actors(id,observed_name) VALUES('actor','合成演员');
                INSERT INTO actor_manual(actor_id,favorite,height_cm,aliases) VALUES('actor',1,165,'[\"合成别名\"]');
                INSERT INTO actor_image_sources(actor_id,path,fingerprint) VALUES('actor','X:/synthetic-photo.jpg','synthetic-photo');
                INSERT INTO actor_tags(name,position) VALUES('合成演员标签',0);
                INSERT INTO actor_tag_links(actor_id,tag) VALUES('actor','合成演员标签');
                INSERT INTO actor_directories(actor_id,directory_key,marker_id,directory_name) VALUES('actor','legacy-path','video','legacy');
                INSERT INTO info_actor_links(external_id,actor_id) VALUES('legacy-external','actor');
                INSERT INTO film_actors(film_id,actor_id,source) VALUES('film','actor','manual');
                INSERT INTO tags(id,name) VALUES('tag','合成标签');
                INSERT INTO film_tags(film_id,tag_id) VALUES('film','tag');
                INSERT INTO file_versions(observation_id,preferred) VALUES('video',1);
                INSERT INTO player_launch_events(id,film_id,observation_id) VALUES('event','film','video');
                INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence,resolution) VALUES('issue','film','film','synthetic','legacy-path','marked');
                INSERT INTO player_settings(singleton,mode,program,arguments) VALUES(1,'custom','X:/synthetic.exe','--synthetic');
                INSERT INTO interface_preferences(singleton,reduce_motion,thin_scrollbar,film_cover_ratio,actor_photo_ratio,film_cover_fit,actor_photo_fit) VALUES(1,1,0,'5:7','3:4','contain','contain');
                INSERT INTO application_options(singleton,close_window_exits,monitor_libraries) VALUES(1,0,1);
                INSERT INTO ignored_directory_names(position,name,name_key) VALUES(0,'合成忽略','合成忽略');
                INSERT INTO translation_settings(singleton,enabled,provider,azure_region,baidu_app_id) VALUES(1,1,'azure','synthetic','123');")?;
            Ok(())
        }).unwrap();
        let prepared = translation::prepare_translation(&storage, "film", false)
            .unwrap()
            .unwrap();
        translation::save_translation(&storage, &prepared, "google", "完整的合成译名", "JA")
            .unwrap();
        storage.with_connection(|db| {
            db.execute_batch("INSERT INTO translation_memory(id,identity_anchor,normalized_code,original_title,normalized_title,body,fingerprint,source_language,translated_title,provider,translated_at,last_matched_at)
                VALUES('manual-memory','unobserved-anchor','','保留原文','保留规范','保留正文','保留指纹','JA','保留人工标题','user','2026-01-02T03:04:05Z','2026-02-03T04:05:06Z');")
        }).unwrap();
        let memory = memory_rows(&storage);
        fs::create_dir_all(home.path().join("external-media")).unwrap();
        fs::create_dir_all(storage.data_directory().join("cover-cache")).unwrap();
        fs::write(
            storage.data_directory().join("cover-cache/index-v1.json"),
            b"synthetic cache",
        )
        .unwrap();
        fs::create_dir_all(storage.data_directory().join("ActorExports/synthetic")).unwrap();
        fs::write(
            storage
                .data_directory()
                .join("ActorExports/synthetic/actor.info"),
            b"synthetic exported actor info",
        )
        .unwrap();
        fs::write(
            storage
                .data_directory()
                .join("ActorExports/synthetic/actor.jpg"),
            b"synthetic exported actor image",
        )
        .unwrap();
        fs::write(
            home.path().join("external-media/video.mp4"),
            b"synthetic media",
        )
        .unwrap();
        fs::write(
            home.path().join("external-media/actor.info"),
            b"synthetic actor info",
        )
        .unwrap();
        storage.create_backup().unwrap();
        storage.create_protection().unwrap();
        let mut credentials = FakeCredentials::default();
        credentials.save("deepl", false, "fake-deepl-key").unwrap();
        credentials
            .save("google", false, "fake-google-key")
            .unwrap();
        (home, storage, credentials, memory)
    }

    fn memory_rows(storage: &Storage) -> Vec<String> {
        storage.with_connection(|db| {
            let mut query = db.prepare("SELECT json_array(id,identity_anchor,normalized_code,original_title,normalized_title,body,fingerprint,source_language,translated_title,provider,translated_at,last_matched_at) FROM translation_memory ORDER BY id")?;
            query.query_map([], |row| row.get(0))?.collect()
        }).unwrap()
    }

    #[test]
    fn reset_keeps_every_memory_value_but_removes_all_other_software_records() {
        let (home, storage, mut credentials, memory) = fixture();
        let backup = storage.list_backups().unwrap().remove(0);
        let backup_path = storage
            .data_directory()
            .join(format!("backups/backup-{}.sqlite3", backup.id));
        let backup_bytes = fs::read(&backup_path).unwrap();
        restore_initial_state_with(&storage, &mut credentials).unwrap();
        assert_eq!(memory_rows(&storage), memory);
        storage.with_connection(|db| {
            let mut query = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT IN ('translation_memory','interface_language')")?;
            let tables: Vec<String> = query.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
            for table in tables {
                assert_eq!(db.query_row::<i64, _, _>(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))?, 0, "{table}");
            }
            assert!(db.prepare("PRAGMA foreign_key_check")?.query([])?.next()?.is_none());
            Ok(())
        }).unwrap();
        assert_eq!(crate::interface_language::load(&storage).unwrap(), "zh-CN");
        assert!(credentials.keys.is_empty());
        assert_eq!(fs::read(backup_path).unwrap(), backup_bytes);
        assert_eq!(
            fs::read(home.path().join("external-media/video.mp4")).unwrap(),
            b"synthetic media"
        );
        assert_eq!(
            fs::read(home.path().join("external-media/actor.info")).unwrap(),
            b"synthetic actor info"
        );
        assert_eq!(
            fs::read(
                storage
                    .data_directory()
                    .join("ActorExports/synthetic/actor.info")
            )
            .unwrap(),
            b"synthetic exported actor info"
        );
        assert_eq!(
            fs::read(
                storage
                    .data_directory()
                    .join("ActorExports/synthetic/actor.jpg")
            )
            .unwrap(),
            b"synthetic exported actor image"
        );
        assert!(!storage.data_directory().join("cover-cache").exists());
        assert!(!storage.data_directory().join("protections").exists());
        let defaults = crate::lifecycle::load_preferences(&storage).unwrap();
        assert_eq!(defaults, crate::lifecycle::InterfacePreferences::default());
        let settings = translation::load_settings(&storage).unwrap();
        assert!(!settings.enabled && settings.provider == "deepl");
        assert_eq!(settings.deepl_plan, "free");
        assert_eq!(settings.target_language, "ZH-HANS");
        assert!(settings.azure_region.is_empty() && settings.baidu_app_id.is_empty());
        assert_eq!(
            crate::playback::load_settings(&storage).unwrap(),
            crate::playback::PlayerSettings::default()
        );
        assert!(crate::scan_rules::load(&storage).unwrap().is_empty());
        assert_eq!(
            crate::application_options::load(&storage).unwrap(),
            crate::application_options::ApplicationOptions::default()
        );
        storage.with_connection(|db| {
            db.execute_batch("INSERT INTO libraries(id,display_name,root_path) VALUES('new-source','再次导入','X:/new'); INSERT INTO films(id) VALUES('new-film'); INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('new-video','new-source','new-film','[TEST-001] 放課後の先生/new.mp4','video');")
        }).unwrap();
        assert_eq!(
            catalog::list_films(&storage).unwrap().remove(0).title,
            "完整的合成译名"
        );
        assert!(
            translation::prepare_translation(&storage, "new-film", false)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn credential_failure_restores_original_keys_database_and_owned_files() {
        let (_home, storage, mut credentials, memory) = fixture();
        credentials.fail_delete = Some(("google".into(), false));
        let keys = credentials.keys.clone();
        assert!(restore_initial_state_with(&storage, &mut credentials).is_err());
        assert_eq!(credentials.keys, keys);
        assert_eq!(memory_rows(&storage), memory);
        assert!(catalog::list_films(&storage).unwrap()[0].favorite);
        assert!(
            storage
                .data_directory()
                .join("cover-cache/index-v1.json")
                .exists()
        );
        assert!(storage.data_directory().join("protections").exists());
    }

    #[test]
    fn protection_and_staged_database_failures_keep_every_original_byte() {
        for protection_failure in [true, false] {
            let (_home, storage, mut credentials, memory) = fixture();
            if protection_failure {
                fs::remove_dir_all(storage.data_directory().join("protections")).unwrap();
                fs::write(
                    storage.data_directory().join("protections"),
                    b"synthetic blocker",
                )
                .unwrap();
            } else {
                storage.with_connection(|db| db.execute_batch("CREATE TRIGGER synthetic_reject_reset BEFORE DELETE ON actors BEGIN SELECT RAISE(ABORT,'synthetic reset failure'); END;")).unwrap();
            }
            let bytes = fs::read(storage.data_directory().join("library.sqlite3")).unwrap();
            let keys = credentials.keys.clone();
            assert!(restore_initial_state_with(&storage, &mut credentials).is_err());
            assert_eq!(
                fs::read(storage.data_directory().join("library.sqlite3")).unwrap(),
                bytes
            );
            assert_eq!(memory_rows(&storage), memory);
            assert_eq!(credentials.keys, keys);
            assert!(!recovery_pending(storage.data_directory()));
        }
    }

    #[test]
    fn credential_read_or_protection_failure_never_changes_user_records_or_keys() {
        for protect_failure in [true, false] {
            let (_home, storage, mut credentials, memory) = fixture();
            let bytes = fs::read(storage.data_directory().join("library.sqlite3")).unwrap();
            let keys = credentials.keys.clone();
            if protect_failure {
                credentials.fail_save = Some(("google".into(), true));
            } else {
                credentials.fail_read = Some(("google".into(), false));
            }
            assert_eq!(
                restore_initial_state_with(&storage, &mut credentials).unwrap_err(),
                "reset_credentials_failed"
            );
            assert_eq!(
                fs::read(storage.data_directory().join("library.sqlite3")).unwrap(),
                bytes
            );
            assert_eq!(memory_rows(&storage), memory);
            assert_eq!(credentials.keys, keys);
            assert!(!recovery_pending(storage.data_directory()));
        }
    }

    #[test]
    fn interrupted_uncommitted_phases_restore_complete_database_keys_and_files() {
        for interrupted in [
            Step::Prepared,
            Step::FilesStaged,
            Step::DatabaseReplaced,
            Step::CredentialsDeleted,
        ] {
            let (_home, storage, mut credentials, memory) = fixture();
            let bytes = fs::read(storage.data_directory().join("library.sqlite3")).unwrap();
            let keys = credentials.keys.clone();
            let crash = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = reset_on(&storage, &mut credentials, |step| {
                    if step == interrupted {
                        panic!("synthetic process interruption");
                    }
                    Ok(())
                });
            }));
            assert!(crash.is_err());
            assert!(recovery_pending(storage.data_directory()));
            recover_with(storage.data_directory(), &mut credentials).unwrap();
            assert_eq!(
                fs::read(storage.data_directory().join("library.sqlite3")).unwrap(),
                bytes
            );
            assert_eq!(memory_rows(&storage), memory);
            assert_eq!(credentials.keys, keys);
            assert!(
                storage
                    .data_directory()
                    .join("cover-cache/index-v1.json")
                    .exists()
            );
            assert!(!recovery_pending(storage.data_directory()));
        }
    }

    #[test]
    fn failed_rollback_keeps_recovery_journal_and_can_resume_without_real_credentials() {
        let (_home, storage, mut credentials, memory) = fixture();
        let bytes = fs::read(storage.data_directory().join("library.sqlite3")).unwrap();
        let keys = credentials.keys.clone();
        credentials.fail_delete = Some(("google".into(), false));
        credentials.fail_save = Some(("google".into(), false));
        assert_eq!(
            restore_initial_state_with(&storage, &mut credentials).unwrap_err(),
            "reset_recovery_required"
        );
        assert!(recovery_pending(storage.data_directory()));
        recover_with(storage.data_directory(), &mut credentials).unwrap();
        assert_eq!(
            fs::read(storage.data_directory().join("library.sqlite3")).unwrap(),
            bytes
        );
        assert_eq!(memory_rows(&storage), memory);
        assert_eq!(credentials.keys, keys);
        assert!(!recovery_pending(storage.data_directory()));
    }

    #[test]
    fn committed_cleanup_failure_and_interruption_continue_cleanup_without_restoring_old_data() {
        for interruption in [false, true] {
            let (_home, storage, mut credentials, memory) = fixture();
            if interruption {
                let crash = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = reset_on(&storage, &mut credentials, |step| {
                        if step == Step::Committed {
                            panic!("synthetic process interruption");
                        }
                        Ok(())
                    });
                }));
                assert!(crash.is_err());
            } else {
                credentials.fail_delete = Some(("google".into(), true));
                assert_eq!(
                    restore_initial_state_with(&storage, &mut credentials).unwrap_err(),
                    "reset_cleanup_pending"
                );
            }
            assert!(recovery_pending(storage.data_directory()));
            assert_eq!(memory_rows(&storage), memory);
            assert!(catalog::list_films(&storage).unwrap().is_empty());
            for provider in PROVIDERS {
                assert_eq!(credentials.read(provider, false).unwrap(), None);
            }
            recover_with(storage.data_directory(), &mut credentials).unwrap();
            assert!(catalog::list_films(&storage).unwrap().is_empty());
            assert_eq!(memory_rows(&storage), memory);
            assert!(credentials.keys.is_empty());
            assert!(!recovery_pending(storage.data_directory()));
            assert!(!storage.data_directory().join("cover-cache").exists());
            assert!(!storage.data_directory().join("protections").exists());
        }
    }

    #[test]
    fn cleanup_failure_after_rollback_keeps_original_state_and_retries_cleanup_only() {
        let (_home, storage, mut credentials, memory) = fixture();
        let bytes = fs::read(storage.data_directory().join("library.sqlite3")).unwrap();
        let keys = credentials.keys.clone();
        credentials.fail_delete = Some(("google".into(), false));
        credentials.fail_cleanup = Some("google".into());
        assert_eq!(
            restore_initial_state_with(&storage, &mut credentials).unwrap_err(),
            "reset_recovery_required"
        );
        assert!(recovery_pending(storage.data_directory()));
        assert_eq!(
            fs::read(storage.data_directory().join("library.sqlite3")).unwrap(),
            bytes
        );
        assert_eq!(memory_rows(&storage), memory);
        for (key, value) in &keys {
            assert_eq!(credentials.keys.get(key), Some(value));
        }
        recover_with(storage.data_directory(), &mut credentials).unwrap();
        assert_eq!(credentials.keys, keys);
        assert!(!recovery_pending(storage.data_directory()));
    }

    #[test]
    fn unpublished_manifest_interruption_and_unknown_maintenance_files_keep_user_data() {
        let (_home, storage, mut credentials, memory) = fixture();
        let stage = storage.data_directory().join(PENDING);
        fs::create_dir(&stage).unwrap();
        fs::write(
            stage.join("manifest.tmp"),
            b"interrupted incomplete metadata",
        )
        .unwrap();
        let keys = credentials.keys.clone();
        recover_with(storage.data_directory(), &mut credentials).unwrap();
        assert_eq!(credentials.keys, keys);
        assert_eq!(memory_rows(&storage), memory);
        let unknown = storage
            .data_directory()
            .join("protections/user-backup.sqlite3");
        fs::write(&unknown, b"synthetic user backup").unwrap();
        assert!(restore_initial_state_with(&storage, &mut credentials).is_err());
        assert_eq!(fs::read(unknown).unwrap(), b"synthetic user backup");
        assert_eq!(credentials.keys, keys);
        assert_eq!(memory_rows(&storage), memory);
        assert!(!recovery_pending(storage.data_directory()));
    }

    #[cfg(windows)]
    #[test]
    fn locked_committed_staging_file_reports_pending_cleanup_and_retries() {
        use std::os::windows::fs::OpenOptionsExt;
        let (_home, storage, mut credentials, memory) = fixture();
        let mut held = None;
        let root = storage.data_directory().canonicalize().unwrap();
        let error = reset_on(&storage, &mut credentials, |step| {
            if step == Step::Committed {
                held = Some(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(3)
                        .open(root.join(".reset-pending/cover-cache/index-v1.json"))
                        .unwrap(),
                );
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error, "reset_cleanup_pending");
        assert!(recovery_pending(storage.data_directory()));
        assert!(catalog::list_films(&storage).unwrap().is_empty());
        assert_eq!(memory_rows(&storage), memory);
        drop(held);
        recover_with(storage.data_directory(), &mut credentials).unwrap();
        assert!(!recovery_pending(storage.data_directory()));
        assert!(credentials.keys.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn redirected_owned_directories_fail_before_mutation_and_preserve_external_files() {
        for name in OWNED_DIRECTORIES {
            let (home, storage, mut credentials, memory) = fixture();
            let directory = storage.data_directory().join(name);
            fs::remove_dir_all(&directory).unwrap();
            let external = home.path().join("external-media");
            let status = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&directory)
                .arg(&external)
                .output()
                .unwrap();
            assert!(
                status.status.success(),
                "synthetic junction could not be created"
            );
            let bytes = fs::read(storage.data_directory().join("library.sqlite3")).unwrap();
            let keys = credentials.keys.clone();
            assert!(restore_initial_state_with(&storage, &mut credentials).is_err());
            assert_eq!(
                fs::read(storage.data_directory().join("library.sqlite3")).unwrap(),
                bytes
            );
            assert_eq!(memory_rows(&storage), memory);
            assert_eq!(credentials.keys, keys);
            assert_eq!(
                fs::read(external.join("video.mp4")).unwrap(),
                b"synthetic media"
            );
            fs::remove_dir(directory).unwrap();
        }
    }
}
