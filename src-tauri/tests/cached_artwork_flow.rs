#[path = "support/display_cache.rs"]
mod display_cache;
use local_film_library_preview_lib::{
    actor_management::{self, SaveActorInput}, actors, catalog, covers, lifecycle,
    observation::scan_into_library,
    source_scan::SourceScanSession,
    storage::Storage,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn picture(path: &Path, color: u8) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::from_pixel(40, 60, image::Rgb([color, 25, 40]))
        .save_with_format(path, image::ImageFormat::Png)
        .unwrap();
}

fn save_actor(storage: &Storage, id: Option<&str>, photo: &Path, film: &str) -> String {
    actor_management::save(storage, &SaveActorInput {
        actor_id: id.map(str::to_owned), name: "演员甲".into(), aliases: vec![], profile: actors::Profile::default(),
        photo_path: Some(photo.to_string_lossy().into_owned()), remove_photo: false, film_ids: vec![film.into()],
    }).unwrap()
}

fn setup() -> (tempfile::TempDir, Storage, PathBuf, String, String, String, String) {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic");
    picture(&root.join("演员甲/actor.jpg"), 80);
    picture(&root.join("演员甲/[TEST-01] 影片/poster.jpg"), 120);
    fs::write(root.join("演员甲/[TEST-01] 影片/movie.mp4"), []).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let session = SourceScanSession::new();
    let preview = session.preview(&storage, &root, None).unwrap();
    let source = session
        .confirm(&storage, &preview.token.unwrap())
        .unwrap()
        .library_id;
    let film = catalog::list_films(&storage).unwrap().remove(0).id;
    let actor_image = storage.with_connection(|db| db.query_row("SELECT id FROM file_observations WHERE kind='image' AND observed_path='演员甲/actor.jpg'", [], |r| r.get::<_, String>(0))).unwrap();
    let actor = save_actor(&storage, None, &root.join("演员甲/actor.jpg"), &film);
    (home, storage, root, source, film, actor_image, actor)
}

fn cache_index(storage: &Storage) -> serde_json::Value {
    serde_json::from_slice(&fs::read(storage.data_directory().join("cover-cache/index-v1.json")).unwrap()).unwrap()
}

fn confirm_refresh(storage: &Storage, root: &Path, source: &str) -> lifecycle::SourceRefreshResult {
    let session = SourceScanSession::new();
    let preview = session.preview(storage, root, Some(source)).unwrap();
    lifecycle::confirm_source_refresh(storage, &session, &preview.token.unwrap()).unwrap()
}

#[test]
fn clearing_picture_cache_rebuilds_visible_film_from_its_saved_path_without_scan() {
    let (_home, storage, root, _source, film, _image, _actor) = setup();
    let before = fs::read(root.join("演员甲/[TEST-01] 影片/poster.jpg")).unwrap();
    lifecycle::clear_generated_cache(&storage).unwrap();
    let thumbnail = covers::cached_thumbnail(&storage, &film, false).unwrap();
    assert!(!thumbnail.is_empty());
    assert_eq!(catalog::list_films(&storage).unwrap()[0].id, film);
    assert_eq!(fs::read(root.join("演员甲/[TEST-01] 影片/poster.jpg")).unwrap(), before);
}

#[test]
fn viewed_images_keep_both_sizes_offline_and_after_restart(
) {
    let (home, storage, root, source, film, _actor_image, actor) = setup();
    let summary = display_cache::visit_source_images(&storage, &source).unwrap();
    assert_eq!((summary.generated, summary.failed), (1, 0));
    let cover_small = covers::cached_thumbnail(&storage, &film, false).unwrap();
    let cover_large = covers::cached_thumbnail(&storage, &film, true).unwrap();
    let actor_small = actors::thumbnail(&storage, &actor, false).unwrap();
    let actor_large = actors::thumbnail(&storage, &actor, true).unwrap();
    fs::remove_file(root.join("演员甲/[TEST-01] 影片/poster.jpg")).unwrap();
    fs::remove_file(root.join("演员甲/actor.jpg")).unwrap();
    fs::rename(&root, home.path().join("synthetic-offline")).unwrap();
    drop(storage);
    let storage = Storage::open(&home.path().join("data")).unwrap();
    assert_eq!(
        covers::cached_thumbnail(&storage, &film, false).unwrap(),
        cover_small
    );
    assert_eq!(
        covers::cached_thumbnail(&storage, &film, true).unwrap(),
        cover_large
    );
    assert_eq!(
        actors::thumbnail(&storage, &actor, false).unwrap(),
        actor_small
    );
    assert_eq!(
        actors::thumbnail(&storage, &actor, true).unwrap(),
        actor_large
    );
}

