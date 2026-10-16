//! Film-owned image observations only. Originals are read-only; the page gets derived JPEG bytes.
use crate::{artwork, folder_groups, storage::Storage};
use image::{ImageFormat, ImageReader, Limits, codecs::jpeg::JpegEncoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

type Result<T> = std::result::Result<T, String>;
const MAX_INPUT: u64 = 32 * 1024 * 1024;
const MAX_CACHED: u64 = 4 * 1024 * 1024;
const MAX_INDEX: u64 = 32 * 1024 * 1024;
const CACHE_INDEX: &str = "index-v1.json";
// One decoder at a time, independent of the storage lock and WebView thread.
static DECODER: Mutex<()> = Mutex::new(());

pub(crate) fn with_generated_cache_lock<T>(action: impl FnOnce() -> Result<T>) -> Result<T> {
    let _permit = DECODER.lock().map_err(|_| "cache_clear_failed")?;
    action()
}

// Only generated flat cache files are eligible. Preflight every entry before deleting
// any, and never follow a link/junction or touch a library or application database.
pub(crate) fn clear_generated_cache(storage: &Storage) -> Result<()> {
    let _permit = DECODER.lock().map_err(|_| "cache_clear_failed")?;
    let root = storage
        .data_directory()
        .canonicalize()
        .map_err(|_| "cache_clear_failed")?;
    let cache = root.join("cover-cache");
    let metadata = match fs::symlink_metadata(&cache) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("cache_clear_failed".into()),
    };
    if redirected(&metadata)
        || !metadata.is_dir()
        || cache.canonicalize().ok().as_ref() != Some(&cache)
    {
        return Err("cache_clear_failed".into());
    }
    let mut generated = Vec::new();
    for entry in fs::read_dir(&cache).map_err(|_| "cache_clear_failed")? {
        let entry = entry.map_err(|_| "cache_clear_failed")?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|_| "cache_clear_failed")?;
        if redirected(&metadata) {
            return Err("cache_clear_failed".into());
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if generated_name(name) {
            if !metadata.is_file() || path.canonicalize().ok().as_ref() != Some(&path) {
                return Err("cache_clear_failed".into());
            }
            generated.push(path);
        }
    }
    for path in generated {
        fs::remove_file(path).map_err(|_| "cache_clear_failed")?;
    }
    Ok(())
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
pub(crate) fn generated_name(name: &str) -> bool {
    if name == CACHE_INDEX {
        return true;
    }
    if let Some(body) = name
        .strip_prefix("index-v1.")
        .and_then(|s| s.strip_suffix(".tmp"))
    {
        return body.split_once('-').is_some_and(|(pid, nonce)| {
            !pid.is_empty()
                && !nonce.is_empty()
                && pid.bytes().chain(nonce.bytes()).all(|b| b.is_ascii_digit())
        });
    }
    let Some(body) = name.strip_prefix("v1-") else {
        return false;
    };
    let Some((digest, suffix)) = body.split_once('-') else {
        return false;
    };
    digest.len() == 64
        && digest.bytes().all(|b| b.is_ascii_hexdigit())
        && (suffix == "480.jpg"
            || suffix == "1200.jpg"
            || ((suffix.starts_with("480.") || suffix.starts_with("1200."))
                && suffix.ends_with(".tmp")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CoverEvidence {
    pub candidates: Vec<Candidate>,
    pub reason: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct CoverInfo {
    pub candidates: Vec<Candidate>,
    pub default_id: Option<String>,
    pub needs_review: bool,
    pub readable: bool,
}

pub(crate) fn canonical_film(storage: &Storage, id: &str) -> Result<String> {
    storage.with_connection(|db| Ok(canonical_film_on(db, id))).map_err(|_| "film_not_found")?
}
fn canonical_film_on(db: &rusqlite::Connection, id: &str) -> Result<String> {
    db.query_row("SELECT COALESCE(a.film_id,f.id) FROM films f LEFT JOIN film_aliases a ON a.id=f.id WHERE f.id=?1", [id], |r|r.get(0)).map_err(|_| "film_not_found".into())
}

pub fn info(storage: &Storage, film_id: &str) -> Result<CoverInfo> {
    storage.with_connection(|db| Ok((|| -> Result<CoverInfo> {
        let film = canonical_film_on(db, film_id)?;
        Ok(CoverIndex::read_for_film(db, &film, false).map_err(|_| "cover_load_failed")?.info(&film))
    })())).map_err(|_| "film_not_found")?
}

// Only observation metadata, read once per catalogue load; never opens or decodes images.
pub(crate) struct CoverIndex {
    videos: Vec<folder_groups::VideoRow>,
    images: Vec<(String, String, String)>,
    unreadable: BTreeSet<String>,
    directory_videos: BTreeMap<(String, String), Vec<usize>>,
    directory_images: BTreeMap<(String, String), Vec<usize>>,
    film_directories: BTreeMap<String, BTreeSet<(String, String)>>,
    shared_directories: BTreeSet<(String, String)>,
    retained: bool,
}
impl CoverIndex {
    pub(crate) fn read(db: &rusqlite::Connection) -> rusqlite::Result<Self> { Self::read_with(db, false) }
    fn read_with(db: &rusqlite::Connection, retained: bool) -> rusqlite::Result<Self> {
        // A complete scan records its current observation IDs without deleting historical
        // observations or manual choices. Missing/offline/incomplete scans never update it.
        let snapshot = artwork::ScanSnapshot::read(db)?;
        let videos: Vec<_> = folder_groups::rows(db)?
            .into_iter()
            .filter(|v| snapshot.contains(&v.source, &v.id))
            .collect();
        let mut q = db.prepare("SELECT o.id,o.library_id,o.observed_path FROM file_observations o JOIN libraries l ON l.id=o.library_id WHERE o.kind='image' AND l.enabled=1 AND l.removed=0 ORDER BY o.observed_path,o.id")?;
        let images: Vec<(String, String, String)> = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let details = artwork::scan_details(db)?;
        Ok(Self::from_rows(snapshot, details, videos, images, retained))
    }
    fn read_for_film(db: &rusqlite::Connection, film: &str, retained: bool) -> rusqlite::Result<Self> {
        let mut q = db.prepare("SELECT o.library_id,o.observed_path FROM file_observations o JOIN libraries l ON l.id=o.library_id WHERE o.kind='video' AND o.film_id=?1 AND l.enabled=1 AND l.removed=0")?;
        let paths = q.query_map([film], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let sources = paths.iter().map(|(source,_)| source.clone()).collect();
        let snapshot = artwork::ScanSnapshot::read_sources(db, &sources)?;
        let details = artwork::scan_details_for_sources(db, &sources)?;
        let directories = paths.into_iter().map(|(source,path)| (source,folder_groups::parent(&path))).collect::<BTreeSet<_>>();
        let videos = folder_groups::rows_for_directories(db, &directories, &snapshot, &details)?;
        let mut q = db.prepare("SELECT o.id,o.library_id,o.observed_path FROM file_observations o JOIN libraries l ON l.id=o.library_id WHERE o.kind='image' AND l.enabled=1 AND l.removed=0 AND o.library_id=?1 AND substr(replace(o.observed_path,char(92),'/'),1,length(?2))=?2 AND instr(substr(replace(o.observed_path,char(92),'/'),length(?2)+1),'/')=0 ORDER BY o.observed_path,o.id")?;
        let mut images = Vec::new();
        for (source,directory) in &directories {
            let prefix = if directory.is_empty() { String::new() } else { format!("{directory}/") };
            images.extend(q.query_map([source,&prefix], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?);
        }
        Ok(Self::from_rows(snapshot, details, videos, images, retained))
    }
    fn from_rows(
        snapshot: artwork::ScanSnapshot,
        details: BTreeMap<String, artwork::ScanDetails>,
        videos: Vec<folder_groups::VideoRow>,
        images: Vec<(String,String,String)>,
        retained: bool,
    ) -> Self {
        let mut checks = BTreeMap::<(String, String), bool>::new();
        let image_paths: BTreeSet<_> = images.iter().map(|(_,source,path)| (source.as_str(),path.as_str())).collect();
        for (source, details) in &details {
            for check in &details.cover_checks {
                if !image_paths.contains(&(source.as_str(),check.relative_path.as_str())) { continue; }
                let unreadable = check.state != "readable";
                checks.entry((source.clone(), check.relative_path.clone()))
                    .and_modify(|state| *state |= unreadable)
                    .or_insert(unreadable);
            }
        }
        let images:Vec<_>=images.into_iter().filter(|(id,source,path)|{
            (snapshot.contains(source,id)||retained) && (artwork::named(path,artwork::FILM_COVER) || checks.contains_key(&(source.clone(),path.clone())))
        }).collect();
        let unreadable:BTreeSet<String> = images.iter().filter(|(_,source,path)|checks.get(&(source.clone(),path.clone()))==Some(&true)).map(|(id,_,_)|id.clone()).collect();
        let mut owners = BTreeMap::<(String, String), BTreeSet<Option<String>>>::new();
        let mut film_directories = BTreeMap::<String, BTreeSet<(String, String)>>::new();
        let mut directory_videos = BTreeMap::<(String, String), Vec<usize>>::new();
        let mut directory_images = BTreeMap::<(String, String), Vec<usize>>::new();
        for (position, video) in videos.iter().enumerate() {
            let directory = folder_groups::parent(&video.path);
            let key = (video.source.clone(), directory.clone());
            directory_videos.entry(key.clone()).or_default().push(position);
            // Unassigned videos also count as a different owner, as before.
            owners.entry(key.clone()).or_default().insert(video.film.clone());
            let name = if directory.is_empty() { video.root_name.as_str() } else { directory.rsplit('/').next().unwrap_or_default() };
            if video.confirmed_film || !folder_groups::category(name) {
                if let Some(film) = &video.film { film_directories.entry(film.clone()).or_default().insert(key); }
            }
        }
        for (position, (_, source, path)) in images.iter().enumerate() {
            directory_images.entry((source.clone(), folder_groups::parent(path))).or_default().push(position);
        }
        Self {
            videos,
            images,
            unreadable,
            directory_videos,
            directory_images,
            film_directories,
            shared_directories: owners.into_iter().filter(|(_,owners)|owners.len()>1).map(|(directory,_)|directory).collect(),
            retained,
        }
    }
    pub(crate) fn info(&self, film: &str) -> CoverInfo {
        let evidence = self.evidence(film);
        let needs_review = evidence.reason.is_some();
        let default_id = (!needs_review && evidence.candidates.len() == 1)
            .then(|| evidence.candidates[0].id.clone());
        CoverInfo {
            readable: default_id
                .as_ref()
                .is_some_and(|id| !self.unreadable.contains(id)),
            candidates: evidence.candidates,
            default_id,
            needs_review,
        }
    }
    /// Reliable direct-file evidence only, independent of the current selected image.
    pub(crate) fn evidence(&self,film:&str)->CoverEvidence {
        let mut candidates=Vec::new();
        if let Some(dirs)=self.film_directories.get(film){for directory in dirs {
            let videos=self.directory_videos.get(directory).map(Vec::as_slice).unwrap_or_default();
            let mut images:Vec<_>=self.directory_images.get(directory).into_iter().flatten().map(|position| &self.images[*position]).filter(|(id,_,p)| (self.retained || !self.unreadable.contains(id)) && (artwork::named(p,artwork::FILM_COVER) || videos.len()==1 && same_stem(p,&self.videos[videos[0]].path))).collect();
            images.sort_by_key(|(id,_,p)|(self.unreadable.contains(id),!artwork::named(p,artwork::FILM_COVER),artwork::image_rank(p).unwrap_or(99),p.to_lowercase()));
            if let Some((id,_,path))=images.first(){candidates.push(Candidate{id:id.clone(),name:path.replace('\\',"/").rsplit('/').next().unwrap_or_default().to_owned()});}
        }}
        let reason=if self.film_directories.get(film).is_some_and(|dirs|dirs.iter().any(|d|self.shared_directories.contains(d))){Some("shared_directory".into())}else{(candidates.len()>1).then(||"multiple_covers".into())};CoverEvidence{candidates,reason}
    }

}

fn resolve_image(storage: &Storage, id: &str) -> Result<PathBuf> {
    let (root, relative, enabled, removed): (String,String,bool,bool) = storage.with_connection(|db| db.query_row("SELECT l.root_path,o.observed_path,l.enabled,l.removed FROM file_observations o JOIN libraries l ON l.id=o.library_id WHERE o.id=?1 AND o.kind='image'", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))).map_err(|_| "cover_unavailable")?;
    if !enabled || removed {
        return Err("cover_unavailable".into());
    }
    safe_image_path(Path::new(&root), &relative)
}

fn safe_image_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
        || artwork::image_rank(&relative.to_string_lossy()).is_none()
    {
        return Err("cover_not_owned".into());
    }
    let root = root.canonicalize().map_err(|_| "cover_unavailable")?;
    let original = root.join(relative);
    // Reject redirections at every segment, not just escapes outside the source.
    let mut current = root.clone();
    for part in relative.components() {
        current.push(part);
        let metadata = fs::symlink_metadata(&current).map_err(|_| "cover_unavailable")?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("cover_not_owned".into());
            }
        }
        if metadata.file_type().is_symlink() {
            return Err("cover_not_owned".into());
        }
    }
    let file = original.canonicalize().map_err(|_| "cover_unavailable")?;
    if !file.starts_with(root) || !file.is_file() {
        return Err("cover_not_owned".into());
    }
    Ok(file)
}

fn cache_directory(storage: &Storage) -> Result<PathBuf> {
    let root = storage
        .data_directory()
        .canonicalize()
        .map_err(|_| "cover_cache_failed")?;
    let cache = root.join("cover-cache");
    fs::create_dir_all(&cache).map_err(|_| "cover_cache_failed")?;
    let metadata = fs::symlink_metadata(&cache).map_err(|_| "cover_cache_failed")?;
    if redirected(&metadata)
        || !metadata.is_dir()
        || cache.canonicalize().map_err(|_| "cover_cache_failed")? != cache
    {
        return Err("cover_cache_failed".into());
    }
    Ok(cache)
}

fn thumbnail_observation_with(storage: &Storage, film_id: &str, retained: bool) -> Result<String> {
    storage.with_connection(|db| Ok(thumbnail_observation_on(db, film_id, retained)))
        .map_err(|_| "cover_load_failed")?
}
fn thumbnail_observation_on(db: &rusqlite::Connection, film_id: &str, retained: bool) -> Result<String> {
    let film = canonical_film_on(db, film_id)?;
    let info = CoverIndex::read_for_film(db, &film, retained).map_err(|_| "cover_load_failed")?.info(&film);
    if info.needs_review { return Err("cover_needs_review".into()); }
    let id = info.default_id.ok_or("cover_missing")?;
    let owned = (|| -> rusqlite::Result<bool> {
        let image: (String,String) = db.query_row("SELECT library_id,observed_path FROM file_observations WHERE id=?1 AND kind='image'", [&id], |r| Ok((r.get(0)?,r.get(1)?)))?;
        let mut query = db.prepare("SELECT library_id,observed_path FROM file_observations WHERE film_id=?1 AND kind='video'")?;
        let videos = query.query_map([&film], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(videos.iter().any(|v| v.0 == image.0 && folder_groups::parent(&v.1) == folder_groups::parent(&image.1)))
    })().map_err(|_| "cover_not_owned")?;
    if !owned {
        return Err("cover_not_owned".into());
    }
    Ok(id)
}

pub fn cached_thumbnail(
    storage: &Storage,
    film_id: &str,
    large: bool,
) -> Result<Vec<u8>> {
    let id = thumbnail_observation_with(storage, film_id, true)?;
    cached_observation_thumbnail(storage, &id, large)
}

pub(crate) fn request_key(storage: &Storage, film: &str, video: Option<&str>) -> String {
    let id = if let Some(video) = video { version_image(storage,film,video) } else { thumbnail_observation_with(storage,film,true) };
    id.and_then(|id| storage.with_connection(|db| db.query_row("SELECT l.root_path,o.observed_path FROM file_observations o JOIN libraries l ON l.id=o.library_id WHERE o.id=?1", [&id], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))))
        .map(|(root,relative)| format!("path:{}",crate::directory_roles::identity(&Path::new(&root).join(relative).to_string_lossy())))
        .map_err(|_| "cover_missing".to_owned())).unwrap_or_else(|_| format!("film:{film}"))
}

pub fn cached_observation_thumbnail(storage: &Storage, id: &str, large: bool) -> Result<Vec<u8>> {
    match cached_observation_only(storage, id, large) {
        Err(error) if error == "cover_cache_missing" => observation_thumbnail(storage, id, large),
        result => result,
    }
}

fn cached_observation_only(storage: &Storage, id: &str, large: bool) -> Result<Vec<u8>> {
    let _permit = DECODER.lock().map_err(|_| "cover_load_failed")?;
    let (source, relative) = storage.with_connection(|db| {
        let (source, relative, enabled, removed): (String, String, bool, bool) = db.query_row("SELECT o.library_id,o.observed_path,l.enabled,l.removed FROM file_observations o JOIN libraries l ON l.id=o.library_id WHERE o.id=?1 AND o.kind='image'", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        let path = Path::new(&relative);
        if !enabled || removed || path.as_os_str().is_empty() || path.components().any(|p| !matches!(p, Component::Normal(_))) || artwork::image_rank(&relative).is_none() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok((source, relative))
    }).map_err(|_| "cover_cache_missing")?;
    let cache = existing_cache_directory(storage)?.ok_or("cover_cache_missing")?;
    let mut index = read_cache_index(&cache).map_err(|_| "cover_cache_failed")?;
    let size = if large { 1200 } else { 480 };
    // A successful software cache remains authoritative until explicit cache clearing.
    // Missing/replaced originals never invalidate its retained observation mapping.
    let digest = if let Some(digest) = index.observations.get(id).and_then(|sizes| sizes.get(&size)) {
        digest.clone()
    } else {
        storage.with_connection(|db| {
            let details = artwork::scan_details_for_sources(db, &BTreeSet::from([source.clone()]))?;
            Ok(details.get(&source).and_then(|d| {
                let checks = &d.cover_checks;
                checks.iter().find(|c|c.relative_path==relative).filter(|c|c.state=="readable").and_then(|c|c.digest.clone())
            }))
        }).map_err(|_| "cover_cache_missing")?.ok_or("cover_cache_missing")?
    };
    if !valid_digest(&digest) {
        return Err("cover_cache_failed".into());
    }
    let bytes = read_cached_jpeg(&cache.join(format!("v1-{digest}-{size}.jpg")), size)?
        .ok_or_else(|| "cover_cache_missing".to_owned())?;
    // A digest hit is also a successful display by this observation. Record its
    // ownership so updating a peer cannot retire a cache it still uses.
    if index.observations.get(id).and_then(|sizes| sizes.get(&size)) != Some(&digest) {
        index.observations.entry(id.to_owned()).or_default().insert(size,digest);
        write_cache_index(&cache,&index)?;
    }
    Ok(bytes)
}

/// Actor photos have an explicit saved path. A retained cache never probes the
/// original; clearing it permits a single on-demand read of that exact path.
pub(crate) fn source_thumbnail(storage: &Storage, owner: &str, path: &Path, fingerprint: &str, large: bool) -> Result<Vec<u8>> {
    let _permit = DECODER.lock().map_err(|_| "cover_load_failed")?;
    // Bind the cached pixels to the saved source, without probing its original.
    // Restoring an earlier path/fingerprint cannot reuse a later photo's mapping.
    let source = serde_json::to_vec(&(path.to_string_lossy(), fingerprint)).map_err(|_| "cover_load_failed")?;
    let key = format!("{owner}@{:x}", Sha256::digest(source));
    let size = if large { 1200 } else { 480 };
    let cache = cache_directory(storage)?;
    let mut index = writable_cache_index(&cache)?;
    if let Some(digest) = index.observations.get(&key).and_then(|sizes| sizes.get(&size)) {
        if let Some(bytes) = read_cached_jpeg(&cache.join(format!("v1-{digest}-{size}.jpg")), size)? { return Ok(bytes); }
    }
    let bytes = crate::actor_management::read_photo_source(path).map_err(|_| "cover_unavailable")?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let file = cache.join(format!("v1-{digest}-{size}.jpg"));
    let output = if let Some(bytes) = read_cached_jpeg(&file, size)? { bytes } else {
        let output = crate::actor_management::portrait_bytes(&bytes, large)?;
        write_generated_jpeg(&file, &output)?;
        output
    };
    index.observations.entry(key.to_owned()).or_default().insert(size, digest);
    write_cache_index(&cache, &index)?;
    Ok(output)
}

fn write_generated_jpeg(path: &Path, output: &[u8]) -> Result<()> {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_| "cover_cache_failed")?.as_nanos();
    let temp = path.with_extension(format!("{}-{nonce}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(output)?; file.sync_all()?;
        if path.exists() { fs::remove_file(path)?; }
        fs::rename(&temp, path)
    })();
    if result.is_err() { let _ = fs::remove_file(&temp); return Err("cover_cache_failed".into()); }
    Ok(())
}

pub(crate) fn invalidate_image_cache(storage: &Storage, keys: &[String]) -> Result<()> {
    with_invalidated_image_cache(storage, keys, || Ok(()))
}

/// Keep the prior generated files until the protected data transaction succeeds.
/// A failed save or delete restores its mappings and still-valid cached pixels.
pub(crate) fn with_invalidated_image_cache<T>(storage: &Storage, keys: &[String], action: impl FnOnce() -> Result<T>) -> Result<T> {
    if keys.is_empty() { return action(); }
    let _permit = DECODER.lock().map_err(|_| "cover_load_failed")?;
    let Some(cache) = existing_cache_directory(storage)? else { return action(); };
    let mut index = writable_cache_index(&cache)?;
    let old = ArtworkCacheIndex { version: index.version, observations: index.observations.clone() };
    let mut removed = BTreeSet::new();
    let mappings: Vec<_> = index.observations.keys().filter(|mapping| keys.iter().any(|key|
        *mapping == key || (key.starts_with("actor:") && mapping.starts_with(&format!("{key}@")))
    )).cloned().collect();
    for key in mappings { if let Some(sizes) = index.observations.remove(&key) { removed.extend(sizes); } }
    let used: BTreeSet<_> = index.observations.values().flat_map(|sizes| sizes.iter().map(|(size,digest)| (*size,digest.clone()))).collect();
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_| "cover_cache_failed")?.as_nanos();
    let mut staged = Vec::new();
    let prepare = (|| -> Result<()> {
    for (size, digest) in removed {
        if !used.contains(&(size, digest.clone())) {
            let file = cache.join(format!("v1-{digest}-{size}.jpg"));
            if owned_cache_file(&file)? {
                let temporary = file.with_extension(format!("{}-{nonce}.tmp",std::process::id()));
                fs::rename(&file,&temporary).map_err(|_| "cover_cache_failed")?;
                staged.push((file,temporary));
            }
        }
    }
    write_cache_index(&cache,&index)
    })();
    let result = prepare.and_then(|_| action());
    if result.is_err() {
        for (file, temporary) in staged.iter().rev() { fs::rename(temporary,file).map_err(|_| "cover_cache_failed")?; }
        write_cache_index(&cache,&old)?;
    } else {
        // A transient inability to remove a private temporary cache does not
        // turn an already-committed data transaction into a reported failure.
        // Such generated .tmp files are also covered by clear picture cache.
        for (_,temporary) in staged { let _ = fs::remove_file(temporary); }
    }
    result
}

// Shared controlled image pipeline. Only trusted ownership resolvers call this.
/// Successful manual scans invalidate changed mappings without loading images.
pub(crate) fn refresh_current_source_artwork(storage:&Storage,source:&str)->Result<Vec<String>> {
    let expected=storage.with_connection(|db| {
        let details=artwork::scan_details(db)?.remove(source).ok_or(rusqlite::Error::InvalidQuery)?;
        let digests:BTreeMap<_,_>=details.cover_checks.into_iter().filter(|c|c.state=="readable").filter_map(|c|c.digest.map(|digest|(c.relative_path,digest))).collect();
        let mut q=db.prepare("SELECT id,observed_path FROM file_observations WHERE library_id=?1 AND kind='image'")?;
        let rows=q.query_map([source],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter_map(|(id,path)|digests.get(&path).cloned().map(|digest|(id,digest))).collect::<BTreeMap<_,_>>())
    }).map_err(|_|"cover_unavailable")?;
    let ids = {
        let _permit = DECODER.lock().map_err(|_| "cover_load_failed")?;
        let Some(cache) = existing_cache_directory(storage)? else { return Ok(Vec::new()); };
        let index = read_cache_index(&cache)?;
        expected.iter().filter(|(id,digest)| index.observations.get(*id).is_some_and(|sizes| sizes.values().any(|old| old != *digest))).map(|(id,_)| id.clone()).collect::<BTreeSet<_>>()
    };
    invalidate_image_cache(storage, &ids.iter().cloned().collect::<Vec<_>>())?;
    storage.with_connection(|db| {
        let mut q=db.prepare("SELECT id,observed_path FROM file_observations WHERE library_id=?1 AND kind='image'")?;
        let images=q.query_map([source],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let directories:BTreeSet<_>=images.into_iter().filter(|(id,_)|ids.contains(id)).map(|(_,path)|folder_groups::parent(&path)).collect();
        let keys:BTreeSet<_>=folder_groups::rows(db)?.into_iter().filter(|v|v.source==source && directories.contains(&folder_groups::parent(&v.path))).filter_map(|v|v.film).collect();
        Ok(keys.into_iter().collect())
    }).map_err(|_|"cover_unavailable".into())
}

pub(crate) fn current_artwork_choices(storage: &Storage) -> Result<BTreeMap<String, Option<String>>> {
    storage.with_connection(|db| {
        let index = CoverIndex::read(db)?;
        let films: BTreeSet<_> = index.videos.iter().filter_map(|v| v.film.clone()).collect();
        Ok(films.into_iter().map(|film| { let choice = index.info(&film).default_id; (film, choice) }).collect())
    }).map_err(|_| "cover_load_failed".into())
}

pub(crate) fn source_artwork_keys(storage: &Storage, source: &str) -> Result<Vec<String>> {
    storage.with_connection(|db| {
        let mut query = db.prepare("SELECT DISTINCT COALESCE(a.film_id,o.film_id) FROM file_observations o LEFT JOIN film_aliases a ON a.id=o.film_id WHERE o.library_id=?1 AND o.kind='video' AND o.film_id IS NOT NULL")?;
        query.query_map([source], |r| r.get::<_,String>(0))?.collect()
    }).map_err(|_| "cover_load_failed".into())
}

pub(crate) fn observation_thumbnail(storage: &Storage, id: &str, large: bool) -> Result<Vec<u8>> {
    let _permit = DECODER.lock().map_err(|_| "cover_load_failed")?;
    let (bytes, digest) = build_observation_thumbnail(storage, id, large)?;
    let cache = cache_directory(storage)?;
    let mut index = writable_cache_index(&cache)?;
    index
        .observations
        .entry(id.to_owned())
        .or_default()
        .insert(if large { 1200 } else { 480 }, digest);
    write_cache_index(&cache, &index)?;
    Ok(bytes)
}

fn build_observation_thumbnail(
    storage: &Storage,
    id: &str,
    large: bool,
) -> Result<(Vec<u8>, String)> {
    let file = resolve_image(storage, id)?;
    let bytes = read_original(&file)?;
    let size = if large { 1200 } else { 480 };
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let cache = cache_directory(storage)?;
    let path = cache.join(format!("v1-{digest}-{size}.jpg"));
    if let Some(cached) = read_cached_jpeg(&path, size)? {
        return Ok((cached, digest));
    }
    let decoded = decode_original(&bytes)?;
    let rgb = decoded.thumbnail(size, size).to_rgb8();
    let mut output = Vec::new();
    JpegEncoder::new_with_quality(&mut output, 85)
        .encode_image(&rgb)
        .map_err(|_| "cover_invalid")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "cover_cache_failed")?
        .as_nanos();
    let temp = path.with_extension(format!("{}-{nonce}.tmp", std::process::id()));
    // Only this process's private generated file is replaced. No original-image writes.
    let result = (|| -> std::io::Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        f.write_all(&output)?;
        f.sync_all()?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        fs::rename(&temp, &path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
        return Err("cover_cache_failed".into());
    }
    Ok((output, digest))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtworkCacheIndex {
    version: u8,
    observations: BTreeMap<String, BTreeMap<u32, String>>,
}

impl Default for ArtworkCacheIndex {
    fn default() -> Self {
        Self {
            version: 1,
            observations: BTreeMap::new(),
        }
    }
}

fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn existing_cache_directory(storage: &Storage) -> Result<Option<PathBuf>> {
    let root = storage
        .data_directory()
        .canonicalize()
        .map_err(|_| "cover_cache_failed")?;
    let cache = root.join("cover-cache");
    let metadata = match fs::symlink_metadata(&cache) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("cover_cache_failed".into()),
    };
    if redirected(&metadata)
        || !metadata.is_dir()
        || cache.canonicalize().ok().as_ref() != Some(&cache)
    {
        return Err("cover_cache_failed".into());
    }
    Ok(Some(cache))
}

fn owned_cache_file(path: &Path) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err("cover_cache_failed".into()),
    };
    if redirected(&metadata)
        || !metadata.is_file()
        || path.canonicalize().ok().as_deref() != Some(path)
    {
        return Err("cover_cache_failed".into());
    }
    Ok(true)
}

