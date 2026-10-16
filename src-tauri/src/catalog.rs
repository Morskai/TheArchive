//! Stable films with directory versions; cross-directory numbers never merge identities.
use crate::folder_groups;
use crate::storage::Storage;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Serialize)]
pub struct CatalogVideo {
    pub id: String,
    pub observed_path: String,
    pub source_id: String,
    pub source_name: String,
    pub directory_path: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CatalogActorLink {
    pub id: String,
    pub name: String,
    pub system: bool,
}

#[derive(Debug, Serialize)]
pub struct CatalogFilm {
    pub actors: Vec<CatalogActorLink>,
    pub has_cover: bool,
    pub id: String,
    pub title: String,
    pub original_title: String,
    pub automatic_title: Option<String>,
    pub translation_stale: bool,
    pub title_edited: bool,
    pub manual_code: Option<String>,
    pub needs_organization: bool,
    pub rating: Option<f64>,
    pub favorite: bool,
    pub tags: Vec<String>,
    pub videos: Vec<CatalogVideo>,
    pub preferred_video_id: Option<String>,
    pub last_launched_at: Option<String>,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
}

pub fn ensure_film_entries(storage: &Storage) -> Result<usize> {
    let pending: i64 = storage.with_connection(|db| {
        db.query_row(
            "SELECT COUNT(*) FROM file_observations JOIN libraries ON libraries.id=file_observations.library_id WHERE file_observations.kind='video' AND film_id IS NULL AND libraries.enabled=1 AND libraries.removed=0",
            [],
            |row| row.get(0),
        )
    })?;
    let groups = storage.with_connection(|db| folder_groups::plan(db))?;
    if pending == 0 && groups.is_empty() {
        return Ok(0);
    }
    // Existing user data is protected before the first relationship is written.
    storage
        .create_protection()
        .map_err(|_| "protection_failed")?;
    storage
        .with_transaction(link_unassigned_videos)
        .map_err(|_| "catalog_failed".into())
}

pub(crate) fn link_unassigned_videos(tx: &Transaction<'_>) -> rusqlite::Result<usize> {
    let all = folder_groups::rows(tx)?;
    let occupied: BTreeSet<_> = all.iter().filter_map(|v| v.film.clone()).collect();
    let mut known = BTreeMap::new();
    for video in &all {
        if let (Some(key), Some(film)) = (folder_groups::key(video, &all), &video.film) {
            known.entry(key).or_insert(film.clone());
        }
    }
    let history = folder_groups::historical_rows(tx)?;
    let mut anchors: BTreeMap<_, BTreeSet<String>> = BTreeMap::new();
    for video in &history {
        if let (Some(key), Some(film)) = (folder_groups::key(video, &history), &video.film) {
            anchors.entry(key).or_default().insert(film.clone());
        }
    }
    for (key, ids) in anchors {
        // Reuse a confirmed directory only if its historical identity is unique.
        if ids.len() == 1 {
            let film = ids.into_iter().next().unwrap();
            // A historical directory cannot also claim a film now present elsewhere.
            if !occupied.contains(&film) {
                known.entry(key).or_insert(film);
            }
        }
    }
    let observation_ids: Vec<_> = all.iter().filter(|v| v.film.is_none()).collect();
    let filenames = historical_filenames(tx)?;
    let mut pending: BTreeMap<VideoGroup, Vec<&folder_groups::VideoRow>> = BTreeMap::new();
    for video in &observation_ids {
        pending
            .entry(video_group(video, &all))
            .or_default()
            .push(video);
    }
    let mut assigned = BTreeMap::new();
    let mut candidates = BTreeMap::new();
    let mut claims: BTreeMap<String, BTreeSet<VideoGroup>> = BTreeMap::new();
    // Plan whole directories before writing. An unmatched new version must not
    // steal the group from a later version with a unique historical filename.
    for (group, videos) in &pending {
        let existing = match group {
            VideoGroup::Directory(source, directory) => {
                known.get(&(source.clone(), directory.clone()))
            }
            VideoGroup::Observation(_) => None,
        };
        if let Some(film) = existing.filter(|film| occupied.contains(*film)) {
            assigned.insert(group.clone(), film.clone());
            continue;
        }
        let candidate = existing.cloned().or_else(|| {
            let mut ids = BTreeSet::new();
            for video in videos {
                if let Some(matches) = filenames.get(&video_filename(&video.path)) {
                    // A same-name ambiguity remains ambiguous even if one of its
                    // films is absent; do not select a convenient subset.
                    if matches.len() != 1 {
                        return None;
                    }
                    ids.extend(matches.iter().cloned());
                }
            }
            if ids.len() != 1 {
                return None;
            }
            ids.into_iter()
                .next()
                .filter(|film| !occupied.contains(film))
        });
        if let Some(film) = candidate {
            claims
                .entry(film.clone())
                .or_default()
                .insert(group.clone());
            candidates.insert(group.clone(), film);
        }
    }
    let mut linked_films = BTreeSet::new();
    for (group, videos) in pending {
        let film_id = if let Some(film) = assigned.get(&group) {
            film.clone()
        } else if let Some(film) = candidates
            .get(&group)
            .filter(|film| claims[*film].len() == 1)
        {
            film.clone()
        } else {
            let film: String =
                tx.query_row("SELECT 'auto-' || lower(hex(randomblob(16)))", [], |row| {
                    row.get(0)
                })?;
            tx.execute("INSERT INTO films (id) VALUES (?1)", [&film])?;
            film
        };
        linked_films.insert(film_id.clone());
        for video in videos {
            tx.execute(
                "UPDATE file_observations SET film_id=?1 WHERE id=?2 AND film_id IS NULL",
                (&film_id, &video.id),
            )?;
        }
    }
    let grouped = folder_groups::merge(tx)?;
    transfer_restored_file_versions(tx, &linked_films)?;
    Ok(observation_ids.len() + grouped)
}

fn transfer_restored_file_versions(
    tx: &Transaction<'_>,
    film_ids: &BTreeSet<String>,
) -> rusqlite::Result<usize> {
    if film_ids.is_empty() {
        return Ok(0);
    }
    let mut scope = BTreeSet::new();
    for film in film_ids {
        let canonical: String = tx.query_row("SELECT COALESCE(a.film_id,f.id) FROM films f LEFT JOIN film_aliases a ON a.id=f.id WHERE f.id=?1", [film], |r| r.get(0))?;
        scope.insert(canonical);
    }
    let all = folder_groups::rows(tx)?;
    let current_ids: BTreeSet<_> = all.iter().map(|v| v.id.clone()).collect();
    let mut current: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for video in &all {
        if let Some(film) = video.film.as_ref().filter(|film| scope.contains(*film)) {
            current
                .entry((film.clone(), video_filename(&video.path)))
                .or_default()
                .push(video.id.clone());
        }
    }
    let mut historical: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    let mut q = tx.prepare("SELECT o.id,COALESCE(a.film_id,o.film_id),o.observed_path FROM file_versions v JOIN file_observations o ON o.id=v.observation_id LEFT JOIN film_aliases a ON a.id=o.film_id WHERE o.kind='video'")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, film, path) = row?;
        if scope.contains(&film) && !current_ids.contains(&id) {
            historical
                .entry((film, video_filename(&path)))
                .or_default()
                .push(id);
        }
    }
    let mut changed = 0;
    for (key, videos) in current {
        let Some(old) = historical.get(&key).filter(|old| old.len() == 1) else {
            continue;
        };
        if videos.len() != 1 {
            continue;
        }
        let already_recorded: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM file_versions WHERE observation_id=?1)",
            [&videos[0]],
            |r| r.get(0),
        )?;
        if already_recorded {
            continue;
        }
        // Move the one software record rather than duplicating it. Player launch
        // events keep their original observation IDs and historical evidence.
        changed += tx.execute(
            "UPDATE file_versions SET observation_id=?1 WHERE observation_id=?2",
            params![videos[0], old[0]],
        )?;
    }
    Ok(changed)
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum VideoGroup {
    Directory(String, String),
    Observation(String),
}

