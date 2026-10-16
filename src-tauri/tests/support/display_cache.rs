//! Synthetic fixture setup: simulate visiting recorded images at both sizes.
//! No production batch rebuild or refresh API is needed to prepare cache tests.
use local_film_library_preview_lib::{covers, storage::Storage};
use std::collections::BTreeSet;

#[allow(dead_code)] // Some fixtures only need to populate cache; count fixtures inspect both.
pub struct DisplayedImages { pub generated: usize, pub failed: usize }

pub fn visit_source_images(storage: &Storage, source: &str) -> Result<DisplayedImages, String> {
    let ids = storage.with_connection(|db| {
        let active: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM libraries WHERE id=?1 AND enabled=1 AND removed=0)", [source], |r| r.get(0))?;
        if !active { return Err(rusqlite::Error::InvalidQuery); }
        let mut q = db.prepare("SELECT id,observed_path,kind FROM file_observations WHERE library_id=?1 AND kind IN ('video','image')")?;
        let rows = q.query_map([source], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let parent = |path: &str| path.rsplit_once('/').map(|(dir,_)|dir.to_owned()).unwrap_or_default();
        let dirs: BTreeSet<_> = rows.iter().filter(|(_,_,kind)|kind=="video").map(|(_,path,_)|parent(path)).collect();
        Ok(rows.into_iter().filter(|(_,path,kind)|kind=="image" && dirs.contains(&parent(path))).map(|(id,_,_)|id).collect::<Vec<_>>())
    }).map_err(|_| "cover_unavailable")?;
    let mut summary = DisplayedImages { generated: 0, failed: 0 };
    for id in ids {
        let mut complete = true;
        for large in [false,true] {
            match covers::cached_observation_thumbnail(storage,&id,large) {
                Ok(_) => {},
                Err(reason) if reason == "cover_cache_failed" => return Err(reason),
                Err(_) => complete = false,
            }
        }
        if complete { summary.generated += 1; } else { summary.failed += 1; }
    }
    Ok(summary)
}