#[test]
fn successful_images_without_confirmed_updates_are_retained_until_explicit_cache_clear() {
    let (_home, storage, root, source, film, _actor_image, actor) = setup();
    display_cache::visit_source_images(&storage, &source).unwrap();
    let first_cover = covers::cached_thumbnail(&storage, &film, false).unwrap();
    let first_actor = actors::thumbnail(&storage, &actor, false).unwrap();
    picture(&root.join("演员甲/[TEST-01] 影片/poster.jpg"), 220);
    picture(&root.join("演员甲/actor.jpg"), 200);
    assert_eq!(
        covers::cached_thumbnail(&storage, &film, false).unwrap(),
        first_cover
    );
    assert_eq!(
        actors::thumbnail(&storage, &actor, false).unwrap(),
        first_actor
    );
    lifecycle::clear_generated_cache(&storage).unwrap();
    assert!(covers::cached_thumbnail(&storage, &film, false).unwrap() != first_cover);
    assert!(actors::thumbnail(&storage, &actor, false).unwrap() != first_actor);
}

#[test]
fn committed_scan_retains_unchanged_cache_and_only_invalidates_confirmed_changed_cover() {
    let (_home, storage, root, source, film, _actor_image, actor) = setup();
    display_cache::visit_source_images(&storage, &source).unwrap();
    let first_cover = covers::cached_thumbnail(&storage, &film, false).unwrap();
    let first_actor = actors::thumbnail(&storage, &actor, false).unwrap();
    let before = cache_index(&storage);
    let unchanged = confirm_refresh(&storage, &root, &source);
    assert!(unchanged.artwork_keys.is_empty());
    assert_eq!(unchanged.artwork_failed, 0);
    assert_eq!(cache_index(&storage), before);
    picture(&root.join("演员甲/[TEST-01] 影片/poster.jpg"), 220);
    picture(&root.join("演员甲/actor.jpg"), 200);
    let cover_id = covers::info(&storage, &film).unwrap().default_id.unwrap();
    let changed = confirm_refresh(&storage, &root, &source);
    assert_eq!(changed.artwork_keys, vec![film.clone()]);
    assert_eq!(changed.artwork_failed, 0);
    let after = cache_index(&storage);
    assert!(after["observations"].get(&cover_id).is_none());
    let actor_mappings = |index: &serde_json::Value| index["observations"].as_object().unwrap().iter()
        .filter(|(key,_)| key.starts_with(&format!("actor:{actor}@")))
        .map(|(key,value)| (key.clone(),value.clone())).collect::<std::collections::BTreeMap<_,_>>();
    assert!(!actor_mappings(&before).is_empty());
    assert_eq!(actor_mappings(&after), actor_mappings(&before));
    // Film scans do not update the separately managed actor source or its cache.
    assert!(actors::thumbnail(&storage, &actor, false).unwrap() == first_actor);
    assert!(covers::cached_thumbnail(&storage, &film, false).unwrap() != first_cover);
    // A redirected or invalid cache index is still rejected.
    let index = storage.data_directory().join("cover-cache/index-v1.json");
    fs::remove_file(&index).unwrap();
    fs::create_dir(&index).unwrap();
    assert!(display_cache::visit_source_images(&storage, &source).is_err());
    assert_eq!(actors::thumbnail(&storage, &actor, false).unwrap_err(), "cover_cache_failed");
}

