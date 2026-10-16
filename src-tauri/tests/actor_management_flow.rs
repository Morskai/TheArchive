use local_film_library_preview_lib::{
    actor_management::{self as managed, SaveActorInput, ScanResolution},
    actors,
    storage::Storage,
};
use serde_json::json;
use std::{fs, path::Path};

fn input(name: &str, photo: Option<&Path>) -> SaveActorInput {
    SaveActorInput {
        actor_id: None,
        name: name.into(),
        aliases: vec!["别名".into()],
        profile: actors::Profile {
            height: "165".into(),
            ..Default::default()
        },
        photo_path: photo.map(|p| p.to_string_lossy().into_owned()),
        remove_photo: false,
        film_ids: vec![],
    }
}
fn png(path: &Path, value: u8) {
    image::RgbImage::from_pixel(8, 10, image::Rgb([value, 0, 0]))
        .save(path)
        .unwrap();
}
fn resolution(key: &str, id: Option<&str>, create: bool) -> ScanResolution {
    ScanResolution {
        key: key.into(),
        actor_id: id.map(str::to_owned),
        create_new: create,
    }
}
fn actor_info(dir: &Path, document: serde_json::Value) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("actor.info"), document.to_string()).unwrap();
}

fn photo_source(storage: &Storage, actor: &str) -> (String, String) {
    storage.with_connection(|db| db.query_row("SELECT path,fingerprint FROM actor_image_sources WHERE actor_id=?1",[actor],|r|Ok((r.get(0)?,r.get(1)?)))).unwrap()
}

fn photo_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}",Sha256::digest(bytes))
}
#[test]
fn exact_identity_defaults_to_update_before_partial_overlap_and_preserves_manual_data() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let mut seed = input("Actor", None);
    seed.aliases = vec!["Beta".into(), "Alpha".into()];
    seed.profile.cup = "D".into();
    let id = managed::save(&s, &seed).unwrap();
    let mut partial = input("Other", None);
    partial.aliases = vec!["Alpha".into()];
    let other = managed::save(&s, &partial).unwrap();
    actors::set_favorite(&s, &id, true).unwrap();
    s.with_connection(|db| {
        db.execute("INSERT INTO films(id) VALUES('film')", [])?;
        db.execute("INSERT INTO film_manual(film_id,cast_override) VALUES('film',1)", [])?;
        db.execute("INSERT INTO film_actors(film_id,actor_id,source) VALUES('film',?1,'manual')", [&id])?;
        Ok(())
    }).unwrap();
    let root = h.path().join("folder");
    actor_info(&root, json!({"format_version":1,"kind":"actor","name":" ACTOR ","aliases":["alpha"," BETA "],"height_cm":175}));
    let preview = managed::preview_import(&s, &root).unwrap();
    let item = &preview.items[0];
    assert_eq!(item.status, "update");
    assert_eq!(serde_json::to_value(item).unwrap()["defaultActorId"], id);
    assert!(!item.can_create);
    let before = s.list_protections().unwrap().len();
    let result = managed::confirm_scan_add(&s, &root, &preview.token, &[]).unwrap();
    assert_eq!((result.updated, result.imported), (1, 0));
    assert_eq!(s.list_protections().unwrap().len(), before + 1);
    let actors = actors::list_actors(&s).unwrap();
    let updated = actors.iter().find(|a| a.id == id).unwrap();
    assert_eq!(updated.height, "175");
    assert_eq!(updated.cup, "D");
    assert!(updated.favorite);
    assert_eq!(actors.iter().find(|a| a.id == other).unwrap().name, "Other");
    s.with_connection(|db| {
        assert_eq!(db.query_row::<String, _, _>("SELECT actor_id FROM film_actors WHERE film_id='film'", [], |r| r.get(0))?, id);
        assert_eq!(db.query_row::<i64, _, _>("SELECT cast_override FROM film_manual WHERE film_id='film'", [], |r| r.get(0))?, 1);
        Ok(())
    }).unwrap();
}

#[test]
fn repeated_default_update_target_requires_explicit_choose_one_before_protection() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let id = managed::save(&s, &input("Actor", None)).unwrap();
    let root = h.path().join("batch");
    for folder in ["one", "two"] {
        actor_info(&root.join(folder), json!({"format_version":1,"kind":"actor","name":"Actor","aliases":["别名"],"height_cm":175}));
    }
    let p = managed::preview_import(&s, &root).unwrap();
    assert!(p.items.iter().all(|i| i.status == "update"));
    let before = s.list_protections().unwrap().len();
    assert_eq!(managed::confirm_scan_add(&s, &root, &p.token, &[]).unwrap_err(), "actor_batch_conflict");
    assert_eq!(s.list_protections().unwrap().len(), before);
    let result = managed::confirm_scan_add(&s, &root, &p.token, &[resolution(&p.items[1].key, None, false)]).unwrap();
    assert_eq!((result.updated, result.skipped), (1, 1));
    assert_eq!(actors::list_actors(&s).unwrap().iter().find(|a| a.id == id).unwrap().height, "175");
}

#[test]
fn ambiguous_legacy_exact_identities_require_selection_and_never_default_to_arbitrary_actor() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    s.with_connection(|db| {
        db.execute("INSERT INTO actors(id,observed_name) VALUES('first','Actor'),('second','actor')", [])?;
        Ok(())
    }).unwrap();
    let root = h.path().join("folder");
    actor_info(&root, json!({"format_version":1,"kind":"actor","name":"ACTOR","aliases":[],"height_cm":175}));
    let p = managed::preview_import(&s, &root).unwrap();
    assert_eq!(p.items[0].status, "conflict");
    assert_eq!(serde_json::to_value(&p.items[0]).unwrap()["defaultActorId"], serde_json::Value::Null);
    assert!(!p.items[0].can_create);
    let before = s.list_protections().unwrap().len();
    assert_eq!(managed::confirm_scan_add(&s, &root, &p.token, &[]).unwrap_err(), "actor_import_selection_invalid");
    assert_eq!(s.list_protections().unwrap().len(), before);
}

#[test]
fn english_export_root_leaves_legacy_chinese_root_untouched() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let id = managed::save(&s, &input("演员甲", None)).unwrap();
    for parent in [s.data_directory().to_path_buf(), h.path().join("selected-parent")] {
        fs::create_dir_all(&parent).unwrap();
        let legacy = parent.join("演员资料导出");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("keep.txt"), b"legacy-original").unwrap();
        let p = managed::preview_export_to(&s, &[id.clone()], &parent).unwrap();
        assert_eq!(Path::new(&p.directory), parent.join("ActorExports"));
        assert_eq!(Path::new(&p.items[0].directory).file_name().unwrap(), "演员甲（别名）");
        managed::confirm_export_to(&s, &[id.clone()], &parent, &p.token, false).unwrap();
        assert!(Path::new(&p.items[0].directory).join("actor.info").is_file());
        assert_eq!(fs::read(legacy.join("keep.txt")).unwrap(), b"legacy-original");
        assert_eq!(fs::read_dir(&legacy).unwrap().count(), 1);
    }
    assert_eq!(managed::export_directory(&s), s.data_directory().join("ActorExports"));
}
#[test]
fn independent_saved_actors_are_visible_without_media_and_name_is_not_reparsed() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO actors(id,observed_name) VALUES('saved','甲（艺名）')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let saved = actors::list_actors(&storage).unwrap();
    assert_eq!(
        saved.len(),
        1,
        "independent actor must remain visible without a film source"
    );
    assert_eq!(saved[0].name, "甲（艺名）");
}

