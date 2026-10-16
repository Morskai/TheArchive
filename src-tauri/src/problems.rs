//! Missing library content. Scan evidence and human resolutions remain separate.
use crate::{
    artwork, catalog, covers::{Candidate, CoverIndex}, folder_groups, inventory::InventoryReport,
    storage::Storage,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, String>;
const FILM: &str = "film_missing_cover";
const CODE: &str = "film_code_conflict";
const CURRENT: &str = "library_problem_current";

#[derive(Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub name: String,
}
#[derive(Serialize, Deserialize)]
struct Evidence {
    rule: String,
    missing: bool,
    episode: u64,
    videos: Vec<Candidate>,
    sources: Vec<Source>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    directories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    artwork: Vec<ArtworkEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    films: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct ArtworkEvidence {
    source: String,
    check: crate::inventory::CoverCheck,
}
#[derive(Serialize)]
pub struct Problem {
    pub id: String,
    pub kind: String,
    pub subject_id: String,
    pub title: String,
    pub reason: String,
    pub expected_file: String,
    pub videos: Vec<Candidate>,
    pub directories: Vec<String>,
    pub sources: Vec<Source>,
    pub handled: bool,
}

// Replace the current read set only after a complete scan, inside the import transaction.
pub(crate) fn record_scan(
    tx: &Transaction<'_>,
    source: &str,
    report: &InventoryReport,
) -> rusqlite::Result<()> {
    if !report.issues.is_empty() {
        return Err(rusqlite::Error::InvalidQuery);
    }
    // Only a complete explicit scan clears suppression for this source.
    tx.execute("DELETE FROM issue_records WHERE subject_type='library_problem_cleared' AND EXISTS(SELECT 1 FROM json_each(evidence,'$.sources') s WHERE json_extract(s.value,'$.id')=?1)", [source])?;
    let paths: BTreeSet<_> = report
        .videos
        .iter()
        .map(|v| &v.relative_path)
        .chain(report.assets.iter().map(|v| &v.relative_path))
        .collect();
    let mut q = tx.prepare("SELECT id,observed_path FROM file_observations WHERE library_id=?1")?;
    let rows = q
        .query_map([source], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let ids: Vec<_> = rows
        .into_iter()
        .filter(|(_, path)| paths.contains(path))
        .map(|(id, _)| id)
        .collect();
    let json = serde_json::to_string(&ids).map_err(|_| rusqlite::Error::InvalidQuery)?;
    tx.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES(?1,'cover_scan',?2,'current',?3) ON CONFLICT(id) DO UPDATE SET evidence=excluded.evidence",params![format!("cover-scan:{source}"),source,json])?;
    let details = artwork::ScanDetails {
        scan_mode: Some(report.scan_mode.clone()),
        directories: report.directories.clone(),
        cover_checks: report.cover_checks.clone(),
        roles: report.roles.clone(),
        video_sizes: report.video_sizes.clone(),
        scanned_at: Some(tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ','now')",[],|r|r.get(0))?),
        directory_created_at: report.directory_created_at.iter().map(|(directory, milliseconds)| {
            let timestamp: String = tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ',?1/1000.0,'unixepoch')", [milliseconds], |row|row.get(0))?;
            Ok((directory.clone(), timestamp))
        }).collect::<rusqlite::Result<_>>()?,
        directory_modified_at: report.directory_modified_at.iter().map(|(directory, milliseconds)| {
            let timestamp: String = tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ',?1/1000.0,'unixepoch')", [milliseconds], |row|row.get(0))?;
            Ok((directory.clone(), timestamp))
        }).collect::<rusqlite::Result<_>>()?,
        ignored_names: Some({ let mut q=tx.prepare("SELECT name_key FROM ignored_directory_names")?; q.query_map([],|r|r.get(0))?.collect::<rusqlite::Result<_>>()? }),
    };
    let json = serde_json::to_string(&details).map_err(|_| rusqlite::Error::InvalidQuery)?;
    tx.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES(?1,'library_scan_details',?2,'current',?3) ON CONFLICT(id) DO UPDATE SET evidence=excluded.evidence",params![format!("scan-details:{source}"),source,json])?;
    // Group film identities; actor records are independent saved software data.
    catalog::link_unassigned_videos(tx)?;
    sync(tx)
}

fn save_evidence(
    tx: &Transaction<'_>,
    kind: &str,
    subject: &str,
    mut evidence: Evidence,
) -> rusqlite::Result<String> {
    let current_id = format!("problem-current:{kind}:{subject}");
    let previous: Option<(String, String)> = tx
        .query_row(
            "SELECT evidence,evidence_key FROM issue_records WHERE id=?1",
            [&current_id],
            |r| Ok((r.get(0)?,r.get(1)?)),
        )
        .optional()?;
    let mut retained_key = None;
    if let Some((json, prior_key)) = previous {
        let mut old: Evidence =
            serde_json::from_str(&json).map_err(|_| rusqlite::Error::InvalidQuery)?;
        // Legacy duplicate reminders stored only video IDs. Resolve their
        // current canonical card IDs before comparing the relevant facts.
        if kind == CODE && old.films.is_empty() {
            let mut ids = BTreeSet::new();
            for video in &old.videos {
                if let Some(id) = tx.query_row("SELECT COALESCE(a.film_id,o.film_id) FROM file_observations o LEFT JOIN film_aliases a ON a.id=o.film_id WHERE o.id=?1", [&video.id], |r|r.get::<_,Option<String>>(0)).optional()?.flatten() {
                    ids.insert(id);
                }
            }
            old.films = ids.into_iter().collect();
        }
        evidence.episode = old.episode + u64::from(old.missing != evidence.missing);
        // Keep existing handled identities, including legacy payload keys, when
        // only filenames, titles or source display labels change.
        if problem_fingerprint(kind, &old)? == problem_fingerprint(kind, &evidence)? {
            retained_key = Some(prior_key.strip_prefix("inactive:").unwrap_or(&prior_key).to_owned());
        }
    }
    let json = serde_json::to_string(&evidence).map_err(|_| rusqlite::Error::InvalidQuery)?;
    let fingerprint = problem_fingerprint(kind, &evidence)?;
    let key = retained_key.unwrap_or_else(|| format!("{:x}", Sha256::digest(fingerprint.as_bytes())));
    if evidence.missing {
        let id = format!("problem:{kind}:{subject}:{key}");
        tx.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) SELECT ?1,?2,?3,?4,?5 WHERE NOT EXISTS(SELECT 1 FROM issue_records WHERE id=?1) AND NOT EXISTS(SELECT 1 FROM issue_records WHERE subject_type='library_problem_cleared' AND subject_id=?2||':'||?3)",params![id,kind,subject,key,json])?;
        tx.execute("UPDATE issue_records SET evidence=?2 WHERE id=?1 AND evidence<>?2", params![id,json])?;
    }
    tx.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET evidence_key=excluded.evidence_key,evidence=excluded.evidence WHERE evidence_key<>excluded.evidence_key OR evidence<>excluded.evidence",params![current_id,CURRENT,subject,key,json])?;
    Ok(current_id)
}

fn problem_fingerprint(kind: &str, evidence: &Evidence) -> rusqlite::Result<String> {
    let value = if kind == CODE {
        serde_json::json!({"missing":evidence.missing,"episode":evidence.episode,"code":evidence.code,"films":evidence.films})
    } else {
        serde_json::json!({"missing":evidence.missing,"episode":evidence.episode})
    };
    serde_json::to_string(&value).map_err(|_| rusqlite::Error::InvalidQuery)
}

fn evidence(
    videos: &[&folder_groups::VideoRow],
    sources: &BTreeMap<String, String>,
    missing: bool,
) -> Evidence {
    let mut files: Vec<_> = videos
        .iter()
        .map(|v| Candidate {
            id: v.id.clone(),
            name: v
                .path
                .replace('\\', "/")
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .into(),
        })
        .collect();
    files.sort_by(|a, b| a.id.cmp(&b.id));
    files.dedup_by(|a, b| a.id == b.id);
    let ids: BTreeSet<_> = videos.iter().map(|v| &v.source).collect();
    Evidence {
        rule: "missing_fixed_artwork_v1".into(),
        missing,
        episode: 0,
        videos: files,
        sources: ids
            .into_iter()
            .map(|id| Source {
                id: id.clone(),
                name: sources.get(id).cloned().unwrap_or_default(),
            })
            .collect(),
        directories: vec![],
        artwork: vec![],
        code: None,
        films: videos.iter().filter_map(|video|video.film.clone()).collect::<BTreeSet<_>>().into_iter().collect(),
    }
}

pub(crate) fn sync(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    let covers = CoverIndex::read(tx)?;
    let details = artwork::scan_details(tx)?;
    let videos = folder_groups::rows(tx)?;
    let mut q = tx.prepare("SELECT id,display_name FROM libraries WHERE enabled=1 AND removed=0")?;
    let sources: BTreeMap<String, String> = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut films: BTreeMap<String, Vec<&folder_groups::VideoRow>> = BTreeMap::new();
    for v in &videos {
        if let Some(id) = &v.film {
            films.entry(id.clone()).or_default().push(v);
        }
    }
    let mut directory_checks: BTreeMap<(String, String), Vec<(usize, &crate::inventory::CoverCheck)>> = BTreeMap::new();
    for (source, detail) in &details {
        for (position, check) in detail.cover_checks.iter().enumerate() {
            directory_checks.entry((source.clone(), folder_groups::parent(&check.relative_path)))
                .or_default().push((position, check));
        }
    }
    let mut live = BTreeSet::new();
    for (film, files) in &films {
        let directories: BTreeSet<_> = files.iter()
            .map(|video| (video.source.clone(), folder_groups::parent(&video.path))).collect();
        let mut e = evidence(files, &sources, !covers.info(film).readable);
        let mut checks_by_source: BTreeMap<&str, Vec<(usize, &crate::inventory::CoverCheck)>> = BTreeMap::new();
        for (source, directory) in &directories {
            if let Some(checks) = directory_checks.get(&(source.clone(), directory.clone())) {
                checks_by_source.entry(source).or_default().extend(checks.iter().copied());
            }
        }
        for (source, mut checks) in checks_by_source {
            checks.sort_by_key(|(position, _)| *position);
            for (_, check) in checks {
                e.artwork.push(ArtworkEvidence { source: source.to_owned(), check: check.clone() });
            }
        }
        live.insert(save_evidence(tx, FILM, film, e)?);
    }
    let mut q = tx.prepare("SELECT id,root_path FROM libraries WHERE enabled=1 AND removed=0 AND root_path IS NOT NULL")?;
    let roots = q.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<BTreeMap<_,_>>>()?;
    sync_code_conflicts(tx, &roots, &sources, &films, &mut live)?;
    let mut q = tx.prepare("SELECT id,evidence FROM issue_records WHERE subject_type=?1")?;
    let old = q
        .query_map([CURRENT], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, json) in old {
        if !live.contains(&id) {
            // Keep the episode anchor when the object disappears. A subsequent return
            // must not resurrect an earlier handled missing episode.
            let mut e: Evidence =
                serde_json::from_str(&json).map_err(|_| rusqlite::Error::InvalidQuery)?;
            if e.sources.iter().any(|s| sources.contains_key(&s.id)) {
                if e.missing {
                    e.missing = false;
                    e.episode += 1;
                }
                let json = serde_json::to_string(&e).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let kind = if id.starts_with("problem-current:film_code_conflict:") { CODE } else { FILM };
                let key = format!("{:x}", Sha256::digest(problem_fingerprint(kind, &e)?.as_bytes()));
                tx.execute(
                    "UPDATE issue_records SET evidence_key=?1,evidence=?2 WHERE id=?3",
                    params![key, json, id],
                )?;
            } else {
                // Pausing/removing a source hides its current problems without changing
                // evidence or human state; re-enabling identical evidence restores them.
                tx.execute("UPDATE issue_records SET evidence_key='inactive:'||evidence_key WHERE id=?1 AND evidence_key NOT LIKE 'inactive:%'",[id])?;
            }
        }
    }
    Ok(())
}

fn sync_code_conflicts(
    tx: &Transaction<'_>,
    roots: &BTreeMap<String, String>,
    sources: &BTreeMap<String, String>,
    films: &BTreeMap<String, Vec<&folder_groups::VideoRow>>,
    live: &mut BTreeSet<String>,
) -> rusqlite::Result<()> {
    let mut query = tx.prepare("SELECT film_id,code FROM film_manual WHERE code IS NOT NULL")?;
    let manual: BTreeMap<String, String> = query.query_map([], |r|Ok((r.get(0)?,r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut groups: BTreeMap<(String, String), BTreeMap<&str, Vec<&folder_groups::VideoRow>>> = BTreeMap::new();
    for (film, files) in films {
        let code = manual.get(film).map(|code|code.trim()).filter(|code|!code.is_empty()).map(str::to_owned)
            .or_else(|| {
                // Match the card: several different automatic values mean
                // “编号待核对”, which is not itself a real duplicate number.
                let codes: BTreeSet<_> = files.iter().filter_map(|video| {
                    let directory = folder_groups::parent(&video.path);
                    let name = directory.rsplit('/').next().filter(|name|!name.is_empty()).unwrap_or(&video.root_name);
                    folder_groups::directory_code(name)
                }).collect();
                (codes.len()==1).then(||codes.into_iter().next().unwrap())
            });
        let Some(code) = code else { continue };
        for video in files {
            if roots.contains_key(&video.source) {
                groups.entry((video.source.clone(), code.to_uppercase())).or_default()
                    .entry(film).or_default().push(video);
            }
        }
    }
    for ((source, code), cards) in groups {
        if cards.len() < 2 { continue; }
        let files: Vec<_> = cards.values().flat_map(|files| files.iter().copied()).collect();
        let mut e = evidence(&files, sources, true);
        e.sources = vec![Source { id: source.clone(), name: sources.get(&source).cloned().unwrap_or_default() }];
        e.rule = "film_code_conflict_v1".into();
        e.directories = files.iter().map(|video| {
            let directory = folder_groups::parent(&video.path);
            if directory.is_empty() { video.root_name.clone() } else { directory }
        }).collect::<BTreeSet<_>>().into_iter().collect();
        e.code = Some(code.clone());
        let subject = format!("code:{source}:{:x}", Sha256::digest(code.as_bytes()));
        live.insert(save_evidence(tx, CODE, &subject, e)?);
    }
    Ok(())
}

pub fn list(storage: &Storage) -> Result<Vec<Problem>> {
    catalog::ensure_film_entries(storage).map_err(|_| "problems_load_failed")?;
    storage
        .with_transaction(sync)
        .map_err(|_| "problems_load_failed")?;
    let titles: BTreeMap<_, _> = catalog::list_films(storage)
        .map_err(|_| "problems_load_failed")?
        .into_iter()
        .map(|f| (f.id, f.title))
        .collect();
    storage.with_connection(|db| {
        let mut q=db.prepare("SELECT i.id,i.subject_type,i.subject_id,i.evidence,i.handled_at IS NOT NULL FROM issue_records i JOIN issue_records c ON c.id='problem-current:'||i.subject_type||':'||i.subject_id AND c.evidence_key=i.evidence_key WHERE i.subject_type IN (?1,?2) ORDER BY i.created_at DESC,i.id")?;
        q.query_map(params![FILM,CODE],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,bool>(4)?)))?.map(|row| {
            let (id,kind,subject_id,json,handled)=row?;
            let e: Evidence=serde_json::from_str(&json).map_err(|_|rusqlite::Error::InvalidQuery)?;
            let (title,expected_file,reason)=if kind==FILM {
                let reason=if e.artwork.iter().any(|a|a.check.state!="readable") {"最新完整扫描中，已识别的 poster 或视频同名图片无法读取为可用封面。"} else {"最新完整扫描中，影片目录未找到可用的 poster 图片（poster.jpg 等；单视频目录也支持视频同名图片）。"};
                (titles.get(&subject_id).cloned().or_else(||e.directories.first().and_then(|d|d.rsplit('/').next()).map(String::from)).unwrap_or_else(||"未命名影片".into()),artwork::FILM_COVER,reason)
            } else if kind==CODE {
                (format!("编号 {}",e.code.as_deref().unwrap_or("待核对")),"有效编号","同一片库中的不同影片卡片使用了相同编号，请核对是否需要调整。")
            } else { return Err(rusqlite::Error::InvalidQuery); };
            let changed: bool=db.query_row("SELECT EXISTS(SELECT 1 FROM issue_records WHERE subject_type=?1 AND subject_id=?2 AND evidence_key<>(SELECT evidence_key FROM issue_records WHERE id=?3))",params![kind,subject_id,format!("problem-current:{kind}:{subject_id}")],|r|r.get(0))?;
            Ok(Problem{id,kind,subject_id,title,reason:if changed {format!("扫描证据已变化，需要重新核对。{reason}")} else {reason.into()},expected_file:expected_file.into(),videos:e.videos,directories:e.directories,sources:e.sources,handled})
        }).collect()
    }).map_err(|_|"problems_load_failed".into())
}

fn current(db: &Connection, id: &str) -> rusqlite::Result<()> {
    db.query_row("SELECT 1 FROM issue_records i JOIN issue_records c ON c.id='problem-current:'||i.subject_type||':'||i.subject_id AND c.evidence_key=i.evidence_key WHERE i.id=?1 AND i.subject_type IN (?2,?3)",params![id,FILM,CODE],|_|Ok(()))
}
pub fn mark(storage: &Storage, ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Err("problem_not_found".into());
    }
    storage.with_transaction(|tx| {
        sync(tx)?;
        for id in ids.iter().collect::<BTreeSet<_>>() {
            current(tx,id)?;
            tx.execute("UPDATE issue_records SET resolution=COALESCE(resolution,'marked'),handled_at=COALESCE(handled_at,CURRENT_TIMESTAMP) WHERE id=?1",[id])?;
        } Ok(())
    }).map_err(|_|"problem_save_failed".into())
}

// Human status and deletion are transactionally separate from current scan evidence.
pub fn restore(storage: &Storage, ids: &[String]) -> Result<()> {
    if ids.is_empty() { return Err("problem_not_found".into()); }
    storage.with_transaction(|tx| {
        for id in ids.iter().collect::<BTreeSet<_>>() {
            current(tx, id)?;
            tx.execute("UPDATE issue_records SET resolution=NULL,handled_at=NULL WHERE id=?1", [id])?;
        }
        Ok(())
    }).map_err(|_| "problem_save_failed".into())
}
pub fn clear(storage: &Storage, ids: &[String]) -> Result<()> {
    if ids.is_empty() { return Err("problem_not_found".into()); }
    storage.create_protection().map_err(|_| "protection_failed".to_owned())?;
    storage.with_transaction(|tx| {
        for id in ids.iter().collect::<BTreeSet<_>>() {
            current(tx, id)?;
            let (kind, subject, json): (String,String,String) = tx.query_row("SELECT subject_type,subject_id,evidence FROM issue_records WHERE id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let e: Evidence = serde_json::from_str(&json).map_err(|_| rusqlite::Error::InvalidQuery)?;
            let scope = format!("{kind}:{subject}");
            let sources = serde_json::json!({ "sources": e.sources }).to_string();
            tx.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES(?1,'library_problem_cleared',?2,'cleared',?3) ON CONFLICT(id) DO UPDATE SET evidence=excluded.evidence", params![format!("problem-cleared:{scope}"),scope,sources])?;
            tx.execute("DELETE FROM issue_records WHERE subject_type=?1 AND subject_id=?2", params![kind,subject])?;
        }
        Ok(())
    }).map_err(|_| "problem_save_failed".into())
}

/// A user-removed card is no longer a current scanned Film directory. Preserve
/// sibling/child evidence; the next explicit scan records the directory again.
pub(crate) fn forget_deleted_films(tx: &Transaction<'_>, ids: &BTreeSet<String>) -> rusqlite::Result<()> {
    let mut directories = BTreeSet::new();
    let mut query = tx.prepare("SELECT library_id,observed_path FROM file_observations WHERE film_id=?1 AND kind='video'")?;
    for id in ids {
        for row in query.query_map([id], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?)))? {
            let (source, path) = row?;
            directories.insert((source, folder_groups::parent(&path)));
        }
    }
    // Old observations can point to a directory now occupied by a different
    // card. Its current role and evidence must survive deleting the old owner.
    let snapshot = artwork::ScanSnapshot::read(tx)?;
    let mut current = tx.prepare("SELECT id,library_id,observed_path,film_id FROM file_observations WHERE kind='video'")?;
    for row in current.query_map([], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,Option<String>>(3)?)))? {
        let (observation, source, path, film) = row?;
        if snapshot.contains(&source, &observation) && film.as_ref().is_none_or(|id| !ids.contains(id)) {
            directories.remove(&(source, folder_groups::parent(&path)));
        }
    }
    let sources = directories.iter().map(|(source, _)| source.clone()).collect();
    let mut subjects = ids.clone();
    for (source, mut details) in artwork::scan_details_for_sources(tx, &sources)? {
        details.roles.retain(|role| role.role != crate::directory_roles::Role::Film || !directories.contains(&(source.clone(), role.directory.clone())));
        let json = serde_json::to_string(&details).map_err(|_| rusqlite::Error::InvalidQuery)?;
        tx.execute("UPDATE issue_records SET evidence=?2 WHERE subject_type='library_scan_details' AND subject_id=?1 AND evidence_key='current'", params![source,json])?;
    }
    for (source, directory) in directories {
        subjects.insert(format!("directory:{source}:{:x}", Sha256::digest(directory.as_bytes())));
    }
    for subject in subjects {
        for kind in [FILM, CODE] {
            let current = format!("problem-current:{kind}:{subject}");
            let scope = format!("{kind}:{subject}");
            tx.execute("DELETE FROM issue_records WHERE (subject_type=?1 AND subject_id=?2) OR (subject_type='library_problem_current' AND id=?3) OR (subject_type='library_problem_cleared' AND subject_id=?4)", params![kind,subject,current,scope])?;
        }
    }
    Ok(())
}