fn video_group(video: &folder_groups::VideoRow, all: &[folder_groups::VideoRow]) -> VideoGroup {
    match folder_groups::key(video, all) {
        Some((source, directory)) => VideoGroup::Directory(source, directory),
        None => VideoGroup::Observation(video.id.clone()),
    }
}

fn video_filename(path: &str) -> String {
    // Include the extension and normalize Windows path separators and case.
    path.rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

fn historical_filenames(db: &Connection) -> rusqlite::Result<BTreeMap<String, BTreeSet<String>>> {
    // Removed, hidden and paused sources remain recovery evidence. Resolve old
    // grouping aliases to the stable film ID; neither images nor content take part.
    let mut q = db.prepare(
        "SELECT o.observed_path,COALESCE(a.film_id,o.film_id) FROM file_observations o \
         LEFT JOIN film_aliases a ON a.id=o.film_id WHERE o.kind='video' AND o.film_id IS NOT NULL",
    )?;
    let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (path, film) = row?;
        names.entry(video_filename(&path)).or_default().insert(film);
    }
    Ok(names)
}

fn untouched_auto_identity(db: &Connection, film: &str) -> rusqlite::Result<bool> {
    if !film.starts_with("auto-") {
        return Ok(false);
    }
    db.query_row(
        "SELECT NOT EXISTS(SELECT 1 FROM film_manual WHERE film_id=?1) \
         AND NOT EXISTS(SELECT 1 FROM film_tags WHERE film_id=?1) \
         AND NOT EXISTS(SELECT 1 FROM player_launch_events WHERE film_id=?1) \
         AND NOT EXISTS(SELECT 1 FROM film_actors WHERE film_id=?1 AND source='manual') \
         AND NOT EXISTS(SELECT 1 FROM film_translations f JOIN translation_memory m ON m.id=f.memory_id WHERE f.film_id=?1 AND m.provider IN ('user','original')) \
         AND NOT EXISTS(SELECT 1 FROM file_versions v JOIN file_observations o ON o.id=v.observation_id WHERE o.film_id=?1) \
         AND NOT EXISTS(SELECT 1 FROM film_aliases WHERE id=?1)",
        [film], |r| r.get(0),
    )
}

