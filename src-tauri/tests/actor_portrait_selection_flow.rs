use local_film_library_preview_lib::{
    actor_management::{self, SaveActorInput}, actors, lifecycle, storage::Storage,
};
use std::{fs, path::Path};

fn picture(path: &Path, color: [u8;3]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::from_pixel(32,48,image::Rgb(color)).save(path).unwrap();
}
fn input(id: Option<&str>, photo: Option<&Path>) -> SaveActorInput {
    SaveActorInput {
        actor_id: id.map(str::to_owned), name: "演员".into(), aliases: vec![],
        profile: actors::Profile { birth_month: "".into(), height: "".into(), cup: "".into(), bust: "".into(), waist: "".into(), hips: "".into() },
        photo_path: photo.map(|path|path.to_string_lossy().into_owned()), remove_photo: false, film_ids: vec![],
    }
}

#[test]
fn explicit_png_replacement_persists_when_original_changes_or_disappears() {
    let home = tempfile::tempdir().unwrap();
    let jpg = home.path().join("original/actor.jpg"); let png = home.path().join("original/actor.png");
    picture(&jpg,[220,20,20]); picture(&png,[20,20,220]);
    let storage = Storage::open(&home.path().join("software")).unwrap();
    let actor = actor_management::save(&storage,&input(None,Some(&jpg))).unwrap();
    actors::set_favorite(&storage,&actor,true).unwrap();
    let red = actors::thumbnail(&storage,&actor,false).unwrap();
    actor_management::save(&storage,&input(Some(&actor),Some(&png))).unwrap();
    let blue = actors::thumbnail(&storage,&actor,false).unwrap();
    assert_ne!(red,blue);
    assert_eq!(image::guess_format(&blue).unwrap(),image::ImageFormat::Jpeg);
    fs::write(&png,b"invalid replacement").unwrap(); fs::remove_file(&jpg).unwrap();
    assert_eq!(actors::thumbnail(&storage,&actor,false).unwrap(),blue);
    let current = actors::list_actors(&storage).unwrap().remove(0);
    assert_eq!(current.id,actor); assert!(current.favorite && current.has_portrait);
}

#[test]
fn unreadable_replacement_preserves_successful_photo_and_actor_fields() {
    let home = tempfile::tempdir().unwrap();
    let jpg = home.path().join("original/actor.jpg"); let png = home.path().join("original/actor.png");
    picture(&jpg,[220,20,20]); fs::write(&png,b"invalid new image").unwrap();
    let storage = Storage::open(&home.path().join("software")).unwrap();
    let actor = actor_management::save(&storage,&input(None,Some(&jpg))).unwrap();
    let red = actors::thumbnail(&storage,&actor,false).unwrap();
    let before = serde_json::to_value(actors::list_actors(&storage).unwrap()).unwrap();
    let mut replacement = input(Some(&actor),Some(&png)); replacement.name = "不应保存".into(); replacement.aliases = vec!["不应保存".into()];
    assert!(actor_management::save(&storage,&replacement).is_err());
    assert_eq!(actors::thumbnail(&storage,&actor,false).unwrap(),red);
    assert_eq!(serde_json::to_value(actors::list_actors(&storage).unwrap()).unwrap(),before);
}

#[test]
fn clearing_generated_cache_rereads_only_the_saved_photo_path() {
    let home = tempfile::tempdir().unwrap();
    let jpg = home.path().join("original/actor.jpg"); let png = home.path().join("original/actor.png");
    picture(&jpg,[220,20,20]); picture(&png,[20,20,220]);
    let storage = Storage::open(&home.path().join("software")).unwrap();
    let actor = actor_management::save(&storage,&input(None,Some(&jpg))).unwrap();
    actor_management::save(&storage,&input(Some(&actor),Some(&png))).unwrap();
    let blue = actors::thumbnail(&storage,&actor,false).unwrap();
    let large_blue = actors::thumbnail(&storage,&actor,true).unwrap();
    let originals = (fs::read(&jpg).unwrap(), fs::read(&png).unwrap());
    let saved = storage.with_connection(|db| db.query_row("SELECT path,fingerprint FROM actor_image_sources WHERE actor_id=?1", [&actor], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))).unwrap();
    assert_eq!(saved.0, png.to_string_lossy());
    assert_eq!(saved.1.len(), 64);
    assert!(!storage.data_directory().join("actor-photos").exists());
    lifecycle::clear_generated_cache(&storage).unwrap();
    assert!(actors::thumbnail(&storage,&actor,false).unwrap() == blue);
    assert!(actors::thumbnail(&storage,&actor,true).unwrap() == large_blue);
    assert!(actors::list_actors(&storage).unwrap()[0].has_portrait);
    assert_eq!(fs::read(&jpg).unwrap(), originals.0);
    assert_eq!(fs::read(&png).unwrap(), originals.1);
    fs::write(&png, b"invalid saved PNG").unwrap();
    assert!(actors::thumbnail(&storage,&actor,false).unwrap() == blue);
    lifecycle::clear_generated_cache(&storage).unwrap();
    assert_eq!(actors::thumbnail(&storage,&actor,false).unwrap_err(), "cover_invalid");
    // The valid former JPG is never substituted for the explicitly saved PNG.
    let after = storage.with_connection(|db| db.query_row("SELECT path,fingerprint FROM actor_image_sources WHERE actor_id=?1", [&actor], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))).unwrap();
    assert_eq!(after, saved);
}