#[test]
fn missing_cache_reads_only_saved_paths_on_demand_and_cache_clear_removes_the_index() {
    let (_home, storage, root, _source, film, actor_image, actor) = setup();
    assert!(!storage.data_directory().join("cover-cache").exists());
    let original_cover = fs::read(root.join("演员甲/[TEST-01] 影片/poster.jpg")).unwrap();
    let original_actor = fs::read(root.join("演员甲/actor.jpg")).unwrap();
    assert!(!covers::cached_thumbnail(&storage, &film, false).unwrap().is_empty());
    assert!(!covers::cached_observation_thumbnail(&storage, &actor_image, false).unwrap().is_empty());
    assert!(!actors::thumbnail(&storage, &actor, false).unwrap().is_empty());
    assert!(!storage.data_directory().join("actor-photos").exists());
    lifecycle::clear_generated_cache(&storage).unwrap();
    assert!(!storage
        .data_directory()
        .join("cover-cache/index-v1.json")
        .exists());
    fs::remove_file(root.join("演员甲/[TEST-01] 影片/poster.jpg")).unwrap();
    fs::remove_file(root.join("演员甲/actor.jpg")).unwrap();
    // Other valid images cannot replace either exact saved source path.
    picture(&root.join("演员甲/[TEST-01] 影片/poster.png"), 240);
    picture(&root.join("演员甲/actor.png"), 220);
    assert_eq!(covers::cached_thumbnail(&storage, &film, false).unwrap_err(), "cover_unavailable");
    assert_eq!(actors::thumbnail(&storage, &actor, false).unwrap_err(), "cover_unavailable");
    fs::write(root.join("演员甲/[TEST-01] 影片/poster.jpg"), &original_cover).unwrap();
    fs::write(root.join("演员甲/actor.jpg"), &original_actor).unwrap();
    assert!(covers::cached_thumbnail(&storage, &film, false).is_ok());
    assert!(actors::thumbnail(&storage, &actor, false).is_ok());
    assert_eq!(
        fs::read(root.join("演员甲/[TEST-01] 影片/poster.jpg")).unwrap(),
        original_cover
    );
    assert_eq!(
        fs::read(root.join("演员甲/actor.jpg")).unwrap(),
        original_actor
    );
}

#[test]
fn old_cover_digest_cache_is_compatible_but_actor_without_saved_mapping_is_not_guessed() {
    let (_home, storage, root, source, film, _actor_image, actor) = setup();
    display_cache::visit_source_images(&storage, &source).unwrap();
    let cover = covers::cached_thumbnail(&storage, &film, false).unwrap();
    storage.with_connection(|db| {
        let evidence: String = db.query_row("SELECT evidence FROM issue_records WHERE subject_type='library_scan_details' AND subject_id=?1", [&source], |r| r.get(0))?;
        let mut details: serde_json::Value = serde_json::from_str(&evidence).unwrap();
        // Old evidence may still contain retired actor-photo scan fields. They
        // must be ignored, never used to recreate an actor's missing source.
        details["portrait_checks"] = serde_json::json!([{"relative_path":"演员甲/actor.jpg","state":"readable","digest":"a".repeat(64)}]);
        db.execute("UPDATE issue_records SET evidence=?1 WHERE subject_type='library_scan_details' AND subject_id=?2", [&details.to_string(), &source])?;
        db.execute("DELETE FROM actor_image_sources WHERE actor_id=?1", [&actor])?;
        Ok(())
    }).unwrap();
    fs::remove_file(storage.data_directory().join("cover-cache/index-v1.json")).unwrap();
    fs::remove_file(root.join("演员甲/[TEST-01] 影片/poster.jpg")).unwrap();
    assert_eq!(
        covers::cached_thumbnail(&storage, &film, false).unwrap(),
        cover
    );
    assert_eq!(
        actors::thumbnail(&storage, &actor, false).unwrap_err(),
        "cover_missing"
    );
    assert!(!actors::list_actors(&storage).unwrap()[0].has_portrait);
    assert!(root.join("演员甲/actor.jpg").is_file());
}

#[test]
fn invalid_index_and_failed_index_write_never_touch_originals_or_follow_untrusted_paths() {
    let (_home, storage, root, source, film, actor_image, _actor) = setup();
    display_cache::visit_source_images(&storage, &source).unwrap();
    let cache = storage.data_directory().join("cover-cache");
    let index = cache.join("index-v1.json");
    let original = fs::read(root.join("演员甲/[TEST-01] 影片/poster.jpg")).unwrap();
    fs::write(&index, format!("{{\"version\":1,\"observations\":{{\"{actor_image}\":{{\"480\":\"../outside.jpg\"}}}}}}" )).unwrap();
    assert_eq!(
        covers::cached_observation_thumbnail(&storage, &actor_image, false).unwrap_err(),
        "cover_cache_failed"
    );
    fs::remove_file(&index).unwrap();
    fs::create_dir(&index).unwrap();
    assert_eq!(
        covers::cached_thumbnail(&storage, &film, false).unwrap_err(),
        "cover_cache_failed"
    );
    assert_eq!(
        fs::read(root.join("演员甲/[TEST-01] 影片/poster.jpg")).unwrap(),
        original
    );
}