/// Recover only transient IDs created during one coordinated refresh batch,
/// after the relevant source snapshots have been refreshed. Older independent
/// films are excluded; a later scan must not resubmit an already completed scope.
pub fn reconcile_filename_history(storage: &Storage, new_film_ids: &[String]) -> Result<usize> {
    const SCOPE: &str = "catalog-filename-restore:pending";
    let saved: Option<String> = storage.with_connection(|db| {
        db.query_row(
            "SELECT evidence FROM issue_records WHERE id=?1",
            [SCOPE],
            |r| r.get(0),
        )
        .optional()
    })?;
    let saved_plans: BTreeMap<String, String> = match saved.as_deref() {
        Some(json) => serde_json::from_str(json)?,
        None => BTreeMap::new(),
    };
    let mut transient: BTreeSet<String> = saved_plans.keys().cloned().collect();
    transient.extend(new_film_ids.iter().cloned());
    if transient.is_empty() {
        return Ok(0);
    }
    let mut plans = storage.with_connection(|db| filename_restore_plan(db, &transient))?;
    // A retry may only restore to the identity proved by the original plan.
    plans.retain(|film, old| saved_plans.get(film).is_none_or(|expected| expected == old));
    if plans.is_empty() && saved.is_none() {
        return Ok(0);
    }
    storage
        .create_protection()
        .map_err(|_| "protection_failed")?;
    let json = serde_json::to_string(&plans)?;
    // Persist only proved new->old plans, never rejected members of the batch.
    // An empty map also safely cancels an old scope before cleanup can fail.
    storage.with_transaction(|tx| {
        tx.execute(
            "INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) \
             VALUES(?1,'catalog_restore_scope','startup','pending',?2) \
             ON CONFLICT(id) DO UPDATE SET evidence=excluded.evidence",
            params![SCOPE, json],
        )?;
        Ok(())
    })?;
    storage
        .with_transaction(|tx| {
            let checked = filename_restore_plan(tx, &transient)?;
            let mut restored = 0;
            let mut restored_films = BTreeSet::new();
            for (film, previous) in &plans {
                if checked.get(film) != Some(previous) {
                    continue;
                }
                tx.execute(
                    "UPDATE film_aliases SET film_id=?1 WHERE film_id=?2",
                    params![previous, film],
                )?;
                tx.execute(
                    "INSERT INTO film_aliases(id,film_id) VALUES(?1,?2)",
                    params![film, previous],
                )?;
                tx.execute(
                    "UPDATE file_observations SET film_id=?1 WHERE film_id=?2",
                    params![previous, film],
                )?;
                restored += 1;
                restored_films.insert(previous.clone());
            }
            if restored != 0 {
                transfer_restored_file_versions(tx, &restored_films)?;
                crate::problems::sync(tx)?;
            }
            // An ambiguity or a user edit is a completed safe refusal. Keep no
            // long-lived merge claim for those films; retry only transaction failures.
            tx.execute("DELETE FROM issue_records WHERE id=?1", [SCOPE])?;
            Ok(restored)
        })
        .map_err(|_| "catalog_failed".into())
}