#[test]
fn actor_without_films_remains_visible_with_placeholder_after_original_and_cache_are_removed() {
    let home = tempfile::tempdir().unwrap();
    let jpg = home.path().join("original/actor.jpg"); picture(&jpg,[220,20,20]);
    let data = home.path().join("software");
    let storage = Storage::open(&data).unwrap();
    let actor = actor_management::save(&storage,&input(None,Some(&jpg))).unwrap();
    actors::set_favorite(&storage,&actor,true).unwrap();
    let red = actors::thumbnail(&storage,&actor,false).unwrap();
    fs::remove_file(&jpg).unwrap();
    assert!(actors::thumbnail(&storage,&actor,false).unwrap() == red);
    picture(&home.path().join("original/actor.png"), [20,20,220]);
    lifecycle::clear_generated_cache(&storage).unwrap();
    drop(storage);
    let storage = Storage::open(&data).unwrap();
    let current = actors::list_actors(&storage).unwrap().remove(0);
    assert_eq!(current.id,actor); assert!(current.favorite && current.film_ids.is_empty() && current.has_portrait);
    assert_eq!(actors::thumbnail(&storage,&actor,false).unwrap_err(), "cover_unavailable");
    let path = storage.with_connection(|db| db.query_row("SELECT path FROM actor_image_sources WHERE actor_id=?1", [&actor], |r| r.get::<_,String>(0))).unwrap();
    assert_eq!(path, jpg.to_string_lossy());
}

#[test]
fn failed_valid_photo_save_preserves_offline_successful_cache_and_actor_fields() {
    let home = tempfile::tempdir().unwrap();
    let jpg = home.path().join("original/actor.jpg");
    let png = home.path().join("original/replacement.png");
    picture(&jpg,[220,20,20]); picture(&png,[20,20,220]);
    let storage = Storage::open(&home.path().join("software")).unwrap();
    let actor = actor_management::save(&storage,&input(None,Some(&jpg))).unwrap();
    actors::set_favorite(&storage,&actor,true).unwrap();
    let small = actors::thumbnail(&storage,&actor,false).unwrap();
    let large = actors::thumbnail(&storage,&actor,true).unwrap();
    let before = serde_json::to_value(actors::list_actors(&storage).unwrap()).unwrap();
    let source_before = storage.with_connection(|db| db.query_row("SELECT path,fingerprint FROM actor_image_sources WHERE actor_id=?1", [&actor], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))).unwrap();
    fs::remove_file(&jpg).unwrap();
    let mut replacement = input(Some(&actor),Some(&png));
    replacement.name = "失败保存不得改名".into();
    replacement.aliases = vec!["失败保存不得改别名".into()];
    replacement.film_ids = vec!["synthetic-missing-film".into()];
    assert!(actor_management::save(&storage,&replacement).is_err());
    assert_eq!(serde_json::to_value(actors::list_actors(&storage).unwrap()).unwrap(),before);
    let source_after = storage.with_connection(|db| db.query_row("SELECT path,fingerprint FROM actor_image_sources WHERE actor_id=?1", [&actor], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))).unwrap();
    assert_eq!(source_after,source_before);
    assert!(actors::thumbnail(&storage,&actor,false).unwrap() == small);
    assert!(actors::thumbnail(&storage,&actor,true).unwrap() == large);
    assert!(png.is_file());
}

#[test]
fn unreadable_returning_jpg_does_not_override_last_explicit_png() {
    let home = tempfile::tempdir().unwrap();
    let jpg = home.path().join("original/actor.jpg"); let png = home.path().join("original/actor.png");
    picture(&jpg,[220,20,20]); picture(&png,[20,20,220]);
    let storage = Storage::open(&home.path().join("software")).unwrap();
    let actor = actor_management::save(&storage,&input(None,Some(&jpg))).unwrap();
    actor_management::save(&storage,&input(Some(&actor),Some(&png))).unwrap();
    let blue = actors::thumbnail(&storage,&actor,false).unwrap();
    fs::remove_file(&png).unwrap(); fs::write(&jpg,b"invalid returning JPG").unwrap();
    assert!(actor_management::save(&storage,&input(Some(&actor),Some(&jpg))).is_err());
    assert_eq!(actors::thumbnail(&storage,&actor,false).unwrap(),blue);
}