#[test]
fn redirected_cache_directory_is_rejected_without_following_it() {
    let (home, storage, _root, source, film, _actor_image, _actor) = setup();
    display_cache::visit_source_images(&storage, &source).unwrap();
    let cache = storage.data_directory().join("cover-cache");
    let outside = home.path().join("synthetic-outside-cache");
    fs::rename(&cache, &outside).unwrap();
    let result = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&cache)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(
        covers::cached_thumbnail(&storage, &film, false).unwrap_err(),
        "cover_cache_failed"
    );
    assert!(!fs::read_dir(&outside)
        .unwrap()
        .collect::<Vec<_>>()
        .is_empty());
}

#[test]
fn redirected_index_or_cached_image_is_rejected_and_clear_preflight_keeps_other_cache_files() {
    for redirect_index in [false, true] {
        let (home, storage, _root, source, film, _actor_image, _actor) = setup();
        display_cache::visit_source_images(&storage, &source).unwrap();
        let cache = storage.data_directory().join("cover-cache");
        let index = cache.join("index-v1.json");
        let target = if redirect_index {
            index.clone()
        } else {
            let json: serde_json::Value =
                serde_json::from_slice(&fs::read(&index).unwrap()).unwrap();
            let image = covers::info(&storage, &film).unwrap().default_id.unwrap();
            cache.join(format!(
                "v1-{}-480.jpg",
                json["observations"][&image]["480"].as_str().unwrap()
            ))
        };
        fs::remove_file(&target).unwrap();
        let outside = home.path().join("synthetic-outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel.txt"), b"synthetic outside content").unwrap();
        let result = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&target)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(result.status.success());
        assert_eq!(
            covers::cached_thumbnail(&storage, &film, false).unwrap_err(),
            "cover_cache_failed"
        );
        let before_count = fs::read_dir(&cache).unwrap().count();
        assert!(lifecycle::clear_generated_cache(&storage).is_err());
        assert_eq!(fs::read_dir(&cache).unwrap().count(), before_count);
        assert_eq!(
            fs::read(outside.join("sentinel.txt")).unwrap(),
            b"synthetic outside content"
        );
    }
}

#[test]
fn scan_keeps_successful_cache_when_original_actor_image_becomes_invalid() {
    let (_home, storage, root, source, film, _actor_image, actor) = setup();
    local_film_library_preview_lib::scan_rules::save(&storage,&["忽略子目录".into()]).unwrap();
    display_cache::visit_source_images(&storage, &source).unwrap();
    let portrait = actors::thumbnail(&storage, &actor, false).unwrap();
    let index_before = cache_index(&storage);
    let ignored = root.join("演员甲/[TEST-01] 影片/忽略子目录");
    fs::create_dir_all(&ignored).unwrap();
    fs::write(
        ignored.join("poster.jpg"),
        b"synthetic ignored invalid image",
    )
    .unwrap();
    fs::write(
        ignored.join("actor.jpg"),
        b"synthetic ignored invalid image",
    )
    .unwrap();
    fs::write(
        root.join("演员甲/actor.jpg"),
        b"synthetic invalid actor image",
    )
    .unwrap();
    let result = confirm_refresh(&storage, &root, &source);
    assert!(result.artwork_keys.is_empty());
    assert_eq!(result.artwork_failed, 0);
    assert_eq!(cache_index(&storage), index_before);
    assert!(covers::cached_thumbnail(&storage, &film, false).is_ok());
    assert!(actors::thumbnail(&storage, &actor, false).unwrap() == portrait);
    assert_eq!(
        fs::read(ignored.join("poster.jpg")).unwrap(),
        b"synthetic ignored invalid image"
    );
    assert_eq!(
        fs::read(root.join("演员甲/actor.jpg")).unwrap(),
        b"synthetic invalid actor image"
    );
}