fn filename_restore_plan(
    db: &Connection,
    transient: &BTreeSet<String>,
) -> rusqlite::Result<BTreeMap<String, String>> {
    let all = folder_groups::rows(db)?;
    let occupied: BTreeSet<_> = all.iter().filter_map(|v| v.film.clone()).collect();
    let mut films: BTreeMap<String, Vec<&folder_groups::VideoRow>> = BTreeMap::new();
    let mut current_names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for video in &all {
        if let Some(film) = &video.film {
            films.entry(film.clone()).or_default().push(video);
            current_names
                .entry(video_filename(&video.path))
                .or_default()
                .insert(film.clone());
        }
    }
    let mut names = historical_filenames(db)?;
    for matches in names.values_mut() {
        matches.retain(|film| !transient.contains(film));
    }
    let mut plans = BTreeMap::new();
    let mut claims: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (film, videos) in &films {
        if !transient.contains(film) || !untouched_auto_identity(db, film)? {
            continue;
        }
        let locations: BTreeSet<_> = videos
            .iter()
            .map(|v| (&v.source, folder_groups::parent(&v.path)))
            .collect();
        if locations.len() != 1 {
            continue;
        }
        let mut candidates = BTreeSet::new();
        let mut ambiguous = false;
        for video in videos {
            let name = video_filename(&video.path);
            if current_names[&name].len() != 1 {
                ambiguous = true;
                break;
            }
            if let Some(matches) = names.get(&name) {
                if matches.len() > 1 {
                    ambiguous = true;
                    break;
                }
                candidates.extend(matches.iter().cloned());
            }
        }
        if ambiguous || candidates.len() != 1 {
            continue;
        }
        let previous = candidates.into_iter().next().unwrap();
        if occupied.contains(&previous) {
            continue;
        }
        claims
            .entry(previous.clone())
            .or_default()
            .insert(film.clone());
        plans.insert(film.clone(), previous);
    }
    plans.retain(|_, previous| claims[previous].len() == 1);
    Ok(plans)
}

/// Sources whose saved current observations block a unique match for this
/// refresh batch. This only reads SQLite; the coordinator owns complete scans.
pub fn filename_recovery_sources(
    storage: &Storage,
    new_film_ids: &[String],
) -> Result<Vec<String>> {
    let mut sources = BTreeSet::new();
    for candidate in filename_recovery_candidates(storage, new_film_ids)? {
        for (source, _) in candidate.previous_locations {
            if source != candidate.target_source {
                sources.insert(source);
            }
        }
    }
    Ok(sources.into_iter().collect())
}

pub(crate) struct FilenameRecoveryCandidate {
    pub target_source: String,
    pub previous_locations: Vec<(String, String)>,
}

/// Saved current locations blocking a unique historical identity. No disk reads.
pub(crate) fn filename_recovery_candidates(
    storage: &Storage,
    new_film_ids: &[String],
) -> Result<Vec<FilenameRecoveryCandidate>> {
    if new_film_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(storage.with_connection(|db| {
        let transient: BTreeSet<_> = new_film_ids.iter().cloned().collect();
        let all = folder_groups::rows(db)?;
        let mut films: BTreeMap<String, Vec<&folder_groups::VideoRow>> = BTreeMap::new();
        let mut current_names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for video in &all {
            if let Some(film) = &video.film {
                films.entry(film.clone()).or_default().push(video);
                current_names
                    .entry(video_filename(&video.path))
                    .or_default()
                    .insert(film.clone());
            }
        }
        let mut names = historical_filenames(db)?;
        for matches in names.values_mut() {
            matches.retain(|film| !transient.contains(film));
        }
        let mut claims: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut candidates = BTreeMap::new();
        for (film, videos) in &films {
            if !transient.contains(film) || !untouched_auto_identity(db, film)? {
                continue;
            }
            let locations: BTreeSet<_> = videos
                .iter()
                .map(|v| (&v.source, folder_groups::parent(&v.path)))
                .collect();
            if locations.len() != 1 {
                continue;
            }
            let mut ids = BTreeSet::new();
            let mut ambiguous = false;
            for video in videos {
                if let Some(matches) = names.get(&video_filename(&video.path)) {
                    if matches.len() > 1 {
                        ambiguous = true;
                        break;
                    }
                    ids.extend(matches.iter().cloned());
                }
            }
            if ambiguous || ids.len() != 1 {
                continue;
            }
            let previous = ids.into_iter().next().unwrap();
            if !films.contains_key(&previous) {
                continue;
            }
            // Other new or independent current groups must not silently join this
            // claim, including groups that already acquired manual information.
            if videos.iter().any(|v| {
                current_names[&video_filename(&v.path)]
                    .iter()
                    .any(|id| id != film && id != &previous)
            }) {
                continue;
            }
            claims
                .entry(previous.clone())
                .or_default()
                .insert(film.clone());
            candidates.insert(film.clone(), previous);
        }
        let mut blocked = Vec::new();
        for (film, previous) in candidates {
            if claims[&previous].len() != 1 {
                continue;
            }
            blocked.push(FilenameRecoveryCandidate {
                target_source: films[&film][0].source.clone(),
                previous_locations: films[&previous].iter()
                    .map(|old| (old.source.clone(), old.path.clone())).collect(),
            });
        }
        Ok(blocked)
    })?)
}