#[test]
fn saved_photo_path_restores_without_copying_content_and_failed_display_requires_explicit_retry() {
    let h = tempfile::tempdir().unwrap();
    let source = h.path().join("original.png");
    png(&source, 9);
    let original = fs::read(&source).unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let id = managed::save(&s, &input("甲", Some(&source))).unwrap();
    let photo = actors::thumbnail(&s, &id, true).unwrap();
    let small = actors::thumbnail(&s, &id, false).unwrap();
    let saved_path = photo_source(&s,&id);
    assert_eq!(saved_path,(source.to_string_lossy().into_owned(),photo_hash(&original)));
    assert_eq!(
        image::guess_format(&photo).unwrap(),
        image::ImageFormat::Jpeg
    );
    let backup = s.create_backup().unwrap();
    fs::remove_file(&source).unwrap();
    assert!(actors::thumbnail(&s,&id,false).unwrap()==small,"successful disk cache remains usable offline");
    local_film_library_preview_lib::lifecycle::clear_generated_cache(&s).unwrap();
    let display = local_film_library_preview_lib::image_session::ImageSession::default();
    let key = format!("actor:{id}");
    assert!(display.read(&key,||actors::thumbnail(&s,&id,false)).is_err());
    let mut edit = input("改名", None);
    edit.actor_id = Some(id.clone());
    edit.remove_photo = true;
    managed::save(&s, &edit).unwrap();
    assert!(actors::thumbnail(&s, &id, false).is_err());
    s.restore_backup(&backup).unwrap();
    let restored = actors::list_actors(&s).unwrap().remove(0);
    assert_eq!(restored.id, id);
    assert_eq!(restored.name, "甲");
    assert_eq!(restored.height, "165");
    assert_eq!(photo_source(&s,&id),saved_path);
    assert!(actors::thumbnail(&s,&id,false).is_err(),"backups restore paths, not missing original content");
    fs::write(&source,&original).unwrap();
    assert!(display.read(&key,||actors::thumbnail(&s,&id,false)).is_err(),"restoring the path must not automatically retry a failed display");
    display.release(&[key.clone()]);
    assert!(display.read(&key,||actors::thumbnail(&s,&id,false)).unwrap()==small);
    assert_eq!(fs::read(&source).unwrap(),original);
}

#[test]
fn export_is_owned_safe_renames_cleanup_requires_confirmation_and_roundtrips_personal_data() {
    let h = tempfile::tempdir().unwrap();
    let source = h.path().join("source.png");
    png(&source, 5);
    let s = Storage::open(&h.path().join("data")).unwrap();
    let id = managed::save(&s, &input("甲:/CON", Some(&source))).unwrap();
    let p = managed::preview_export(&s, &[id.clone()]).unwrap();
    let directory = p.items[0].directory.clone();
    assert!(Path::new(&directory).starts_with(managed::export_directory(&s)));
    managed::confirm_export(&s, &[id.clone()], &p.token, false).unwrap();
    assert!(Path::new(&directory).join("actor.jpg").is_file());
    let doc: serde_json::Value =
        serde_json::from_slice(&fs::read(Path::new(&directory).join("actor.info")).unwrap())
            .unwrap();
    assert_eq!(doc["id"], id);
    let imported = Storage::open(&h.path().join("other-data")).unwrap();
    let preview = managed::preview_import(&imported, &managed::export_directory(&s)).unwrap();
    assert_eq!(preview.items.len(), 1);
    managed::confirm_scan_add(
        &imported,
        &managed::export_directory(&s),
        &preview.token,
        &[],
    )
    .unwrap();
    let roundtrip = actors::list_actors(&imported).unwrap().remove(0);
    assert_ne!(roundtrip.id, id);
    assert_eq!(roundtrip.name, "甲:/CON");
    assert_eq!(roundtrip.height, "165");
    let mut edit = input("新名", None);
    edit.actor_id = Some(id.clone());
    managed::save(&s, &edit).unwrap();
    let p = managed::preview_export(&s, &[id.clone()]).unwrap();
    assert_eq!(p.items[0].previous_directories, vec![directory.clone()]);
    assert_eq!(
        managed::confirm_export(&s, &[id.clone()], &p.token, false).unwrap_err(),
        "actor_export_exists"
    );
    managed::confirm_export(&s, &[id.clone()], &p.token, true).unwrap();
    assert!(!Path::new(&directory).exists());
    let p = managed::preview_export(&s, &[id.clone()]).unwrap();
    fs::write(
        Path::new(&p.items[0].directory).join("unrelated.txt"),
        b"keep",
    )
    .unwrap();
    assert!(managed::confirm_export(&s, &[id.clone()], &p.token, true).is_err());
    assert!(
        Path::new(&p.items[0].directory)
            .join("unrelated.txt")
            .exists()
    );
}

#[test]
fn export_same_names_are_disambiguated_no_photo_has_info_only_and_database_race_rejected() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let a = managed::save(&s, &input("同名", None)).unwrap();
    let mut second = input("同名", None);
    second.aliases = vec!["另一别名".into()];
    let b = managed::save(&s, &second).unwrap();
    let p = managed::preview_export(&s, &[a.clone(), b]).unwrap();
    assert_ne!(p.items[0].directory, p.items[1].directory);
    assert!(p.items.iter().all(|i| !i.has_photo));
    actors::set_favorite(&s, &a, true).unwrap();
    assert_eq!(
        managed::confirm_export(&s, &[], &p.token, false).unwrap_err(),
        "actor_export_changed"
    );
    let p = managed::preview_export(&s, &[]).unwrap();
    managed::confirm_export(&s, &[], &p.token, false).unwrap();
    let p = managed::preview_import(&s, &managed::export_directory(&s)).unwrap();
    assert_eq!(p.items.len(), 2);
    assert!(
        p.items
            .iter()
            .all(|i| i.status == "update" && !i.can_create)
    );
}

