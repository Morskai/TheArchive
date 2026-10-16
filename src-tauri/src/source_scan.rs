//! Controlled source preview and confirmation. Only aggregate counts cross into the UI.
use crate::inventory::{AssetKind, InventoryReport, inventory_with_rules};
use crate::observation::{commit_confirmed_inventory, roots_match, roots_overlap};
use crate::storage::Storage;
use crate::storage_session::StorageSession;
use std::collections::BTreeSet;
use serde::Serialize;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Serialize)]
pub struct LibrarySource {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub observation_count: usize,
    pub film_count: usize,
    pub video_bytes: Option<u64>,
    pub last_scanned_at: Option<String>,
    pub enabled: bool,
    pub removed: bool,
}

/// Open only a stored source root. No scan, state change or media write is involved.
pub fn open_source_directory_with(
    storage: &Storage,
    library_id: &str,
    opener: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let root: String = storage.with_connection(|db| {
        db.query_row(
            "SELECT root_path FROM libraries WHERE id=?1 AND removed=0 AND root_path IS NOT NULL",
            [library_id],
            |row| row.get(0),
        )
    }).map_err(|_| "source_not_found")?;
    let path = Path::new(&root);
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "directory_unavailable")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("directory_unavailable".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("directory_unavailable".into());
        }
    }
    let directory = path.canonicalize().map_err(|_| "directory_unavailable")?;
    opener(&directory)
}

pub fn open_source_directory(storage: &Storage, library_id: &str) -> Result<()> {
    open_source_directory_with(storage, library_id, |directory| {
        #[cfg(windows)]
        {
            std::process::Command::new("explorer.exe")
                .arg(directory)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map(|_| ())
                .map_err(|_| "open_failed".into())
        }
        #[cfg(not(windows))]
        {
            let _ = directory;
            Err("open_failed".into())
        }
    })
}

#[derive(Serialize)]
pub struct PreviewSummary {
    pub token: Option<String>,
    pub video_count: usize,
    pub needs_review: usize,
    pub issue_count: usize,
    pub unreadable_directory_count: usize,
    pub complete: bool,
    pub video_directory_count: usize,
    pub image_count: usize,
    pub subtitle_count: usize,
    pub cover_hint_count: usize,
}

#[derive(Serialize)]
pub struct ConfirmSummary {
    pub library_id: String,
    pub discovered: usize,
    pub inserted: usize,
    pub missing_count: usize,
}

pub(crate) struct PendingPreview {
    token: String,
    root: PathBuf,
    mode: PreviewMode,
    report: InventoryReport,
}

enum PreviewMode {
    Add,
    Scan(String),
}

struct PreviewRead {
    root: PathBuf,
    mode: PreviewMode,
    known: BTreeSet<String>,
    ignored: BTreeSet<String>,
}

pub(crate) struct ConfirmationRead {
    pending: PendingPreview,
    known: BTreeSet<String>,
    ignored: BTreeSet<String>,
}

impl ConfirmationRead {
    pub(crate) fn read_with(self, reader: impl FnOnce(&Path, &BTreeSet<String>, &BTreeSet<String>) -> std::io::Result<InventoryReport>) -> Result<PendingPreview> {
        let report = reader(&self.pending.root, &self.known, &self.ignored)
            .map_err(|_| "root_unavailable")?;
        if !report.issues.is_empty() { return Err("incomplete".into()); }
        if report != self.pending.report { return Err("scan_changed".into()); }
        Ok(self.pending)
    }
}

#[derive(Default)]
pub struct SourceScanSession {
    pending: Mutex<Option<PendingPreview>>,
}

impl SourceScanSession {
    pub fn reset(&self) { if let Ok(mut pending) = self.pending.lock() { *pending = None; } }
    pub fn new() -> Self {
        Self::default()
    }

    pub fn preview(
        &self,
        storage: &Storage,
        selected_root: &Path,
        library_id: Option<&str>,
    ) -> Result<PreviewSummary> {
        let read = self.prepare_preview(storage, selected_root, library_id)?;
        let report = inventory_with_rules(&read.root, &read.known, &read.ignored).map_err(|_| "root_unavailable")?;
        self.store_preview(storage, read.root, read.mode, report)
    }

    pub fn preview_on(&self, state: &StorageSession, selected_root: &Path, library_id: Option<&str>) -> Result<PreviewSummary> {
        self.preview_on_with(state, selected_root, library_id, inventory_with_rules)
    }

