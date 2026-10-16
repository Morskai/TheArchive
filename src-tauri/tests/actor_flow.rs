#[path = "support/display_cache.rs"]
mod display_cache;
use local_film_library_preview_lib::{
    actor_management::{self, SaveActorInput}, actors, catalog, covers,
    observation::{ScanResult, scan_into_library},
    source_scan::{SourceScanSession, change_source_state}, storage::Storage,
};
use std::{fs, path::Path};

fn touch(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, []).unwrap();
}
fn scan(storage: &Storage, root: &Path, source: Option<&str>) -> String {
    let session = SourceScanSession::new();
    let preview = session.preview(storage, root, source).unwrap();
    session.confirm(storage, preview.token.as_deref().unwrap()).unwrap().library_id
}
fn empty_profile() -> actors::Profile {
    actors::Profile { birth_month: "".into(), height: "".into(), cup: "".into(), bust: "".into(), waist: "".into(), hips: "".into() }
}
fn saved_actor(storage: &Storage, name: &str, photo: Option<&Path>, films: Vec<String>) -> String {
    actor_management::save(storage, &SaveActorInput {
        actor_id: None, name: name.into(), aliases: vec![], profile: empty_profile(),
        photo_path: photo.map(|path| path.to_string_lossy().into_owned()), remove_photo: false, film_ids: films,
    }).unwrap()
}

#[test]
fn film_actor_photo_uses_saved_identity_and_verified_selected_version() {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    for path in ["甲/actor.jpg", "甲/一/poster.jpg", "甲/一/one.mp4", "甲/二/two.mp4", "乙/三/three.mp4"] { touch(&media, path); }
    image::RgbImage::from_pixel(24, 32, image::Rgb([180, 30, 40])).save(media.join("甲/actor.jpg")).unwrap();
    image::RgbImage::from_pixel(24, 32, image::Rgb([20, 160, 40])).save(media.join("甲/一/poster.jpg")).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = scan(&storage, &media, None);
    display_cache::visit_source_images(&storage, &source).unwrap();
    let films = catalog::list_films(&storage).unwrap();
    let direct = films.iter().find(|f| f.videos[0].observed_path.ends_with("一/one.mp4")).unwrap();
    let nested = films.iter().find(|f| f.videos[0].observed_path.ends_with("二/two.mp4")).unwrap();
    let other = films.iter().find(|f| f.videos[0].observed_path.ends_with("三/three.mp4")).unwrap();
    let actor = saved_actor(&storage, "甲", Some(&media.join("甲/actor.jpg")), vec![direct.id.clone(), nested.id.clone()]);
    let portrait = actors::film_thumbnail(&storage, &actor, &direct.id, &direct.videos[0].id).unwrap();
    assert_eq!(portrait, actors::thumbnail(&storage, &actor, false).unwrap());
    assert!(image::load_from_memory(&portrait).unwrap().to_rgb8().get_pixel(0, 0)[0] > 150);
    assert_eq!(actors::film_thumbnail(&storage, &actor, &nested.id, &nested.videos[0].id).unwrap(), portrait);
    assert_eq!(actors::film_thumbnail(&storage, &actor, &direct.id, &other.videos[0].id).unwrap_err(), "cover_not_owned");
    assert_eq!(actors::film_thumbnail(&storage, &actor, &other.id, &other.videos[0].id).unwrap_err(), "cover_not_owned");
    fs::remove_file(media.join("甲/actor.jpg")).unwrap();
    scan(&storage, &media, Some(&source));
    assert_eq!(actors::thumbnail(&storage, &actor, false).unwrap(), portrait);
    change_source_state(&storage, &source, "pause").unwrap();
    assert_eq!(actors::film_thumbnail(&storage, &actor, &direct.id, &direct.videos[0].id).unwrap_err(), "cover_not_owned");
    assert_eq!(actors::thumbnail(&storage, &actor, false).unwrap(), portrait);
    change_source_state(&storage, &source, "remove").unwrap();
    scan(&storage, &media, None);
    let current = catalog::list_films(&storage).unwrap();
    let restored = current.iter().find(|f| f.id == direct.id).unwrap();
    assert_ne!(restored.videos[0].id, direct.videos[0].id);
    assert_eq!(actors::film_thumbnail(&storage, &actor, &restored.id, &restored.videos[0].id).unwrap(), portrait);
    assert_eq!(actors::film_thumbnail(&storage, &actor, &restored.id, &direct.videos[0].id).unwrap_err(), "cover_not_owned");
}