#[test]
fn realistic_photo_saves_only_its_path_and_derives_frontend_and_explicit_export_sizes() {
    let h = tempfile::tempdir().unwrap();
    let original = h.path().join("original.png");
    image::RgbImage::new(1800, 2400).save(&original).unwrap();
    let original_bytes = fs::read(&original).unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let id = managed::save(&s, &input("甲", Some(&original))).unwrap();
    for (large, maximum) in [(false, 480), (true, 1200)] {
        let image = image::load_from_memory(&actors::thumbnail(&s, &id, large).unwrap()).unwrap();
        assert_eq!(image.height(), maximum);
        assert!(image.width() <= maximum);
    }
    assert_eq!(photo_source(&s,&id),(original.to_string_lossy().into_owned(),photo_hash(&original_bytes)));
    s.with_connection(|db| {
        assert!(!db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='actor_photos')",[],|r|r.get::<_,bool>(0))?);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM pragma_table_info('actor_image_sources') WHERE type='BLOB'",[],|r|r.get::<_,i64>(0))?,0);
        Ok(())
    }).unwrap();
    let export = managed::preview_export(&s,&[id.clone()]).unwrap();
    managed::confirm_export(&s,&[id.clone()],&export.token,false).unwrap();
    let output = fs::read(Path::new(&export.items[0].directory).join("actor.jpg")).unwrap();
    assert_eq!(image::guess_format(&output).unwrap(),image::ImageFormat::Jpeg);
    assert_eq!(image::load_from_memory(&output).unwrap().height(),1200);
    assert_eq!(fs::read(&original).unwrap(),original_bytes);
}

#[test]
fn complete_cast_edit_keeps_hidden_canonical_links_and_failure_rolls_back_photo() {
    let h = tempfile::tempdir().unwrap();
    let photo = h.path().join("photo.png");
    png(&photo, 1);
    let s = Storage::open(&h.path().join("data")).unwrap();
    s.with_connection(|db| {
        db.execute("INSERT INTO films(id) VALUES('hidden')", [])?;
        Ok(())
    })
    .unwrap();
    let mut draft = input("甲", Some(&photo));
    draft.film_ids = vec!["hidden".into()];
    let id = managed::save(&s, &draft).unwrap();
    let saved = actors::list_actors(&s).unwrap().remove(0);
    assert_eq!(saved.film_ids, vec!["hidden"]);
    let old = actors::thumbnail(&s, &id, false).unwrap();
    png(&photo, 4);
    draft.actor_id = Some(id.clone());
    draft.name = "改名".into();
    draft.film_ids = saved.film_ids;
    managed::save(&s, &draft).unwrap();
    assert_eq!(actors::list_actors(&s).unwrap()[0].film_ids, vec!["hidden"]);
    let changed = actors::thumbnail(&s, &id, false).unwrap();
    assert_ne!(old, changed);
    let changed_source = photo_source(&s,&id);
    draft.film_ids = vec!["missing".into()];
    draft.name = "不能提交".into();
    png(&photo, 8);
    assert!(managed::save(&s, &draft).is_err());
    assert_eq!(actors::list_actors(&s).unwrap()[0].name, "改名");
    assert_eq!(photo_source(&s,&id),changed_source);
    assert!(actors::thumbnail(&s, &id, false).unwrap()==changed,"a failed save must preserve the successful cached photo");
}

#[test]
fn all_supported_actor_formats_are_case_insensitive_and_one_directory_is_one_actor() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("actors");
    fs::create_dir(&root).unwrap();
    for (folder, filename, format) in [
        ("jpeg", "ACTOR.JPEG", image::ImageFormat::Jpeg),
        ("jpg", "actor.jpg", image::ImageFormat::Jpeg),
        ("png", "Actor.PnG", image::ImageFormat::Png),
        ("webp", "actor.WEBP", image::ImageFormat::WebP),
    ] {
        let dir = root.join(folder);
        fs::create_dir(&dir).unwrap();
        image::RgbImage::from_pixel(20, 30, image::Rgb([30, 60, 90]))
            .save_with_format(dir.join(filename), format)
            .unwrap();
    }
    let avif = root.join("avif");
    fs::create_dir(&avif).unwrap();
    fs::write(
        avif.join("ACTOR.AVIF"),
        include_bytes!("fixtures/tiny.avif"),
    )
    .unwrap();
    // Several formats in one folder remain one actor; valid JPG takes precedence.
    png(&root.join("jpg/actor.png"), 250);
    for name in ["jpeg", "jpg", "png", "webp", "avif"] {
        actor_info(
            &root.join(name),
            json!({"format_version":1,"kind":"actor","name":name}),
        );
    }
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let preview = managed::preview_import(&storage, &root).unwrap();
    assert_eq!(preview.items.len(), 5);
    assert!(preview.items.iter().all(|i| i.status == "new"));
    let choices = preview
        .items
        .iter()
        .map(|i| resolution(&i.key, None, true))
        .collect::<Vec<_>>();
    assert_eq!(
        managed::confirm_scan_add(&storage, &root, &preview.token, &choices)
            .unwrap()
            .imported,
        5
    );
    for actor in actors::list_actors(&storage).unwrap() {
        assert!(
            image::load_from_memory_with_format(
                &actors::thumbnail(&storage, &actor.id, false).unwrap(),
                image::ImageFormat::Jpeg
            )
            .is_ok()
        );
    }
}

#[test]
fn readonly_old_export_is_rejected_before_new_name_publication() {
    let h = tempfile::tempdir().unwrap();
    let source = h.path().join("photo.png");
    png(&source, 1);
    let s = Storage::open(&h.path().join("data")).unwrap();
    let id = managed::save(&s, &input("旧名", Some(&source))).unwrap();
    let p = managed::preview_export(&s, &[id.clone()]).unwrap();
    managed::confirm_export(&s, &[id.clone()], &p.token, false).unwrap();
    let old = Path::new(&p.items[0].directory);
    let photo = old.join("actor.jpg");
    let mut permissions = fs::metadata(&photo).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&photo, permissions).unwrap();
    let mut edit = input("新名", None);
    edit.actor_id = Some(id.clone());
    managed::save(&s, &edit).unwrap();
    let p = managed::preview_export(&s, &[id.clone()]).unwrap();
    assert!(managed::confirm_export(&s, &[id], &p.token, true).is_err());
    assert!(
        !Path::new(&p.items[0].directory).exists(),
        "readonly cleanup must stop before publishing a new directory"
    );
    assert!(
        old.join(".thearchive-actor.json").exists(),
        "ownership manifest must remain"
    );
    let mut permissions = fs::metadata(&photo).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&photo, permissions).unwrap();
}

#[test]
fn actor_record_dates_are_persisted_and_edit_changes_only_its_modification_time() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let a = managed::save(&s, &input("甲", None)).unwrap();
    let b = managed::save(&s, &input("乙", None)).unwrap();
    let actors = actors::list_actors(&s).unwrap();
    assert!(
        actors
            .iter()
            .all(|a| a.directory_created_at.is_some() && a.directory_modified_at.is_some())
    );
    let b_before = actors
        .iter()
        .find(|x| x.id == b)
        .unwrap()
        .directory_modified_at
        .clone();
    let creation = actors
        .iter()
        .find(|x| x.id == a)
        .unwrap()
        .directory_created_at
        .clone();
    s.with_connection(|db| {
        db.execute(
            "UPDATE actors SET modified_at='2000-01-01 00:00:00' WHERE id=?1",
            [&a],
        )?;
        Ok(())
    })
    .unwrap();
    actors::set_favorite(&s, &a, true).unwrap();
    let backup = s.create_backup().unwrap();
    let after = actors::list_actors(&s).unwrap();
    let a_after = after.iter().find(|x| x.id == a).unwrap();
    assert_eq!(a_after.directory_created_at, creation);
    assert_ne!(
        a_after.directory_modified_at.as_deref(),
        Some("2000-01-01T00:00:00.000Z")
    );
    assert_eq!(
        after
            .iter()
            .find(|x| x.id == b)
            .unwrap()
            .directory_modified_at,
        b_before
    );
    let modified = a_after.directory_modified_at.clone();
    s.restore_backup(&backup).unwrap();
    let reopened = Storage::open(s.data_directory()).unwrap();
    let after = actors::list_actors(&reopened).unwrap();
    assert_eq!(
        after
            .iter()
            .find(|x| x.id == a)
            .unwrap()
            .directory_modified_at,
        modified
    );
}

