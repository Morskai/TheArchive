#[path = "support/display_cache.rs"]
mod display_cache;
use local_film_library_preview_lib::{
    actor_management::{self, SaveActorInput}, actors, catalog, covers, observation::scan_into_library, problems, storage::Storage,
};
use std::{fs, path::Path};

fn image(path: &Path, color: u8) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::from_pixel(24, 32, image::Rgb([color, 40, 80]))
        .save_with_format(path, image::ImageFormat::Png)
        .unwrap();
}
fn video(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, []).unwrap();
}
fn bind(storage: &Storage, root: &Path) {
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO libraries(id,display_name,root_path) VALUES('source','虚构来源',?1)",
                [root.to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
        .unwrap();
    scan_into_library(storage, "source", root).unwrap();
    display_cache::visit_source_images(storage, "source").unwrap();
}
fn saved_actor(storage: &Storage, portrait: &Path, films: Vec<String>) -> String {
    actor_management::save(storage, &SaveActorInput {
        actor_id: None, name: "甲".into(), aliases: vec![], profile: actors::Profile::default(),
        photo_path: Some(portrait.to_string_lossy().into_owned()), remove_photo: false, film_ids: films,
    }).unwrap()
}

#[test]
fn fixed_cover_wins_over_other_images_and_legacy_manual_choice() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    image(&root.join("甲/actor.jpg"), 240);
    let dir = root.join("甲/[TEST-01] 影片");
    video(&dir.join("movie.mp4"));
    video(&dir.join("second.mkv"));
    for (name, color) in [
        ("poster.jpg", 20),
        ("movie.jpg", 100),
        ("actor.jpg", 160),
        ("cover.png", 210),
    ] {
        image(&dir.join(name), color);
    }
    let s = Storage::open(&home.path().join("data")).unwrap();
    bind(&s, &root);
    let film = catalog::list_films(&s).unwrap().remove(0);
    s.with_connection(|db| {
        db.execute(
            "INSERT INTO film_manual(film_id,cover_choice,rating) VALUES(?1,'legacy-choice',8)",
            [&film.id],
        )?;
        Ok(())
    })
    .unwrap();
    let info = covers::info(&s, &film.id).unwrap();
    assert_eq!(info.candidates.len(), 1);
    assert_eq!(info.candidates[0].name, "poster.jpg");
    assert!(!info.needs_review);
    assert!(covers::cached_thumbnail(&s, &film.id, false).is_ok());
    assert!(problems::list(&s).unwrap().is_empty());
    s.with_connection(|db| {
        assert_eq!(
            db.query_row(
                "SELECT cover_choice FROM film_manual WHERE film_id=?1",
                [&film.id],
                |r| r.get::<_, String>(0)
            )?,
            "legacy-choice"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn unrelated_names_never_become_a_cover_or_actor_marker() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    let dir = root.join("[TEST-01] 影片");
    video(&dir.join("movie.mp4"));
    image(&root.join("random.jpg"), 80);
    image(&root.join("portrait.png"), 100);
    for name in [
        "random-movie.jpg",
        "front.png",
        "cover.jpeg",
        "cover.png",
        "portrait.png",
    ] {
        image(&dir.join(name), 50);
    }
    let s = Storage::open(&home.path().join("data")).unwrap();
    bind(&s, &root);
    let film = catalog::list_films(&s).unwrap().remove(0);
    assert!(covers::info(&s, &film.id).unwrap().candidates.is_empty());
    assert!(actors::list_actors(&s).unwrap().is_empty());
    let problems = problems::list(&s).unwrap();
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].kind, "film_missing_cover");
}

#[test]
fn explicitly_saved_actor_photo_serves_library_and_film_detail_without_borrowing_cover() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    video(&root.join("甲/[TEST-01] 影片/movie.mp4"));
    image(&root.join("甲/actor.jpg"), 180);
    image(&root.join("甲/poster.jpg"), 80);
    image(&root.join("甲/[TEST-01] 影片/poster.jpg"), 20);
    let s = Storage::open(&home.path().join("data")).unwrap();
    bind(&s, &root);
    let film = catalog::list_films(&s).unwrap().remove(0);
    let actor = saved_actor(&s, &root.join("甲/actor.jpg"), vec![film.id.clone()]);
    let portrait = actors::thumbnail(&s, &actor, false).unwrap();
    assert_eq!(
        portrait,
        actors::film_thumbnail(&s, &actor, &film.id, &film.videos[0].id).unwrap()
    );
    assert_ne!(
        portrait,
        covers::cached_thumbnail(&s, &film.id, false).unwrap()
    );
}

