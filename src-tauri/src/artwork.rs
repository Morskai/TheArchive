//! One naming contract for every artwork consumer. Never infer from other images.
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const FILM_COVER: &str = "poster.jpg";

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct ScanDetails {
    #[serde(default)]
    pub scan_mode: Option<String>,
    pub directories: Vec<String>,
    pub cover_checks: Vec<crate::inventory::CoverCheck>,
    #[serde(default)]
    pub roles: Vec<crate::directory_roles::DirectoryRole>,
    #[serde(default)]
    pub video_sizes: BTreeMap<String, u64>,
    #[serde(default)]
    pub scanned_at: Option<String>,
    #[serde(default)]
    pub directory_created_at: BTreeMap<String, String>,
    #[serde(default)]
    pub directory_modified_at: BTreeMap<String, String>,
    #[serde(default)]
    pub ignored_names: Option<Vec<String>>,
}

pub(crate) fn scan_details(db: &Connection) -> rusqlite::Result<BTreeMap<String, ScanDetails>> {
    let mut details = BTreeMap::new();
    let mut q = db.prepare("SELECT subject_id,evidence FROM issue_records WHERE subject_type='library_scan_details' AND evidence_key='current'")?;
    for row in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (source, json) = row?;
        details.insert(
            source,
            serde_json::from_str(&json).map_err(|_| rusqlite::Error::InvalidQuery)?,
        );
    }
    Ok(details)
}

/// Read only the sources needed by one artwork ownership lookup.
pub(crate) fn scan_details_for_sources(db: &Connection, sources: &BTreeSet<String>) -> rusqlite::Result<BTreeMap<String, ScanDetails>> {
    let mut details = BTreeMap::new();
    let mut q = db.prepare("SELECT evidence FROM issue_records WHERE subject_type='library_scan_details' AND evidence_key='current' AND subject_id=?1")?;
    for source in sources {
        for row in q.query_map([source], |r| r.get::<_, String>(0))? {
            details.insert(source.clone(), serde_json::from_str(&row?).map_err(|_| rusqlite::Error::InvalidQuery)?);
        }
    }
    Ok(details)
}

pub(crate) fn named(path: &str, name: &str) -> bool {
    path.replace('\\', "/")
        .rsplit('/')
        .next()
        .is_some_and(|file| { let actual=std::path::Path::new(file); let expected=std::path::Path::new(name); actual.file_stem().is_some_and(|s|s.to_string_lossy().eq_ignore_ascii_case(&expected.file_stem().unwrap_or_default().to_string_lossy())) && image_rank(file).is_some() })
}

pub(crate) struct ScanSnapshot(BTreeMap<String, BTreeSet<String>>);
impl ScanSnapshot {
    pub(crate) fn read_sources(db: &Connection, sources: &BTreeSet<String>) -> rusqlite::Result<Self> {
        let mut snapshots = BTreeMap::new();
        let mut q = db.prepare("SELECT evidence FROM issue_records WHERE subject_type='cover_scan' AND evidence_key='current' AND subject_id=?1")?;
        for source in sources {
            for row in q.query_map([source], |r| r.get::<_, String>(0))? {
                let ids: Vec<String> = serde_json::from_str(&row?).map_err(|_| rusqlite::Error::InvalidQuery)?;
                snapshots.insert(source.clone(), ids.into_iter().collect());
            }
        }
        Ok(Self(snapshots))
    }
    pub(crate) fn read(db: &Connection) -> rusqlite::Result<Self> {
        let mut snapshots = BTreeMap::new();
        let mut q = db.prepare("SELECT subject_id,evidence FROM issue_records WHERE subject_type='cover_scan' AND evidence_key='current'")?;
        for row in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (source, json) = row?;
            let ids: Vec<String> =
                serde_json::from_str(&json).map_err(|_| rusqlite::Error::InvalidQuery)?;
            snapshots.insert(source, ids.into_iter().collect());
        }
        Ok(Self(snapshots))
    }
    pub(crate) fn contains(&self, source: &str, id: &str) -> bool {
        self.0.get(source).is_none_or(|ids| ids.contains(id))
    }
}

pub(crate) fn image_rank(path:&str)->Option<usize>{let ext=std::path::Path::new(path).extension()?.to_string_lossy().to_lowercase();["jpg","jpeg","png","webp","avif"].iter().position(|e|*e==ext)}