#[test]
#[cfg(windows)]
fn junction_roots_and_export_targets_are_rejected_without_touching_original() {
    use std::os::windows::process::CommandExt;
    fn junction(link: &Path, target: &Path) {
        let result = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(result.status.success(), "junction setup failed");
    }
    let h = tempfile::tempdir().unwrap();
    let original = h.path().join("original");
    fs::create_dir(&original).unwrap();
    png(&original.join("actor.png"), 1);
    let before = fs::read(original.join("actor.png")).unwrap();
    let link = h.path().join("linked-import");
    junction(&link, &original);
    let s = Storage::open(&h.path().join("data")).unwrap();
    assert_eq!(
        managed::preview_import(&s, &link).unwrap_err(),
        "actor_unsafe_path"
    );
    let id = managed::save(&s, &input("甲", None)).unwrap();
    junction(&managed::export_directory(&s), &original);
    assert!(managed::preview_export(&s, &[id]).is_err());
    assert_eq!(fs::read(original.join("actor.png")).unwrap(), before);
}

#[test]
fn selected_export_parent_is_bound_and_only_its_export_child_is_written() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let first = h.path().join("selected");
    let second = h.path().join("other");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    fs::write(first.join("original.mp4"), b"original-media").unwrap();
    let id = managed::save(&s, &input("旧名", None)).unwrap();
    let p = managed::preview_export_to(&s, &[id.clone()], &first).unwrap();
    assert_eq!(Path::new(&p.directory), first.join("ActorExports"));
    assert!(!first.join("ActorExports").exists(), "preview is read-only");
    assert_eq!(
        managed::confirm_export_to(&s, &[id.clone()], &second, &p.token, false).unwrap_err(),
        "actor_export_changed"
    );
    assert!(fs::read_dir(&second).unwrap().next().is_none());
    let result = managed::confirm_export_to(&s, &[id.clone()], &first, &p.token, false).unwrap();
    assert_eq!(result.exported, 1);
    assert_eq!(result.directory, p.directory);
    assert!(!managed::export_directory(&s).exists());
    assert_eq!(
        fs::read(first.join("original.mp4")).unwrap(),
        b"original-media"
    );
    assert!(!first.join("actor.info").exists());
    let mut edit = input("新名", None);
    edit.actor_id = Some(id.clone());
    managed::save(&s, &edit).unwrap();
    let rename = managed::preview_export_to(&s, &[id.clone()], &first).unwrap();
    assert_eq!(
        rename.items[0].previous_directories,
        vec![p.items[0].directory.clone()]
    );
    managed::confirm_export_to(&s, &[id], &first, &rename.token, true).unwrap();
    assert!(!Path::new(&p.items[0].directory).exists());
    assert!(
        Path::new(&rename.items[0].directory)
            .join("actor.info")
            .is_file()
    );
}

#[test]
fn selected_export_parent_must_be_existing_directory_and_owned_content_stays_protected() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let id = managed::save(&s, &input("甲", None)).unwrap();
    let file = h.path().join("file");
    fs::write(&file, b"keep").unwrap();
    for path in [h.path().join("missing"), file, "relative".into()] {
        assert!(managed::preview_export_to(&s, &[id.clone()], &path).is_err());
    }
    let parent = h.path().join("parent");
    fs::create_dir(&parent).unwrap();
    let p = managed::preview_export_to(&s, &[id.clone()], &parent).unwrap();
    managed::confirm_export_to(&s, &[id.clone()], &parent, &p.token, false).unwrap();
    let p = managed::preview_export_to(&s, &[id.clone()], &parent).unwrap();
    let unrelated = Path::new(&p.items[0].directory).join("keep.txt");
    fs::write(&unrelated, b"do-not-touch").unwrap();
    assert!(managed::confirm_export_to(&s, &[id], &parent, &p.token, true).is_err());
    assert_eq!(fs::read(unrelated).unwrap(), b"do-not-touch");
}