pub fn list_films(storage: &Storage) -> Result<Vec<CatalogFilm>> {
    list_films_with_scope(storage, true)
}

// Playback needs the selected card only, rather than presenting the whole library.
pub fn player_film(storage: &Storage, id: &str) -> Result<Option<CatalogFilm>> {
    Ok(storage.with_transaction(|db| {
        let mut films = raw_films_for(db, true, Some(id))?;
        crate::translation::present_films(db, &mut films)?;
        Ok(films.into_iter().next())
    })?)
}

pub fn list_all_films(storage: &Storage) -> Result<Vec<CatalogFilm>> {
    list_films_with_scope(storage, false)
}

fn list_films_with_scope(storage: &Storage, active_only: bool) -> Result<Vec<CatalogFilm>> {
    Ok(storage.with_transaction(|db| {
        let mut films = raw_films(db, active_only)?;
        crate::translation::present_films(db, &mut films)?;
        Ok(films)
    })?)
}

pub(crate) fn raw_films(db: &Connection, active_only: bool) -> rusqlite::Result<Vec<CatalogFilm>> {
    raw_films_for(db, active_only, None)
}

pub(crate) fn raw_films_for(db: &Connection, active_only: bool, film_id: Option<&str>) -> rusqlite::Result<Vec<CatalogFilm>> {
        // These are film-level facts. Fetch once rather than repeating a
        // correlated search for every version belonging to the same film.
        let mut preferred_query = db.prepare("SELECT o.film_id,MIN(v.observation_id) FROM file_versions v JOIN file_observations o ON o.id=v.observation_id WHERE o.kind='video' AND v.preferred=1 AND o.film_id IS NOT NULL GROUP BY o.film_id")?;
        let preferred: BTreeMap<String, String> = preferred_query.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut history_query = db.prepare("SELECT COALESCE(a.film_id,e.film_id),e.launched_at FROM player_launch_events e LEFT JOIN film_aliases a ON a.id=e.film_id ORDER BY julianday(e.launched_at) DESC,e.id DESC")?;
        let mut latest = BTreeMap::new();
        for row in history_query.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
            let (film, date) = row?;
            latest.entry(film).or_insert(date);
        }
        let mut statement = db.prepare(
            "SELECT films.id, film_manual.code, film_manual.rating, film_manual.favorite, \
                    file_observations.id, file_observations.observed_path, \
                    libraries.id, libraries.display_name, \
                    libraries.root_path \
             FROM films JOIN file_observations ON file_observations.film_id=films.id \
             JOIN libraries ON libraries.id=file_observations.library_id \
             LEFT JOIN film_manual ON film_manual.film_id=films.id \
             WHERE file_observations.kind='video' AND (?1=0 OR (libraries.enabled=1 AND libraries.removed=0)) AND (?2 IS NULL OR films.id=?2) AND NOT EXISTS(SELECT 1 FROM film_aliases a WHERE a.id=films.id) \
             ORDER BY films.id, file_observations.observed_path, file_observations.id",
        )?;
        let rows = statement.query_map(params![active_only, film_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
            ))
        })?;
        let snapshot = crate::artwork::ScanSnapshot::read(db)?;
        let covers = crate::covers::CoverIndex::read(db)?;
        let actor_links = crate::actors::film_cast(db)?;
        let scan_details = crate::artwork::scan_details(db)?;
        let film_directories: BTreeSet<_> = scan_details.iter()
            .flat_map(|(source, details)| {
                details.roles.iter()
                    .filter(|role| role.role == crate::directory_roles::Role::Film)
                    .map(move |role| (source.clone(), role.directory.clone()))
            })
            .collect();
        let mut films = BTreeMap::<String, CatalogFilm>::new();
        for row in rows {
            let (
                film_id,
                manual_code,
                rating,
                favorite,
                video_id,
                observed_path,
                source_id,
                source_name,
                root_path,
            ) = row?;
            let preferred_video_id = preferred.get(&film_id).cloned();
            let last_launched_at = latest.get(&film_id).cloned();
            if active_only && !snapshot.contains(&source_id, &video_id) { continue }
            let root_name = root_path.as_deref().and_then(|root|std::path::Path::new(root).file_name()).and_then(|name|name.to_str()).unwrap_or(&source_name);
            let confirmed = film_directories.contains(&(source_id.clone(), folder_groups::parent(&observed_path)));
            let original_title = original_title(&observed_path, root_name, confirmed);
            let title = folder_groups::title(
                    &observed_path,
                    root_name,
                    confirmed,
                );
            let created_at = scan_details.get(&source_id)
                .and_then(|details| details.directory_created_at.get(&folder_groups::parent(&observed_path))).cloned();
            let modified_at = scan_details.get(&source_id)
                .and_then(|details| details.directory_modified_at.get(&folder_groups::parent(&observed_path))).cloned();
            let relative=std::path::Path::new(&observed_path);
            let directory_path=if relative.is_relative() && relative.components().all(|part|matches!(part,std::path::Component::Normal(_))) {root_path.as_deref().map(|root|std::path::Path::new(root).join(relative).parent().unwrap_or(std::path::Path::new(root)).to_string_lossy().into_owned()).unwrap_or_default()} else {String::new()};
            let film = films.entry(film_id.clone()).or_insert_with(|| CatalogFilm {
                actors: actor_links.get(&film_id).cloned().unwrap_or_default(),
                has_cover: { let info = covers.info(&film_id); info.readable },
                needs_organization: film_id.starts_with("auto-"),
                id: film_id,
                title,
                original_title,
                automatic_title: None,
                translation_stale: false,
                title_edited: false,
                manual_code,
                rating: rating.map(|score| score as f64 / 2.0),
                favorite: favorite.unwrap_or(0) != 0,
                tags: Vec::new(),
                videos: Vec::new(),
                created_at: created_at.clone(),
                modified_at: modified_at.clone(),
                preferred_video_id,
                last_launched_at,
            });
            // A film represented in multiple directories uses the earliest known folder time.
            if created_at.as_ref().is_some_and(|date| film.created_at.as_ref().is_none_or(|saved| date < saved)) {
                film.created_at = created_at;
            }
            // Most recently modified known folder wins across a film's directories.
            if modified_at.as_ref().is_some_and(|date| film.modified_at.as_ref().is_none_or(|saved| date > saved)) {
                film.modified_at = modified_at;
            }
            film.videos.push(CatalogVideo {
                id: video_id,
                observed_path,
                source_id,
                source_name,
                directory_path,
            });
        }
        let mut tags = db.prepare(
            "SELECT film_tags.film_id, tags.name FROM film_tags JOIN tags ON tags.id=film_tags.tag_id WHERE (?1 IS NULL OR film_tags.film_id=?1) ORDER BY tags.position, tags.name, tags.id",
        )?;
        for row in tags.query_map([film_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
            let (film_id, name) = row?;
            if let Some(film) = films.get_mut(&film_id) { film.tags.push(name); }
        }
        Ok(films.into_values().collect())
}