fn read_cached_jpeg(path: &Path, size: u32) -> Result<Option<Vec<u8>>> {
    if !owned_cache_file(path)? {
        return Ok(None);
    }
    let mut cached = Vec::new();
    fs::File::open(path)
        .map_err(|_| "cover_cache_failed")?
        .take(MAX_CACHED + 1)
        .read_to_end(&mut cached)
        .map_err(|_| "cover_cache_failed")?;
    if cached.len() as u64 > MAX_CACHED {
        return Ok(None);
    }
    let mut reader = ImageReader::with_format(Cursor::new(&cached), ImageFormat::Jpeg);
    let mut limits = Limits::default();
    limits.max_image_width = Some(size);
    limits.max_image_height = Some(size);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    if reader.decode().is_err() {
        return Ok(None);
    }
    Ok(Some(cached))
}

fn read_cache_index(cache: &Path) -> Result<ArtworkCacheIndex> {
    let path = cache.join(CACHE_INDEX);
    if !owned_cache_file(&path)? {
        return Ok(ArtworkCacheIndex::default());
    }
    let mut json = Vec::new();
    fs::File::open(&path)
        .map_err(|_| "cover_cache_failed")?
        .take(MAX_INDEX + 1)
        .read_to_end(&mut json)
        .map_err(|_| "cover_cache_failed")?;
    if json.len() as u64 > MAX_INDEX {
        return Err("cover_cache_index_invalid".into());
    }
    let index: ArtworkCacheIndex =
        serde_json::from_slice(&json).map_err(|_| "cover_cache_index_invalid")?;
    if index.version != 1
        || index.observations.values().any(|sizes| {
            sizes
                .iter()
                .any(|(size, digest)| !matches!(size, 480 | 1200) || !valid_digest(digest))
        })
    {
        return Err("cover_cache_index_invalid".into());
    }
    Ok(index)
}

