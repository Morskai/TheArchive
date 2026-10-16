use local_film_library_preview_lib::{covers, storage::Storage};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    fs,
};

struct CountingAllocator;
thread_local! {
    static COUNT: Cell<bool> = const { Cell::new(false) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            let _ = BYTES.try_with(|n| n.set(n.get() + layout.size()));
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            let _ = BYTES.try_with(|n| n.set(n.get() + size));
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn setup(noise: usize) -> (tempfile::TempDir, Storage, Vec<u8>) {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("software")).unwrap();
    storage.with_transaction(|db| {
        db.execute("INSERT INTO libraries(id,display_name) VALUES('owner','synthetic'),('noise','unrelated')",[])?;
        db.execute("INSERT INTO films(id) VALUES('film')",[])?;
        db.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('video','owner','film','work/movie.mp4','video')",[])?;
        db.execute("INSERT INTO file_observations(id,library_id,observed_path,kind) VALUES('cover','owner','work/poster.jpg','image')",[])?;
        let mut checks=Vec::new();let mut roles=Vec::new();
        for n in 0..noise {
            let film=format!("noise-film-{n}");let video=format!("noise-video-{n}");let image=format!("noise-image-{n}");let directory=format!("noise-dir-{n}");
            db.execute("INSERT INTO films(id) VALUES(?1)",[&film])?;
            db.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES(?1,'noise',?2,?3,'video')",[&video,&film,&format!("{directory}/movie.mp4")])?;
            db.execute("INSERT INTO file_observations(id,library_id,observed_path,kind) VALUES(?1,'noise',?2,'image')",[&image,&format!("{directory}/poster.jpg")])?;
            checks.push(serde_json::json!({"relative_path":format!("{directory}/poster.jpg"),"state":"readable"}));
            roles.push(serde_json::json!({"directory":directory,"role":"film"}));
        }
        for (source,evidence) in [("owner",serde_json::json!({"directories":["work"],"cover_checks":[{"relative_path":"work/poster.jpg","state":"readable"}],"roles":[{"directory":"work","role":"film"}]})),("noise",serde_json::json!({"directories":[],"cover_checks":checks,"roles":roles}))] {
            db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES(?1,'library_scan_details',?1,'current',?2)",[source,&evidence.to_string()])?;
        }
        Ok(())
    }).unwrap();
    let digest = "a".repeat(64);
    let cache = storage.data_directory().join("cover-cache");
    fs::create_dir_all(&cache).unwrap();
    let file = cache.join(format!("v1-{digest}-480.jpg"));
    image::RgbImage::from_pixel(24, 32, image::Rgb([40, 80, 120]))
        .save(&file)
        .unwrap();
    fs::copy(&file, cache.join(format!("v1-{digest}-1200.jpg"))).unwrap();
    fs::write(
        cache.join("index-v1.json"),
        serde_json::json!({"version":1,"observations":{"cover":{"480":digest,"1200":digest}}})
            .to_string(),
    )
    .unwrap();
    let bytes = fs::read(file).unwrap();
    (home, storage, bytes)
}

#[test]
fn one_cached_cover_does_not_allocate_unrelated_source_observations() {
    let (_home, storage, expected) = setup(1000);
    BYTES.set(0);
    COUNT.set(true);
    let result = covers::cached_thumbnail(&storage, "film", false);
    COUNT.set(false);
    let bytes = BYTES.get();
    assert_eq!(result.unwrap(), expected);
    eprintln!("scoped_cached_cover_allocated_bytes={bytes}");
    assert!(
        bytes < 256 * 1024,
        "one target cover allocated{bytes} Rust bytes from1000 unrelated works"
    );
}

#[test]
fn unrelated_source_bad_evidence_does_not_block_cached_cover_or_version_fallback() {
    let (_home, storage, expected) = setup(2);
    storage.with_connection(|db| {
        db.execute("UPDATE issue_records SET evidence='not json' WHERE subject_id='noise'",[])?;
        db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('noise-snapshot','cover_scan','noise','current','not json')",[])?;
        Ok(())
    }).unwrap();
    assert_eq!(
        covers::cached_thumbnail(&storage, "film", false).unwrap(),
        expected
    );
    assert!(covers::info(&storage, "film").unwrap().readable);
    assert_eq!(
        covers::cached_version_thumbnail(&storage, "film", "video", false).unwrap(),
        expected
    );
}

#[test]
fn targeted_lookup_keeps_shared_owner_and_cross_source_alias_rules() {
    let (_home, storage, expected) = setup(0);
    storage.with_connection(|db| {
        db.execute("INSERT INTO file_observations(id,library_id,observed_path,kind) VALUES('unassigned-peer','owner','work/other.mp4','video')",[])?;
        Ok(())
    }).unwrap();
    assert!(covers::info(&storage, "film").unwrap().needs_review);
    assert_eq!(
        covers::cached_thumbnail(&storage, "film", false).unwrap_err(),
        "cover_needs_review"
    );
    storage.with_connection(|db| {
        db.execute("DELETE FROM file_observations WHERE id='unassigned-peer'",[])?;
        db.execute("INSERT INTO films(id) VALUES('alias')",[])?;
        db.execute("INSERT INTO film_aliases(id,film_id) VALUES('alias','film')",[])?;
        db.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('second-location','noise','film','elsewhere/movie.mp4','video')",[])?;
        db.execute("INSERT INTO file_observations(id,library_id,observed_path,kind) VALUES('second-cover','noise','elsewhere/poster.jpg','image')",[])?;
        Ok(())
    }).unwrap();
    assert!(covers::info(&storage, "alias").unwrap().needs_review);
    assert_eq!(
        covers::cached_thumbnail(&storage, "alias", false).unwrap_err(),
        "cover_needs_review"
    );
    storage
        .with_connection(|db| {
            db.execute("UPDATE libraries SET enabled=0 WHERE id='noise'", [])?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        covers::cached_thumbnail(&storage, "alias", false).unwrap(),
        expected
    );
    assert_eq!(
        covers::cached_version_thumbnail(&storage, "alias", "video", false).unwrap(),
        expected
    );
    assert_eq!(
        covers::info(&storage, "missing").unwrap_err(),
        "film_not_found"
    );
}

#[test]
fn saved_cache_mapping_does_not_need_scan_digest_but_still_checks_source() {
    let (_home, storage, expected) = setup(0);
    storage
        .with_connection(|db| {
            db.execute(
                "UPDATE issue_records SET evidence='not json' WHERE subject_id='owner'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        covers::cached_observation_thumbnail(&storage, "cover", false).unwrap(),
        expected
    );
    assert_eq!(
        covers::cached_thumbnail(&storage, "film", false).unwrap_err(),
        "cover_load_failed",
        "a film lookup still requires valid ownership/snapshot evidence"
    );
    storage
        .with_connection(|db| {
            db.execute("UPDATE libraries SET enabled=0 WHERE id='owner'", [])?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        covers::cached_observation_thumbnail(&storage, "cover", false).unwrap_err(),
        "cover_unavailable",
        "an inactive source cannot provide or regenerate a displayed image"
    );
}

#[test]
fn missing_mapping_uses_only_its_source_readable_digest() {
    let (_home, storage, expected) = setup(2);
    let digest = "a".repeat(64);
    storage.with_connection(|db| {
        db.execute("UPDATE issue_records SET evidence=?1 WHERE subject_id='owner'",[serde_json::json!({"directories":[],"cover_checks":[{"relative_path":"work/poster.jpg","state":"readable","digest":digest}]}).to_string()])?;
        db.execute("UPDATE issue_records SET evidence='not json' WHERE subject_id='noise'",[])?;
        Ok(())
    }).unwrap();
    fs::remove_file(storage.data_directory().join("cover-cache/index-v1.json")).unwrap();
    assert_eq!(
        covers::cached_observation_thumbnail(&storage, "cover", false).unwrap(),
        expected
    );
}