// Keep the complete chosen source title for translation evidence and review.
// Presentation without a translation keeps the existing cleaner and fallback.
fn original_title(path: &str, root_name: &str, confirmed_film: bool) -> String {
    let dir = folder_groups::parent(path);
    let folder = if dir.is_empty() { root_name } else { dir.rsplit('/').next().unwrap_or_default() };
    if confirmed_film || (!folder_groups::category(folder) && (!dir.is_empty() || folder_groups::has_code(folder))) {
        return folder.to_owned();
    }
    std::path::Path::new(path).file_stem().unwrap_or_default().to_string_lossy().into_owned()
}

pub fn list_tags(storage: &Storage) -> Result<Vec<String>> {
    Ok(storage.with_connection(|db| {
        let mut query = db.prepare("SELECT name FROM tags ORDER BY position, name, id")?;
        query.query_map([], |row| row.get(0))?.collect()
    })?)
}

pub fn create_tag(storage: &Storage, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 20 {
        return Err("invalid_tag".into());
    }
    storage.with_transaction(|tx| {
        let mut query = tx.prepare("SELECT name FROM tags")?;
        let existing: Vec<String> = query
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        if existing
            .iter()
            .any(|value| value.to_lowercase() == name.to_lowercase())
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "duplicate_tag".into(),
            ));
        }
        if existing.len() >= 30 { return Err(rusqlite::Error::InvalidParameterName("tag_limit".into())); }
        tx.execute(
            "INSERT INTO tags (id, name, position) VALUES (lower(hex(randomblob(16))), ?1, (SELECT COALESCE(MAX(position), -1)+1 FROM tags))",
            [name],
        )?;
        Ok(())
    })?;
    Ok(())
}

