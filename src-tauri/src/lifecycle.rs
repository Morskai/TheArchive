//! Application settings, generated cache and user records. Never edits library originals.
use crate::{
    observation::{self, ScanResult},
    storage::Storage,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Mutex};

type Result<T> = std::result::Result<T, String>;

#[derive(Serialize)]
pub struct SourceRefreshResult {
    #[serde(flatten)]
    pub source: crate::source_scan::ConfirmSummary,
    pub related_retained: usize,
    pub recovery_failed: bool,
    pub artwork_failed: usize,
    pub artwork_keys: Vec<String>,
    pub successful_artwork_keys: Vec<String>,
}

fn film_ids(
    storage: &Storage,
) -> std::result::Result<std::collections::BTreeSet<String>, Box<dyn std::error::Error + Send + Sync>>
{
    storage.with_connection(|db| {
        let mut q = db.prepare("SELECT id FROM films")?;
        q.query_map([], |r| r.get(0))?.collect()
    })
}

/// Confirm one source, then verify only occupied historical filename candidates.
/// A completed refusal never leaves a claim that could merge independent films later.
pub fn confirm_source_refresh(
    storage: &Storage,
    session: &crate::source_scan::SourceScanSession,
    token: &str,
) -> std::result::Result<SourceRefreshResult, Box<dyn std::error::Error + Send + Sync>> {
    let before = film_ids(storage)?;
    let old_choices = crate::covers::current_artwork_choices(storage).unwrap_or_default();
    let mut result = SourceRefreshResult {
        source: session.confirm(storage, token)?,
        related_retained: 0,
        recovery_failed: false,
        artwork_failed: 0,
        artwork_keys: Vec::new(),
        successful_artwork_keys: Vec::new(),
    };
    record_successful_artwork(storage, &result.source.library_id.clone(), &mut result);
    // The selected-source transaction has succeeded. Report coordination errors
    // separately instead of claiming that its completed scan was rolled back.
    let coordinated = (|| -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let new_ids = film_ids(storage)?
            .difference(&before)
            .cloned()
            .collect::<Vec<_>>();
        for source in crate::catalog::filename_recovery_sources(storage, &new_ids)? {
            let root: Option<String> = storage.with_connection(|db| {
                db.query_row(
                    "SELECT root_path FROM libraries WHERE id=?1 AND enabled=1 AND removed=0",
                    [&source],
                    |r| r.get(0),
                )
                .optional()
                .map(|row: Option<Option<String>>| row.flatten())
            })?;
            let complete = root.as_deref().is_some_and(|root| {
                matches!(
                    observation::scan_into_library(storage, &source, Path::new(root)),
                    Ok(ScanResult::Complete(_))
                )
            });
            if !complete {
                result.related_retained += 1;
            } else {
                record_successful_artwork(storage, &source, &mut result);
            }
        }
        crate::catalog::reconcile_filename_history(storage, &new_ids)?;
        Ok(())
    })();
    result.recovery_failed = coordinated.is_err();
    finish_artwork_choices(storage, old_choices, &mut result);
    Ok(result)
}

/// Desktop scans hold a lifecycle lease, but release the ordinary command mutex
/// during filesystem reads and derived artwork generation.
pub fn confirm_source_refresh_on(
    state: &crate::storage_session::StorageSession,
    session: &crate::source_scan::SourceScanSession,
    token: &str,
) -> std::result::Result<SourceRefreshResult, Box<dyn std::error::Error + Send + Sync>> {
    confirm_source_refresh_on_with(state, session, token, crate::inventory::inventory_with_rules)
}

pub fn confirm_source_refresh_on_with(
    state: &crate::storage_session::StorageSession,
    session: &crate::source_scan::SourceScanSession,
    token: &str,
    reader: impl FnMut(&Path, &std::collections::BTreeSet<String>, &std::collections::BTreeSet<String>) -> std::io::Result<crate::inventory::InventoryReport>,
) -> std::result::Result<SourceRefreshResult, Box<dyn std::error::Error + Send + Sync>> {
    confirm_source_refresh_on_at(state, state.generation(), session, token, reader)
}

