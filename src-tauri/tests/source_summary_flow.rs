use local_film_library_preview_lib::{
    source_scan::{self, SourceScanSession},
    storage::Storage,
};
use std::fs;
#[test]
fn saved_source_summary_is_current_and_available_offline() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("synthetic-source");
    fs::create_dir_all(root.join("film")).unwrap();
    fs::write(root.join("film/movie.mp4"), [0u8; 123]).unwrap();
    let storage = Storage::open(&temp.path().join("software")).unwrap();
    let scan = SourceScanSession::new();
    let preview = scan.preview(&storage, &root, None).unwrap();
    scan.confirm(&storage, &preview.token.unwrap()).unwrap();
    let saved = serde_json::to_value(&source_scan::list_sources(&storage).unwrap()[0]).unwrap();
    assert_eq!(saved["film_count"], 1);
    assert_eq!(saved["video_bytes"], 123);
    assert!(saved["last_scanned_at"].as_str().is_some());
    fs::rename(&root, temp.path().join("synthetic-disconnected")).unwrap();
    let offline = serde_json::to_value(&source_scan::list_sources(&storage).unwrap()[0]).unwrap();
    assert_eq!(saved, offline);
}