    /// The reader boundary keeps filesystem enumeration outside the database mutex.
    pub fn preview_on_with(&self, state: &StorageSession, selected_root: &Path, library_id: Option<&str>,
        reader: impl FnOnce(&Path, &BTreeSet<String>, &BTreeSet<String>) -> std::io::Result<InventoryReport>) -> Result<PreviewSummary> {
        self.preview_on_at(state, state.generation(), selected_root, library_id, reader)
    }

    pub(crate) fn preview_on_at(&self, state: &StorageSession, generation: u64, selected_root: &Path, library_id: Option<&str>,
        reader: impl FnOnce(&Path, &BTreeSet<String>, &BTreeSet<String>) -> std::io::Result<InventoryReport>) -> Result<PreviewSummary> {
        let _lease = state.begin_scan_at(generation)?;
        let read = {
            let storage = state.lock().map_err(|_| "storage_unavailable")?;
            self.prepare_preview(&storage, selected_root, library_id)?
        };
        let report = reader(&read.root, &read.known, &read.ignored).map_err(|_| "root_unavailable")?;
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        self.store_preview(&storage, read.root, read.mode, report)
    }

    fn prepare_preview(&self, storage: &Storage, selected_root: &Path, library_id: Option<&str>) -> Result<PreviewRead> {
        self.clear_pending()?;
        purge_legacy_removed_sources(storage)?;
        let sources = source_roots(storage)?;
        let root = canonical_directory(selected_root)?;
        let mode = if let Some(id) = library_id {
            let Some(source) = sources.iter().find(|source| source.id == id) else {
                return Err("wrong_root".into());
            };
            if !roots_match(&source.root, &root) {
                return Err("wrong_root".into());
            }
            if !source.enabled {
                return Err("source_paused".into());
            } else {
                PreviewMode::Scan(id.to_owned())
            }
        } else {
            for source in &sources {
                if !source.removed && roots_overlap(&source.root, &root) {
                    return Err(if roots_match(&source.root, &root) {
                        "already_bound"
                    } else {
                        "range_overlap"
                    }
                    .into());
                }
            }
            PreviewMode::Add
        };
        let known = BTreeSet::new();
        let ignored = crate::scan_rules::keys(storage)?;
        Ok(PreviewRead { root, mode, known, ignored })
    }

    fn clear_pending(&self) -> Result<()> {
        self.pending
            .lock()
            .map_err(|_| "scan session unavailable")?
            .take();
        Ok(())
    }