pub(crate) fn confirm_source_refresh_on_at(
    state: &crate::storage_session::StorageSession,
    generation: u64,
    session: &crate::source_scan::SourceScanSession,
    token: &str,
    mut reader: impl FnMut(&Path, &std::collections::BTreeSet<String>, &std::collections::BTreeSet<String>) -> std::io::Result<crate::inventory::InventoryReport>,
) -> std::result::Result<SourceRefreshResult, Box<dyn std::error::Error + Send + Sync>> {
    let _lease = state.begin_scan_at(generation)?;
    let (before, old_choices, read, artwork_storage) = {
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        (film_ids(&storage)?, crate::covers::current_artwork_choices(&storage).unwrap_or_default(), session.prepare_confirmation(&storage, token)?, storage.clone())
    };
    let pending = read.read_with(&mut reader)?;
    let source = {
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        session.commit_confirmation(&storage, pending)?
    };
    let mut result = SourceRefreshResult {
        source, related_retained: 0, recovery_failed: false, artwork_failed: 0, artwork_keys: Vec::new(), successful_artwork_keys: Vec::new(),
    };
    record_successful_artwork(&artwork_storage, &result.source.library_id.clone(), &mut result);
    // A coordination failure does not undo the already committed selected source.
    let coordinated = (|| -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (new_ids, sources) = {
            let storage = state.lock().map_err(|_| "storage_unavailable")?;
            let new_ids = film_ids(&storage)?.difference(&before).cloned().collect::<Vec<_>>();
            let sources = crate::catalog::filename_recovery_sources(&storage, &new_ids)?;
            (new_ids, sources)
        };
        for source in sources {
            let (root, known, ignored) = {
                let storage = state.lock().map_err(|_| "storage_unavailable")?;
                let root: Option<String> = storage.with_connection(|db| {
                    db.query_row("SELECT root_path FROM libraries WHERE id=?1 AND enabled=1 AND removed=0", [&source], |r| r.get(0))
                        .optional().map(|row: Option<Option<String>>| row.flatten())
                })?;
                (root, std::collections::BTreeSet::new(), crate::scan_rules::keys(&storage)?)
            };
            let complete = if let Some(root) = root {
                match reader(Path::new(&root), &known, &ignored) {
                    Ok(report) => {
                        let storage = state.lock().map_err(|_| "storage_unavailable")?;
                        matches!(observation::finish_inventory(&storage, &source, Path::new(&root), report), Ok(ScanResult::Complete(_)))
                    }
                    Err(_) => false,
                }
            } else { false };
            if !complete {
                result.related_retained += 1;
            } else {
                record_successful_artwork(&artwork_storage, &source, &mut result);
            }
        }
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        crate::catalog::reconcile_filename_history(&storage, &new_ids)?;
        Ok(())
    })();
    result.recovery_failed = coordinated.is_err();
    finish_artwork_choices(&artwork_storage, old_choices, &mut result);
    Ok(result)
}