#[test]
#[cfg(windows)]
fn selected_export_parent_rejects_junction_ancestry() {
    use std::os::windows::process::CommandExt;
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let id = managed::save(&s, &input("甲", None)).unwrap();
    let original = h.path().join("original");
    fs::create_dir_all(original.join("nested")).unwrap();
    let link = h.path().join("link");
    let result = std::process::Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&original)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(managed::preview_export_to(&s, &[id], &link.join("nested")).is_err());
    assert!(
        fs::read_dir(original.join("nested"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn scan_add_has_one_protection_for_whole_batch_and_fails_before_any_write_without_it() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let root = h.path().join("actors");
    for folder in ["a", "b"] {
        fs::create_dir_all(root.join(folder)).unwrap();
        png(&root.join(folder).join("actor.png"), 5);
        actor_info(
            &root.join(folder),
            json!({"format_version":1,"kind":"actor","name":folder}),
        );
    }
    let p = managed::preview_import(&s, &root).unwrap();
    fs::write(s.data_directory().join("protections"), b"blocked").unwrap();
    assert_eq!(
        managed::confirm_scan_add(&s, &root, &p.token, &[]).unwrap_err(),
        "protection_failed"
    );
    assert!(actors::list_actors(&s).unwrap().is_empty());
    fs::remove_file(s.data_directory().join("protections")).unwrap();
    let before = s.list_protections().unwrap().len();
    assert_eq!(
        managed::confirm_scan_add(&s, &root, &p.token, &[])
            .unwrap()
            .imported,
        2
    );
    assert_eq!(s.list_protections().unwrap().len(), before + 1);
}
#[test]
fn actor_info_is_the_only_marker_and_info_only_records_ignore_external_identity() {
    let h = tempfile::tempdir().unwrap();
    let root = h.path().join("folder name must be ignored");
    fs::create_dir_all(root.join("photo-only")).unwrap();
    png(&root.join("photo-only/actor.png"), 7);
    fs::write(
        root.join("photo-only/actor.info.json"),
        json!({"format_version":1,"kind":"actor","name":"Legacy ignored"}).to_string(),
    )
    .unwrap();
    fs::write(root.join("actor.info"), json!({"format_version":1,"kind":"actor","id":{"archival":"external-only"},"photo":{"archival":"untrusted"},"name":"  Actual Name  ","aliases":[" A ","a"],"height_cm":170,"film_ids":["unknown"]}).to_string()).unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let preview = managed::preview_import(&s, &root).unwrap();
    assert_eq!(preview.items.len(), 1);
    assert_eq!(preview.items[0].name, "Actual Name");
    assert_eq!(preview.items[0].aliases, ["A"]);
    let result = managed::confirm_scan_add(&s, &root, &preview.token, &[]).unwrap();
    assert_eq!(result.imported, 1);
    let actor = actors::list_actors(&s).unwrap().remove(0);
    assert_ne!(actor.id, "external-only");
    assert_eq!(actor.height, "170");
    assert!(!actor.has_portrait);
}
#[test]
fn scan_updates_full_personal_fields_missing_preserves_null_clears_and_keeps_identity_links() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let photo = h.path().join("photo.png");
    png(&photo, 17);
    let mut seed = input("Original", Some(&photo));
    seed.aliases = vec!["Known".into()];
    seed.profile.cup = "D".into();
    let id = managed::save(&s, &seed).unwrap();
    actors::set_favorite(&s, &id, true).unwrap();
    s.with_connection(|db| {
        db.execute("INSERT INTO films(id) VALUES('film')", [])?;
        db.execute(
            "INSERT INTO film_manual(film_id,cast_override) VALUES('film',1)",
            [],
        )?;
        db.execute(
            "INSERT INTO film_actors(film_id,actor_id,source) VALUES('film',?1,'manual')",
            [&id],
        )?;
        db.execute(
            "INSERT INTO info_actor_links(external_id,actor_id) VALUES('archive-id',?1)",
            [&id],
        )?;
        Ok(())
    })
    .unwrap();
    let original_photo = actors::thumbnail(&s, &id, false).unwrap();
    let root = h.path().join("unrelated-folder");
    actor_info(
        &root,
        json!({"format_version":1,"kind":"actor","id":"archive-id","name":"Known","height_cm":175,"cup":null,"film_ids":{"ignored":"anything"},"photo":"ignored.jpg"}),
    );
    let p = managed::preview_import(&s, &root).unwrap();
    assert_eq!(p.items[0].candidate_ids, [id.clone()]);
    assert!(managed::confirm_scan_add(&s, &root, &p.token, &[]).is_err());
    let result = managed::confirm_scan_add(
        &s,
        &root,
        &p.token,
        &[resolution(&p.items[0].key, Some(&id), false)],
    )
    .unwrap();
    assert_eq!(result.updated, 1);
    let a = actors::list_actors(&s).unwrap().remove(0);
    assert_eq!(a.id, id);
    assert_eq!(a.name, "Known");
    assert_eq!(a.aliases.as_deref(), Some("Known"));
    assert_eq!(a.height, "175");
    assert_eq!(a.cup, "");
    assert!(a.favorite);
    assert_eq!(actors::thumbnail(&s, &id, false).unwrap(), original_photo);
    s.with_connection(|db| {
        assert_eq!(
            db.query_row::<String, _, _>(
                "SELECT actor_id FROM film_actors WHERE film_id='film'",
                [],
                |r| r.get(0)
            )?,
            id
        );
        assert_eq!(
            db.query_row::<i64, _, _>(
                "SELECT cast_override FROM film_manual WHERE film_id='film'",
                [],
                |r| r.get(0)
            )?,
            1
        );
        assert_eq!(
            db.query_row::<i64, _, _>("SELECT count(*) FROM info_actor_links", [], |r| r.get(0))?,
            1
        );
        Ok(())
    })
    .unwrap();
    actor_info(
        &root,
        json!({"format_version":1,"kind":"actor","name":"Known","aliases":null}),
    );
    let p = managed::preview_import(&s, &root).unwrap();
    managed::confirm_scan_add(
        &s,
        &root,
        &p.token,
        &[resolution(&p.items[0].key, Some(&id), false)],
    )
    .unwrap();
    let a = actors::list_actors(&s).unwrap().remove(0);
    assert!(a.aliases.as_deref().is_none_or(str::is_empty));
    assert_eq!(a.height, "175");
}

#[test]
fn complete_duplicate_identity_uses_case_and_unordered_aliases_and_guards_all_manual_edits() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let mut first = input(" Actor ", None);
    first.aliases = vec![" Beta ".into(), "Alpha".into()];
    let id = managed::save(&s, &first).unwrap();
    let mut duplicate = input("actor", None);
    duplicate.aliases = vec!["ALPHA".into(), "beta".into(), " beta ".into()];
    let before = s.list_protections().unwrap().len();
    assert_eq!(
        managed::save(&s, &duplicate).unwrap_err(),
        "actor_duplicate_identity"
    );
    assert_eq!(s.list_protections().unwrap().len(), before);
    let mut other = duplicate;
    other.aliases = vec!["different".into()];
    let other_id = managed::save(&s, &other).unwrap();
    assert_eq!(
        actors::save_details(
            &s,
            &other_id,
            &actors::Profile::default(),
            Some("BETA、 alpha ")
        )
        .unwrap_err(),
        "actor_duplicate_identity"
    );
    let root = h.path().join("any-folder");
    actor_info(
        &root,
        json!({"format_version":1,"kind":"actor","name":"ACTOR","aliases":["alpha","BETA"]}),
    );
    let p = managed::preview_import(&s, &root).unwrap();
    assert_eq!(p.items[0].status, "update");
    assert!(!p.items[0].can_create);
    assert!(p.items[0].candidate_ids.contains(&id));
    assert!(
        managed::confirm_scan_add(
            &s,
            &root,
            &p.token,
            &[resolution(&p.items[0].key, None, true)]
        )
        .is_err()
    );
    other.actor_id = Some(other_id.clone());
    other.aliases = vec!["Alpha".into(), "Beta".into()];
    assert_eq!(
        managed::save(&s, &other).unwrap_err(),
        "actor_duplicate_identity"
    );
    let own = SaveActorInput {
        actor_id: Some(id),
        ..first
    };
    managed::save(&s, &own).unwrap();
}

#[test]
fn candidates_compare_complete_values_across_name_alias_fields_without_external_or_history_lookup()
{
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let mut a = input("Alice", None);
    a.aliases = vec![" BETA ".into()];
    let id = managed::save(&s, &a).unwrap();
    let mut b = input("Other", None);
    b.aliases = vec![];
    let unrelated = managed::save(&s, &b).unwrap();
    s.with_connection(|db| {
        db.execute(
            "INSERT INTO info_actor_links(external_id,actor_id) VALUES('archive',?1)",
            [&unrelated],
        )?;
        Ok(())
    })
    .unwrap();
    let root = h.path().join("Alice");
    actor_info(
        &root,
        json!({"format_version":1,"kind":"actor","id":"archive","name":" beta "}),
    );
    let p = managed::preview_import(&s, &root).unwrap();
    assert_eq!(p.items[0].candidate_ids, [id.clone()]);
    assert_eq!(p.items[0].status, "conflict");
    assert!(p.items[0].default_actor_id.is_none());
    assert!(p.items[0].can_create);
    let before = s.list_protections().unwrap().len();
    assert_eq!(managed::confirm_scan_add(&s, &root, &p.token, &[]).unwrap_err(), "actor_import_selection_invalid");
    assert_eq!(s.list_protections().unwrap().len(), before);
    actor_info(
        &root,
        json!({"format_version":1,"kind":"actor","id":id,"name":"Ali"}),
    );
    let p = managed::preview_import(&s, &root).unwrap();
    assert!(p.items[0].candidate_ids.is_empty());
    managed::confirm_scan_add(&s, &root, &p.token, &[]).unwrap();
    assert_eq!(actors::list_actors(&s).unwrap().len(), 3);
}