#[test]
fn actor_markers_at_any_depth_do_not_create_actors_or_cast() {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    for path in ["甲（别名）/actor.jpg", "甲（别名）/一/a.mp4", "甲（别名）/一/b.mkv", "多人/actor.jpg", "多人/三/a.mp4", "别区/甲（别名）/actor.jpg", "别区/甲（别名）/一/a.mp4"] { touch(&media, path); }
    let storage = Storage::open(&home.path().join("data")).unwrap();
    scan(&storage, &media, None);
    assert!(actors::list_actors(&storage).unwrap().is_empty());
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 3);
    storage.with_connection(|db| {
        assert_eq!(db.query_row::<i64, _, _>("SELECT COUNT(*) FROM film_actors", [], |r| r.get(0))?, 0);
        Ok(())
    }).unwrap();
}

#[test]
fn source_removal_and_fresh_readd_preserve_saved_actor_profile_links_and_media() {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    for path in ["甲/actor.jpg", "甲/一/a.mp4", "甲/一/b.mkv", "甲/二/c.mp4"] { touch(&media, path); }
    image::RgbImage::new(24, 32).save(media.join("甲/actor.jpg")).unwrap();
    let original_photo = fs::read(media.join("甲/actor.jpg")).unwrap();
    let data = home.path().join("data");
    let storage = Storage::open(&data).unwrap();
    let source = scan(&storage, &media, None);
    let films = catalog::list_films(&storage).unwrap();
    let film_ids: Vec<_> = films.iter().map(|f| f.id.clone()).collect();
    let actor = saved_actor(&storage, "甲", Some(&media.join("甲/actor.jpg")), film_ids.clone());
    let portrait = actors::thumbnail(&storage, &actor, false).unwrap();
    actors::set_favorite(&storage, &actor, true).unwrap();
    actors::save_profile(&storage, &actor, &actors::Profile { birth_month: "1990-04".into(), height: "165".into(), cup: "c".into(), bust: "85".into(), waist: "60".into(), hips: "88".into() }).unwrap();
    let film = &films[0];
    catalog::set_rating(&storage, &film.id, Some(4)).unwrap();
    catalog::set_favorite(&storage, &film.id, true).unwrap();
    catalog::create_tag(&storage, "人工标签").unwrap();
    catalog::set_film_tags(&storage, &film.id, &["人工标签".into()]).unwrap();
    let video = &film.videos[0];
    storage.with_connection(|db| {
        db.execute("INSERT INTO file_versions(observation_id,preferred) VALUES(?1,1)", [&video.id])?;
        db.execute("INSERT INTO player_launch_events(id,film_id,observation_id,launched_at) VALUES('event',?1,?2,'2026-09-30T10:00:00Z')", rusqlite::params![film.id, video.id])?;
        Ok(())
    }).unwrap();
    for state in ["pause", "resume", "remove"] {
        change_source_state(&storage, &source, state).unwrap();
        let current = actors::list_actors(&storage).unwrap().remove(0);
        assert_eq!(current.id, actor);
        assert!(current.favorite);
        assert_eq!(actors::thumbnail(&storage, &actor, false).unwrap(), portrait);
    }
    scan(&storage, &media, None);
    let actor_current = actors::list_actors(&storage).unwrap().remove(0);
    assert_eq!(actor_current.id, actor);
    assert_eq!(actor_current.film_ids.into_iter().collect::<std::collections::BTreeSet<_>>(), film_ids.into_iter().collect());
    assert_eq!((actor_current.birth_month.as_str(), actor_current.height.as_str(), actor_current.cup.as_str(), actor_current.bust.as_str(), actor_current.waist.as_str(), actor_current.hips.as_str()), ("1990-04", "165", "C", "85", "60", "88"));
    let current_films = catalog::list_films(&storage).unwrap();
    let current = current_films.iter().find(|f| f.id == film.id).unwrap();
    assert_eq!(current.rating, Some(4.0));
    assert!(current.favorite);
    assert_eq!(current.tags, ["人工标签"]);
    assert_eq!(current.preferred_video_id, Some(current.videos.iter().find(|v| v.observed_path == video.observed_path).unwrap().id.clone()));
    assert_eq!(current.last_launched_at.as_deref(), Some("2026-09-30T10:00:00Z"));
    drop(storage);
    let reopened = Storage::open(&data).unwrap();
    assert_eq!(actors::list_actors(&reopened).unwrap()[0].id, actor);
    assert_eq!(actors::thumbnail(&reopened, &actor, false).unwrap(), portrait);
    assert_eq!(fs::read(media.join("甲/actor.jpg")).unwrap(), original_photo);
}

