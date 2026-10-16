use local_film_library_preview_lib::source_scan::{SourceScanSession, change_source_state, open_source_directory_with};
use local_film_library_preview_lib::storage::Storage;
use std::fs;

#[test]
fn source_folder_opens_saved_enabled_or_paused_root_without_changing_data() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic source 空格");
    fs::create_dir_all(root.join("Film")).unwrap();
    fs::write(root.join("Film/example.mp4"), b"synthetic").unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let session = SourceScanSession::new();
    let preview = session.preview(&storage, &root, None).unwrap();
    let id = session.confirm(&storage, &preview.token.unwrap()).unwrap().library_id;
    for paused in [false, true] {
        if paused { change_source_state(&storage, &id, "pause").unwrap(); }
        open_source_directory_with(&storage, &id, |actual| {
            assert_eq!(actual, root.canonicalize().unwrap());
            Ok(())
        }).unwrap();
        assert_eq!(fs::read(root.join("Film/example.mp4")).unwrap(), b"synthetic");
    }
    change_source_state(&storage, &id, "remove").unwrap();
    let error = open_source_directory_with(&storage, &id, |_| panic!("removed source must not launch")).unwrap_err();
    assert_eq!(error.to_string(), "source_not_found");
}

#[test]
fn source_folder_rejects_missing_root_and_unknown_id_without_launching() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("source");
    fs::create_dir_all(&root).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let session = SourceScanSession::new();
    let preview = session.preview(&storage, &root, None).unwrap();
    let id = session.confirm(&storage, &preview.token.unwrap()).unwrap().library_id;
    fs::remove_dir(&root).unwrap();
    assert_eq!(open_source_directory_with(&storage, &id, |_| panic!("missing source must not launch")).unwrap_err().to_string(), "directory_unavailable");
    assert_eq!(open_source_directory_with(&storage, "unknown", |_| panic!("unknown source must not launch")).unwrap_err().to_string(), "source_not_found");
}