#[test]
fn scan_batch_competing_targets_and_duplicate_new_signatures_reject_before_protection() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let mut saved = input("Saved", None);
    saved.aliases = vec!["a".into(), "b".into()];
    let id = managed::save(&s, &saved).unwrap();
    let root = h.path().join("batch");
    for name in ["a", "b"] {
        actor_info(
            &root.join(name),
            json!({"format_version":1,"kind":"actor","name":name,"height_cm":170}),
        );
    }
    let p = managed::preview_import(&s, &root).unwrap();
    let choices = p
        .items
        .iter()
        .map(|i| resolution(&i.key, Some(&id), false))
        .collect::<Vec<_>>();
    let before = s.list_protections().unwrap().len();
    assert_eq!(
        managed::confirm_scan_add(&s, &root, &p.token, &choices).unwrap_err(),
        "actor_batch_conflict"
    );
    assert_eq!(s.list_protections().unwrap().len(), before);
    assert_eq!(actors::list_actors(&s).unwrap()[0].name, "Saved");
    let choices = vec![
        resolution(&p.items[0].key, Some(&id), false),
        resolution(&p.items[1].key, None, false),
    ];
    assert_eq!(
        managed::confirm_scan_add(&s, &root, &p.token, &choices)
            .unwrap()
            .updated,
        1
    );
    let fresh = h.path().join("duplicates");
    actor_info(
        &fresh.join("one"),
        json!({"format_version":1,"kind":"actor","name":"New","aliases":["X","Y"]}),
    );
    actor_info(
        &fresh.join("two"),
        json!({"format_version":1,"kind":"actor","name":" new ","aliases":[" y ","x"]}),
    );
    let p = managed::preview_import(&s, &fresh).unwrap();
    assert_eq!(
        p.items[0].name_alias_signature,
        p.items[1].name_alias_signature
    );
    let before = s.list_protections().unwrap().len();
    assert_eq!(
        managed::confirm_scan_add(&s, &fresh, &p.token, &[]).unwrap_err(),
        "actor_batch_conflict"
    );
    assert_eq!(s.list_protections().unwrap().len(), before);
    assert_eq!(
        managed::confirm_scan_add(
            &s,
            &fresh,
            &p.token,
            &[resolution(&p.items[1].key, None, false)]
        )
        .unwrap()
        .imported,
        1
    );
}

#[test]
fn scan_update_recomputes_omitted_aliases_and_prevents_new_global_duplicate() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let mut first = input("Old", None);
    first.aliases = vec!["Shared".into(), "Target".into()];
    let id = managed::save(&s, &first).unwrap();
    let mut second = input("Target", None);
    second.aliases = vec!["shared".into()];
    managed::save(&s, &second).unwrap();
    let root = h.path().join("folder");
    actor_info(
        &root,
        json!({"format_version":1,"kind":"actor","name":"Target"}),
    );
    let p = managed::preview_import(&s, &root).unwrap();
    assert_eq!(p.items[0].candidate_ids.len(), 2);
    assert!(
        p.items[0].can_create,
        "creation has empty aliases but update preserves saved aliases"
    );
    let before = s.list_protections().unwrap().len();
    assert_eq!(
        managed::confirm_scan_add(
            &s,
            &root,
            &p.token,
            &[resolution(&p.items[0].key, Some(&id), false)]
        )
        .unwrap_err(),
        "actor_duplicate_identity"
    );
    assert_eq!(s.list_protections().unwrap().len(), before);
    assert_eq!(
        actors::list_actors(&s)
            .unwrap()
            .iter()
            .find(|a| a.id == id)
            .unwrap()
            .name,
        "Old"
    );
}

#[test]
fn invalid_info_or_all_bad_photos_fail_the_directory_and_valid_photo_fallback_is_read_only() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let root = h.path().join("batch");
    actor_info(
        &root.join("badphoto"),
        json!({"format_version":1,"kind":"actor","name":"Bad"}),
    );
    fs::write(root.join("badphoto/actor.jpg"), b"bad").unwrap();
    actor_info(
        &root.join("missingname"),
        json!({"format_version":1,"kind":"actor"}),
    );
    actor_info(
        &root.join("invalidprofile"),
        json!({"format_version":1,"kind":"actor","name":"Invalid","height_cm":999}),
    );
    actor_info(
        &root.join("fallback"),
        json!({"format_version":1,"kind":"actor","name":"Good"}),
    );
    fs::write(root.join("fallback/actor.jpg"), b"bad").unwrap();
    png(&root.join("fallback/actor.png"), 33);
    let original = fs::read(root.join("fallback/actor.png")).unwrap();
    let p = managed::preview_import(&s, &root).unwrap();
    assert_eq!(p.items.iter().filter(|i| i.status == "failed").count(), 3);
    let result = managed::confirm_scan_add(&s, &root, &p.token, &[]).unwrap();
    assert_eq!(result.imported, 1);
    assert_eq!(result.failed, 3);
    assert_eq!(fs::read(root.join("fallback/actor.png")).unwrap(), original);
    assert_eq!(fs::read(root.join("fallback/actor.jpg")).unwrap(), b"bad");
    fs::write(
        root.join("fallback/actor.info.json"),
        json!({"format_version":1,"kind":"actor","name":"Ignored"}).to_string(),
    )
    .unwrap();
    assert_eq!(managed::preview_import(&s, &root).unwrap().items.len(), 4);
}

#[test]
fn changed_database_or_any_source_file_rejects_batch_and_sql_failure_rolls_back_all() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let root = h.path().join("batch");
    for name in ["a", "b"] {
        actor_info(
            &root.join(name),
            json!({"format_version":1,"kind":"actor","name":name}),
        );
        png(&root.join(name).join("actor.png"), 3);
    }
    let p = managed::preview_import(&s, &root).unwrap();
    png(&root.join("b/actor.png"), 6);
    assert_eq!(
        managed::confirm_scan_add(&s, &root, &p.token, &[]).unwrap_err(),
        "actor_import_changed"
    );
    assert!(actors::list_actors(&s).unwrap().is_empty());
    let p = managed::preview_import(&s, &root).unwrap();
    managed::save(&s, &input("unrelated", None)).unwrap();
    assert_eq!(
        managed::confirm_scan_add(&s, &root, &p.token, &[]).unwrap_err(),
        "actor_import_changed"
    );
    let p = managed::preview_import(&s, &root).unwrap();
    s.with_connection(|db| {db.execute_batch("CREATE TRIGGER reject_second_actor BEFORE INSERT ON actors WHEN NEW.observed_name='b' BEGIN SELECT RAISE(ABORT,'synthetic fail'); END;")?;Ok(())}).unwrap();
    assert!(managed::confirm_scan_add(&s, &root, &p.token, &[]).is_err());
    let names = actors::list_actors(&s)
        .unwrap()
        .into_iter()
        .map(|a| a.name)
        .collect::<Vec<_>>();
    assert_eq!(names, ["unrelated"]);
}

