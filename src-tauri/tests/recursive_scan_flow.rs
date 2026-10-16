#[path = "support/display_cache.rs"]
mod display_cache;
use local_film_library_preview_lib::{
    actors, catalog, covers, observation::scan_into_library, storage::Storage,
};
use std::{fs, path::Path};
fn file(root: &Path, p: &str) {
    let p = root.join(p);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, []).unwrap();
}
fn picture(root: &Path, p: &str) {
    let p = root.join(p);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    image::RgbImage::new(12, 16).save(p).unwrap();
}
fn bind(storage: &Storage, root: &Path) {
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO libraries(id,display_name,root_path) VALUES('s','合成来源',?1)",
                [root.to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn root_deep_and_actor_direct_videos_are_separate_directory_works() {
    let h = tempfile::tempdir().unwrap();
    let root = h.path().join("source");
    for p in [
        "one.mp4",
        "two.mkv",
        "分类/年份/甲/作品/a.mp4",
        "分类/年份/甲/作品/b.mp4",
        "分类/年份/甲/作品/花絮/extra.mp4",
        "分类/年份/甲/direct.mp4",
    ] {
        file(&root, p);
    }
    picture(&root, "分类/年份/甲/actor.png");
    let s = Storage::open(&h.path().join("data")).unwrap();
    bind(&s, &root);
    scan_into_library(&s, "s", &root).unwrap();
    let films = catalog::list_films(&s).unwrap();
    assert_eq!(films.len(), 4);
    assert_eq!(films.iter().map(|f| f.videos.len()).sum::<usize>(), 6);
    let a = actors::list_actors(&s).unwrap();
    assert!(a.is_empty());
    assert!(films.iter().all(|film| film.actors.is_empty()));
}

#[test]
fn poster_format_fallback_and_single_video_same_name_are_readable() {
    let h = tempfile::tempdir().unwrap();
    let root = h.path().join("source");
    file(&root, "multi/a.mp4");
    file(&root, "multi/b.mp4");
    file(&root, "multi/poster.jpg");
    picture(&root, "multi/poster.jpeg");
    picture(&root, "multi/a.png");
    file(&root, "single/main.mp4");
    picture(&root, "single/main.png");
    picture(&root, "single/random.jpg");
    let s = Storage::open(&h.path().join("data")).unwrap();
    bind(&s, &root);
    scan_into_library(&s, "s", &root).unwrap();
    let films = catalog::list_films(&s).unwrap();
    assert_eq!(films.len(), 2);
    for f in films {
        let info = covers::info(&s, &f.id).unwrap();
        assert!(info.readable);
        assert!(!info.needs_review);
        assert_eq!(info.candidates.len(), 1);
    }
}

#[test]
fn actor_markers_at_any_depth_never_create_actors_or_cast() {
    let h = tempfile::tempdir().unwrap();
    let root = h.path().join("source");
    file(&root, "甲/first/a.mp4");
    file(&root, "甲/乙/deep/b.mp4");
    picture(&root, "甲/actor.png");
    picture(&root, "甲/乙/actor.png");
    let s = Storage::open(&h.path().join("data")).unwrap();
    bind(&s, &root);
    scan_into_library(&s, "s", &root).unwrap();
    assert_eq!(catalog::list_films(&s).unwrap().len(), 2);
    assert!(actors::list_actors(&s).unwrap().is_empty());
    scan_into_library(&s, "s", &root).unwrap();
    let a = actors::list_actors(&s).unwrap();
    assert!(a.is_empty());
    assert!(catalog::list_films(&s).unwrap().iter().all(|film| film.actors.is_empty()));
}

#[test]
fn avif_and_version_images_decode_into_independent_cache_entries() {
    let h = tempfile::tempdir().unwrap();
    let root = h.path().join("source");
    file(&root, "multi/a.mp4");
    file(&root, "multi/b.mp4");
    picture(&root, "multi/poster.png");
    // Synthetic tiny image from the BSD-3-Clause avif-decode 3.0.0 test corpus.
    fs::write(
        root.join("multi/a.avif"),
        include_bytes!("fixtures/tiny.avif"),
    )
    .unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    bind(&s, &root);
    scan_into_library(&s, "s", &root).unwrap();
    display_cache::visit_source_images(&s, "s").unwrap();
    let film = catalog::list_films(&s).unwrap().remove(0);
    let a = film
        .videos
        .iter()
        .find(|v| v.observed_path.ends_with("a.mp4"))
        .unwrap();
    let b = film
        .videos
        .iter()
        .find(|v| v.observed_path.ends_with("b.mp4"))
        .unwrap();
    let card = covers::cached_thumbnail(&s, &film.id, true).unwrap();
    let avif = covers::cached_version_thumbnail(&s, &film.id, &a.id, true).unwrap();
    assert_ne!(card, avif);
    assert_eq!(
        card,
        covers::cached_version_thumbnail(&s, &film.id, &b.id, true).unwrap()
    );
    assert_eq!(card, covers::cached_thumbnail(&s, &film.id, true).unwrap());
}
