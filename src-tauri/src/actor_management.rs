//! Independent actor records and read-only import. SQLite stores photo paths only.
use crate::{
    actors::{self, Profile},
    actor_info,
    storage::Storage,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tauri::Manager;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

type Result<T> = std::result::Result<T, String>;
const MAX_PHOTO: usize = 32 * 1024 * 1024;
const MAX_JPEG: usize = 4 * 1024 * 1024;
const MANIFEST: &str = ".thearchive-actor.json";
static PHOTO_DECODER: Mutex<()> = Mutex::new(());

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveActorInput {
    pub actor_id: Option<String>,
    pub name: String,
    pub aliases: Vec<String>,
    pub profile: Profile,
    pub photo_path: Option<String>,
    pub remove_photo: bool,
    pub film_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    pub key: String,
    pub directory: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub candidate_ids: Vec<String>,
    pub default_actor_id: Option<String>,
    pub status: String,
    pub reason: Option<String>,
    pub profile: Value,
    pub can_create: bool,
    pub name_alias_signature: String,
    pub filmography: Vec<FilmMatch>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilmMatch {
    pub folder_name: String,
    pub video_files: Vec<String>,
    pub film_id: Option<String>,
    pub existing_actor_ids: Vec<String>,
    pub reason: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmatchedFilm {
    pub actor_name: String,
    pub folder_name: String,
    pub video_files: Vec<String>,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RelationshipResult {
    pub available: usize,
    pub existing: usize,
    pub not_found: usize,
    pub unmatched: Vec<UnmatchedFilm>,
}
#[derive(Debug, Serialize)]
pub struct ImportPreview {
    pub token: String,
    pub root: String,
    pub items: Vec<ImportItem>,
}
#[derive(Debug, Serialize, Default)]
pub struct ScanAddResult {
    pub imported: usize,
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
    pub conflicts: usize,
    pub relationships: RelationshipResult,
    pub artwork_keys: Vec<String>,
    pub successful_artwork_keys: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScanResolution {
    pub key: String,
    pub actor_id: Option<String>,
    pub create_new: bool,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportItem {
    pub actor_id: String,
    pub name: String,
    pub directory: String,
    pub has_photo: bool,
    pub exists: bool,
    pub previous_directories: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct ExportPreview {
    pub token: String,
    pub directory: String,
    pub items: Vec<ExportItem>,
}
#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub exported: usize,
    pub directory: String,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn sql_error(code: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(code.into())
}
fn mapped_error(e: impl std::fmt::Display, fallback: &str) -> String {
    let text = e.to_string();
    for code in [
        "actor_import_changed",
        "actor_import_selection_invalid",
        "actor_identity_conflict",
        "actor_duplicate_identity",
        "actor_batch_conflict",
        "actor_not_found",
        "invalid_actor_profile",
        "film_not_found",
        "actor_invalid_name",
    ] {
        if text.contains(code) {
            return code.into();
        }
    }
    fallback.into()
}
fn valid_text(value: &str, limit: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= limit
        && !value.chars().any(char::is_control)
}
pub(crate) fn clean_aliases(name: &str, aliases: &[String]) -> Result<Vec<String>> {
    if aliases.len() > 100 {
        return Err("actor_invalid_name".into());
    }
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for member in aliases {
        // SQLite and manual inputs use separators between independent alias values.
        // Reject controls before trimming or splitting so they cannot be hidden.
        if member.chars().any(char::is_control) {
            return Err("actor_invalid_name".into());
        }
        for alias in member.split(['、', '，', ',']) {
            let alias = alias.trim();
            if alias.is_empty() {
                continue;
            }
            if !valid_text(alias, 200) {
                return Err("actor_invalid_name".into());
            }
            if normalize(alias) != normalize(name) && seen.insert(normalize(alias)) {
                result.push(alias.to_owned());
            }
        }
    }
    if result.len() > 100 || result.join("、").chars().count() > 1000 {
        return Err("actor_invalid_name".into());
    }
    Ok(result)
}
fn redirected(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}
fn safe_directory(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("actor_unsafe_path".into());
    }
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part);
        if matches!(part, Component::Prefix(_) | Component::RootDir) {
            continue;
        }
        let metadata = fs::symlink_metadata(&current).map_err(|_| "actor_directory_unavailable")?;
        if redirected(&metadata) || !metadata.is_dir() {
            return Err("actor_unsafe_path".into());
        }
    }
    Ok(())
}
fn read_regular(path: &Path, limit: usize) -> Result<Vec<u8>> {
    safe_directory(path.parent().ok_or("actor_unsafe_path")?)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| "actor_import_failed")?;
    if redirected(&metadata) || !metadata.is_file() {
        return Err("actor_unsafe_path".into());
    }
    if metadata.len() > limit as u64 {
        return Err("actor_photo_too_large".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| "actor_import_failed")?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "actor_import_failed")?;
    if bytes.len() > limit {
        return Err("actor_photo_too_large".into());
    }
    Ok(bytes)
}
fn validate_photo(bytes: &[u8]) -> Result<()> {
    let _guard = PHOTO_DECODER.lock().map_err(|_| "actor_photo_invalid")?;
    crate::covers::decode_original(bytes).map_err(|e| {
        if e == "cover_too_large" {
            "actor_photo_too_large"
        } else {
            "actor_photo_invalid"
        }
    })?;
    Ok(())
}
pub(crate) fn read_photo_source(path: &Path) -> Result<Vec<u8>> { read_regular(path, MAX_PHOTO) }
pub(crate) fn portrait_bytes(bytes: &[u8], large: bool) -> Result<Vec<u8>> {
    let _guard = PHOTO_DECODER.lock().map_err(|_| "cover_invalid")?;
    let image = crate::covers::decode_original(bytes)?;
    let size = (if large { 1200 } else { 480 }).min(image.width().max(image.height()));
    let rgb = image.thumbnail(size, size).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
        .encode_image(&rgb)
        .map_err(|_| "cover_invalid")?;
    if out.len() > MAX_JPEG {
        return Err("cover_too_large".into());
    }
    Ok(out)
}
fn profile(db: &Connection, id: &str) -> rusqlite::Result<Profile> {
    db.query_row("SELECT COALESCE(birth_month,''),COALESCE(CAST(height_cm AS TEXT),''),COALESCE(cup,''),COALESCE(CAST(bust_cm AS TEXT),''),COALESCE(CAST(waist_cm AS TEXT),''),COALESCE(CAST(hip_cm AS TEXT),'') FROM actor_manual WHERE actor_id=?1",[id],|r|Ok(Profile{birth_month:r.get(0)?,height:r.get(1)?,cup:r.get(2)?,bust:r.get(3)?,waist:r.get(4)?,hips:r.get(5)?})).optional().map(|p|p.unwrap_or_default())
}
fn validate_profile(db: &Connection, p: &Profile) -> rusqlite::Result<()> {
    for (s, min, max) in [
        (&p.height, 100, 230),
        (&p.bust, 40, 160),
        (&p.waist, 40, 160),
        (&p.hips, 40, 160),
    ] {
        if !s.is_empty()
            && (!s.bytes().all(|b| b.is_ascii_digit())
                || s.parse::<u16>().ok().is_none_or(|n| n < min || n > max))
        {
            return Err(sql_error("invalid_actor_profile"));
        }
    }
    if !p.cup.is_empty() && (p.cup.len() != 1 || !p.cup.bytes().all(|b| b.is_ascii_alphabetic())) {
        return Err(sql_error("invalid_actor_profile"));
    }
    let current: String = db.query_row("SELECT strftime('%Y-%m','now','localtime')", [], |r| {
        r.get(0)
    })?;
    if !actors::valid_birth_month(&p.birth_month, &current) {
        return Err(sql_error("invalid_actor_profile"));
    }
    Ok(())
}
fn store_photo(tx: &Transaction<'_>, id: &str, photo: &PhotoSource) -> rusqlite::Result<()> {
    tx.execute("INSERT INTO actor_image_sources(actor_id,path,fingerprint) VALUES(?1,?2,?3) ON CONFLICT(actor_id) DO UPDATE SET path=excluded.path,fingerprint=excluded.fingerprint",params![id,photo.path,photo.fingerprint])?;
    Ok(())
}
pub fn save(storage: &Storage, input: &SaveActorInput) -> Result<String> {
    let name = input.name.trim();
    if !valid_text(name, 200) {
        return Err("actor_invalid_name".into());
    }
    let aliases = clean_aliases(name, &input.aliases)?.join("、");
    if input.remove_photo && input.photo_path.is_some() {
        return Err("actor_import_selection_invalid".into());
    }
    let photo = input
        .photo_path
        .as_ref()
        .map(|p| {
            let bytes = read_photo_source(Path::new(p))?;
            validate_photo(&bytes)?;
            Ok::<_,String>(PhotoSource { path: p.clone(), fingerprint: hash(&bytes) })
        })
        .transpose()?;
    storage
        .with_connection(|db| {
            validate_profile(db, &input.profile)?;
            assert_unique(
                db,
                name,
                &clean_aliases(name, &input.aliases).map_err(|e| sql_error(&e))?,
                input.actor_id.as_deref(),
            )
        })
        .map_err(|e| mapped_error(e, "actor_save_failed"))?;
    storage
        .create_protection()
        .map_err(|_| "protection_failed")?;
    let changed: Vec<_> = input.actor_id.as_ref().filter(|_| input.photo_path.is_some() || input.remove_photo).map(|id|format!("actor:{id}")).into_iter().collect();
    crate::covers::with_invalidated_image_cache(storage,&changed,|| storage.with_transaction(|tx| {
        let id=if let Some(id)=&input.actor_id {
            if !tx.query_row("SELECT EXISTS(SELECT 1 FROM actors WHERE id=?1)",[id],|r|r.get::<_,bool>(0))? {return Err(sql_error("actor_not_found"));} id.clone()
        } else {tx.query_row("SELECT 'actor-'||lower(hex(randomblob(16)))",[],|r|r.get(0))?};
        assert_unique(tx,name,&clean_aliases(name,&input.aliases).map_err(|e|sql_error(&e))?,Some(&id))?;
        tx.execute("INSERT INTO actors(id,observed_name) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET observed_name=excluded.observed_name",params![id,name])?;
        actors::save_profile_tx(tx,&id,&input.profile,Some(&aliases))?;
        if input.remove_photo {tx.execute("DELETE FROM actor_image_sources WHERE actor_id=?1",[&id])?;}
        if let Some(source)=&photo {store_photo(tx,&id,source)?;}
        let mut desired=BTreeSet::new();
        for film in &input.film_ids {
            let canonical=tx.query_row("SELECT COALESCE(a.film_id,f.id) FROM films f LEFT JOIN film_aliases a ON a.id=f.id WHERE f.id=?1",[film],|r|r.get::<_,String>(0)).optional()?.ok_or_else(||sql_error("film_not_found"))?;
            desired.insert(canonical);
        }
        let mut links=tx.prepare("SELECT DISTINCT COALESCE(a.film_id,fa.film_id) FROM film_actors fa LEFT JOIN film_aliases a ON a.id=fa.film_id WHERE fa.actor_id=?1")?;
        let old=links.query_map([&id],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<BTreeSet<_>>>()?;drop(links);
        for film in old.symmetric_difference(&desired) {actors::freeze_cast(tx,film)?;}
        tx.execute("DELETE FROM film_actors WHERE actor_id=?1",[&id])?;
        for film in desired {tx.execute("INSERT INTO film_actors(film_id,actor_id,source) VALUES(?1,?2,'manual')",params![film,id])?;}
        Ok(id)
    }).map_err(|e|mapped_error(e,"actor_save_failed")))
}

#[derive(Clone)]
struct PhotoSource { path: String, fingerprint: String }
#[derive(Clone)]
struct ImportData {
    item: ImportItem,
    map: Map<String, Value>,
    photo: Option<PhotoSource>,
    evidence: String,
}
fn directory_key(path: &Path) -> String {
    crate::directory_roles::identity(&path.to_string_lossy())
}
fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}
fn signature(name: &str, aliases: &[String]) -> String {
    let names = aliases
        .iter()
        .map(|a| normalize(a))
        .filter(|a| !a.is_empty() && a != &normalize(name))
        .collect::<BTreeSet<_>>();
    serde_json::to_string(&(normalize(name), names)).expect("strings serialize")
}
fn saved_identities(db: &Connection) -> rusqlite::Result<Vec<(String, String, Vec<String>)>> {
    let mut q = db.prepare("SELECT a.id,COALESCE(a.observed_name,''),COALESCE(m.aliases,'') FROM actors a LEFT JOIN actor_manual m ON m.actor_id=a.id ORDER BY a.id")?;
    q.query_map([], |r| {
        Ok((
            r.get(0)?,
            r.get(1)?,
            r.get::<_, String>(2)?
                .split('、')
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
                .collect(),
        ))
    })?
    .collect()
}
pub(crate) fn assert_unique(
    db: &Connection,
    name: &str,
    aliases: &[String],
    excluding: Option<&str>,
) -> rusqlite::Result<()> {
    let desired = signature(name, aliases);
    if saved_identities(db)?
        .iter()
        .any(|(id, n, a)| Some(id.as_str()) != excluding && signature(n, a) == desired)
    {
        return Err(sql_error("actor_duplicate_identity"));
    }
    Ok(())
}
fn apply_fields(p: &mut Profile, m: &Map<String, Value>) {
    for (key, field) in [
        ("birth_month", &mut p.birth_month),
        ("height_cm", &mut p.height),
        ("cup", &mut p.cup),
        ("bust_cm", &mut p.bust),
        ("waist_cm", &mut p.waist),
        ("hip_cm", &mut p.hips),
    ] {
        if let Some(v) = m.get(key) {
            *field = if v.is_null() {
                String::new()
            } else if let Some(s) = v.as_str() {
                if key == "cup" {
                    s.trim().to_ascii_uppercase()
                } else {
                    s.trim().into()
                }
            } else {
                v.as_u64().unwrap_or_default().to_string()
            };
        }
    }
}
fn partial_profile(m: &Map<String, Value>) -> Value {
    let mut out = Map::new();
    for (key, target) in [
        ("birth_month", "birthMonth"),
        ("height_cm", "height"),
        ("cup", "cup"),
        ("bust_cm", "bust"),
        ("waist_cm", "waist"),
        ("hip_cm", "hips"),
    ] {
        if let Some(v) = m.get(key) {
            out.insert(
                target.into(),
                if v.is_null() {
                    json!("")
                } else if v.is_number() {
                    json!(v.as_u64().unwrap_or_default().to_string())
                } else {
                    v.clone()
                },
            );
        }
    }
    Value::Object(out)
}
fn db_fingerprint(db: &Connection) -> rusqlite::Result<String> {
    let mut bytes = Vec::new();
    for sql in [
        "SELECT json_array(a.id,a.observed_name,m.favorite,m.birth_month,m.height_cm,m.cup,m.bust_cm,m.waist_cm,m.hip_cm,m.aliases,p.path,p.fingerprint) FROM actors a LEFT JOIN actor_manual m ON m.actor_id=a.id LEFT JOIN actor_image_sources p ON p.actor_id=a.id ORDER BY a.id",
        "SELECT json_array(film_id,actor_id,source) FROM film_actors ORDER BY film_id,actor_id,source",
        "SELECT json_array(id,film_id,library_id,kind,observed_path) FROM file_observations WHERE kind='video' ORDER BY id",
        "SELECT json_array(id,root_path,enabled,removed) FROM libraries ORDER BY id",
        "SELECT json_array(id,film_id) FROM film_aliases ORDER BY id",
        "SELECT json_array(subject_id,evidence) FROM issue_records WHERE subject_type='cover_scan' AND evidence_key='current' ORDER BY subject_id",
    ] {
        let mut q = db.prepare(sql)?;
        for row in q.query_map([], |r| r.get::<_, String>(0))? {
            bytes.extend(row?.as_bytes());
            bytes.push(0);
        }
    }
    Ok(hash(&bytes))
}

/// One descriptor per scanned directory, never a union across parent/child directories.
fn film_reference_index(db: &Connection, active_only: bool) -> rusqlite::Result<BTreeMap<actor_info::FilmReference, BTreeSet<String>>> {
    let snapshot = crate::artwork::ScanSnapshot::read(db)?;
    let mut q = db.prepare("SELECT o.id,o.library_id,COALESCE(a.film_id,o.film_id),o.observed_path,l.root_path FROM file_observations o JOIN libraries l ON l.id=o.library_id LEFT JOIN film_aliases a ON a.id=o.film_id WHERE o.kind='video' AND o.film_id IS NOT NULL AND (?1=0 OR (l.enabled=1 AND l.removed=0)) ORDER BY o.id")?;
    let mut directories = BTreeMap::<(String,String,String), (String,BTreeSet<String>)>::new();
    for row in q.query_map([active_only], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,Option<String>>(4)?)))? {
        let (id,source,film,path,root) = row?;
        if !snapshot.contains(&source,&id) { continue; }
        let parent = crate::folder_groups::parent(&path);
        let folder = if parent.is_empty() {
            root.as_deref().and_then(|root| Path::new(root).file_name()).map(|n|n.to_string_lossy().into_owned()).unwrap_or_default()
        } else { parent.rsplit('/').next().unwrap_or_default().into() };
        let filename = path.rsplit(['/', '\\']).next().unwrap_or_default().to_owned();
        let group = directories.entry((source,parent,film)).or_insert_with(||(folder,BTreeSet::new()));
        group.1.insert(filename);
    }
    let mut index = BTreeMap::<actor_info::FilmReference,BTreeSet<String>>::new();
    for ((_,_,film),(folder,videos)) in directories {
        if !folder.is_empty() && !videos.is_empty() {
            index.entry(actor_info::FilmReference { folder_name:folder,video_files:videos.into_iter().collect() }).or_default().insert(film);
        }
    }
    Ok(index)
}

fn match_filmography(map: &Map<String,Value>, index: &BTreeMap<actor_info::FilmReference,BTreeSet<String>>, cast: &BTreeMap<String,Vec<String>>) -> Result<Vec<FilmMatch>> {
    let mut matches = Vec::new();
    let mut films = BTreeSet::new();
    for reference in actor_info::filmography(map)? {
        let candidates = index.get(&reference);
        let film = candidates.filter(|ids|ids.len()==1).and_then(|ids|ids.first()).cloned();
        if let Some(id) = &film { if !films.insert(id.clone()) { continue; } }
        let reason = if film.is_some() { None } else { Some(if candidates.is_some_and(|ids|ids.len()>1) { "找到多个相同条目，未关联" } else { "未找到完全匹配的影片" }.into()) };
        matches.push(FilmMatch { folder_name:reference.folder_name,video_files:reference.video_files,existing_actor_ids:film.as_ref().and_then(|id|cast.get(id)).cloned().unwrap_or_default(),film_id:film,reason });
    }
    Ok(matches)
}
fn build_import(db: &Connection, root: &Path) -> Result<(String, Vec<ImportData>)> {
    safe_directory(root)?;
    let identities = saved_identities(db).map_err(|_| "actor_import_failed")?;
    let films = film_reference_index(db,true).map_err(|_| "actor_import_failed")?;
    let mut cast = BTreeMap::<String,Vec<String>>::new();
    let mut cast_query = db.prepare("SELECT DISTINCT COALESCE(a.film_id,fa.film_id),fa.actor_id FROM film_actors fa LEFT JOIN film_aliases a ON a.id=fa.film_id ORDER BY 1,2").map_err(|_| "actor_import_failed")?;
    for row in cast_query.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).map_err(|_| "actor_import_failed")? {
        let (film,actor) = row.map_err(|_| "actor_import_failed")?;
        cast.entry(film).or_default().push(actor);
    }
    let mut pending = vec![root.to_path_buf()];
    let mut data = Vec::new();
    while let Some(dir) = pending.pop() {
        safe_directory(&dir)?;
        let mut entries = fs::read_dir(&dir)
            .map_err(|_| "actor_import_failed")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| "actor_import_failed")?;
        entries.sort_by_key(|e| e.file_name());
        let mut photos = Vec::new();
        let mut infos = Vec::new();
        for entry in &entries {
            let md = fs::symlink_metadata(entry.path()).map_err(|_| "actor_import_failed")?;
            if md.is_dir() && !redirected(&md) {
                pending.push(entry.path());
            }
            let filename = entry.file_name().to_string_lossy().into_owned();
            if filename.eq_ignore_ascii_case("actor.info") {
                infos.push(entry.path());
            }
            if !md.is_dir()
                && Path::new(&filename)
                    .file_stem()
                    .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("actor"))
            {
                if let Some(rank) = crate::artwork::image_rank(&filename) {
                    photos.push((rank, filename.to_lowercase(), entry.path()));
                }
            }
        }
        if infos.is_empty() {
            continue;
        }
        photos.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
        let mut item = ImportItem {
            key: directory_key(&dir),
            directory: dir.to_string_lossy().into_owned(),
            name: String::new(),
            aliases: vec![],
            candidate_ids: vec![],
            default_actor_id: None,
            status: "new".into(),
            reason: None,
            profile: json!({}),
            can_create: true,
            name_alias_signature: String::new(),
            filmography: vec![],
        };
        let mut raw = Vec::new();
        let mut map = Map::new();
        let mut photo = None;
        // Bind every candidate, including corrupt and nonselected fallback files.
        let mut info_bytes = Vec::new();
        let mut photo_bytes = Vec::new();
        for path in &infos {
            raw.extend(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .as_bytes(),
            );
            raw.push(0);
            let read = read_regular(path, 1024 * 1024);
            match &read {
                Ok(bytes) => raw.extend(bytes),
                Err(e) => raw.extend(e.as_bytes()),
            };
            raw.push(0);
            info_bytes.push(read);
        }
        for (_, _, path) in &photos {
            raw.extend(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .as_bytes(),
            );
            raw.push(0);
            let read = read_regular(path, MAX_PHOTO);
            match &read {
                Ok(bytes) => raw.extend(bytes),
                Err(e) => raw.extend(e.as_bytes()),
            };
            raw.push(0);
            photo_bytes.push(read);
        }
        let checked = (|| -> Result<()> {
            if infos.len() != 1 {
                return Err("info_invalid".into());
            }
            let bytes = info_bytes[0].as_ref().map_err(Clone::clone)?;
            let text = std::str::from_utf8(bytes).map_err(|_| "info_invalid")?;
            map = actor_info::parse(text)?;
            item.filmography = match_filmography(&map,&films,&cast)?;
            let name = map
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|n| valid_text(n, 200))
                .ok_or("actor_invalid_name")?;
            item.name = name.into();
            let aliases = map
                .get("aliases")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|v| v.as_str().unwrap_or_default().into())
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();
            item.aliases = clean_aliases(name, &aliases)?;
            item.profile = partial_profile(&map);
            item.name_alias_signature = signature(name, &item.aliases);
            let incoming = std::iter::once(normalize(name))
                .chain(item.aliases.iter().map(|a| normalize(a)))
                .collect::<BTreeSet<_>>();
            let mut exact_ids = Vec::new();
            for (id, n, a) in &identities {
                if std::iter::once(n)
                    .chain(a.iter())
                    .any(|n| incoming.contains(&normalize(n)))
                {
                    item.candidate_ids.push(id.clone());
                }
                if signature(n, a) == item.name_alias_signature {
                    item.can_create = false;
                    exact_ids.push(id.clone());
                }
            }
            if exact_ids.len() == 1 {
                item.status = "update".into();
                item.default_actor_id = exact_ids.pop();
            } else if !item.candidate_ids.is_empty() {
                item.status = "conflict".into();
                item.reason = Some(
                    if item.can_create {
                        "actor_name_conflict"
                    } else {
                        "actor_duplicate_identity"
                    }
                    .into(),
                );
            }
            let mut p = Profile::default();
            apply_fields(&mut p, &map);
            validate_profile(db, &p).map_err(|e| mapped_error(e, "info_invalid"))?;
            for ((_, _, path), bytes) in photos.iter().zip(&photo_bytes) {
                if let Ok(bytes) = bytes {
                    if validate_photo(bytes).is_ok() {
                        photo = Some(PhotoSource { path: path.to_string_lossy().into_owned(), fingerprint: hash(bytes) });
                        break;
                    }
                }
            }
            if !photos.is_empty() && photo.is_none() {
                return Err("actor_photo_invalid".into());
            }
            Ok(())
        })();
        if let Err(reason) = checked {
            crate::diagnostics::record_code("actor.import", &reason);
            item.status = "failed".into();
            item.reason = Some(reason);
            item.can_create = false;
        }
        data.push(ImportData {
            item,
            map,
            photo,
            evidence: hash(&raw),
        });
    }
    data.sort_by(|a, b| a.item.key.cmp(&b.item.key));
    let evidence = data
        .iter()
        .map(|d| json!([d.item.key, d.evidence]))
        .collect::<Vec<_>>();
    let token = hash(
        &serde_json::to_vec(&json!([
            directory_key(root),
            db_fingerprint(db).map_err(|_| "actor_import_failed")?,
            evidence
        ]))
        .map_err(|_| "actor_import_failed")?,
    );
    Ok((token, data))
}
pub fn preview_import(storage: &Storage, root: &Path) -> Result<ImportPreview> {
    storage
        .with_connection(|db| build_import(db, root).map_err(|e| sql_error(&e)))
        .map(|(token, data)| ImportPreview {
            token,
            root: root.to_string_lossy().into_owned(),
            items: data.into_iter().map(|d| d.item).collect(),
        })
        .map_err(|e| {
            e.to_string()
                .strip_prefix("Invalid parameter name: ")
                .unwrap_or("actor_import_failed")
                .to_owned()
        })
}
struct PlannedWrite {
    data: ImportData,
    actor_id: Option<String>,
    aliases: Vec<String>,
    profile: Profile,
    photo_changed: bool,
    film_ids: Vec<String>,
}
fn plan_writes(
    db: &Connection,
    data: Vec<ImportData>,
    resolutions: &[ScanResolution],
) -> rusqlite::Result<(Vec<PlannedWrite>, ScanAddResult)> {
    let mut keys = BTreeSet::new();
    for r in resolutions {
        let d = data
            .iter()
            .find(|d| d.item.key == r.key && d.item.status != "failed")
            .ok_or_else(|| sql_error("actor_import_selection_invalid"))?;
        if !keys.insert(&r.key)
            || (r.create_new && (r.actor_id.is_some() || !d.item.can_create))
            || r.actor_id
                .as_ref()
                .is_some_and(|id| !d.item.candidate_ids.contains(id))
        {
            return Err(sql_error("actor_import_selection_invalid"));
        }
    }
    let mut writes = Vec::new();
    let mut result = ScanAddResult::default();
    let mut touched = BTreeSet::new();
    let mut signatures = BTreeSet::new();
    for d in data {
        if d.item.status == "failed" {
            result.failed += 1;
            continue;
        }
        let choice = resolutions.iter().find(|r| r.key == d.item.key);
        if d.item.status == "conflict" && choice.is_none() {
            return Err(sql_error("actor_import_selection_invalid"));
        }
        if choice.is_some_and(|r| !r.create_new && r.actor_id.is_none()) {
            result.skipped += 1;
            if d.item.status == "conflict" {
                result.conflicts += 1;
            }
            continue;
        }
        let id = match choice {
            Some(r) => r.actor_id.clone(),
            None if d.item.status == "update" => d.item.default_actor_id.clone(),
            None => None,
        };
        if let Some(id) = &id {
            if !touched.insert(id.clone()) {
                return Err(sql_error("actor_batch_conflict"));
            }
        }
        let mut p = if let Some(id) = &id {
            profile(db, id)?
        } else {
            Profile::default()
        };
        let previous = p.clone();
        apply_fields(&mut p, &d.map);
        validate_profile(db, &p)?;
        let previous_aliases = if let Some(id) = &id {
            db.query_row(
                "SELECT COALESCE(aliases,'') FROM actor_manual WHERE actor_id=?1",
                [id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_default()
            .split('、')
            .filter(|a| !a.trim().is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
        } else {
            vec![]
        };
        let aliases = if d.map.contains_key("aliases") {
            d.item.aliases.clone()
        } else {
            previous_aliases.clone()
        };
        assert_unique(db, &d.item.name, &aliases, id.as_deref())?;
        if !signatures.insert(signature(&d.item.name, &aliases)) {
            return Err(sql_error("actor_batch_conflict"));
        }
        let old_name = if let Some(id) = &id {
            Some(
                db.query_row("SELECT observed_name FROM actors WHERE id=?1", [id], |r| {
                    r.get::<_, String>(0)
                })?,
            )
        } else {
            None
        };
        let old_photo = if let Some(id) = &id {
            db.query_row(
                "SELECT path,fingerprint FROM actor_image_sources WHERE actor_id=?1",
                [id],
                |r| Ok(PhotoSource { path: r.get(0)?, fingerprint: r.get(1)? }),
            )
            .optional()?
        } else {
            None
        };
        let photo_changed = d
            .photo
            .as_ref()
            .is_some_and(|photo| old_photo.as_ref().is_none_or(|old| old.path != photo.path || old.fingerprint != photo.fingerprint));
        let mut film_ids = Vec::new();
        for relation in &d.item.filmography {
            if let Some(film) = &relation.film_id {
                if id.as_ref().is_some_and(|id| relation.existing_actor_ids.contains(id)) {
                    result.relationships.existing += 1;
                } else {
                    result.relationships.available += 1;
                    film_ids.push(film.clone());
                }
            } else {
                result.relationships.not_found += 1;
                result.relationships.unmatched.push(UnmatchedFilm { actor_name:d.item.name.clone(),folder_name:relation.folder_name.clone(),video_files:relation.video_files.clone(),reason:relation.reason.clone().unwrap_or_default() });
            }
        }
        if let Some(id) = &id { result.successful_artwork_keys.push(format!("actor:{id}")); }
        if id.is_some()
            && old_name.as_ref() == Some(&d.item.name)
            && aliases == previous_aliases
            && p == previous
            && !photo_changed
            && film_ids.is_empty()
        {
            result.skipped += 1;
            continue;
        }
        writes.push(PlannedWrite {
            data: d,
            actor_id: id,
            aliases,
            profile: p,
            photo_changed,
            film_ids,
        });
    }
    Ok((writes, result))
}
/// Confirm personal data and append missing cast in the same protected transaction.
pub fn confirm_scan_add(
    storage: &Storage,
    root: &Path,
    token: &str,
    resolutions: &[ScanResolution],
) -> Result<ScanAddResult> {
    let (writes, result) = storage
        .with_connection(|db| {
            let (current, data) = build_import(db, root).map_err(|e| sql_error(&e))?;
            if current != token {
                return Err(sql_error("actor_import_changed"));
            }
            plan_writes(db, data, resolutions)
        })
        .map_err(|e| mapped_error(e, "actor_import_failed"))?;
    if writes.is_empty() {
        return Ok(result);
    }
    storage
        .create_protection()
        .map_err(|_| "protection_failed")?;
    let changed: Vec<_> = writes.iter().filter(|write| write.photo_changed).filter_map(|write| write.actor_id.as_ref().map(|id| format!("actor:{id}"))).collect();
    crate::covers::with_invalidated_image_cache(storage, &changed, || storage.with_transaction(|tx| {
        let (current,data)=build_import(tx,root).map_err(|e|sql_error(&e))?;
        if current!=token {return Err(sql_error("actor_import_changed"));}
        let (writes,mut result)=plan_writes(tx,data,resolutions)?;
        for write in writes {
            let created=write.actor_id.is_none();
            let id=match write.actor_id {Some(id)=>id,None=>tx.query_row("SELECT 'actor-'||lower(hex(randomblob(16)))",[],|r|r.get::<_,String>(0))?};
            tx.execute("INSERT INTO actors(id,observed_name) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET observed_name=excluded.observed_name",params![id,write.data.item.name])?;
            actors::save_profile_tx(tx,&id,&write.profile,Some(&write.aliases.join("、")))?;
            if write.photo_changed {store_photo(tx,&id,write.data.photo.as_ref().ok_or_else(||sql_error("actor_photo_invalid"))?)?;}
            if write.photo_changed { result.artwork_keys.push(format!("actor:{id}")); }
            result.successful_artwork_keys.push(format!("actor:{id}"));
            for film in write.film_ids {
                tx.execute("INSERT INTO film_actors(film_id,actor_id,source) VALUES(?1,?2,'manual')",params![film,id])?;
                tx.execute("INSERT INTO film_manual(film_id,cast_override) VALUES(?1,1) ON CONFLICT(film_id) DO UPDATE SET cast_override=1",[film])?;
            }
            if created {result.imported+=1;} else {result.updated+=1;}
        }
        Ok(result)
    }).map_err(|e|mapped_error(e,"actor_import_failed")))
}

pub fn export_directory(storage: &Storage) -> PathBuf {
    storage.data_directory().join("ActorExports")
}
fn safe_name(name: &str) -> String {
    let cleaned = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(80)
        .collect::<String>();
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']);
    let mut out = if cleaned.is_empty() {
        "演员".into()
    } else {
        cleaned.to_owned()
    };
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&stem.as_str())
    {
        out.insert(0, '_');
    }
    out
}
#[derive(Clone)]
struct ExportData {
    item: ExportItem,
    files: BTreeMap<String, Vec<u8>>,
    expected: BTreeMap<String, BTreeMap<String, String>>,
}
fn actor_files(
    db: &Connection,
    id: &str,
    references: &BTreeMap<actor_info::FilmReference,BTreeSet<String>>,
) -> rusqlite::Result<(String, Vec<String>, BTreeMap<String, Vec<u8>>)> {
    let name: String = db.query_row("SELECT observed_name FROM actors WHERE id=?1", [id], |r| {
        r.get(0)
    })?;
    let aliases: Option<String> = db
        .query_row(
            "SELECT aliases FROM actor_manual WHERE actor_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let aliases = aliases
        .unwrap_or_default()
        .split('、')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let p = profile(db, id)?;
    let photo_path: Option<String> = db
        .query_row(
            "SELECT path FROM actor_image_sources WHERE actor_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let photo = photo_path.map(|path| {
        read_photo_source(Path::new(&path)).and_then(|bytes| portrait_bytes(&bytes, true))
            .map_err(|_| sql_error("actor_export_photo_unavailable"))
    }).transpose()?;
    let mut links=db.prepare("SELECT DISTINCT COALESCE(a.film_id,fa.film_id) FROM film_actors fa LEFT JOIN film_aliases a ON a.id=fa.film_id WHERE fa.actor_id=?1 ORDER BY 1")?;
    let film_ids = links
        .query_map([id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let filmography = references.iter().filter_map(|(reference,films)| films.iter().any(|film|film_ids.contains(film)).then_some(reference)).collect::<Vec<_>>();
    if film_ids.iter().any(|film| !references.values().any(|films| films.contains(film))) {
        return Err(sql_error("actor_export_film_unavailable"));
    }
    let num = |s: &str| s.parse::<u64>().ok();
    let doc = json!({"format_version":1,"kind":"actor","id":id,"name":name,"aliases":aliases,"birth_month":if p.birth_month.is_empty(){None}else{Some(p.birth_month.as_str())},"height_cm":num(&p.height),"cup":if p.cup.is_empty(){None}else{Some(p.cup.as_str())},"bust_cm":num(&p.bust),"waist_cm":num(&p.waist),"hip_cm":num(&p.hips),"photo":photo.as_ref().map(|_|"actor.jpg"),"film_ids":film_ids,"filmography":filmography});
    let info_bytes = serde_json::to_vec_pretty(&doc).map_err(|_|sql_error("actor_export_failed"))?;
    actor_info::parse(std::str::from_utf8(&info_bytes).map_err(|_|sql_error("actor_export_failed"))?).map_err(|e|sql_error(&e))?;
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::from([(
        "actor.info".into(),
        info_bytes,
    )]);
    if let Some(photo) = photo {
        files.insert("actor.jpg".into(), photo);
    }
    let digests = files
        .iter()
        .map(|(key, value)| (key.clone(), hash(value)))
        .collect::<BTreeMap<_, _>>();
    files.insert(
        MANIFEST.into(),
        serde_json::to_vec_pretty(&json!({"format_version":1,"actor_id":id,"files":digests}))
            .map_err(|_| sql_error("actor_export_failed"))?,
    );
    Ok((name, aliases, files))
}
fn owned_export(path: &Path) -> Result<(String, BTreeMap<String, String>)> {
    safe_directory(path).map_err(|_| "actor_export_unsafe")?;
    let manifest =
        read_regular(&path.join(MANIFEST), 1024 * 1024).map_err(|_| "actor_export_unsafe")?;
    let v: Value = serde_json::from_slice(&manifest).map_err(|_| "actor_export_unsafe")?;
    if v.get("format_version") != Some(&json!(1)) {
        return Err("actor_export_unsafe".into());
    }
    let id = v
        .get("actor_id")
        .and_then(Value::as_str)
        .filter(|id| valid_text(id, 200))
        .ok_or("actor_export_unsafe")?
        .to_owned();
    let declared = v
        .get("files")
        .and_then(Value::as_object)
        .ok_or("actor_export_unsafe")?;
    let cleanup_pending = v.get("cleanup_pending").and_then(Value::as_bool) == Some(true);
    if (!declared.contains_key("actor.info") && !declared.contains_key("actor.info.json"))
        || (declared.contains_key("actor.info") && declared.contains_key("actor.info.json"))
        || declared
            .keys()
            .any(|s| s != "actor.jpg" && s != "actor.info" && s != "actor.info.json")
    {
        return Err("actor_export_unsafe".into());
    }
    let mut actual = BTreeMap::from([(MANIFEST.into(), hash(&manifest))]);
    for entry in fs::read_dir(path).map_err(|_| "actor_export_unsafe")? {
        let entry = entry.map_err(|_| "actor_export_unsafe")?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == MANIFEST {
            continue;
        }
        let expected = declared
            .get(&name)
            .and_then(Value::as_str)
            .ok_or("actor_export_unsafe")?;
        let bytes = read_regular(&entry.path(), MAX_JPEG.max(1024 * 1024))
            .map_err(|_| "actor_export_unsafe")?;
        let digest = hash(&bytes);
        if digest != expected {
            return Err("actor_export_unsafe".into());
        }
        actual.insert(name, digest);
    }
    if !cleanup_pending && actual.len() != declared.len() + 1 {
        return Err("actor_export_unsafe".into());
    }
    Ok((id, actual))
}
fn build_export(
    db: &Connection,
    actor_ids: &[String],
    destination: &Path,
) -> Result<(String, Vec<ExportData>)> {
    safe_directory(destination).map_err(|_| "actor_export_unsafe")?;
    let metadata = fs::symlink_metadata(destination).map_err(|_| "actor_export_unsafe")?;
    if !metadata.is_dir() || redirected(&metadata) {
        return Err("actor_export_unsafe".into());
    }
    let root = destination.join("ActorExports");
    let exists = fs::symlink_metadata(&root)
        .map(|_| true)
        .or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(false)
            } else {
                Err(e)
            }
        })
        .map_err(|_| "actor_export_unsafe")?;
    if exists {
        safe_directory(&root).map_err(|_| "actor_export_unsafe")?;
    }
    let mut q = db
        .prepare("SELECT id FROM actors ORDER BY id")
        .map_err(|_| "actor_export_failed")?;
    let all = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|_| "actor_export_failed")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| "actor_export_failed")?;
    let selected = actor_ids.iter().cloned().collect::<BTreeSet<_>>();
    if selected.len() != actor_ids.len() || selected.iter().any(|id| !all.contains(id)) {
        return Err("actor_not_found".into());
    }
    let selected = if selected.is_empty() {
        all.iter().cloned().collect()
    } else {
        selected
    };
    let mut documents = Vec::new();
    let references = film_reference_index(db,false).map_err(|_| "actor_export_failed")?;
    let mut bases = BTreeMap::<String, usize>::new();
    for (_, name, aliases) in saved_identities(db).map_err(|_| "actor_export_failed")? {
        let label = if aliases.is_empty() { name } else { format!("{}（{}）",name,aliases.join("、")) };
        *bases.entry(safe_name(&label).to_lowercase()).or_default() += 1;
    }
    for id in all {
        if !selected.contains(&id) { continue; }
        let (name, aliases, files) = actor_files(db, &id, &references).map_err(|e| if e.to_string().contains("actor_export_film_unavailable") { "actor_export_film_unavailable" } else { "actor_export_failed" })?;
        let label = if aliases.is_empty() {
            name.clone()
        } else {
            format!("{}（{}）", name, aliases.join("、"))
        };
        let base = safe_name(&label);
        documents.push((id, name, base, files));
    }
    let mut old = BTreeMap::<String, Vec<(PathBuf, BTreeMap<String, String>)>>::new();
    if exists {
        for entry in fs::read_dir(&root).map_err(|_| "actor_export_failed")? {
            let entry = entry.map_err(|_| "actor_export_failed")?;
            if fs::symlink_metadata(entry.path().join(MANIFEST)).is_ok() {
                let (id, files) = owned_export(&entry.path())?;
                if selected.contains(&id) {
                    old.entry(id).or_default().push((entry.path(), files));
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut binding = Vec::new();
    for (id, name, base, files) in documents {
        if !selected.contains(&id) {
            continue;
        }
        let base = if bases[&base.to_lowercase()] > 1 {
            let suffix = hash(id.as_bytes());
            let suffix = &suffix[..12];
            if base.ends_with('）') {
                if let Some(index) = base.rfind('（') {
                    format!("{}-{}{}", &base[..index], suffix, &base[index..])
                } else {
                    format!("{}-{}", base, suffix)
                }
            } else {
                format!("{}-{}", base, suffix)
            }
        } else {
            base
        };
        let path = root.join(base);
        let target_exists = fs::symlink_metadata(&path).is_ok();
        if target_exists {
            let (owner, _) = owned_export(&path)?;
            if owner != id {
                return Err("actor_export_unsafe".into());
            }
        }
        let prior = old.remove(&id).unwrap_or_default();
        let previous = prior
            .iter()
            .filter(|(p, _)| p != &path)
            .map(|(p, _)| p.to_string_lossy().into_owned())
            .collect();
        binding.push(json!([
            id,
            path,
            files
                .iter()
                .map(|(n, b)| (n, hash(b)))
                .collect::<BTreeMap<_, _>>(),
            prior
        ]));
        let expected = prior
            .into_iter()
            .map(|(p, files)| (p.to_string_lossy().into_owned(), files))
            .collect();
        out.push(ExportData {
            item: ExportItem {
                actor_id: id,
                name,
                directory: path.to_string_lossy().into_owned(),
                has_photo: files.contains_key("actor.jpg"),
                exists: target_exists,
                previous_directories: previous,
            },
            files,
            expected,
        });
    }
    let token = hash(
        &serde_json::to_vec(&json!([
            destination,
            root,
            exists,
            db_fingerprint(db).map_err(|_| "actor_export_failed")?,
            binding
        ]))
        .map_err(|_| "actor_export_failed")?,
    );
    Ok((token, out))
}
pub fn preview_export(storage: &Storage, actor_ids: &[String]) -> Result<ExportPreview> {
    preview_export_to(storage, actor_ids, storage.data_directory())
}
pub fn preview_export_to(
    storage: &Storage,
    actor_ids: &[String],
    destination: &Path,
) -> Result<ExportPreview> {
    storage
        .with_connection(|db| build_export(db, actor_ids, destination).map_err(|e| sql_error(&e)))
        .map(|(token, data)| ExportPreview {
            token,
            directory: destination
                .join("ActorExports")
                .to_string_lossy()
                .into_owned(),
            items: data.into_iter().map(|d| d.item).collect(),
        })
        .map_err(|e| {
            e.to_string()
                .strip_prefix("Invalid parameter name: ")
                .unwrap_or("actor_export_failed")
                .into()
        })
}
fn preflight_cleanup(path: &Path, id: &str) -> Result<()> {
    let (owner, files) = owned_export(path)?;
    if owner != id {
        return Err("actor_export_unsafe".into());
    }
    for name in files.keys() {
        let file = path.join(name);
        let metadata = fs::symlink_metadata(&file).map_err(|_| "actor_export_failed")?;
        if metadata.permissions().readonly() {
            return Err("actor_export_failed".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // Request DELETE access without deleting: a handle denying delete-share blocks here.
            let _handle = OpenOptions::new()
                .read(true)
                .access_mode(0x00010000)
                .share_mode(0x7)
                .open(&file)
                .map_err(|_| "actor_export_failed")?;
        }
    }
    Ok(())
}
fn export_path_key(path: &Path) -> String {
    let normalized: PathBuf = path.components().collect();
    let value = normalized.to_string_lossy();
    #[cfg(windows)]
    {
        let value = value.replace('/', "\\").to_lowercase();
        if let Some(unc) = value.strip_prefix(r"\\?\unc\") {
            format!(r"\\{unc}")
        } else {
            value.strip_prefix(r"\\?\").unwrap_or(&value).to_owned()
        }
    }
    #[cfg(not(windows))]
    {
        value.into_owned()
    }
}
fn remove_owned(path: &Path, id: &str) -> Result<()> {
    let (owner, files) = owned_export(path)?;
    if owner != id {
        return Err("actor_export_unsafe".into());
    }
    // A durable cleanup marker permits a later retry after partially removed owned files.
    let manifest = path.join(MANIFEST);
    let mut document: Value = serde_json::from_slice(&read_regular(&manifest, 1024 * 1024)?)
        .map_err(|_| "actor_export_unsafe")?;
    document["cleanup_pending"] = json!(true);
    let bytes = serde_json::to_vec_pretty(&document).map_err(|_| "actor_export_failed")?;
    let temporary = path.join(".thearchive-cleanup.tmp");
    let written = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| "actor_export_failed")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "actor_export_failed")?;
        drop(file);
        fs::rename(&temporary, &manifest).map_err(|_| "actor_export_failed")?;
        Ok(())
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written?;
    for name in files.keys().filter(|name| name.as_str() != MANIFEST) {
        safe_directory(path).map_err(|_| "actor_export_unsafe")?;
        let expected = files.get(name).ok_or("actor_export_unsafe")?;
        if hash(&read_regular(&path.join(name), MAX_JPEG.max(1024 * 1024))?) != *expected {
            return Err("actor_export_changed".into());
        }
        fs::remove_file(path.join(name)).map_err(|_| "actor_export_failed")?;
    }
    safe_directory(path).map_err(|_| "actor_export_unsafe")?;
    if fs::read_dir(path)
        .map_err(|_| "actor_export_failed")?
        .any(|entry| entry.map(|e| e.file_name() != MANIFEST).unwrap_or(true))
    {
        return Err("actor_export_unsafe".into());
    }
    fs::remove_file(&manifest).map_err(|_| "actor_export_failed")?;
    fs::remove_dir(path).map_err(|_| "actor_export_failed".into())
}
pub fn confirm_export(
    storage: &Storage,
    actor_ids: &[String],
    token: &str,
    overwrite: bool,
) -> Result<ExportResult> {
    confirm_export_to(
        storage,
        actor_ids,
        storage.data_directory(),
        token,
        overwrite,
    )
}
pub fn confirm_export_to(
    storage: &Storage,
    actor_ids: &[String],
    destination: &Path,
    token: &str,
    overwrite: bool,
) -> Result<ExportResult> {
    let (current, data) = storage
        .with_connection(|db| build_export(db, actor_ids, destination).map_err(|e| sql_error(&e)))
        .map_err(|e| {
            e.to_string()
                .strip_prefix("Invalid parameter name: ")
                .unwrap_or("actor_export_failed")
                .to_owned()
        })?;
    if current != token {
        return Err("actor_export_changed".into());
    }
    if !overwrite
        && data
            .iter()
            .any(|d| d.item.exists || !d.item.previous_directories.is_empty())
    {
        return Err("actor_export_exists".into());
    }
    // An export can become an original photo source after being scanned back in.
    // Protect all actors' sources before publishing any part of this batch. This
    // comparison must not read unrelated originals, including offline network paths.
    let replaced_files = data.iter().flat_map(|d| d.expected.iter().flat_map(|(directory, files)| {
        files.keys().map(move |name| export_path_key(&Path::new(directory).join(name)))
    })).collect::<BTreeSet<_>>();
    if !replaced_files.is_empty() {
        let source_paths = storage.with_connection(|db| {
            let mut query = db.prepare("SELECT path FROM actor_image_sources")?;
            query.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()
        }).map_err(|_| "actor_export_failed")?;
        if source_paths.iter().any(|source| replaced_files.contains(&export_path_key(Path::new(source)))) {
            return Err("actor_export_photo_in_use".into());
        }
    }
    for d in &data {
        for prior in d.expected.keys() {
            preflight_cleanup(Path::new(prior), &d.item.actor_id)?;
        }
    }
    safe_directory(destination).map_err(|_| "actor_export_unsafe")?;
    let root = destination.join("ActorExports");
    if !root.exists() {
        fs::create_dir(&root).map_err(|_| "actor_export_failed")?;
    }
    safe_directory(&root).map_err(|_| "actor_export_unsafe")?;
    let mut published = 0;
    for d in data {
        let outcome = (|| -> Result<()> {
            safe_directory(&root).map_err(|_| "actor_export_unsafe")?;
            let path = PathBuf::from(&d.item.directory);
            // Revalidate ownership immediately before any replacement or cleanup.
            if !d.item.exists && fs::symlink_metadata(&path).is_ok() {
                return Err("actor_export_changed".into());
            }
            for (prior, expected) in &d.expected {
                let (id, actual) = owned_export(Path::new(prior))?;
                if id != d.item.actor_id {
                    return Err("actor_export_unsafe".into());
                }
                if &actual != expected {
                    return Err("actor_export_changed".into());
                }
            }
            let nonce = storage
                .with_connection(|db| {
                    db.query_row("SELECT lower(hex(randomblob(16)))", [], |r| {
                        r.get::<_, String>(0)
                    })
                })
                .map_err(|_| "actor_export_failed")?;
            let stage = root.join(format!(".actor-stage-{nonce}"));
            fs::create_dir(&stage).map_err(|_| "actor_export_failed")?;
            let result = (|| -> Result<()> {
                for (name, bytes) in &d.files {
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(stage.join(name))
                        .map_err(|_| "actor_export_failed")?;
                    file.write_all(bytes)
                        .and_then(|_| file.sync_all())
                        .map_err(|_| "actor_export_failed")?;
                }
                let rollback = root.join(format!(".actor-rollback-{nonce}"));
                if d.item.exists {
                    fs::rename(&path, &rollback).map_err(|_| "actor_export_failed")?;
                }
                if let Err(e) = fs::rename(&stage, &path) {
                    if d.item.exists {
                        let _ = fs::rename(&rollback, &path);
                    }
                    return Err(format!("actor_export_failed:{e}"));
                }
                published += 1;
                if d.item.exists {
                    remove_owned(&rollback, &d.item.actor_id)?;
                }
                for prior in &d.item.previous_directories {
                    remove_owned(Path::new(prior), &d.item.actor_id)?;
                }
                Ok(())
            })();
            if stage.exists() {
                for name in d.files.keys() {
                    let _ = fs::remove_file(stage.join(name));
                }
                let _ = fs::remove_dir(&stage);
            }
            result
        })();
        if let Err(e) = outcome {
            return Err(if published > 0 {
                format!("actor_export_partial:{published}:{e}")
            } else {
                e
            });
        }
    }
    Ok(ExportResult {
        exported: published,
        directory: root.to_string_lossy().into_owned(),
    })
}

use tauri_plugin_dialog::DialogExt;
#[tauri::command]
pub async fn choose_actor_photo(app: tauri::AppHandle) -> Result<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(crate::interface_language::text(&app, crate::interface_language::NativeText::ChoosePhoto))
            .add_filter(crate::interface_language::text(&app, crate::interface_language::NativeText::Photos), &["jpg", "jpeg", "png", "webp", "avif"])
            .blocking_pick_file()
            .map(|f| f.into_path().map(|p| p.to_string_lossy().into_owned()))
            .transpose()
            .map_err(|_| "actor_photo_invalid".into())
    })
    .await
    .map_err(|_| "actor_photo_invalid".to_owned())?
}
#[tauri::command]
pub async fn choose_actor_import_root(app: tauri::AppHandle) -> Result<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(crate::interface_language::text(&app, crate::interface_language::NativeText::ChooseActorImportRoot))
            .blocking_pick_folder()
            .map(|f| f.into_path().map(|p| p.to_string_lossy().into_owned()))
            .transpose()
            .map_err(|_| "actor_directory_unavailable".into())
    })
    .await
    .map_err(|_| "actor_directory_unavailable".to_owned())?
}
#[tauri::command]
pub async fn choose_actor_export_destination(app: tauri::AppHandle) -> Result<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(crate::interface_language::text(&app, crate::interface_language::NativeText::ChooseActorExportDestination))
            .blocking_pick_folder()
            .map(|f| f.into_path().map(|p| p.to_string_lossy().into_owned()))
            .transpose()
            .map_err(|_| "actor_directory_unavailable".into())
    })
    .await
    .map_err(|_| "actor_directory_unavailable".to_owned())?
}
#[tauri::command]
pub async fn save_managed_actor(app: tauri::AppHandle, input: SaveActorInput,
    diagnostic_operation_id: Option<String>,
) -> Result<String> {
    crate::diagnostics::spawn_blocking("save_managed_actor", diagnostic_operation_id, move || {
        let state = crate::ready_storage(&app)?;
        let storage = state.exclusive().map_err(|_| "storage_unavailable")?;
        let id = save(&storage, &input)?;
        if input.photo_path.is_some() || input.remove_photo { app.state::<crate::image_session::ImageSession>().release(&[format!("actor:{id}")]); }
        Ok(id)
    })
    .await
    .map_err(|_| "actor_save_failed".to_owned())?
}
#[tauri::command]
pub async fn preview_actor_import(app: tauri::AppHandle, root: String,
    diagnostic_operation_id: Option<String>,
) -> Result<ImportPreview> {
    crate::diagnostics::spawn_blocking("preview_actor_import", diagnostic_operation_id, move || {
        let state = crate::ready_storage(&app)?;
        let _lease = state.begin_scan()?;
        let storage = state.lock().map_err(|_| "storage_unavailable")?.clone();
        preview_import(&storage, Path::new(&root))
    })
    .await
    .map_err(|_| "actor_import_failed".to_owned())?
}
#[tauri::command]
pub async fn confirm_actor_scan_add(
    app: tauri::AppHandle,
    root: String,
    token: String,
    resolutions: Vec<ScanResolution>,
    diagnostic_operation_id: Option<String>,
) -> Result<ScanAddResult> {
    crate::diagnostics::spawn_blocking("confirm_actor_scan_add", diagnostic_operation_id, move || {
        let state = crate::ready_storage(&app)?;
        let storage = state.exclusive().map_err(|_| "storage_unavailable")?;
        let result = confirm_scan_add(&storage, Path::new(&root), &token, &resolutions)?;
        app.state::<crate::image_session::ImageSession>().release(&result.successful_artwork_keys);
        Ok(result)
    })
    .await
    .map_err(|_| "actor_import_failed".to_owned())?
}
#[tauri::command]
pub async fn preview_actor_export(
    app: tauri::AppHandle,
    actor_ids: Vec<String>,
    destination: String,
    diagnostic_operation_id: Option<String>,
) -> Result<ExportPreview> {
    crate::diagnostics::spawn_blocking("preview_actor_export", diagnostic_operation_id, move || {
        let state = crate::ready_storage(&app)?;
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        preview_export_to(&storage, &actor_ids, Path::new(&destination))
    })
    .await
    .map_err(|_| "actor_export_failed".to_owned())?
}
#[tauri::command]
pub async fn confirm_actor_export(
    app: tauri::AppHandle,
    actor_ids: Vec<String>,
    destination: String,
    token: String,
    overwrite: bool,
    diagnostic_operation_id: Option<String>,
) -> Result<ExportResult> {
    crate::diagnostics::spawn_blocking("confirm_actor_export", diagnostic_operation_id, move || {
        let state = crate::ready_storage(&app)?;
        let _lease = state.begin_scan()?;
        let storage = state.lock().map_err(|_| "storage_unavailable")?;
        confirm_export_to(
            &storage,
            &actor_ids,
            Path::new(&destination),
            &token,
            overwrite,
        )
    })
    .await
    .map_err(|_| "actor_export_failed".to_owned())?
}
