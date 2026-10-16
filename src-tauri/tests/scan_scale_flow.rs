use local_film_library_preview_lib::{
    catalog,
    inventory::inventory,
    observation::commit_confirmed_inventory,
    source_scan::{self, SourceScanSession},
    storage::Storage,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    fs,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

struct CountingAllocator;
thread_local! { static COUNT_ALLOCATIONS: Cell<bool> = const { Cell::new(false) }; }
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT_ALLOCATIONS.try_with(Cell::get).unwrap_or(false) {
            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNT_ALLOCATIONS.try_with(Cell::get).unwrap_or(false) {
            ALLOCATED.fetch_add(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn fixture(count: usize) -> (tempfile::TempDir, std::path::PathBuf, Storage) {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-source");
    fs::create_dir_all(root.join("clips")).unwrap();
    for i in 0..count {
        fs::write(root.join(format!("clips/clip-{i:05}.mp4")), []).unwrap();
    }
    let storage = Storage::open(&home.path().join("software")).unwrap();
    (home, root, storage)
}

#[test]
fn four_thousand_versions_preview_keeps_review_count_with_bounded_allocations() {
    let (_home, root, storage) = fixture(4000);
    ALLOCATED.store(0, Ordering::Relaxed);
    COUNT_ALLOCATIONS.set(true);
    let preview = SourceScanSession::new().preview(&storage, &root, None);
    COUNT_ALLOCATIONS.set(false);
    let allocated = ALLOCATED.load(Ordering::Relaxed);
    eprintln!("scale preview_allocated_bytes={allocated}");
    let preview = preview.unwrap();
    assert_eq!(preview.video_count, 4000);
    assert_eq!(preview.needs_review, 4000);
    assert_eq!(preview.video_directory_count, 1);
    assert!(
        allocated < 64 * 1024 * 1024,
        "preview allocated {allocated} Rust bytes for 4000 versions"
    );
}

#[test]
#[ignore = "release scale budget; run explicitly with --release --ignored --nocapture"]
fn ten_thousand_versions_commit_catalog_and_summary_stay_within_budget() {
    let (_home, root, storage) = fixture(10000);
    let report = inventory(&root).unwrap();
    storage.create_protection().unwrap();
    let started = Instant::now();
    let source = commit_confirmed_inventory(&storage, None, &root, &report).unwrap();
    let commit = started.elapsed();
    eprintln!("scale commit_ms={}", commit.as_millis());
    assert!(
        commit < Duration::from_secs(2),
        "10000-version commit took {commit:?}"
    );
    let started = Instant::now();
    let films = catalog::list_films(&storage).unwrap();
    let catalog = started.elapsed();
    eprintln!("scale catalog_ms={}", catalog.as_millis());
    assert_eq!(films.len(), 1);
    assert_eq!(films[0].videos.len(), 10000);
    assert!(
        catalog < Duration::from_secs(2),
        "10000-version catalog took {catalog:?}"
    );
    let started = Instant::now();
    let sources = source_scan::list_sources(&storage).unwrap();
    let summary = started.elapsed();
    eprintln!("scale summary_ms={}", summary.as_millis());
    assert_eq!(sources[0].id, source.library_id);
    assert_eq!(sources[0].film_count, 1);
    assert_eq!(sources[0].observation_count, 10000);
    assert!(
        summary < Duration::from_secs(1),
        "10000-version summary took {summary:?}"
    );
}