fn writable_cache_index(cache: &Path) -> Result<ArtworkCacheIndex> {
    match read_cache_index(cache) {
        Err(error) if error == "cover_cache_index_invalid" => Ok(ArtworkCacheIndex::default()),
        result => result,
    }
}

fn write_cache_index(cache: &Path, index: &ArtworkCacheIndex) -> Result<()> {
    let path = cache.join(CACHE_INDEX);
    owned_cache_file(&path)?;
    let bytes = serde_json::to_vec(index).map_err(|_| "cover_cache_failed")?;
    if bytes.len() as u64 > MAX_INDEX {
        return Err("cover_cache_failed".into());
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "cover_cache_failed")?
        .as_nanos();
    let temp = cache.join(format!("index-v1.{}-{nonce}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temp, &path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
        return Err("cover_cache_failed".into());
    }
    Ok(())
}

fn read_original(file: &Path) -> Result<Vec<u8>> {
    let mut input = fs::File::open(file).map_err(|_| "cover_unavailable")?;
    if input.metadata().map_err(|_| "cover_unavailable")?.len() > MAX_INPUT {
        return Err("cover_too_large".into());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut input)
        .take(MAX_INPUT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cover_unavailable")?;
    if bytes.len() as u64 > MAX_INPUT {
        return Err("cover_too_large".into());
    }
    Ok(bytes)
}

pub(crate) fn decode_original(bytes: &[u8]) -> Result<image::DynamicImage> {
    if image::guess_format(bytes).ok()==Some(ImageFormat::Avif) {
        let parsed=avif_parse::read_avif(&mut &bytes[..]).map_err(|_|"cover_invalid")?;
        for metadata in std::iter::once(parsed.primary_item_metadata().map_err(|_|"cover_invalid")?).chain(parsed.alpha_item_metadata().map_err(|_|"cover_invalid")?) {
            let (w,h)=(metadata.max_frame_width.get(),metadata.max_frame_height.get());
            if w>12000 || h>12000 || u64::from(w)*u64::from(h)>16_000_000 {return Err("cover_too_large".into());}
        }
        let decoded=avif_decode::Decoder::from_avif(bytes).and_then(|d|d.to_image()).map_err(|_|"cover_invalid")?;
        use avif_decode::Image;
        let (w,h,rgba)=match decoded {
            Image::Rgb8(i)=>(i.width(),i.height(),i.buf().iter().flat_map(|p|[p.r,p.g,p.b,255]).collect()),
            Image::Rgba8(i)=>(i.width(),i.height(),i.buf().iter().flat_map(|p|[p.r,p.g,p.b,p.a]).collect()),
            Image::Rgb16(i)=>(i.width(),i.height(),i.buf().iter().flat_map(|p|[(p.r>>8) as u8,(p.g>>8) as u8,(p.b>>8) as u8,255]).collect()),
            Image::Rgba16(i)=>(i.width(),i.height(),i.buf().iter().flat_map(|p|[(p.r>>8) as u8,(p.g>>8) as u8,(p.b>>8) as u8,(p.a>>8) as u8]).collect()),
            Image::Gray8(i)=>(i.width(),i.height(),i.buf().iter().flat_map(|p|[p.value(),p.value(),p.value(),255]).collect()),
            Image::Gray16(i)=>(i.width(),i.height(),i.buf().iter().flat_map(|p|{let v=(p.value()>>8) as u8;[v,v,v,255]}).collect()),
        };
        return image::RgbaImage::from_raw(w as u32,h as u32,rgba).map(image::DynamicImage::ImageRgba8).ok_or_else(||"cover_invalid".into());
    }
    let mut reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| "cover_invalid")?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP)
    ) {
        return Err("cover_invalid".into());
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(12000);
    limits.max_image_height = Some(12000);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits.clone());
    let mut dimension_reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| "cover_invalid")?;
    dimension_reader.limits(limits);
    let (width, height) = dimension_reader
        .into_dimensions()
        .map_err(|_| "cover_invalid")?;
    if width > 12000 || height > 12000 || u64::from(width) * u64::from(height) > 24_000_000 {
        return Err("cover_too_large".into());
    }
    reader.decode().map_err(|_| "cover_invalid".into())
}