#[test]
fn confirmed_film_roles_with_category_names_keep_their_own_fixed_cover_and_cache() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-category-film-source");
    for (number, name) in ["日本", "多人", "FC2"].iter().enumerate() {
        picture(&root.join(name).join("poster.jpg"), 80 + number as u8);
        fs::write(root.join(name).join(format!("synthetic-{number}.mp4")), []).unwrap();
    }
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let session = SourceScanSession::new();
    let preview = session.preview(&storage, &root, None).unwrap();
    let source = session
        .confirm(&storage, &preview.token.unwrap())
        .unwrap()
        .library_id;
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 3);
    for film in &films {
        let info = covers::info(&storage, &film.id).unwrap();
        assert_eq!(info.candidates.len(), 1);
        assert_eq!(info.default_id.as_ref(), Some(&info.candidates[0].id));
    }
    let summary = display_cache::visit_source_images(&storage, &source).unwrap();
    assert_eq!((summary.generated, summary.failed), (3, 0));
    fs::rename(&root, home.path().join("synthetic-offline")).unwrap();
    for film in films {
        assert!(covers::cached_thumbnail(&storage, &film.id, false).is_ok());
        assert!(covers::cached_thumbnail(&storage, &film.id, true).is_ok());
    }
}

#[test]
fn conflicting_fixed_covers_keep_cache_generation_but_cannot_be_selected_for_display() {
    let (_home, storage, root, source, film, _actor_image, _actor) = setup();
    picture(&root.join("演员甲/[TEST-02] 另一个目录/poster.jpg"), 210);
    fs::write(root.join("演员甲/[TEST-02] 另一个目录/another.mp4"), []).unwrap();
    scan_into_library(&storage, &source, &root).unwrap();
    // A fictional established film identity spans two saved video directories.
    storage
        .with_connection(|db| {
            db.execute(
                "UPDATE file_observations SET film_id=?1 WHERE library_id=?2 AND kind='video'",
                [&film, &source],
            )?;
            Ok(())
        })
        .unwrap();
    let info = covers::info(&storage, &film).unwrap();
    assert!(info.needs_review);
    assert_eq!(info.candidates.len(), 2);
    let summary = display_cache::visit_source_images(&storage, &source).unwrap();
    assert_eq!((summary.generated, summary.failed), (2, 0));
    for large in [false, true] {
        assert_eq!(covers::cached_thumbnail(&storage, &film, large).unwrap_err(), "cover_needs_review");
    }
}

#[test]
fn rescan_keeps_cached_covers_and_portraits_after_originals_disappear() {
 let (_home,storage,root,source,film,_actor_image, actor) = setup();
 display_cache::visit_source_images(&storage,&source).unwrap();
 let small=covers::cached_thumbnail(&storage,&film,false).unwrap();
 let portrait=actors::thumbnail(&storage, &actor,false).unwrap();
 fs::remove_file(root.join("演员甲/[TEST-01] 影片/poster.jpg")).unwrap();
 fs::remove_file(root.join("演员甲/actor.jpg")).unwrap();
 let session=SourceScanSession::new();let preview=session.preview(&storage,&root,Some(&source)).unwrap();lifecycle::confirm_source_refresh(&storage,&session,&preview.token.unwrap()).unwrap();
 assert_eq!(covers::cached_thumbnail(&storage,&film,false).unwrap(),small);
 assert_eq!(actors::thumbnail(&storage, &actor,false).unwrap(),portrait);
 let actor=local_film_library_preview_lib::actors::list_actors(&storage).unwrap().remove(0);
 assert!(actor.has_portrait);
 assert_eq!(local_film_library_preview_lib::actors::thumbnail(&storage,&actor.id,false).unwrap(),portrait);
 lifecycle::clear_generated_cache(&storage).unwrap();
 assert!(covers::cached_thumbnail(&storage,&film,false).is_err());
}
#[test]
fn rescan_invalidates_changed_artwork_and_leaves_new_artwork_uncached_until_requested() {
 let (_home,storage,root,source,film,_, _actor) = setup();display_cache::visit_source_images(&storage,&source).unwrap();
 let original=covers::cached_thumbnail(&storage,&film,false).unwrap();
 picture(&root.join("演员甲/[TEST-01] 影片/poster.jpg"),240);
 picture(&root.join("演员甲/[TEST-02] 新影片/poster.jpg"),180);fs::write(root.join("演员甲/[TEST-02] 新影片/new.mp4"),[]).unwrap();
 let original_cover_id = covers::info(&storage,&film).unwrap().default_id.unwrap();
 let result = confirm_refresh(&storage,&root,&source);
 assert_eq!(result.artwork_failed,0);
 assert_eq!(catalog::list_films(&storage).unwrap().len(),2);
 let added=catalog::list_films(&storage).unwrap().into_iter().find(|f|f.id!=film).unwrap();
 let added_cover_id = covers::info(&storage,&added.id).unwrap().default_id.unwrap();
 assert!(result.artwork_keys.contains(&film));
 assert!(result.successful_artwork_keys.contains(&film));
 assert!(result.successful_artwork_keys.contains(&added.id));
 let index = cache_index(&storage);
 assert!(index["observations"].get(&original_cover_id).is_none());
 assert!(index["observations"].get(&added_cover_id).is_none());
 assert!(covers::cached_thumbnail(&storage,&film,false).unwrap()!=original);
 assert!(covers::cached_thumbnail(&storage,&added.id,false).is_ok());
 let index = cache_index(&storage);
 assert!(index["observations"][&original_cover_id]["480"].is_string());
 assert!(index["observations"][&added_cover_id]["480"].is_string());
 assert!(index["observations"][&original_cover_id].get("1200").is_none());
 assert!(index["observations"][&added_cover_id].get("1200").is_none());
}


