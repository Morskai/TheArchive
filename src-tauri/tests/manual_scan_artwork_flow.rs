use local_film_library_preview_lib::{
    catalog, covers, lifecycle, source_scan::SourceScanSession, storage::Storage,
    storage_session::StorageSession,
};
use std::fs;

fn picture(path: &std::path::Path, color: u8) {
    image::RgbImage::from_pixel(16,16,image::Rgb([color,30,40]))
        .save_with_format(path,image::ImageFormat::Jpeg).unwrap();
}

#[test]
fn manual_scan_invalidates_changed_sizes_lazily_and_bad_image_retains_successful_cache() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    fs::create_dir_all(root.join("film")).unwrap();
    fs::write(root.join("film/movie.mp4"), []).unwrap();
    let path = root.join("film/poster.jpg");
    picture(&path,20);
    let storage = Storage::open(&home.path().join("software")).unwrap();
    let scan = SourceScanSession::new();
    let token = scan.preview(&storage,&root,None).unwrap().token.unwrap();
    let source = scan.confirm(&storage,&token).unwrap().library_id;
    let film = catalog::list_films(&storage).unwrap().remove(0);
    let old_small = covers::cached_thumbnail(&storage,&film.id,false).unwrap();
    let old_large = covers::cached_thumbnail(&storage,&film.id,true).unwrap();
    let state = StorageSession::new(storage);
    picture(&path,200);
    let token = scan.preview_on(&state,&root,Some(&source)).unwrap().token.unwrap();
    let result = lifecycle::confirm_source_refresh_on(&state,&scan,&token).unwrap();
    assert!(result.artwork_keys.contains(&film.id));
    let storage = state.lock().unwrap();
    let index: serde_json::Value = serde_json::from_slice(&fs::read(storage.data_directory().join("cover-cache/index-v1.json")).unwrap()).unwrap();
    assert!(index["observations"].as_object().unwrap().is_empty(),"scan invalidates changed mappings without generating either size");
    let new_small = covers::cached_thumbnail(&storage,&film.id,false).unwrap();
    let new_large = covers::cached_thumbnail(&storage,&film.id,true).unwrap();
    assert_ne!(old_small,new_small);
    assert_ne!(old_large,new_large);
    drop(storage);
    fs::write(&path,b"synthetic incomplete image").unwrap();
    let token = scan.preview_on(&state,&root,Some(&source)).unwrap().token.unwrap();
    lifecycle::confirm_source_refresh_on(&state,&scan,&token).unwrap();
    let storage = state.lock().unwrap();
    assert_eq!(covers::cached_thumbnail(&storage,&film.id,false).unwrap(),new_small);
    assert_eq!(covers::cached_thumbnail(&storage,&film.id,true).unwrap(),new_large);
}

#[test]
fn manual_scan_updates_readable_image_while_bad_peer_keeps_its_prior_cache() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    for name in ["a","b"] {
        fs::create_dir_all(root.join(name)).unwrap();
        fs::write(root.join(name).join(format!("{name}.mp4")), []).unwrap();
        picture(&root.join(name).join("poster.jpg"),20);
    }
    let storage = Storage::open(&home.path().join("software")).unwrap();
    let scan = SourceScanSession::new();
    let token = scan.preview(&storage,&root,None).unwrap().token.unwrap();
    let source = scan.confirm(&storage,&token).unwrap().library_id;
    let films = catalog::list_films(&storage).unwrap();
    let a = &films.iter().find(|f|f.videos[0].observed_path=="a/a.mp4").unwrap().id;
    let b = &films.iter().find(|f|f.videos[0].observed_path=="b/b.mp4").unwrap().id;
    let old_a = covers::cached_thumbnail(&storage,a,false).unwrap();
    let old_b = covers::cached_thumbnail(&storage,b,false).unwrap();
    picture(&root.join("a/poster.jpg"),200);
    fs::write(root.join("b/poster.jpg"),b"synthetic incomplete image").unwrap();
    let token = scan.preview(&storage,&root,Some(&source)).unwrap().token.unwrap();
    lifecycle::confirm_source_refresh(&storage,&scan,&token).unwrap();
    assert_ne!(covers::cached_thumbnail(&storage,a,false).unwrap(),old_a);
    assert_eq!(covers::cached_thumbnail(&storage,b,false).unwrap(),old_b);
    assert_eq!(catalog::list_films(&storage).unwrap().len(),2);
}

#[test]
fn on_demand_cache_write_failure_keeps_original_and_film_and_reports_cache_error() {
    use sha2::{Digest,Sha256};
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    fs::create_dir_all(root.join("film")).unwrap();
    fs::write(root.join("film/movie.mp4"), []).unwrap();
    let path = root.join("film/poster.jpg");
    picture(&path,20);
    let original = fs::read(&path).unwrap();
    let storage = Storage::open(&home.path().join("software")).unwrap();
    let scan = SourceScanSession::new();
    let token = scan.preview(&storage,&root,None).unwrap().token.unwrap();
    scan.confirm(&storage,&token).unwrap();
    let film = catalog::list_films(&storage).unwrap().remove(0);
    let digest = format!("{:x}",Sha256::digest(&original));
    let cache = storage.data_directory().join("cover-cache");
    fs::create_dir_all(cache.join(format!("v1-{digest}-480.jpg"))).unwrap();
    assert_eq!(covers::cached_thumbnail(&storage,&film.id,false).unwrap_err(),"cover_cache_failed");
    assert!(!cache.join("index-v1.json").exists());
    assert_eq!(fs::read(&path).unwrap(),original);
    assert_eq!(catalog::list_films(&storage).unwrap()[0].id,film.id);
}
