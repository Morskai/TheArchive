use local_film_library_preview_lib::{actors, storage::Storage};
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

// Saved actors and exact paths: listing must not load original pictures or videos.
fn fixture(count: usize) -> (tempfile::TempDir, Storage) {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-source");
    let storage = Storage::open(&home.path().join("software")).unwrap();
    storage.with_transaction(|db| {
        db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('source','合成',?1)", [root.to_string_lossy().as_ref()])?;
        for i in 0..count {
            let directory = format!("actor-{i:04}");
            let work = format!("{directory}/work-{i:04}");
            let actor = format!("actor-{i}");
            let film = format!("film-{i}");
            db.execute("INSERT INTO actors(id,observed_name) VALUES(?1,?2)", rusqlite::params![actor,directory])?;
            db.execute("INSERT INTO actor_image_sources(actor_id,path,fingerprint) VALUES(?1,?2,'synthetic')", rusqlite::params![actor,home.path().join(format!("original-{i}.jpg")).to_string_lossy().as_ref()])?;
            db.execute("INSERT INTO films(id) VALUES(?1)", [&film])?;
            db.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES(?1,'source',?2,?3,'video')", rusqlite::params![format!("video-{i}"),film,format!("{work}/movie-{i}.mp4")])?;
            db.execute("INSERT INTO file_observations(id,library_id,observed_path,kind) VALUES(?1,'source',?2,'image')", rusqlite::params![format!("cover-{i}"),format!("{work}/poster.jpg")])?;
            db.execute("INSERT INTO film_actors(film_id,actor_id,source) VALUES(?1,?2,'manual')", rusqlite::params![film,actor])?;
        }
        Ok(())
    }).unwrap();
    (home, storage)
}

#[test]
fn two_hundred_saved_actors_list_without_repeated_image_candidate_work() {
    let (_home, storage) = fixture(200);
    ALLOCATED.set(0);
    COUNT_ALLOCATIONS.set(true);
    let result = actors::list_actors(&storage);
    COUNT_ALLOCATIONS.set(false);
    let allocated = ALLOCATED.get();
    assert_eq!(result.unwrap().len(), 200);
    eprintln!("actor_candidates count=200 allocated_bytes={allocated}");
    assert!(allocated < 32 * 1024 * 1024, "200 saved actors allocated {allocated} bytes while checking already synchronized entries");
    storage.with_connection(|db| {
        assert_eq!(db.query_row::<usize,_,_>("SELECT COUNT(*) FROM film_actors WHERE source='manual'", [], |r|r.get(0))?, 200);
        assert_eq!(db.query_row::<usize,_,_>("SELECT COUNT(*) FROM actors", [], |r|r.get(0))?, 200);
        Ok(())
    }).unwrap();
}

#[test]
fn one_path_actor_thumbnail_does_not_load_every_other_actor_photo() {
    let (home, storage) = fixture(1000);
    let original = home.path().join("original-0.jpg");
    image::RgbImage::new(24, 32).save(&original).unwrap();
    let original_bytes = std::fs::read(&original).unwrap();
    ALLOCATED.set(0);
    COUNT_ALLOCATIONS.set(true);
    // Only the selected path may be read, even with many other saved paths.
    let result = actors::thumbnail(&storage,"actor-0",false);
    COUNT_ALLOCATIONS.set(false);
    let allocated = ALLOCATED.get();
    assert_eq!(image::guess_format(&result.unwrap()).unwrap(), image::ImageFormat::Jpeg);
    eprintln!("actor_single_thumbnail actors=1000 allocated_bytes={allocated}");
    assert!(allocated < 8 * 1024 * 1024,"single actor lookup allocated {allocated} bytes for other actors");
    assert_eq!(std::fs::read(&original).unwrap(), original_bytes);
    assert!(!home.path().join("original-1.jpg").exists());
}

#[test]
#[ignore = "Explicit release synthetic actor candidate scale budget; no real-library acceptance"]
fn two_thousand_saved_actor_candidates_release_budget() {
    assert!(!cfg!(debug_assertions), "run with --release");
    let (_home, storage) = fixture(2000);
    let started = Instant::now();
    assert_eq!(actors::list_actors(&storage).unwrap().len(), 2000);
    let elapsed = started.elapsed();
    eprintln!("actor_candidates count=2000 elapsed_ms={}", elapsed.as_millis());
    assert!(elapsed < Duration::from_secs(2), "saved actor candidates took {elapsed:?}");
}