#[test]
fn offline_roots_preserve_manual_cast_without_guessing_actor_names() {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    for path in ["甲/actor.jpg", "甲/一/a.mp4", "无名/actor.jpg", "无名/二/b.mp4"] { touch(&media, path); }
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = scan(&storage, &media, None);
    let film = catalog::list_films(&storage).unwrap().remove(0);
    let actor = saved_actor(&storage, "人工演员", None, vec![film.id.clone()]);
    scan(&storage, &media, Some(&source));
    assert_eq!(actors::list_actors(&storage).unwrap().len(), 1);
    let bound = media.canonicalize().unwrap();
    fs::rename(&media, home.path().join("offline")).unwrap();
    assert!(matches!(scan_into_library(&storage, &source, &bound).unwrap(), ScanResult::RootUnavailable { .. }));
    let current = actors::list_actors(&storage).unwrap().remove(0);
    assert_eq!(current.id, actor);
    assert_eq!(current.film_ids, [film.id]);
}

#[test]
fn owned_actor_portrait_keeps_ratio_and_never_uses_film_cover() {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    touch(&media, "甲/一/a.mp4");
    let portrait = media.join("甲/actor.jpg");
    image::RgbImage::from_pixel(50, 70, image::Rgb([200, 50, 10])).save(&portrait).unwrap();
    image::RgbImage::from_pixel(80, 54, image::Rgb([10, 50, 200])).save(media.join("甲/一/poster.jpg")).unwrap();
    let original = fs::read(&portrait).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = scan(&storage, &media, None);
    display_cache::visit_source_images(&storage, &source).unwrap();
    let film = catalog::list_films(&storage).unwrap().remove(0);
    let actor = saved_actor(&storage, "甲", Some(&portrait), vec![film.id.clone()]);
    let bytes = actors::thumbnail(&storage, &actor, false).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert!((decoded.width() as f64 / decoded.height() as f64 - 5.0 / 7.0).abs() < 0.003);
    assert_ne!(bytes, covers::cached_thumbnail(&storage, &film.id, false).unwrap());
    assert_eq!(fs::read(&portrait).unwrap(), original);
    fs::write(&portrait, b"broken").unwrap();
    scan(&storage, &media, Some(&source));
    fs::remove_file(&portrait).unwrap();
    scan(&storage, &media, Some(&source));
    change_source_state(&storage, &source, "pause").unwrap();
    assert_eq!(actors::thumbnail(&storage, &actor, false).unwrap(), bytes);
}

#[test]
fn profile_validation_is_atomic_and_preserves_favorite_and_empty_fields() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let actor = saved_actor(&storage, "人工演员", None, vec![]);
    actors::set_favorite(&storage, &actor, true).unwrap();
    let mut profile = empty_profile();
    for month in ["1899-12", "2099-01", "1990-13", "出生年月"] {
        profile.birth_month = month.into();
        assert!(actors::save_profile(&storage, &actor, &profile).is_err());
    }
    profile.birth_month = "".into();
    for height in ["99", "165.5"] {
        profile.height = height.into();
        assert!(actors::save_profile(&storage, &actor, &profile).is_err());
    }
    profile.height = "".into(); profile.cup = "AA".into();
    assert!(actors::save_profile(&storage, &actor, &profile).is_err());
    profile.cup = "".into();
    actors::save_profile(&storage, &actor, &profile).unwrap();
    storage.with_connection(|db| {
        let row: (bool, Option<String>, Option<i64>) = db.query_row("SELECT favorite,birth_month,height_cm FROM actor_manual WHERE actor_id=?1", [&actor], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        assert_eq!(row, (true, None, None)); Ok(())
    }).unwrap();
}