#[test]
fn legacy_marker_does_not_replace_saved_photo_path_and_cache_clear_keeps_manual_profile() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    video(&root.join("甲/[TEST-01] 影片/movie.mp4"));
    image(&root.join("甲/actor.jpg"), 180);
    let data = home.path().join("data");
    let s = Storage::open(&data).unwrap();
    bind(&s, &root);
    let actor = saved_actor(&s, &root.join("甲/actor.jpg"), vec![]);
    actors::set_favorite(&s, &actor, true).unwrap();
    let portrait = actors::thumbnail(&s, &actor, false).unwrap();
    // A legacy source marker cannot replace the explicitly saved actor path.
    image(&root.join("甲/poster.jpg"), 20);
    scan_into_library(&s, "source", &root).unwrap();
    s.with_connection(|db| {
        db.execute("INSERT INTO file_observations(id,library_id,observed_path,kind) VALUES('legacy-other-image','source','甲/poster.jpg','image')", [])?;
        db.execute("INSERT INTO actor_directories(actor_id,directory_key,directory_name,marker_id) VALUES(?1,'legacy-directory','甲','legacy-other-image')", [&actor])?;
        Ok(())
    }).unwrap();
    assert_eq!(actors::list_actors(&s).unwrap()[0].id, actor);
    fs::remove_file(root.join("甲/actor.jpg")).unwrap();
    scan_into_library(&s, "source", &root).unwrap();
    let current = actors::list_actors(&s).unwrap().remove(0);
    assert!(current.favorite);
    assert!(current.has_portrait);
    let cached = actors::thumbnail(&s, &actor, false).unwrap();
    assert!(cached == portrait);
    let pixels = image::load_from_memory(&cached).unwrap().to_rgb8();
    assert!(pixels.get_pixel(0, 0)[0] > 150);
    local_film_library_preview_lib::lifecycle::clear_generated_cache(&s).unwrap();
    assert!(actors::list_actors(&s).unwrap()[0].has_portrait);
    assert_eq!(actors::thumbnail(&s, &actor, false).unwrap_err(), "cover_unavailable");
    assert!(root.join("甲/poster.jpg").is_file());
    let reopened = Storage::open(&data).unwrap();
    let current = actors::list_actors(&reopened).unwrap().remove(0);
    assert_eq!(current.id, actor);
    assert!(current.favorite && current.has_portrait);
    assert_eq!(actors::thumbnail(&reopened, &actor, false).unwrap_err(), "cover_unavailable");
}

#[test]
fn actor_child_film_and_extra_directory_own_their_respective_covers() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    image(&root.join("甲/actor.jpg"), 180);
    video(&root.join("甲/direct.mp4"));
    video(&root.join("甲/[TEST-01] 影片/movie.mp4"));
    video(&root.join("甲/[TEST-01] 影片/花絮/extra.mp4"));
    image(&root.join("甲/[TEST-01] 影片/poster.jpg"), 20);
    image(&root.join("甲/[TEST-01] 影片/actor.jpg"), 80);
    image(&root.join("甲/[TEST-01] 影片/花絮/poster.jpg"), 100);
    let s = Storage::open(&home.path().join("data")).unwrap();
    bind(&s, &root);
    let film = catalog::list_films(&s)
        .unwrap()
        .into_iter()
        .find(|f| {
            f.videos
                .iter()
                .any(|v| v.observed_path.ends_with("/movie.mp4"))
        })
        .unwrap();
    assert!(covers::cached_thumbnail(&s, &film.id, false).is_ok());
    assert_eq!(film.videos.len(), 1);
    assert!(actors::list_actors(&s).unwrap().is_empty());
    s.with_connection(|db| {
        assert_eq!(db.query_row("SELECT COUNT(*) FROM file_observations WHERE observed_path='甲/direct.mp4' OR observed_path LIKE '%/花絮/%'", [], |r| r.get::<_, i64>(0))?, 3);
        Ok(())
    }).unwrap();
}

#[test]
fn source_root_files_do_not_conflict_with_an_owned_leaf_cover() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    video(&root.join("[TEST-01] 影片/movie.mp4"));
    for name in ["one.mp4", "two.mp4"] {
        video(&root.join(name));
    }
    image(&root.join("poster.jpg"), 160);
    image(&root.join("[TEST-01] 影片/poster.jpg"), 20);
    let s = Storage::open(&home.path().join("data")).unwrap();
    bind(&s, &root);
    let film = catalog::list_films(&s)
        .unwrap()
        .into_iter()
        .find(|f| {
            f.videos
                .iter()
                .any(|v| v.observed_path.ends_with("/movie.mp4"))
        })
        .unwrap();
    assert_eq!(catalog::list_films(&s).unwrap().len(), 2);
    assert_eq!(film.videos.len(), 1);
    let info = covers::info(&s, &film.id).unwrap();
    assert!(!info.needs_review);
    assert!(covers::cached_thumbnail(&s, &film.id, false).is_ok());
}

#[test]
fn actor_marker_and_direct_video_at_source_root_produce_only_film_records() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("甲");
    video(&root.join("movie.mp4"));
    image(&root.join("actor.jpg"), 180);
    video(&root.join("[TEST-01] 影片/leaf.mp4"));
    image(&root.join("[TEST-01] 影片/poster.jpg"), 80);
    let s = Storage::open(&home.path().join("data")).unwrap();
    bind(&s, &root);
    assert!(actors::list_actors(&s).unwrap().is_empty());
    assert_eq!(catalog::list_films(&s).unwrap().len(), 2);
    let film = catalog::list_films(&s).unwrap().into_iter().find(|f|f.videos[0].observed_path.contains("leaf.mp4")).unwrap();
    assert_eq!(film.videos.len(), 1);
    assert_eq!(film.videos[0].observed_path, "[TEST-01] 影片/leaf.mp4");
    s.with_connection(|db| {
        assert_eq!(db.query_row("SELECT COUNT(*) FROM file_observations WHERE observed_path IN ('movie.mp4','actor.jpg')", [], |r| r.get::<_, i64>(0))?, 2);
        Ok(())
    }).unwrap();
}

#[test]
fn fixed_names_follow_windows_case_convention_without_a_candidate_selector() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    video(&root.join("甲/[TEST-01] 影片/movie.mp4"));
    image(&root.join("甲/ACTOR.JPG"), 180);
    image(&root.join("甲/[TEST-01] 影片/POSTER.JPG"), 20);
    let s = Storage::open(&home.path().join("data")).unwrap();
    bind(&s, &root);
    let film = catalog::list_films(&s).unwrap().remove(0);
    assert!(covers::cached_thumbnail(&s, &film.id, false).is_ok());
    assert!(actors::list_actors(&s).unwrap().is_empty());
    let actor = saved_actor(&s, &root.join("甲/ACTOR.JPG"), vec![film.id.clone()]);
    assert!(actors::thumbnail(&s, &actor, false).is_ok());
}
