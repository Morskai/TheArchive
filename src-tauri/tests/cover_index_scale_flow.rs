use local_film_library_preview_lib::{catalog, covers, storage::Storage};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    time::{Duration, Instant},
};

struct CountingAllocator;
thread_local! {
    static COUNT_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
    static ALLOCATED: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT_ALLOCATIONS.try_with(Cell::get).unwrap_or(false) {
            let _ = ALLOCATED.try_with(|bytes| bytes.set(bytes.get() + layout.size()));
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNT_ALLOCATIONS.try_with(Cell::get).unwrap_or(false) {
            let _ = ALLOCATED.try_with(|bytes| bytes.set(bytes.get() + size));
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

// Saved synthetic observations only: catalogue reads must not need original images.
fn fixture(count: usize) -> (tempfile::TempDir, Storage) {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("software")).unwrap();
    storage.with_transaction(|db| {
        db.execute("INSERT INTO libraries(id,display_name) VALUES('source','合成来源')", [])?;
        let mut checks = Vec::new();
        for i in 0..count {
            let film = format!("film-{i:05}");
            let video = format!("video-{i:05}");
            let image = format!("image-{i:05}");
            let directory = format!("work-{i:05}");
            db.execute("INSERT INTO films(id) VALUES(?1)", [&film])?;
            db.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES(?1,'source',?2,?3,'video')", rusqlite::params![video,film,format!("{directory}/movie.mp4")])?;
            let path = format!("{directory}/poster.jpg");
            db.execute("INSERT INTO file_observations(id,library_id,observed_path,kind) VALUES(?1,'source',?2,'image')", rusqlite::params![image,path])?;
            checks.push(serde_json::json!({"relative_path":path,"state":"readable","digest":null}));
        }
        let evidence = serde_json::json!({"directories":[],"cover_checks":checks}).to_string();
        db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('details','library_scan_details','source','current',?1)", [evidence])?;
        Ok(())
    }).unwrap();
    (home, storage)
}

#[test]
fn one_thousand_separate_covers_do_not_allocate_whole_library_paths_per_film() {
    let (_home, storage) = fixture(1000);
    ALLOCATED.set(0);
    COUNT_ALLOCATIONS.set(true);
    let films = catalog::list_films(&storage);
    COUNT_ALLOCATIONS.set(false);
    let allocated = ALLOCATED.get();
    eprintln!("cover_index catalogue_allocated_bytes={allocated}");
    let films = films.unwrap();
    assert_eq!(films.len(), 1000);
    assert!(films.iter().all(|film| film.has_cover));
    assert!(
        allocated < 32 * 1024 * 1024,
        "1000 independent covers allocated {allocated} Rust bytes"
    );
}

#[test]
fn one_thousand_saved_film_roles_do_not_allocate_a_parent_path_per_role() {
    let (_home, storage) = fixture(1000);
    storage
        .with_connection(|db| {
            let json: String = db.query_row(
                "SELECT evidence FROM issue_records WHERE id='details'",
                [],
                |r| r.get(0),
            )?;
            let mut details: serde_json::Value = serde_json::from_str(&json).unwrap();
            details["roles"] = serde_json::Value::Array(
                (0..1000)
                    .map(|i| serde_json::json!({"directory":format!("work-{i:05}"),"role":"film"}))
                    .collect(),
            );
            db.execute(
                "UPDATE issue_records SET evidence=?1 WHERE id='details'",
                [details.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    ALLOCATED.set(0);
    COUNT_ALLOCATIONS.set(true);
    let info = covers::info(&storage, "film-00000");
    COUNT_ALLOCATIONS.set(false);
    let allocated = ALLOCATED.get();
    eprintln!("cover_index populated_roles_allocated_bytes={allocated}");
    assert!(info.unwrap().readable);
    assert!(
        allocated < 8 * 1024 * 1024,
        "1000 saved Film roles allocated {allocated} Rust bytes for one cover lookup"
    );
}

#[test]
fn confirmed_film_roles_match_the_exact_directory_and_source() {
    let (_home, storage) = fixture(1);
    storage.with_connection(|db| {
        db.execute("UPDATE file_observations SET observed_path='日本/movie.mp4' WHERE id='video-00000'", [])?;
        db.execute("UPDATE file_observations SET observed_path='日本/poster.jpg' WHERE id='image-00000'", [])?;
        db.execute("INSERT INTO libraries(id,display_name) VALUES('other','另一个合成来源')", [])?;
        db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('other-details','library_scan_details','other','current',?1)", [serde_json::json!({"directories":[],"cover_checks":[],"roles":[{"directory":"日本","role":"film"}]}).to_string()])?;
        db.execute("UPDATE issue_records SET evidence=?1 WHERE id='details'", [serde_json::json!({"directories":[],"cover_checks":[],"roles":[{"directory":"日本/子目录","role":"film"},{"directory":"日本","role":"actor"}]}).to_string()])?;
        Ok(())
    }).unwrap();
    assert!(
        covers::info(&storage, "film-00000")
            .unwrap()
            .candidates
            .is_empty()
    );
    storage.with_connection(|db| {
        db.execute("UPDATE issue_records SET evidence=?1 WHERE id='details'", [serde_json::json!({"directories":[],"cover_checks":[],"roles":[{"directory":"日本","role":"film"}]}).to_string()])?;
        Ok(())
    }).unwrap();
    let info = covers::info(&storage, "film-00000").unwrap();
    assert!(info.readable);
    assert_eq!(info.default_id.as_deref(), Some("image-00000"));
}

#[test]
fn shared_directory_ownership_does_not_borrow_another_directories_cover() {
    let (_home, storage) = fixture(2);
    storage.with_connection(|db| {
        db.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES('other-owner','source','film-00001','work-00000/other.mp4','video')", [])?;
        Ok(())
    }).unwrap();
    let info = covers::info(&storage, "film-00000").unwrap();
    assert!(info.needs_review);
    assert!(info.default_id.is_none());
    assert_eq!(info.candidates.len(), 1);
    assert_eq!(info.candidates[0].id, "image-00000");
}

#[test]
#[ignore = "release scale budget; run explicitly with --release --ignored --nocapture"]
fn ten_thousand_saved_covers_catalogue_stays_within_budget() {
    let (_home, storage) = fixture(10000);
    let started = Instant::now();
    let films = catalog::list_films(&storage).unwrap();
    let elapsed = started.elapsed();
    eprintln!("cover_index catalogue_ms={}", elapsed.as_millis());
    assert_eq!(films.len(), 10000);
    assert!(films.iter().all(|film| film.has_cover));
    assert!(
        elapsed < Duration::from_secs(2),
        "10000 independent covers catalogue took {elapsed:?}"
    );
}