fn record_successful_artwork(storage: &Storage, source: &str, result: &mut SourceRefreshResult) {
    if let Ok(keys) = crate::covers::source_artwork_keys(storage, source) { result.successful_artwork_keys.extend(keys); }
    match crate::covers::refresh_current_source_artwork(storage, source) {
        Ok(keys) => result.artwork_keys.extend(keys), Err(error) => {
            crate::diagnostics::record_code("scan.artwork", &error);
            result.artwork_failed += 1;
        },
    }
}
fn finish_artwork_choices(storage: &Storage, old: std::collections::BTreeMap<String, Option<String>>, result: &mut SourceRefreshResult) {
    if let Ok(current) = crate::covers::current_artwork_choices(storage) {
        for id in &result.successful_artwork_keys {
            if old.get(id) != current.get(id) { result.artwork_keys.push(id.clone()); }
        }
    }
    result.artwork_keys.sort(); result.artwork_keys.dedup();
    result.successful_artwork_keys.sort(); result.successful_artwork_keys.dedup();
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct InterfacePreferences {
    pub reduce_motion: bool,
    pub thin_scrollbar: bool,
    pub film_cover_ratio: String,
    pub actor_photo_ratio: String,
    pub film_cover_fit: String,
    pub actor_photo_fit: String,
}
impl Default for InterfacePreferences {
    fn default() -> Self {
        Self {
            reduce_motion: false,
            thin_scrollbar: true,
            film_cover_ratio: "original".into(),
            actor_photo_ratio: "original".into(),
            film_cover_fit: "cover".into(),
            actor_photo_fit: "cover".into(),
        }
    }
}

pub fn load_preferences(storage: &Storage) -> Result<InterfacePreferences> {
    storage
        .with_connection(|db| {
            db.query_row(
                "SELECT reduce_motion,thin_scrollbar,film_cover_ratio,actor_photo_ratio,film_cover_fit,actor_photo_fit FROM interface_preferences WHERE singleton=1",
                [],
                |r| {
                    Ok(InterfacePreferences {
                        reduce_motion: r.get(0)?,
                        thin_scrollbar: r.get(1)?,
                        film_cover_ratio: r.get(2)?,
                        actor_photo_ratio: r.get(3)?,
                        film_cover_fit: r.get(4)?,
                        actor_photo_fit: r.get(5)?,
                    })
                },
            )
            .optional()
        })
        .map(|row| row.unwrap_or_default())
        .map_err(|_| "preferences_load_failed".into())
}
pub fn save_preferences(storage: &Storage, preferences: &InterfacePreferences) -> Result<()> {
    if !["original", "16:9", "3:2", "5:7"].contains(&preferences.film_cover_ratio.as_str())
        || !["original", "2:3", "3:4"].contains(&preferences.actor_photo_ratio.as_str())
        || !["cover", "contain"].contains(&preferences.film_cover_fit.as_str())
        || !["cover", "contain"].contains(&preferences.actor_photo_fit.as_str())
    {
        return Err("preferences_save_failed".into());
    }
    storage.with_transaction(|tx| {
        tx.execute("INSERT INTO interface_preferences(singleton,reduce_motion,thin_scrollbar,film_cover_ratio,actor_photo_ratio,film_cover_fit,actor_photo_fit) VALUES(1,?1,?2,?3,?4,?5,?6) ON CONFLICT(singleton) DO UPDATE SET reduce_motion=excluded.reduce_motion,thin_scrollbar=excluded.thin_scrollbar,film_cover_ratio=excluded.film_cover_ratio,actor_photo_ratio=excluded.actor_photo_ratio,film_cover_fit=excluded.film_cover_fit,actor_photo_fit=excluded.actor_photo_fit",params![preferences.reduce_motion,preferences.thin_scrollbar,preferences.film_cover_ratio,preferences.actor_photo_ratio,preferences.film_cover_fit,preferences.actor_photo_fit])?;
        Ok(())
    }).map_err(|_|"preferences_save_failed".into())
}

pub fn clear_generated_cache(storage: &Storage) -> Result<()> {
    crate::covers::clear_generated_cache(storage)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupSummary {
    pub saved_sources: usize,
}

/// Startup reads software records only; navigation and restarts never inventory originals.
#[derive(Default)]
pub struct StartupSession {
    result: Mutex<Option<StartupSummary>>,
}
impl StartupSession {
    pub fn reset(&self) { if let Ok(mut result) = self.result.lock() { *result = None; } }
    pub fn initialize(&self, storage: &Storage) -> Result<StartupSummary> {
        let mut result = self.result.lock().map_err(|_| "startup_failed")?;
        if let Some(summary) = &*result {
            return Ok(summary.clone());
        }
        let saved_sources = storage
            .with_connection(|db| {
                db.query_row(
                    "SELECT COUNT(*) FROM libraries WHERE enabled=1 AND removed=0",
                    [],
                    |r| r.get(0),
                )
            })
            .map_err(|_| "startup_failed")?;
        let summary = StartupSummary { saved_sources };
        *result = Some(summary.clone());
        Ok(summary)
    }
}
