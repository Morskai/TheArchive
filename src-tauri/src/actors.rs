//! Actors are saved software records, independent of media directories.
//! Catalog reads do not create actors or cast links; imported and manual actors share permissions.
use crate::{artwork, storage::Storage};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Serialize)]
pub struct ActorSource {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogActor {
    pub id: String,
    pub name: String,
    pub directory_name: String,
    pub library: String,
    pub sources: Vec<ActorSource>,
    pub directory_created_at: Option<String>,
    pub directory_modified_at: Option<String>,
    pub favorite: bool,
    pub has_portrait: bool,
    pub film_ids: Vec<String>,
    pub tags: Vec<String>,
    pub drawable: bool,
    pub birth_month: String,
    pub height: String,
    pub cup: String,
    pub bust: String,
    pub waist: String,
    pub hips: String,
    pub aliases: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub birth_month: String,
    pub height: String,
    pub cup: String,
    pub bust: String,
    pub waist: String,
    pub hips: String,
}

pub(crate) fn valid_birth_month(value: &str, current_month: &str) -> bool {
    value.is_empty()
        || (value.is_ascii()
            && (value.len() == 4 || value.len() == 7)
            && value[..4].bytes().all(|c| c.is_ascii_digit())
            && (value.len() == 4
                || (value.as_bytes()[4] == b'-'
                    && value[5..].bytes().all(|c| c.is_ascii_digit())
                    && value[5..].parse::<u8>().is_ok_and(|n| (1..=12).contains(&n))))
            && value >= "1900"
            && value <= current_month)
}

pub(crate) fn film_links(
    db: &Connection,
) -> rusqlite::Result<BTreeMap<String, Vec<(String, String)>>> {
    Ok(film_cast(db)?
        .into_iter()
        .map(|(film, actors)| {
            (
                film,
                actors
                    .into_iter()
                    .map(|actor| (actor.id, actor.name))
                    .collect(),
            )
        })
        .collect())
}

pub(crate) fn film_cast(
    db: &Connection,
) -> rusqlite::Result<BTreeMap<String, Vec<crate::catalog::CatalogActorLink>>> {
    let mut q = db.prepare("SELECT COALESCE(al.film_id,fa.film_id),a.id,COALESCE(a.observed_name,''),MAX(fa.source='observation') FROM film_actors fa JOIN actors a ON a.id=fa.actor_id LEFT JOIN film_aliases al ON al.id=fa.film_id GROUP BY COALESCE(al.film_id,fa.film_id),a.id ORDER BY a.id")?;
    let mut result: BTreeMap<String, Vec<crate::catalog::CatalogActorLink>> = BTreeMap::new();
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, bool>(3)?,
        ))
    })? {
        let (film, id, name, system) = row?;
        result
            .entry(film)
            .or_default()
            .push(crate::catalog::CatalogActorLink { id, name, system });
    }
    Ok(result)
}

#[derive(Debug, Serialize)]
pub struct CastCandidate {
    pub id: String,
    pub name: String,
    pub library: String,
}

fn cast_error(error: Box<dyn std::error::Error + Send + Sync>, fallback: &str) -> String {
    match error.downcast_ref::<rusqlite::Error>() {
        Some(rusqlite::Error::InvalidParameterName(code))
            if matches!(
                code.as_str(),
                "film_not_found"
                    | "film_inactive"
                    | "cast_candidate_unavailable"
                    | "system_actor_protected"
            ) =>
        {
            code.clone()
        }
        _ => fallback.into(),
    }
}

fn canonical_active_film(db: &Connection, film_id: &str) -> rusqlite::Result<String> {
    let film: String = db.query_row("SELECT COALESCE(a.film_id,f.id) FROM films f LEFT JOIN film_aliases a ON a.id=f.id WHERE f.id=?1", [film_id], |r| r.get(0))
        .optional()?.ok_or_else(|| rusqlite::Error::InvalidParameterName("film_not_found".into()))?;
    let snapshot = artwork::ScanSnapshot::read(db)?;
    let mut q = db.prepare("SELECT o.id,o.library_id FROM file_observations o JOIN libraries l ON l.id=o.library_id LEFT JOIN film_aliases a ON a.id=o.film_id WHERE o.kind='video' AND COALESCE(a.film_id,o.film_id)=?1 AND l.enabled=1 AND l.removed=0")?;
    for row in q.query_map([&film], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (id, source) = row?;
        if snapshot.contains(&source, &id) {
            return Ok(film);
        }
    }
    Err(rusqlite::Error::InvalidParameterName(
        "film_inactive".into(),
    ))
}