// Scan-time validation shares the exact boundary, read and decoding rules with thumbnails.
// It creates no thumbnail and does not write to a media file or application cache.
pub(crate) fn inspect_cover(root: &Path, relative: &str) -> crate::inventory::CoverCheck {
    let mut digest = None;
    let checked = (|| -> Result<()> {
        let _permit = DECODER.lock().map_err(|_| "cover_load_failed")?;
        let file = safe_image_path(root, relative)?;
        let bytes = read_original(&file)?;
        digest = Some(format!("{:x}", Sha256::digest(&bytes)));
        decode_original(&bytes)?;
        Ok(())
    })();
    crate::inventory::CoverCheck {
        relative_path: relative.into(),
        state: checked.err().unwrap_or_else(|| "readable".into()),
        digest,
    }
}

fn same_stem(image:&str,video:&str)->bool {std::path::Path::new(image).file_stem().zip(std::path::Path::new(video).file_stem()).is_some_and(|(a,b)|a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()))}
fn version_image(storage:&Storage,film:&str,video:&str)->Result<String> {
 let film=canonical_film(storage,film)?;
 let chosen=storage.with_connection(|db|{
  let index=CoverIndex::read_for_film(db,&film,true)?;let v=index.videos.iter().find(|v|v.id==video && v.film.as_deref()==Some(&film)).ok_or(rusqlite::Error::InvalidQuery)?;
  let mut images:Vec<_>=index.images.iter().filter(|(id,s,p)|s==&v.source && folder_groups::parent(p)==folder_groups::parent(&v.path) && same_stem(p,&v.path) && !index.unreadable.contains(id)).collect();
  images.sort_by_key(|(_,_,p)|(artwork::image_rank(p).unwrap_or(99),p.to_lowercase()));Ok(images.first().map(|(id,_,_)|id.clone()))
 }).map_err(|_|"cover_unavailable")?;
 chosen.map(Ok).unwrap_or_else(||thumbnail_observation_with(storage,&film,true))
}
pub fn cached_version_thumbnail(storage:&Storage,film:&str,video:&str,large:bool)->Result<Vec<u8>> {
 cached_observation_thumbnail(storage,&version_image(storage,film,video)?,large)
}
