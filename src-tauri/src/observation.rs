//! Import of a complete inventory into one explicitly chosen library.
//! The command layer owns confirmation; complete imports attach conservative video entries.
#[cfg(test)]
use crate::inventory::inventory;
use crate::inventory::{AssetKind, InventoryReport, ScanIssue};
use crate::storage::Storage;
use rusqlite::{Transaction, params};
use std::collections::HashSet;
use std::error::Error;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, PartialEq, Eq)]
pub struct ObservationImport {
    pub discovered: usize,
    pub inserted: usize,
    /// Ephemeral evidence only. No saved relationship is changed.
    pub missing: Vec<MissingObservation>,
}

#[derive(Debug)]
pub struct CommittedObservation {
    pub library_id: String,
    pub import: ObservationImport,
}

#[derive(Debug, PartialEq, Eq)]
pub struct MissingObservation {
    pub observation_id: String,
    pub observed_path: String,
    pub kind: String,
}

#[derive(Debug)]
pub enum ScanResult {
    RootUnavailable { detail: String },
    Incomplete { issues: Vec<ScanIssue> },
    WrongRoot,
    Complete(ObservationImport),
}

pub fn scan_into_library(storage: &Storage, library_id: &str, root: &Path) -> Result<ScanResult> {
    // Check the explicit binding before touching the candidate directory.
    let binding: (String, i64, i64) = storage.with_connection(|db| {
        db.query_row(
            "SELECT root_path, enabled, removed FROM libraries WHERE id=?1",
            [library_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
    })?;
    if !root.is_absolute() || !roots_match(&binding.0, root) || binding.1 != 1 || binding.2 != 0 {
        return Ok(ScanResult::WrongRoot);
    }
    let known = std::collections::BTreeSet::new();
    let ignored = crate::scan_rules::keys(storage)?;
    let report = match crate::inventory::inventory_with_rules(root, &known, &ignored) {
        Ok(report) => report,
        Err(error) => {
            return Ok(ScanResult::RootUnavailable {
                detail: error.to_string(),
            });
        }
    };
    finish_inventory(storage, library_id, root, report)
}

pub(crate) fn finish_inventory(
    storage: &Storage,
    library_id: &str,
    root: &Path,
    report: InventoryReport,
) -> Result<ScanResult> {
    if let Some(issue) = report
        .issues
        .iter()
        .find(|issue| issue.relative_path.is_empty() && issue.kind == "read_directory")
    {
        return Ok(ScanResult::RootUnavailable {
            detail: issue.detail.clone(),
        });
    }
    if !report.issues.is_empty() {
        return Ok(ScanResult::Incomplete {
            issues: report.issues,
        });
    }
    storage.create_protection()?;
    let import = import_complete_inventory(storage, library_id, root, &report)?;
    Ok(match import {
        Some(import) => ScanResult::Complete(import),
        None => ScanResult::WrongRoot,
    })
}

fn import_complete_inventory(
    storage: &Storage,
    library_id: &str,
    root: &Path,
    report: &InventoryReport,
) -> Result<Option<ObservationImport>> {
    let import = storage.with_transaction(|tx| {
        let binding: (String, i64, i64) = tx.query_row(
            "SELECT root_path, enabled, removed FROM libraries WHERE id=?1",
            [library_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if !roots_match(&binding.0, root) || binding.1 != 1 || binding.2 != 0 {
            return Ok(None);
        }
        let import = import_in_transaction(tx, library_id, report)?;
        crate::problems::record_scan(tx, library_id, report)?;
        Ok(Some(import))
    })?;
    Ok(import)
}

pub fn commit_confirmed_inventory(
    storage: &Storage,
    library_id: Option<&str>,
    root: &Path,
    report: &InventoryReport,
) -> Result<CommittedObservation> {
    if !root.is_absolute() || !report.issues.is_empty() {
        return Err("inventory is not complete for an absolute root".into());
    }
    Ok(storage.with_transaction(|tx| {
        let id = if let Some(id) = library_id {
            let binding: (String, i64, i64) = tx.query_row(
                "SELECT root_path, enabled, removed FROM libraries WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            if !roots_match(&binding.0, root)
                || !(binding.1 == 1 && binding.2 == 0)
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
            let mut others = tx.prepare(
                "SELECT id, root_path, removed FROM libraries WHERE id<>?1 AND root_path IS NOT NULL",
            )?;
            let other_roots: Vec<(String, String, i64)> = others
                .query_map([id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?;
            if other_roots.iter().any(|(_, bound, removed)| *removed == 0 && roots_overlap(bound, root)) {
                return Err(rusqlite::Error::InvalidQuery);
            }
            id.to_owned()
        } else {
            let mut statement = tx.prepare(
                "SELECT id, root_path, removed FROM libraries WHERE root_path IS NOT NULL",
            )?;
            let bound_roots: Vec<(String, String, i64)> = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?;
            if bound_roots
                .iter()
                .any(|(_, bound, removed)| *removed == 0 && roots_overlap(bound, root))
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
            let id: String =
                tx.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
            let name = root
                .file_name()
                .and_then(|part| part.to_str())
                .filter(|part| !part.is_empty())
                .unwrap_or("片库来源");
            tx.execute(
                "INSERT INTO libraries (id, display_name, root_path,scan_mode) VALUES (?1, ?2, ?3,?4)",
                params![id, name, root.to_string_lossy().as_ref(),report.scan_mode],
            )?;
            id
        };
        let import = import_in_transaction(tx, &id, report)?;
        crate::problems::record_scan(tx, &id, report)?;
        Ok(CommittedObservation {
            library_id: id,
            import,
        })
    })?)
}

fn import_in_transaction(
    tx: &Transaction<'_>,
    library_id: &str,
    report: &InventoryReport,
) -> rusqlite::Result<ObservationImport> {
    let discovered = report.videos.len() + report.assets.len();
    let found: HashSet<&str> = report
        .videos
        .iter()
        .map(|entry| entry.relative_path.as_str())
        .chain(
            report
                .assets
                .iter()
                .map(|entry| entry.relative_path.as_str()),
        )
        .collect();
    let mut statement = tx.prepare(
            "SELECT id, observed_path, kind FROM file_observations WHERE library_id=?1 ORDER BY observed_path, id",
        )?;
    let previous: Vec<MissingObservation> = statement
        .query_map([library_id], |row| {
            Ok(MissingObservation {
                observation_id: row.get(0)?,
                observed_path: row.get(1)?,
                kind: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    // The transaction already owns a complete previous set. Re-querying the
    // source for every new path makes an initial large import quadratic.
    let mut observed: HashSet<String> = previous.iter().map(|entry| entry.observed_path.clone()).collect();
    let missing = previous
        .into_iter()
        .filter(|entry| !found.contains(entry.observed_path.as_str()))
        .collect();
    let mut inserted = 0;
    for video in &report.videos {
        if observed.insert(video.relative_path.clone()) {
            inserted += insert_observation(tx, library_id, &video.relative_path, "video")?;
        }
    }
    for asset in &report.assets {
        let kind = match asset.kind {
            AssetKind::Image => "image",
            AssetKind::Subtitle => "subtitle",
        };
        if observed.insert(asset.relative_path.clone()) {
            inserted += insert_observation(tx, library_id, &asset.relative_path, kind)?;
        }
    }
    Ok(ObservationImport {
        discovered,
        inserted,
        missing,
    })
}

pub(crate) fn roots_match(bound: &str, selected: &Path) -> bool {
    let path = Path::new(bound);
    path.is_absolute()
        && (path == selected
            || matches!((path.canonicalize(), selected.canonicalize()), (Ok(a), Ok(b)) if a == b))
}

fn root_parts(path: &Path) -> Vec<String> {
    path.components()
        .map(|part| {
            let value = part.as_os_str().to_string_lossy().into_owned();
            if cfg!(windows) {
                value.to_lowercase()
            } else {
                value
            }
        })
        .collect()
}

pub(crate) fn roots_overlap(bound: &str, selected: &Path) -> bool {
    let bound = Path::new(bound);
    if !bound.is_absolute() || !selected.is_absolute() {
        return false;
    }
    let canonical_bound = bound.canonicalize().unwrap_or_else(|_| bound.to_path_buf());
    let canonical_selected = selected
        .canonicalize()
        .unwrap_or_else(|_| selected.to_path_buf());
    let left = root_parts(&canonical_bound);
    let right = root_parts(&canonical_selected);
    left.starts_with(&right) || right.starts_with(&left)
}

fn insert_observation(
    tx: &Transaction<'_>,
    library_id: &str,
    relative_path: &str,
    kind: &str,
) -> rusqlite::Result<usize> {
    tx.execute(
        "INSERT INTO file_observations (id, library_id, observed_path, kind)
         VALUES (lower(hex(randomblob(16))), ?1, ?2, ?3)",
        params![library_id, relative_path, kind],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ScanIssue;
    use std::fs;

    #[test]
    fn child_read_error_reports_incomplete_without_importing_partial_tree() {
        let home = tempfile::tempdir().unwrap();
        let media = home.path().join("media");
        fs::create_dir(&media).unwrap();
        fs::write(media.join("found.mp4"), []).unwrap();
        let storage = Storage::open(&home.path().join("data")).unwrap();
        storage.with_connection(|db| {
            db.execute("INSERT INTO libraries (id, display_name, root_path) VALUES ('test', '虚构', ?1)", [media.to_string_lossy().as_ref()])?;
            db.execute("INSERT INTO file_observations (id, library_id, observed_path, kind) VALUES ('old-observation', 'test', 'old.mp4', 'video')", [])?;
            db.execute("INSERT INTO films(id) VALUES('film')", [])?;
            db.execute("INSERT INTO film_manual(film_id,cover_choice) VALUES('film','chosen-image')", [])?;
            db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('cover-scan:test','cover_scan','test','current','[\"old-observation\"]')", [])?;
            db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('cover-current:film','cover_current','film','saved-key','saved-evidence')", [])?;
            db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence,handled_at,resolution) VALUES('cover:film:saved-key','cover_ambiguity','film','saved-key','saved-evidence','2026-10-01','marked')", [])?;
            Ok(())
        }).unwrap();
        let mut report = inventory(&media).unwrap();
        report.issues.push(ScanIssue {
            relative_path: "unreadable-child".into(),
            kind: "read_directory".into(),
            detail: "injected read failure".into(),
        });
        let result = finish_inventory(&storage, "test", &media, report).unwrap();
        match result {
            ScanResult::Incomplete { issues } => {
                assert_eq!(issues[0].kind, "read_directory");
                assert_eq!(issues[0].relative_path, "unreadable-child");
            }
            other => panic!("expected incomplete scan, got {other:?}"),
        }
        storage
            .with_connection(|db| {
                let count: i64 =
                    db.query_row("SELECT COUNT(*) FROM file_observations", [], |row| {
                        row.get(0)
                    })?;
                assert_eq!(count, 1);
                let old: String = db.query_row("SELECT id FROM file_observations",[],|row|row.get(0))?;
                assert_eq!(old,"old-observation");
                let scan: String = db.query_row("SELECT evidence FROM issue_records WHERE id='cover-scan:test'", [], |r| r.get(0))?;
                assert_eq!(scan, "[\"old-observation\"]");
                let current: (String, String) = db.query_row("SELECT evidence_key,evidence FROM issue_records WHERE id='cover-current:film'", [], |r| Ok((r.get(0)?,r.get(1)?)))?;
                assert_eq!(current, ("saved-key".into(), "saved-evidence".into()));
                let handled: (String, String) = db.query_row("SELECT handled_at,resolution FROM issue_records WHERE id='cover:film:saved-key'", [], |r| Ok((r.get(0)?,r.get(1)?)))?;
                assert_eq!(handled, ("2026-10-01".into(), "marked".into()));
                let choice: String = db.query_row("SELECT cover_choice FROM film_manual WHERE film_id='film'", [], |r| r.get(0))?;
                assert_eq!(choice, "chosen-image");
                Ok(())
            })
            .unwrap();
    }
}