fn saved_cast_candidates(db: &Connection) -> rusqlite::Result<Vec<CastCandidate>> {
    let mut q =
        db.prepare("SELECT id,COALESCE(observed_name,'') FROM actors ORDER BY observed_name,id")?;
    q.query_map([], |r| {
        Ok(CastCandidate {
            id: r.get(0)?,
            name: r.get(1)?,
            library: String::new(),
        })
    })?
    .collect()
}

pub fn list_cast_candidates(storage: &Storage, film_id: &str) -> Result<Vec<CastCandidate>> {
    storage
        .with_connection(|db| {
            canonical_active_film(db, film_id)?;
            saved_cast_candidates(db)
        })
        .map_err(|error| cast_error(error, "cast_load_failed"))
}

pub(crate) fn freeze_cast(tx: &Transaction<'_>, film: &str) -> rusqlite::Result<()> {
    let actors = film_cast(tx)?.remove(film).unwrap_or_default();
    tx.execute("DELETE FROM film_actors WHERE film_id=?1 OR film_id IN(SELECT id FROM film_aliases WHERE film_id=?1)",[film])?;
    for actor in actors {
        tx.execute(
            "INSERT INTO film_actors(film_id,actor_id,source) VALUES(?1,?2,'manual')",
            params![film, actor.id],
        )?;
    }
    tx.execute("INSERT INTO film_manual(film_id,cast_override) VALUES(?1,1) ON CONFLICT(film_id) DO UPDATE SET cast_override=1",[film])?;
    Ok(())
}

pub fn save_aliases(storage: &Storage, id: &str, value: &str) -> Result<()> {
    if value.chars().count() > 1000 || value.contains(['\0', '\r', '\n']) {
        return Err("invalid_actor_profile".into());
    }
    storage.with_transaction(|tx|{
        let changed=tx.execute("INSERT INTO actor_manual(actor_id,aliases) SELECT id,?2 FROM actors WHERE id=?1 ON CONFLICT(actor_id) DO UPDATE SET aliases=excluded.aliases",params![id,value.trim()])?;
        if changed!=1 {return Err(rusqlite::Error::QueryReturnedNoRows);} Ok(())
    }).map_err(|_|"actor_save_failed".into())
}

pub fn add_film_cast_actor(
    storage: &Storage,
    film_id: &str,
    actor_id: &str,
) -> Result<Vec<crate::catalog::CatalogActorLink>> {
    storage
        .with_transaction(|tx| {
            let film = canonical_active_film(tx, film_id)?;
            if !saved_cast_candidates(tx)?
                .iter()
                .any(|actor| actor.id == actor_id)
            {
                return Err(rusqlite::Error::InvalidParameterName(
                    "cast_candidate_unavailable".into(),
                ));
            }
            freeze_cast(tx, &film)?;
            tx.execute(
                "INSERT OR IGNORE INTO film_actors(film_id,actor_id,source) VALUES(?1,?2,'manual')",
                params![film, actor_id],
            )?;
            Ok(film_cast(tx)?.remove(&film).unwrap_or_default())
        })
        .map_err(|error| cast_error(error, "cast_save_failed"))
}

pub fn remove_film_cast_actor(
    storage: &Storage,
    film_id: &str,
    actor_id: &str,
) -> Result<Vec<crate::catalog::CatalogActorLink>> {
    storage.with_transaction(|tx| {
        let film = canonical_active_film(tx, film_id)?;
        freeze_cast(tx,&film)?;
        tx.execute("DELETE FROM film_actors WHERE actor_id=?2 AND (film_id=?1 OR film_id IN (SELECT id FROM film_aliases WHERE film_id=?1))", params![film, actor_id])?;
        Ok(film_cast(tx)?.remove(&film).unwrap_or_default())
    }).map_err(|error| cast_error(error, "cast_save_failed"))
}