#[test]
fn unchanged_scan_and_explicit_skip_do_not_create_protection_or_rewrite_same_photo() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let root = h.path().join("folder");
    actor_info(
        &root,
        json!({"format_version":1,"kind":"actor","name":"Same","aliases":[],"height_cm":170}),
    );
    png(&root.join("actor.png"), 9);
    let p = managed::preview_import(&s, &root).unwrap();
    managed::confirm_scan_add(&s, &root, &p.token, &[]).unwrap();
    let a = actors::list_actors(&s).unwrap().remove(0);
    s.with_connection(|db| {
        db.execute("INSERT INTO films(id) VALUES('unchanged-film')", [])?;
        db.execute("INSERT INTO film_manual(film_id,cast_override) VALUES('unchanged-film',1)", [])?;
        db.execute("INSERT INTO film_actors(film_id,actor_id,source) VALUES('unchanged-film',?1,'manual')", [&a.id])?;
        Ok(())
    }).unwrap();
    let before = s.list_protections().unwrap().len();
    let photo = actors::thumbnail(&s, &a.id, false).unwrap();
    let p = managed::preview_import(&s, &root).unwrap();
    assert_eq!(
        managed::confirm_scan_add(&s, &root, &p.token, &[])
        .unwrap()
        .skipped,
        1
    );
    assert_eq!(s.list_protections().unwrap().len(), before);
    assert_eq!(actors::thumbnail(&s, &a.id, false).unwrap(), photo);
    s.with_connection(|db| {
        assert_eq!(db.query_row::<String, _, _>("SELECT actor_id FROM film_actors WHERE film_id='unchanged-film'", [], |r| r.get(0))?, a.id);
        assert_eq!(db.query_row::<i64, _, _>("SELECT cast_override FROM film_manual WHERE film_id='unchanged-film'", [], |r| r.get(0))?, 1);
        Ok(())
    }).unwrap();
    assert_eq!(
        managed::confirm_scan_add(
            &s,
            &root,
            &p.token,
            &[resolution(&p.items[0].key, None, false)]
        )
        .unwrap()
        .skipped,
        1
    );
    assert_eq!(s.list_protections().unwrap().len(), before);
    assert!(
        managed::confirm_scan_add(
            &s,
            &root,
            &p.token,
            &[resolution(&p.items[0].key, Some("invalid"), false)]
        )
        .is_err()
    );
}
#[test]
fn owned_legacy_export_is_replaced_safely_but_is_never_scan_input() {
    use sha2::{Digest, Sha256};
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let id = managed::save(&s, &input("Legacy", None)).unwrap();
    let p = managed::preview_export(&s, &[id.clone()]).unwrap();
    managed::confirm_export(&s, &[id.clone()], &p.token, false).unwrap();
    let dir = Path::new(&p.items[0].directory);
    fs::rename(dir.join("actor.info"), dir.join("actor.info.json")).unwrap();
    let bytes = fs::read(dir.join("actor.info.json")).unwrap();
    let manifest = dir.join(".thearchive-actor.json");
    fs::write(&manifest,json!({"format_version":1,"actor_id":id,"files":{"actor.info.json":format!("{:x}",Sha256::digest(&bytes))}}).to_string()).unwrap();
    assert!(managed::preview_import(&s, dir).unwrap().items.is_empty());
    let p = managed::preview_export(&s, &[id.clone()]).unwrap();
    managed::confirm_export(&s, &[id], &p.token, true).unwrap();
    assert!(dir.join("actor.info").is_file());
    assert!(!dir.join("actor.info.json").exists());
}

#[test]
fn export_reimport_updates_the_photo_path_once_and_archival_films_never_recreate_cast() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let original = h.path().join("original.png");
    png(&original, 77);
    let original_bytes = fs::read(&original).unwrap();
    s.with_connection(|db| {
        db.execute("INSERT INTO films(id) VALUES('archival-film')", [])?;
        Ok(())
    })
    .unwrap();
    let seed = input("Saved", Some(&original));
    let id = managed::save(&s, &seed).unwrap();
    let p = managed::preview_export(&s, &[id.clone()]).unwrap();
    managed::confirm_export(&s, &[id.clone()], &p.token, false).unwrap();
    let dir = Path::new(&p.items[0].directory);
    let mut doc: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("actor.info")).unwrap()).unwrap();
    // Legacy ID-only files remain readable, even if that ID exists in the new database.
    doc["film_ids"] = json!(["archival-film"]);
    doc.as_object_mut().unwrap().remove("filmography");
    fs::write(dir.join("actor.info"),doc.to_string()).unwrap();
    assert_eq!(doc["film_ids"], json!(["archival-film"]));
    let exported = fs::read(dir.join("actor.jpg")).unwrap();
    let before = s.list_protections().unwrap().len();
    let q = managed::preview_import(&s, dir).unwrap();
    let result = managed::confirm_scan_add(
        &s,
        dir,
        &q.token,
        &[resolution(&q.items[0].key, Some(&id), false)],
    )
    .unwrap();
    assert_eq!(result.updated, 1,"an exported file is a new photo source path");
    assert_eq!(result.skipped, 0);
    assert_eq!(s.list_protections().unwrap().len(), before + 1);
    assert_eq!(photo_source(&s,&id),(dir.join("actor.jpg").to_string_lossy().into_owned(),photo_hash(&exported)));
    let repeated = managed::preview_import(&s,dir).unwrap();
    let repeated_result = managed::confirm_scan_add(&s,dir,&repeated.token,&[resolution(&repeated.items[0].key,Some(&id),false)]).unwrap();
    assert_eq!(repeated_result.skipped,1,"the same source path and content must not rewrite a photo");
    assert_eq!(s.list_protections().unwrap().len(),before+1);
    let other = Storage::open(&h.path().join("other")).unwrap();
    let q = managed::preview_import(&other, dir).unwrap();
    managed::confirm_scan_add(&other, dir, &q.token, &[]).unwrap();
    assert!(actors::list_actors(&other).unwrap()[0].film_ids.is_empty());
    assert_eq!(fs::read(&original).unwrap(), original_bytes);
    assert_eq!(fs::read(dir.join("actor.jpg")).unwrap(), exported);
}