#[test]
fn deleted_film_keeps_manual_records_and_cache_across_scan_restart_and_return() {
    let (home, storage, root, source, film, _, _actor) = setup();
    display_cache::visit_source_images(&storage, &source).unwrap();
    let original = covers::cached_thumbnail(&storage, &film, false).unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO film_manual(film_id,favorite,rating) VALUES(?1,1,10) ON CONFLICT(film_id) DO UPDATE SET favorite=1,rating=10", [&film])?;
        Ok(())
    }).unwrap();
    // Only synthetic temporary media is removed by the fixture.
    fs::remove_dir_all(root.join("演员甲/[TEST-01] 影片")).unwrap();
    let session = SourceScanSession::new();
    let preview = session.preview(&storage, &root, Some(&source)).unwrap();
    lifecycle::confirm_source_refresh(&storage, &session, &preview.token.unwrap()).unwrap();
    assert!(catalog::list_films(&storage).unwrap().is_empty());
    let index = fs::read(storage.data_directory().join("cover-cache/index-v1.json")).unwrap();
    assert!(!serde_json::from_slice::<serde_json::Value>(&index).unwrap()["observations"].as_object().unwrap().is_empty());
    drop(storage);
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let saved = storage.with_connection(|db| db.query_row("SELECT favorite,rating FROM film_manual WHERE film_id=?1", [&film], |r| Ok((r.get::<_,bool>(0)?,r.get::<_,i32>(1)?)))).unwrap();
    assert_eq!(saved,(true,10));
    fs::create_dir_all(root.join("演员甲/[TEST-01] 影片")).unwrap();
    fs::write(root.join("演员甲/[TEST-01] 影片/movie.mp4"), []).unwrap();
    let preview = session.preview(&storage, &root, Some(&source)).unwrap();
    lifecycle::confirm_source_refresh(&storage, &session, &preview.token.unwrap()).unwrap();
    let returned = catalog::list_films(&storage).unwrap().remove(0);
    assert!(returned.favorite);
    assert_eq!(returned.rating,Some(5.0));
    assert!(covers::cached_thumbnail(&storage, &returned.id, false).unwrap() == original);
}


#[test]
fn refreshed_fixed_image_names_do_not_conflict_with_retained_historical_observations() {
    let (_home, storage, root, source, film, _, _actor) = setup();
    display_cache::visit_source_images(&storage, &source).unwrap();
    fs::rename(root.join("演员甲/[TEST-01] 影片/poster.jpg"), root.join("演员甲/[TEST-01] 影片/POSTER.JPG")).unwrap();
    fs::rename(root.join("演员甲/actor.jpg"), root.join("演员甲/ACTOR.JPG")).unwrap();
    let session = SourceScanSession::new();
    let preview = session.preview(&storage, &root, Some(&source)).unwrap();
    lifecycle::confirm_source_refresh(&storage, &session, &preview.token.unwrap()).unwrap();
    assert!(covers::cached_thumbnail(&storage, &film, false).is_ok());
    let actor = local_film_library_preview_lib::actors::list_actors(&storage).unwrap().remove(0);
    assert!(local_film_library_preview_lib::actors::thumbnail(&storage, &actor.id, false).is_ok());
}