pub fn list_actors(storage: &Storage) -> Result<Vec<CatalogActor>> {
    let films = crate::catalog::list_films(storage).map_err(|_| "actors_load_failed")?;
    list_actors_with_films(storage, &films)
}

pub(crate) fn list_actors_with_films(
    storage: &Storage,
    films: &[crate::catalog::CatalogFilm],
) -> Result<Vec<CatalogActor>> {
    storage.with_connection(|db| {
        let mut q = db.prepare("SELECT a.id,COALESCE(a.observed_name,''),COALESCE(m.favorite,0),COALESCE(m.birth_month,''),COALESCE(CAST(m.height_cm AS TEXT),''),COALESCE(m.cup,''),COALESCE(CAST(m.bust_cm AS TEXT),''),COALESCE(CAST(m.waist_cm AS TEXT),''),COALESCE(CAST(m.hip_cm AS TEXT),''),m.aliases,EXISTS(SELECT 1 FROM actor_image_sources p WHERE p.actor_id=a.id),strftime('%Y-%m-%dT%H:%M:%fZ',a.created_at),strftime('%Y-%m-%dT%H:%M:%fZ',a.modified_at) FROM actors a LEFT JOIN actor_manual m ON m.actor_id=a.id ORDER BY a.observed_name,a.id")?;
        let mut result = Vec::new();
        for row in q.query_map([], |r| Ok(CatalogActor {
            id:r.get(0)?,name:r.get(1)?,directory_name:r.get(1)?,favorite:r.get(2)?,birth_month:r.get(3)?,height:r.get(4)?,cup:r.get(5)?,bust:r.get(6)?,waist:r.get(7)?,hips:r.get(8)?,aliases:r.get(9)?,has_portrait:r.get(10)?,
            library:String::new(),sources:Vec::new(),directory_created_at:r.get(11)?,directory_modified_at:r.get(12)?,film_ids:Vec::new(),tags:Vec::new(),drawable:true,
        }))? {
            let mut actor = row?;
            let mut sources = BTreeMap::new();
            for film in films.iter().filter(|f| f.actors.iter().any(|a| a.id == actor.id)) {
                for video in &film.videos { sources.insert(video.source_id.clone(), video.source_name.clone()); }
            }
            let mut links = db.prepare("SELECT DISTINCT COALESCE(al.film_id,fa.film_id) FROM film_actors fa LEFT JOIN film_aliases al ON al.id=fa.film_id WHERE fa.actor_id=?1 ORDER BY 1")?;
            actor.film_ids=links.query_map([&actor.id],|r|r.get(0))?.collect::<rusqlite::Result<_>>()?;
            actor.tags=db.prepare("SELECT t.name FROM actor_tag_links l JOIN actor_tags t ON t.name=l.tag WHERE l.actor_id=?1 ORDER BY t.position,t.name")?.query_map([&actor.id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            actor.library=sources.values().cloned().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join("、");
            actor.sources=sources.into_iter().map(|(id,name)|ActorSource{id,name}).collect();
            result.push(actor);
        }
        Ok(result)
    }).map_err(|_| "actors_load_failed".into())
}

pub fn set_favorite(storage: &Storage, id: &str, favorite: bool) -> Result<()> {
    storage.with_transaction(|tx| {
        let changed=tx.execute("INSERT INTO actor_manual(actor_id,favorite) SELECT id,?2 FROM actors WHERE id=?1 ON CONFLICT(actor_id) DO UPDATE SET favorite=excluded.favorite",params![id,favorite])?;
        if changed!=1 {return Err(rusqlite::Error::QueryReturnedNoRows)} Ok(())
    }).map_err(|_|"actor_save_failed".into())
}

/// Delete only the explicit actor identities. A failed item rolls back the batch.
/// External media/photo paths are data; this operation never touches those files.
pub fn delete_actors(storage: &Storage, actor_ids: &[String]) -> Result<usize> {
    let ids: BTreeSet<&String> = actor_ids.iter().collect();
    if ids.is_empty() { return Err("no_actors_selected".into()); }
    storage.with_connection(|db| {
        for id in &ids {
            if !db.query_row("SELECT EXISTS(SELECT 1 FROM actors WHERE id=?1)", [*id], |r| r.get::<_, bool>(0))? { return Err(rusqlite::Error::InvalidParameterName("actor_not_found".into())); }
        }
        Ok(())
    }).map_err(|_| "actor_not_found")?;
    storage.create_protection().map_err(|_| "protection_failed")?;
    crate::covers::with_invalidated_image_cache(storage, &ids.iter().map(|id| format!("actor:{id}")).collect::<Vec<_>>(), || storage.with_transaction(|tx| {
        for id in &ids {
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM actors WHERE id=?1)", [*id], |r| r.get(0))?;
            if !exists { return Err(rusqlite::Error::InvalidParameterName("actor_not_found".into())); }
        }
        for id in &ids {
            // Compatibility tables predate cascades; remove only this identity.
            tx.execute("DELETE FROM actor_directories WHERE actor_id=?1", [*id])?;
            tx.execute("DELETE FROM info_actor_links WHERE actor_id=?1", [*id])?;
            if tx.execute("DELETE FROM actors WHERE id=?1", [*id])? != 1 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
        }
        Ok(ids.len())
    }).map_err(|error| match error.downcast_ref::<rusqlite::Error>() {
        Some(rusqlite::Error::InvalidParameterName(code)) if code == "actor_not_found" => code.clone(),
        _ => "actor_delete_failed".into(),
    })).map_err(|error| if error=="actor_not_found" { error } else { "actor_delete_failed".into() })
}
pub fn save_profile(storage: &Storage, id: &str, profile: &Profile) -> Result<()> {
    save_details(storage, id, profile, None)
}
pub fn save_details(
    storage: &Storage,
    id: &str,
    profile: &Profile,
    aliases: Option<&str>,
) -> Result<()> {
    storage
        .with_transaction(|tx| {
            let (name,stored):(String,String)=tx.query_row("SELECT a.observed_name,COALESCE(m.aliases,'') FROM actors a LEFT JOIN actor_manual m ON m.actor_id=a.id WHERE a.id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
            let supplied=aliases.map(|value|crate::actor_management::clean_aliases(&name,&value.split('、').map(str::to_owned).collect::<Vec<_>>()).map_err(rusqlite::Error::InvalidParameterName)).transpose()?;
            let values=supplied.clone().unwrap_or_else(||stored.split('、').map(str::to_owned).collect());
            crate::actor_management::assert_unique(tx,&name,&values,Some(id))?;
            let supplied=supplied.map(|values|values.join("、"));
            save_profile_tx(tx, id, profile, supplied.as_deref())
        })
        .map_err(|e| {
            if e.to_string().contains("actor_duplicate_identity") {
                "actor_duplicate_identity".into()
            } else if e.to_string().contains("invalid_actor_profile") {
                "invalid_actor_profile".into()
            } else {
                "actor_save_failed".into()
            }
        })
}
pub(crate) fn save_profile_tx(
    tx: &Transaction<'_>,
    id: &str,
    profile: &Profile,
    aliases: Option<&str>,
) -> rusqlite::Result<()> {
    fn number(value: &str, min: u16, max: u16) -> rusqlite::Result<Option<u16>> {
        if value.is_empty() {
            return Ok(None);
        }
        if !value.bytes().all(|c| c.is_ascii_digit()) {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid_actor_profile".into(),
            ));
        }
        let n = value
            .parse::<u16>()
            .map_err(|_| rusqlite::Error::InvalidParameterName("invalid_actor_profile".into()))?;
        if !(min..=max).contains(&n) {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid_actor_profile".into(),
            ));
        }
        Ok(Some(n))
    }
    let height = number(&profile.height, 100, 230)?;
    let bust = number(&profile.bust, 40, 160)?;
    let waist = number(&profile.waist, 40, 160)?;
    let hips = number(&profile.hips, 40, 160)?;
    let cup = profile.cup.to_ascii_uppercase();
    if !cup.is_empty() && (cup.len() != 1 || !cup.bytes().all(|c| c.is_ascii_alphabetic())) {
        return Err(rusqlite::Error::InvalidParameterName(
            "invalid_actor_profile".into(),
        ));
    }

    let month = &profile.birth_month;
    let current: String = tx.query_row("SELECT strftime('%Y-%m','now','localtime')", [], |r| {
        r.get(0)
    })?;
    if !valid_birth_month(month, &current) {
        return Err(rusqlite::Error::InvalidParameterName(
            "invalid_actor_profile".into(),
        ));
    }
    let changed=tx.execute("INSERT INTO actor_manual(actor_id,birth_month,height_cm,cup,bust_cm,waist_cm,hip_cm) SELECT id,NULLIF(?2,''),?3,NULLIF(?4,''),?5,?6,?7 FROM actors WHERE id=?1 ON CONFLICT(actor_id) DO UPDATE SET birth_month=excluded.birth_month,height_cm=excluded.height_cm,cup=excluded.cup,bust_cm=excluded.bust_cm,waist_cm=excluded.waist_cm,hip_cm=excluded.hip_cm",params![id,month,height,cup,bust,waist,hips])?;
    if changed != 1 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    if let Some(value) = aliases {
        if value.chars().count() > 1000 || value.contains(['\0', '\r', '\n']) {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid_actor_profile".into(),
            ));
        }
        tx.execute(
            "UPDATE actor_manual SET aliases=?2 WHERE actor_id=?1",
            params![id, value.trim()],
        )?;
    }
    Ok(())
}

