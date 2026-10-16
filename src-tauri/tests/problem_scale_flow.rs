use local_film_library_preview_lib::{problems, storage::Storage};
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

// Saved synthetic metadata: no original media, application database, or decoder work.
fn fixture(count: usize) -> (tempfile::TempDir, Storage) {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("software")).unwrap();
    storage.with_transaction(|db| {
        db.execute("INSERT INTO libraries(id,display_name,root_path,scan_mode) VALUES('source','合成来源',?1,'general')", [home.path().join("fictional-source").to_string_lossy().as_ref()])?;
        let mut checks = Vec::new();
        let mut roles = Vec::new();
        let mut ids = Vec::new();
        for i in 0..count {
            let film = format!("film-{i:05}");
            let video = format!("video-{i:05}");
            let image = format!("image-{i:05}");
            let directory = format!("[TEST-{i:05}] work");
            db.execute("INSERT INTO films(id) VALUES(?1)", [&film])?;
            db.execute("INSERT INTO file_observations(id,library_id,film_id,observed_path,kind) VALUES(?1,'source',?2,?3,'video')", rusqlite::params![video,film,format!("{directory}/movie.mp4")])?;
            let path = format!("{directory}/poster.jpg");
            db.execute("INSERT INTO file_observations(id,library_id,observed_path,kind) VALUES(?1,'source',?2,'image')", rusqlite::params![image,path])?;
            checks.push(serde_json::json!({"relative_path":path,"state":if i % 5 == 0 { "unreadable" } else { "readable" },"digest":null}));
            roles.push(serde_json::json!({"directory":directory,"role":"film"}));
            ids.extend([video,image]);
        }
        let details = serde_json::json!({"directories":[],"cover_checks":checks,"roles":roles,"scan_mode":"general"}).to_string();
        db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('details','library_scan_details','source','current',?1)", [details])?;
        db.execute("INSERT INTO issue_records(id,subject_type,subject_id,evidence_key,evidence) VALUES('snapshot','cover_scan','source','current',?1)", [serde_json::to_string(&ids).unwrap()])?;
        Ok(())
    }).unwrap();
    (home, storage)
}

#[test]
fn one_thousand_work_directories_keep_problem_refresh_allocation_bounded() {
    let (_home, storage) = fixture(1000);
    let initial = problems::list(&storage).unwrap();
    assert_eq!(initial.len(), 200);
    let handled = initial[0].id.clone();
    problems::mark(&storage, std::slice::from_ref(&handled)).unwrap();
    ALLOCATED.set(0);
    COUNT_ALLOCATIONS.set(true);
    let refreshed = problems::list(&storage);
    COUNT_ALLOCATIONS.set(false);
    let allocated = ALLOCATED.get();
    eprintln!("problem_scale refresh_allocated_bytes={allocated}");
    let refreshed = refreshed.unwrap();
    assert_eq!(refreshed.len(), 200);
    assert!(refreshed.iter().find(|row| row.id == handled).unwrap().handled);
    assert!(allocated < 32 * 1024 * 1024,
        "1000 independent work directories allocated {allocated} Rust bytes during problem refresh");
}

#[test]
#[ignore = "release scale measurement; run explicitly with --release --ignored --nocapture"]
fn independent_work_problem_refresh_stays_within_budget() {
    for count in [500, 1000, 2000] {
        let (_home, storage) = fixture(count);
        problems::list(&storage).unwrap();
        let started = Instant::now();
        let rows = problems::list(&storage).unwrap();
        let elapsed = started.elapsed();
        eprintln!("problem_scale works={count} refresh_ms={}", elapsed.as_millis());
        assert_eq!(rows.len(), count / 5);
        assert!(elapsed < Duration::from_secs(3), "{count} works took {elapsed:?}");
    }
}

#[test]
fn problem_refresh_preserves_scan_evidence_order_across_work_directories() {
    let (_home, storage) = fixture(2);
    let paths = ["[TEST-00001] work/poster.jpg", "[TEST-00000] work/poster.jpg", "[TEST-00000] work/poster.png"];
    storage.with_connection(|db| {
        db.execute("UPDATE file_observations SET film_id='film-00000' WHERE id='video-00001'", [])?;
        let json: String = db.query_row("SELECT evidence FROM issue_records WHERE id='details'", [], |row| row.get(0))?;
        let mut details: serde_json::Value = serde_json::from_str(&json).unwrap();
        details["cover_checks"] = serde_json::json!(paths.map(|path| serde_json::json!({"relative_path":path,"state":"unreadable","digest":null})));
        db.execute("UPDATE issue_records SET evidence=?1 WHERE id='details'", [details.to_string()])?;
        Ok(())
    }).unwrap();
    let rows = problems::list(&storage).unwrap();
    let film = rows.iter().find(|row| row.subject_id == "film-00000").unwrap();
    let expected: Vec<String> = paths.into_iter().map(String::from).collect();
    storage.with_connection(|db| {
        let json: String = db.query_row("SELECT evidence FROM issue_records WHERE id='problem-current:film_missing_cover:film-00000'", [], |row| row.get(0))?;
        let evidence: serde_json::Value = serde_json::from_str(&json).unwrap();
        let actual: Vec<_> = evidence["artwork"].as_array().unwrap().iter()
            .map(|entry| entry["check"]["relative_path"].as_str().unwrap().to_owned()).collect();
        assert_eq!(actual, expected);
        Ok(())
    }).unwrap();
    problems::mark(&storage, std::slice::from_ref(&film.id)).unwrap();
    assert!(problems::list(&storage).unwrap().iter().find(|row| row.id == film.id).unwrap().handled);
}