pub fn rename_tag(storage: &Storage, name: &str, new_name: &str) -> Result<()> {
    let new_name = new_name.trim();
    if new_name.is_empty() || new_name.chars().count() > 20 { return Err("invalid_tag".into()); }
    storage.with_transaction(|tx| {
        let id: String = tx.query_row("SELECT id FROM tags WHERE name=?1", [name], |row| row.get(0))?;
        let existing: Vec<(String, String)> = tx.prepare("SELECT id, name FROM tags")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        if existing.iter().any(|(other_id, value)| other_id != &id && value.to_lowercase() == new_name.to_lowercase()) {
            return Err(rusqlite::Error::InvalidParameterName("duplicate_tag".into()));
        }
        tx.execute("UPDATE tags SET name=?1 WHERE id=?2", params![new_name, id])?;
        Ok(())
    })?;
    Ok(())
}

pub fn delete_tag(storage: &Storage, name: &str) -> Result<()> {
    // Removing a global tag removes manual assignments; protect the application data first.
    storage
        .create_protection()
        .map_err(|_| "protection_failed")?;
    storage.with_transaction(|tx| {
        let changed = tx.execute("DELETE FROM tags WHERE name=?1", [name])?;
        if changed != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    })?;
    Ok(())
}

/// Remove application records only. Scanning the untouched directory may add it again.
pub fn delete_films(storage: &Storage, film_ids: &[String]) -> std::result::Result<usize, String> {
    let selected: BTreeSet<_> = film_ids.iter().cloned().collect();
    if selected.is_empty() { return Err("no_films_selected".into()); }
    let ids = storage.with_connection(|db| {
        let mut ids = selected.clone();
        for id in &selected {
            db.query_row("SELECT 1 FROM films WHERE id=?1 AND NOT EXISTS(SELECT 1 FROM film_aliases WHERE id=?1)", [id], |_| Ok(()))?;
            let mut query = db.prepare("SELECT id FROM film_aliases WHERE film_id=?1")?;
            ids.extend(query.query_map([id], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?);
        }
        Ok(ids)
    }).map_err(|_| "film_not_found")?;
    storage.create_protection().map_err(|_| "protection_failed")?;
    storage.with_transaction(|tx| {
        for id in &selected { require_film(tx, id)?; }
        crate::problems::forget_deleted_films(tx, &ids)?;
        for id in &ids {
            tx.execute("DELETE FROM issue_records WHERE (subject_type IN ('film','cover_current','cover_ambiguity') AND subject_id=?1) OR subject_id IN (SELECT id FROM file_observations WHERE film_id=?1)", [id])?;
            tx.execute("DELETE FROM player_launch_events WHERE film_id=?1 OR observation_id IN (SELECT id FROM file_observations WHERE film_id=?1)", [id])?;
            tx.execute("UPDATE actor_directories SET marker_id=NULL WHERE marker_id IN (SELECT id FROM file_observations WHERE film_id=?1)", [id])?;
            tx.execute("DELETE FROM file_observations WHERE film_id=?1", [id])?;
            tx.execute("DELETE FROM film_aliases WHERE id=?1 OR film_id=?1", [id])?;
        }
        for id in &ids { tx.execute("DELETE FROM films WHERE id=?1", [id])?; }
        Ok(selected.len())
    }).map_err(|_| "film_delete_failed".into())
}

pub fn set_favorite(storage: &Storage, film_id: &str, favorite: bool) -> Result<()> {
    storage.with_transaction(|tx| {
        let changed = tx.execute(
            "INSERT INTO film_manual (film_id, favorite) SELECT id, ?2 FROM films WHERE id=?1 \
             ON CONFLICT(film_id) DO UPDATE SET favorite=excluded.favorite",
            params![film_id, favorite],
        )?;
        if changed != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    })?;
    Ok(())
}

pub fn set_rating(storage: &Storage, film_id: &str, stars: Option<u8>) -> Result<()> {
    if stars.is_some_and(|value| !(1..=5).contains(&value)) {
        return Err("invalid_rating".into());
    }
    let score = stars.map(|value| i64::from(value) * 2);
    storage.with_transaction(|tx| {
        let changed = tx.execute(
            "INSERT INTO film_manual (film_id, rating) SELECT id, ?2 FROM films WHERE id=?1 \
             ON CONFLICT(film_id) DO UPDATE SET rating=excluded.rating",
            params![film_id, score],
        )?;
        if changed != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    })?;
    Ok(())
}

pub fn set_film_tags(storage: &Storage, film_id: &str, names: &[String]) -> Result<()> {
    storage.with_transaction(|tx| {
        require_film(tx, film_id)?;
        let mut tag_ids = Vec::new();
        for name in names {
            let id: String = tx.query_row("SELECT id FROM tags WHERE name=?1", [name], |row| {
                row.get(0)
            })?;
            if !tag_ids.contains(&id) {
                tag_ids.push(id);
            }
        }
        tx.execute("DELETE FROM film_tags WHERE film_id=?1", [film_id])?;
        for id in tag_ids {
            tx.execute(
                "INSERT INTO film_tags (film_id, tag_id) VALUES (?1, ?2)",
                params![film_id, id],
            )?;
        }
        Ok(())
    })?;
    Ok(())
}

pub fn apply_bulk_tag(
    storage: &Storage,
    film_ids: &[String],
    name: &str,
    add: bool,
) -> Result<usize> {
    Ok(storage.with_transaction(|tx| {
        let tag_id: String = tx.query_row("SELECT id FROM tags WHERE name=?1", [name], |row| {
            row.get(0)
        })?;
        let mut unique = std::collections::BTreeSet::new();
        for film_id in film_ids {
            require_film(tx, film_id)?;
            unique.insert(film_id);
        }
        for film_id in &unique {
            if add {
                tx.execute(
                    "INSERT OR IGNORE INTO film_tags (film_id, tag_id) VALUES (?1, ?2)",
                    params![film_id, tag_id],
                )?;
            } else {
                tx.execute(
                    "DELETE FROM film_tags WHERE film_id=?1 AND tag_id=?2",
                    params![film_id, tag_id],
                )?;
            }
        }
        Ok(unique.len())
    })?)
}

fn require_film(tx: &Transaction<'_>, film_id: &str) -> rusqlite::Result<()> {
    tx.query_row("SELECT 1 FROM films WHERE id=?1", [film_id], |_| Ok(()))
}

pub fn reorder_tags(storage: &Storage, names: &[String]) -> Result<()> {
    storage.with_transaction(|tx| {
        let existing: Vec<String> = tx.prepare("SELECT name FROM tags")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        let unique: std::collections::HashSet<_> = names.iter().collect();
        if names.len() != existing.len() || unique.len() != names.len() || existing.iter().any(|name| !unique.contains(name)) {
            return Err(rusqlite::Error::InvalidParameterName("stale_tag_order".into()));
        }
        for (position, name) in names.iter().enumerate() { tx.execute("UPDATE tags SET position=?1 WHERE name=?2", params![position as i64, name])?; }
        Ok(())
    })?;
    Ok(())
}

/// Empty manual value restores the directory-derived code.
pub fn set_code(storage:&Storage,film:&str,value:&str)->Result<()> {
 if value.chars().count()>200 || value.contains(['\0','\r','\n']) {return Err("invalid_film_code".into());}
 storage.with_transaction(|tx|{let changed=tx.execute("INSERT INTO film_manual(film_id,code) SELECT id,NULLIF(?2,'') FROM films WHERE id=?1 ON CONFLICT(film_id) DO UPDATE SET code=excluded.code",params![film,value.trim()])?;if changed!=1{return Err(rusqlite::Error::QueryReturnedNoRows);}crate::problems::sync(tx)})
}