#[test]
fn export_preserves_saved_photo_sources_during_replacement_and_rename_cleanup() {
    for (rename, shared, alternate_path) in [(true, false, false), (true, true, false), (false, true, false), (true, true, true)] {
        let h = tempfile::tempdir().unwrap();
        let s = Storage::open(&h.path().join("data")).unwrap();
        let original = h.path().join("original.png");
        png(&original, 91);
        let original_bytes = fs::read(&original).unwrap();
        let id = managed::save(&s, &input("Exported", Some(&original))).unwrap();
        let first = managed::preview_export(&s, &[id.clone()]).unwrap();
        managed::confirm_export(&s, &[id.clone()], &first.token, false).unwrap();
        let directory = Path::new(&first.items[0].directory);
        let photo = directory.join("actor.jpg");
        let photo_bytes = fs::read(&photo).unwrap();
        let info_bytes = fs::read(directory.join("actor.info")).unwrap();
        let manifest_bytes = fs::read(directory.join(".thearchive-actor.json")).unwrap();
        let referenced_actor = if shared {
            let source = if alternate_path && cfg!(windows) {
                // Windows sources may retain a verbatim prefix and different casing.
                std::path::PathBuf::from(photo.canonicalize().unwrap().to_string_lossy().to_uppercase())
            } else { photo.clone() };
            managed::save(&s, &input("Other actor", Some(&source))).unwrap()
        } else {
            let preview = managed::preview_import(&s, directory).unwrap();
            managed::confirm_scan_add(&s, directory, &preview.token, &[]).unwrap();
            id.clone()
        };
        let source_before = photo_source(&s, &referenced_actor);
        if rename {
            let mut edit = input("Renamed", None);
            edit.actor_id = Some(id.clone());
            managed::save(&s, &edit).unwrap();
        }
        let protection_count = s.list_protections().unwrap().len();
        let database_before = fs::read(s.data_directory().join("library.sqlite3")).unwrap();
        let next = managed::preview_export(&s, &[id.clone()]).unwrap();
        assert_eq!(
            managed::confirm_export(&s, &[id.clone()], &next.token, true).unwrap_err(),
            "actor_export_photo_in_use",
            "overwriting or cleaning up an owned export must protect every saved photo source"
        );
        assert_eq!(fs::read(&photo).unwrap(), photo_bytes);
        assert_eq!(fs::read(directory.join("actor.info")).unwrap(), info_bytes);
        assert_eq!(fs::read(directory.join(".thearchive-actor.json")).unwrap(), manifest_bytes);
        assert_eq!(photo_source(&s, &referenced_actor), source_before);
        assert_eq!(fs::read(&original).unwrap(), original_bytes);
        assert_eq!(fs::read(s.data_directory().join("library.sqlite3")).unwrap(), database_before);
        assert_eq!(s.list_protections().unwrap().len(), protection_count);
        assert_eq!(fs::read_dir(managed::export_directory(&s)).unwrap().count(), 1);
        if rename { assert!(!Path::new(&next.items[0].directory).exists()); }
        local_film_library_preview_lib::lifecycle::clear_generated_cache(&s).unwrap();
        assert!(actors::thumbnail(&s, &referenced_actor, false).is_ok());
    }
}

#[test]
fn same_batch_personal_updates_cannot_create_a_new_duplicate_identity() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let mut first = input("First", None);
    first.aliases = vec!["Shared".into()];
    let first = managed::save(&s, &first).unwrap();
    let mut second = input("Second", None);
    second.aliases = vec!["Shared".into()];
    let second = managed::save(&s, &second).unwrap();
    let root = h.path().join("batch");
    actor_info(
        &root.join("a"),
        json!({"format_version":1,"kind":"actor","name":"Shared","aliases":null}),
    );
    actor_info(
        &root.join("b"),
        json!({"format_version":1,"kind":"actor","name":"Shared","aliases":[]}),
    );
    let p = managed::preview_import(&s, &root).unwrap();
    let before = s.list_protections().unwrap().len();
    assert_eq!(
        managed::confirm_scan_add(
            &s,
            &root,
            &p.token,
            &[
                resolution(&p.items[0].key, Some(&first), false),
                resolution(&p.items[1].key, Some(&second), false)
            ]
        )
        .unwrap_err(),
        "actor_batch_conflict"
    );
    assert_eq!(s.list_protections().unwrap().len(), before);
    assert!(
        actors::list_actors(&s)
            .unwrap()
            .iter()
            .all(|a| a.name != "Shared")
    );
}
#[test]
fn compound_alias_members_normalize_identically_for_manual_save_scan_and_repeat() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("data")).unwrap();
    let mut seed = input("Actor", None);
    seed.aliases = vec![" Alpha、beta， GAMMA, delta ".into()];
    let id = managed::save(&s, &seed).unwrap();
    let root = h.path().join("folder");
    actor_info(
        &root,
        json!({"format_version":1,"kind":"actor","name":"actor","aliases":["DELTA, gamma、BETA， alpha"]}),
    );
    let preview = managed::preview_import(&s, &root).unwrap();
    assert!(
        !preview.items[0].can_create,
        "compound aliases must have the same complete identity after save and during preview"
    );
    assert_eq!(
        preview.items[0].aliases,
        ["DELTA", "gamma", "BETA", "alpha"]
    );
    let before = s.list_protections().unwrap().len();
    let mut duplicate = input("ACTOR", None);
    duplicate.aliases = vec!["delta".into(), "Gamma、BETA, Alpha".into()];
    assert_eq!(
        managed::save(&s, &duplicate).unwrap_err(),
        "actor_duplicate_identity"
    );
    assert_eq!(s.list_protections().unwrap().len(), before);
    assert!(
        managed::confirm_scan_add(
            &s,
            &root,
            &preview.token,
            &[resolution(&preview.items[0].key, None, true)]
        )
        .is_err()
    );
    let result = managed::confirm_scan_add(
        &s,
        &root,
        &preview.token,
        &[resolution(&preview.items[0].key, Some(&id), false)],
    )
    .unwrap();
    assert_eq!(result.updated, 1);
    let repeat = managed::preview_import(&s, &root).unwrap();
    assert!(!repeat.items[0].can_create);
    let before = s.list_protections().unwrap().len();
    assert_eq!(
        managed::confirm_scan_add(
            &s,
            &root,
            &repeat.token,
            &[resolution(&repeat.items[0].key, Some(&id), false)]
        )
        .unwrap()
        .skipped,
        1
    );
    assert_eq!(s.list_protections().unwrap().len(), before);
    let fresh = Storage::open(&h.path().join("fresh-data")).unwrap();
    let first = managed::preview_import(&fresh, &root).unwrap();
    assert_eq!(
        managed::confirm_scan_add(&fresh, &root, &first.token, &[])
            .unwrap()
            .imported,
        1
    );
    let repeat = managed::preview_import(&fresh, &root).unwrap();
    assert!(
        !repeat.items[0].can_create,
        "a scanned compound alias must never permit creating the same saved identity again"
    );
    assert_eq!(actors::list_actors(&fresh).unwrap().len(), 1);
    let mut invalid = input("Invalid", None);
    invalid.aliases = vec!["Alpha、\nbeta".into()];
    assert_eq!(
        managed::save(&s, &invalid).unwrap_err(),
        "actor_invalid_name"
    );
}