#[test]
fn nested_actor_markers_do_not_change_recursive_film_ownership() {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    for path in ["作品/poster.jpg", "作品/main.mp4", "作品/花絮/extra.mp4", "外层/actor.jpg", "外层/甲/actor.jpg", "外层/甲/一/a.mp4"] { touch(&media, path); }
    let storage = Storage::open(&home.path().join("data")).unwrap();
    scan(&storage, &media, None);
    assert!(actors::list_actors(&storage).unwrap().is_empty());
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 3);
    assert!(films.iter().all(|f| f.videos.len() == 1 && f.actors.is_empty()));
}

#[test]
fn nearer_actor_marker_does_not_replace_saved_parent_cast() {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    for path in ["甲/actor.jpg", "甲/film/main.mp4", "甲/film/乙/actor.jpg", "甲/film/乙/nested.mp4"] { touch(&media, path); }
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = scan(&storage, &media, None);
    let film = catalog::list_films(&storage).unwrap().into_iter().find(|f| f.videos[0].observed_path.ends_with("film/main.mp4")).unwrap();
    let actor = saved_actor(&storage, "人工演员", None, vec![film.id.clone()]);
    actors::set_favorite(&storage, &actor, true).unwrap();
    scan(&storage, &media, Some(&source));
    fs::rename(&media, home.path().join("offline")).unwrap();
    let current = actors::list_actors(&storage).unwrap().remove(0);
    assert_eq!(current.id, actor);
    assert!(current.favorite);
    assert_eq!(current.film_ids, [film.id]);
}

#[test]
fn actor_write_failure_preserves_profile_photo_cast_and_film_observations() {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    touch(&media, "甲/一/a.mp4");
    image::RgbImage::new(24, 32).save(media.join("甲/actor.jpg")).unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    scan(&storage, &media, None);
    let films = catalog::list_films(&storage).unwrap();
    let actor = saved_actor(&storage, "甲", Some(&media.join("甲/actor.jpg")), vec![films[0].id.clone()]);
    actors::set_favorite(&storage, &actor, true).unwrap();
    let before = serde_json::to_value(actors::list_actors(&storage).unwrap()).unwrap();
    let films_before = serde_json::to_value(catalog::list_films(&storage).unwrap()).unwrap();
    let portrait = actors::thumbnail(&storage, &actor, false).unwrap();
    // Fail after the saved name, profile and photo changes, while replacing cast.
    storage.with_connection(|db| db.execute_batch("CREATE TRIGGER fail_actor BEFORE DELETE ON film_actors BEGIN SELECT RAISE(ABORT,'injected actor failure'); END;")).unwrap();
    assert!(actor_management::save(&storage, &SaveActorInput {
        actor_id: Some(actor.clone()), name: "新姓名".into(), aliases: vec!["新别名".into()], profile: empty_profile(), photo_path: None, remove_photo: true, film_ids: vec![],
    }).is_err());
    assert_eq!(serde_json::to_value(actors::list_actors(&storage).unwrap()).unwrap(), before);
    assert_eq!(actors::thumbnail(&storage, &actor, false).unwrap(), portrait);
    assert_eq!(serde_json::to_value(catalog::list_films(&storage).unwrap()).unwrap(), films_before);
}

#[test]
fn incomplete_rescan_keeps_saved_actors_and_manual_profile() {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    touch(&media, "甲/一/a.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let source = scan(&storage, &media, None);
    let film = catalog::list_films(&storage).unwrap().remove(0);
    let actor = saved_actor(&storage, "甲", None, vec![film.id.clone()]);
    actors::set_favorite(&storage, &actor, true).unwrap();
    let outside = home.path().join("outside"); fs::create_dir(&outside).unwrap();
    #[cfg(windows)] {
        let status = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(media.join("linked")).arg(&outside).output().unwrap();
        assert!(status.status.success());
    }
    #[cfg(unix)] std::os::unix::fs::symlink(&outside, media.join("linked")).unwrap();
    assert!(matches!(scan_into_library(&storage, &source, &media).unwrap(), ScanResult::Incomplete { .. }));
    let current = actors::list_actors(&storage).unwrap().remove(0);
    assert_eq!(current.id, actor); assert!(current.favorite); assert_eq!(current.film_ids, [film.id]);
}