pub fn thumbnail(storage: &Storage, actor_id: &str, large: bool) -> Result<Vec<u8>> {
    let (path, fingerprint) = storage
        .with_connection(|db| {
            db.query_row(
                "SELECT path,fingerprint FROM actor_image_sources WHERE actor_id=?1",
                [actor_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
        })
        .map_err(|_| "cover_missing")?;
    crate::covers::source_thumbnail(storage, &format!("actor:{actor_id}"), std::path::Path::new(&path), &fingerprint, large)
}
pub(crate) fn request_key(storage: &Storage, id: &str) -> String {
    storage.with_connection(|db| db.query_row("SELECT path FROM actor_image_sources WHERE actor_id=?1", [id], |r| r.get::<_,String>(0)))
        .map(|path| format!("path:{}",crate::directory_roles::identity(&path))).unwrap_or_else(|_|format!("actor:{id}"))
}

/// Shared actor identity/portrait for all views, after verifying film and video ownership.
pub fn film_thumbnail(
    storage: &Storage,
    actor_id: &str,
    film_id: &str,
    video_id: &str,
) -> Result<Vec<u8>> {
    storage.with_connection(|db| {
        let film: String = db.query_row("SELECT COALESCE(a.film_id,f.id) FROM films f LEFT JOIN film_aliases a ON a.id=f.id WHERE f.id=?1", [film_id], |r| r.get(0))?;
        if !film_links(db)?.get(&film).is_some_and(|actors| actors.iter().any(|(id, _)| id == actor_id)) {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM file_observations o JOIN libraries l ON l.id=o.library_id LEFT JOIN film_aliases a ON a.id=o.film_id WHERE o.id=?1 AND o.kind='video' AND COALESCE(a.film_id,o.film_id)=?2 AND l.enabled=1 AND l.removed=0)", params![video_id,film], |r| r.get(0))?;
        if !valid { return Err(rusqlite::Error::QueryReturnedNoRows) }
        Ok(())
    }).map_err(|_| "cover_not_owned")?;
    thumbnail(storage, actor_id, false)
}

#[cfg(test)]
mod management_tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Storage) {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::open(root.path()).unwrap();
        storage.with_connection(|db| db.execute_batch("CREATE TABLE IF NOT EXISTS actor_tags(name TEXT PRIMARY KEY COLLATE NOCASE,position INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS actor_tag_links(actor_id TEXT NOT NULL REFERENCES actors(id) ON DELETE CASCADE,tag TEXT NOT NULL REFERENCES actor_tags(name) ON UPDATE CASCADE ON DELETE CASCADE,PRIMARY KEY(actor_id,tag)); CREATE TABLE IF NOT EXISTS actor_image_sources(actor_id TEXT PRIMARY KEY REFERENCES actors(id) ON DELETE CASCADE,path TEXT NOT NULL,fingerprint TEXT NOT NULL); INSERT INTO actors(id,observed_name) VALUES('a','甲'),('b','乙'),('c','丙'); INSERT INTO films(id) VALUES('film'); INSERT INTO film_manual(film_id,favorite) VALUES('film',1); INSERT INTO tags(id,name) VALUES('film-tag','影片标签'); INSERT INTO film_tags(film_id,tag_id) VALUES('film','film-tag'); INSERT INTO film_actors(film_id,actor_id,source) VALUES('film','a','manual'),('film','b','manual'),('film','c','manual'); INSERT INTO actor_manual(actor_id,favorite,aliases) VALUES('a',1,'旧别名'); INSERT INTO actor_image_sources(actor_id,path,fingerprint) VALUES('a','external.jpg','path'); INSERT INTO actor_tags(name,position) VALUES('演员标签',0); INSERT INTO actor_tag_links(actor_id,tag) VALUES('a','演员标签'),('c','演员标签'); INSERT INTO info_actor_links(external_id,actor_id) VALUES('external-id','a'); INSERT INTO actor_directories(actor_id,directory_key,directory_name) VALUES('a','legacy-key','甲');")).unwrap();
        (root, storage)
    }

    #[test]
    fn delete_actors_removes_only_explicit_unique_records_and_preserves_other_data_and_originals() {
        let (root, storage) = fixture();
        let external = root.path().join("external.jpg");
        std::fs::write(&external, b"original bytes").unwrap();
        storage.with_connection(|db| db.execute("UPDATE actor_image_sources SET path=?1", [external.to_string_lossy().as_ref()])).unwrap();
        assert_eq!(delete_actors(&storage, &["a".into(), "b".into(), "a".into()]).unwrap(), 2);
        for table in ["actor_manual", "actor_image_sources", "actor_directories", "info_actor_links"] {
            assert_eq!(storage.with_connection(|db| db.query_row(&format!("SELECT COUNT(*) FROM {table} WHERE actor_id='a'"), [], |r| r.get::<_, i64>(0))).unwrap(), 0, "{table}");
        }
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM actors WHERE id='c'", [], |r| r.get::<_, i64>(0))).unwrap(), 1);
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM film_actors WHERE actor_id='c'", [], |r| r.get::<_, i64>(0))).unwrap(), 1);
        for table in ["films", "film_manual", "tags", "film_tags", "actor_tags", "actor_tag_links"] {
            assert_eq!(storage.with_connection(|db| db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0))).unwrap(), 1, "{table}");
        }
        assert_eq!(std::fs::read(&external).unwrap(), b"original bytes");
        assert_eq!(storage.list_protections().unwrap().len(), 1);
    }

    #[test]
    fn delete_actors_missing_id_and_transaction_failure_rollback_entire_batch() {
        let (_root, storage) = fixture();
        assert_eq!(delete_actors(&storage, &["a".into(), "missing".into()]).unwrap_err(), "actor_not_found");
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM actors", [], |r| r.get::<_, i64>(0))).unwrap(), 3);
        storage.with_connection(|db| db.execute_batch("CREATE TRIGGER reject_actor_delete BEFORE DELETE ON actors WHEN OLD.id='b' BEGIN SELECT RAISE(ABORT,'injected failure'); END;")).unwrap();
        assert_eq!(delete_actors(&storage, &["a".into(), "b".into()]).unwrap_err(), "actor_delete_failed");
        for table in ["actor_manual", "actor_image_sources", "actor_directories", "info_actor_links"] {
            assert_eq!(storage.with_connection(|db| db.query_row(&format!("SELECT COUNT(*) FROM {table} WHERE actor_id='a'"), [], |r| r.get::<_, i64>(0))).unwrap(), 1, "{table}");
        }
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM actors", [], |r| r.get::<_, i64>(0))).unwrap(), 3);
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM film_actors", [], |r| r.get::<_, i64>(0))).unwrap(), 3);
    }

    #[test]
    fn delete_actors_protection_failure_and_empty_ids_leave_database_unchanged() {
        let (root, storage) = fixture();
        assert_eq!(delete_actors(&storage, &[]).unwrap_err(), "no_actors_selected");
        assert!(storage.list_protections().unwrap().is_empty());
        std::fs::write(root.path().join("protections"), b"blocker").unwrap();
        assert_eq!(delete_actors(&storage, &["a".into()]).unwrap_err(), "protection_failed");
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM actors", [], |r| r.get::<_, i64>(0))).unwrap(), 3);
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM actor_image_sources", [], |r| r.get::<_, i64>(0))).unwrap(), 1);
    }
}