    fn store_preview(
        &self,
        storage: &Storage,
        root: PathBuf,
        mode: PreviewMode,
        report: InventoryReport,
    ) -> Result<PreviewSummary> {
        // The UI only needs a count, not each video's duplicated list of peers.
        let needs_review = report
            .containers
            .iter()
            .filter(|container| {
                container.video_paths.len() > 1
                    || container.prompts.contains(&crate::inventory::Prompt::ConflictingCodes)
                    || container.prompts.contains(&crate::inventory::Prompt::RepeatedCodeAcrossDirectories)
            })
            .map(|container| container.video_paths.len())
            .sum();
        let complete = report.issues.is_empty();
        let mut summary = PreviewSummary {
            token: None,
            video_count: report.videos.len(),
            needs_review,
            issue_count: report.issues.len(),
            unreadable_directory_count: report
                .issues
                .iter()
                .filter(|issue| issue.kind == "read_directory")
                .count(),
            complete,
            video_directory_count: report
                .videos
                .iter()
                .map(|video| video.directory.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len(),
            image_count: report
                .assets
                .iter()
                .filter(|asset| asset.kind == AssetKind::Image)
                .count(),
            subtitle_count: report
                .assets
                .iter()
                .filter(|asset| asset.kind == AssetKind::Subtitle)
                .count(),
            cover_hint_count: report
                .assets
                .iter()
                .filter(|asset| {
                    asset.kind == AssetKind::Image
                        && asset.relative_path.rsplit('/').next().is_some_and(|name| {
                            crate::artwork::named(name, crate::artwork::FILM_COVER)
                        })
                })
                .count(),
        };
        if complete {
            let token: String = storage.with_connection(|db| {
                db.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
            })?;
            summary.token = Some(token.clone());
            *self
                .pending
                .lock()
                .map_err(|_| "scan session unavailable")? = Some(PendingPreview {
                token,
                root,
                mode,
                report,
            });
        }
        Ok(summary)
    }

    pub fn cancel(&self, token: &str) {
        if let Ok(mut pending) = self.pending.lock() {
            if pending
                .as_ref()
                .is_some_and(|preview| preview.token == token)
            {
                pending.take();
            }
        }
    }

    pub fn confirm(&self, storage: &Storage, token: &str) -> Result<ConfirmSummary> {
        let pending = self.prepare_confirmation(storage, token)?.read_with(inventory_with_rules)?;
        self.commit_confirmation(storage, pending)
    }

    pub(crate) fn prepare_confirmation(&self, storage: &Storage, token: &str) -> Result<ConfirmationRead> {
        let pending = {
            let mut slot = self
                .pending
                .lock()
                .map_err(|_| "scan session unavailable")?;
            if !slot.as_ref().is_some_and(|preview| preview.token == token) {
                return Err("preview_expired".into());
            }
            slot.take().ok_or("preview_expired")?
        };
        let known = BTreeSet::new();
        let ignored = crate::scan_rules::keys(storage)?;
        Ok(ConfirmationRead { pending, known, ignored })
    }

    pub(crate) fn commit_confirmation(&self, storage: &Storage, pending: PendingPreview) -> Result<ConfirmSummary> {
        // A durable application-data snapshot is mandatory before any observation write.
        storage
            .create_protection()
            .map_err(|_| "protection_failed")?;
        let committed = match pending.mode {
            PreviewMode::Add => commit_confirmed_inventory(storage, None, &pending.root, &pending.report),
            PreviewMode::Scan(id) => {
                commit_confirmed_inventory(storage, Some(&id), &pending.root, &pending.report)
            }
        }
        .map_err(|_| "import_failed")?;
        Ok(ConfirmSummary {
            library_id: committed.library_id,
            discovered: committed.import.discovered,
            inserted: committed.import.inserted,
            missing_count: committed.import.missing.len(),
        })
    }
}

pub fn list_sources(storage: &Storage) -> Result<Vec<LibrarySource>> {
    purge_legacy_removed_sources(storage)?;
    Ok(storage.with_connection(|db| {
        let details=crate::artwork::scan_details(db)?;
        let snapshot = crate::artwork::ScanSnapshot::read(db)?;
        let mut statement = db.prepare(
            "SELECT id,display_name,root_path,enabled,removed FROM libraries \
             WHERE root_path IS NOT NULL AND removed=0 AND record_hidden=0 ORDER BY created_at,id",
        )?;
        let mut sources = statement.query_map([], |row| Ok(LibrarySource {
            id: row.get(0)?, name: row.get(1)?, root_path: row.get(2)?, observation_count: 0,
            enabled: row.get::<_, i64>(3)? == 1, removed: row.get::<_, i64>(4)? == 1,
            film_count: 0, video_bytes: None, last_scanned_at: None,
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut counts = std::collections::BTreeMap::<String, (usize, std::collections::BTreeSet<String>)>::new();
        let mut observations = db.prepare("SELECT id,library_id,kind,film_id FROM file_observations")?;
        for row in observations.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?)))? {
            let (id, source, kind, film) = row?;
            if snapshot.contains(&source, &id) {
                let count = counts.entry(source).or_default();
                count.0 += 1;
                if kind == "video" { if let Some(film) = film { count.1.insert(film); } }
            }
        }
        for source in &mut sources {
            if let Some((observations, films)) = counts.get(&source.id) {
                source.observation_count = *observations;
                source.film_count = films.len();
            }
            if let Some(detail) = details.get(&source.id) {
                source.video_bytes = (!detail.video_sizes.is_empty() || source.film_count == 0)
                    .then(|| detail.video_sizes.values().fold(0u64, |total, size| total.saturating_add(*size)));
                source.last_scanned_at = detail.scanned_at.clone();
            }
        }
        Ok(sources)
    })?)
}

struct SourceRoot {
    id: String,
    root: String,
    enabled: bool,
    removed: bool,
}

// A saved binding can be compared even when its disk is disconnected. No filesystem reads.
fn saved_path_key(path: &str) -> String {
    let value = path.replace('\\', "/");
    let value = value
        .strip_prefix("//?/UNC/")
        .map(|v| format!("//{v}"))
        .unwrap_or_else(|| value.strip_prefix("//?/").unwrap_or(&value).to_owned());
    if cfg!(windows) {
        value.trim_end_matches('/').to_lowercase()
    } else {
        value.trim_end_matches('/').to_owned()
    }
}
fn saved_overlap(left: &str, right: &str) -> bool {
    let left = saved_path_key(left);
    let right = saved_path_key(right);
    left == right || left.starts_with(&(right.clone() + "/")) || right.starts_with(&(left + "/"))
}
fn source_roots(storage: &Storage) -> Result<Vec<SourceRoot>> {
    Ok(storage.with_connection(|db| {
        let mut statement = db.prepare(
            "SELECT id, root_path, enabled, removed FROM libraries WHERE root_path IS NOT NULL AND removed=0",
        )?;
        statement
            .query_map([], |row| {
                Ok(SourceRoot {
                    id: row.get(0)?,
                    root: row.get(1)?,
                    enabled: row.get::<_, i64>(2)? == 1,
                    removed: row.get::<_, i64>(3)? == 1,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
    })?)
}

fn canonical_directory(root: &Path) -> Result<PathBuf> {
    let root = root.canonicalize().inspect_err(|error| crate::diagnostics::record_error("scan.inventory", error)).map_err(|_| "root_unavailable")?;
    if !root.is_dir() {
        return Err("root_unavailable".into());
    }
    Ok(root)
}

pub fn change_source_state(storage: &Storage, library_id: &str, action: &str) -> Result<()> {
    if !matches!(action, "pause" | "resume" | "remove") {
        return Err("invalid_action".into());
    }
    let sources = source_roots(storage)?;
    let source = sources
        .iter()
        .find(|source| source.id == library_id)
        .ok_or("source_missing")?;
    let allowed = match action {
        "pause" => source.enabled && !source.removed,
        "resume" => !source.enabled && !source.removed,
        "remove" => !source.removed,
        _ => false,
    };
    if !allowed {
        return Err("source_changed".into());
    }
    if action == "resume"
        && sources.iter().any(|other| {
            other.id != library_id && !other.removed && saved_overlap(&other.root, &source.root)
        })
    {
        return Err("range_overlap".into());
    }
    storage
        .create_protection()
        .map_err(|_| "protection_failed")?;
    storage.with_transaction(|tx| {
        let changed = match action {
            "pause" => tx.execute("UPDATE libraries SET enabled=0 WHERE id=?1 AND enabled=1 AND removed=0 AND root_path=?2", rusqlite::params![library_id, source.root]),
            "resume" => tx.execute("UPDATE libraries SET enabled=1 WHERE id=?1 AND enabled=0 AND removed=0 AND root_path=?2", rusqlite::params![library_id, source.root]),
            "remove" => { remove_source_in_transaction(tx, library_id)?; Ok(1) },
            _ => unreachable!(),
        }?;
        if changed != 1 { return Err(rusqlite::Error::InvalidQuery); }
        if action == "resume" {
            let mut statement = tx.prepare("SELECT root_path FROM libraries WHERE id<>?1 AND removed=0 AND root_path IS NOT NULL")?;
            let others: Vec<String> = statement.query_map([library_id], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
            if others.iter().any(|other| saved_overlap(other, &source.root)) { return Err(rusqlite::Error::InvalidQuery); }
        }
        Ok(())
    }).map_err(|_| "state_failed".into())
}

// Filename/identity evidence needed by manual records belongs to a non-source
// archive with no root. It is never scanned, displayed or used for artwork.
fn remove_source_in_transaction(tx: &rusqlite::Transaction<'_>, id: &str) -> rusqlite::Result<()> {
    const HISTORY: &str = "software-manual-history";
    tx.execute("INSERT OR IGNORE INTO libraries(id,display_name,root_path,enabled,removed,record_hidden) VALUES(?1,'使用资料',NULL,0,1,1)", [HISTORY])?;
    let mut q = tx.prepare("SELECT id,observed_path FROM file_observations WHERE library_id=?1 AND (kind='video' OR id IN (SELECT marker_id FROM actor_directories WHERE marker_id IS NOT NULL) OR id IN (SELECT observation_id FROM player_launch_events WHERE observation_id IS NOT NULL))")?;
    let retained = q.query_map([id], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (observation, path) in retained {
        let filename = path.rsplit(['/', '\\']).next().unwrap_or_default();
        tx.execute("UPDATE file_observations SET library_id=?1,observed_path=?2,file_name=NULL,code_candidate=NULL,created_at=NULL WHERE id=?3", rusqlite::params![HISTORY,filename,observation])?;
    }
    tx.execute("DELETE FROM file_observations WHERE library_id=?1", [id])?;
    tx.execute("DELETE FROM issue_records WHERE (subject_type IN ('cover_scan','library_scan_details') AND subject_id=?1) OR (subject_type IN ('film_missing_cover','film_code_conflict','library_problem_current','library_problem_cleared') AND handled_at IS NULL AND resolution IS NULL AND EXISTS(SELECT 1 FROM json_each(evidence,'$.sources') s WHERE json_extract(s.value,'$.id')=?1))", [id])?;
    if tx.execute("DELETE FROM libraries WHERE id=?1 AND root_path IS NOT NULL", [id])? != 1 { return Err(rusqlite::Error::InvalidQuery); }
    crate::problems::sync(tx)?;
    Ok(())
}
fn purge_legacy_removed_sources(storage: &Storage) -> Result<()> {
    let ids = storage.with_connection(|db| {
        let mut q = db.prepare("SELECT id FROM libraries WHERE removed=1 AND root_path IS NOT NULL")?;
        q.query_map([], |r| r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()
    })?;
    if ids.is_empty() { return Ok(()); }
    storage.create_protection().map_err(|_| "protection_failed")?;
    storage.with_transaction(|tx| { for id in &ids { remove_source_in_transaction(tx,id)?; } Ok(()) }).map_err(|_| "state_failed".into())
}

#[cfg(test)]
mod generation_tests {
    use super::*;
    struct NoKeys;
    impl crate::reset::Credentials for NoKeys {
        fn read(&mut self, _: &str, _: bool) -> std::result::Result<Option<String>, String> { Ok(None) }
        fn save(&mut self, _: &str, _: bool, _: &str) -> std::result::Result<(), String> { Err("synthetic_unexpected_key".into()) }
        fn delete(&mut self, _: &str, _: bool) -> std::result::Result<(), String> { Ok(()) }
    }

    #[test]
    fn source_preview_prepared_before_reset_cannot_scan_or_create_a_new_token() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("synthetic-media");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("movie.mp4"), []).unwrap();
        let state = StorageSession::new(Storage::open(&home.path().join("data")).unwrap());
        let session = SourceScanSession::new();
        // The IPC request has passed ready_storage, but has not acquired its scan lease.
        let generation = state.generation();
        {
            let storage = state.exclusive_for_reset().unwrap();
            crate::reset::restore_initial_state_with(&storage, &mut NoKeys).unwrap();
            state.reset_generation();
            session.reset();
        }
        let mut read = false;
        let result = session.preview_on_at(&state, generation, &root, None, |root, known, ignored| {
            read = true;
            inventory_with_rules(root, known, ignored)
        });
        assert_eq!(result.err().map(|error| error.to_string()), Some("reset_state_changed".into()));
        assert!(!read, "an expired request must be rejected before filesystem access");
        assert!(session.pending.lock().unwrap().is_none());
    }

    #[test]
    fn source_confirmation_prepared_before_restore_cannot_publish_old_inventory() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("synthetic-media");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("movie.mp4"), []).unwrap();
        let state = StorageSession::new(Storage::open(&home.path().join("data")).unwrap());
        let session = SourceScanSession::new();
        let backup = state.lock().unwrap().create_backup().unwrap();
        let preview = session.preview_on(&state, &root, None).unwrap();
        let generation = state.generation();
        state.exclusive_at(generation).unwrap().restore_backup(&backup).unwrap();
        let mut read = false;
        let result = crate::lifecycle::confirm_source_refresh_on_at(&state, generation, &session, &preview.token.unwrap(), |root, known, ignored| {
            read = true;
            inventory_with_rules(root, known, ignored)
        });
        assert_eq!(result.err().map(|error| error.to_string()), Some("reset_state_changed".into()));
        assert!(!read, "an expired confirmation must be rejected before filesystem access");
        assert_eq!(state.lock().unwrap().with_connection(|db| db.query_row("SELECT COUNT(*) FROM libraries WHERE root_path IS NOT NULL", [], |r| r.get::<_, i64>(0))).unwrap(), 0);
    }
}
