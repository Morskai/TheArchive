use local_film_library_preview_lib::{
    actor_info,
    actor_management::{self as managed, SaveActorInput, ScanResolution},
    actors, catalog,
    observation::{ScanResult, scan_into_library},
    storage::Storage,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn scan(s: &Storage, root: &Path, source: &str, folders: &[(&str, &[&str])]) {
    for (folder, videos) in folders {
        fs::create_dir_all(root.join(folder)).unwrap();
        for video in *videos {
            fs::write(root.join(folder).join(video), []).unwrap();
        }
    }
    s.with_connection(|db| {
        db.execute(
            "INSERT INTO libraries(id,display_name,root_path) VALUES(?1,?1,?2)",
            [source, root.to_str().unwrap()],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        scan_into_library(s, source, root).unwrap(),
        ScanResult::Complete(_)
    ));
}
fn input(name: &str, films: Vec<String>, photo: Option<&Path>) -> SaveActorInput {
    SaveActorInput {
        actor_id: None,
        name: name.into(),
        aliases: vec![],
        profile: actors::Profile {
            birth_month: "1993".into(),
            ..Default::default()
        },
        photo_path: photo.map(|p| p.to_str().unwrap().into()),
        remove_photo: false,
        film_ids: films,
    }
}
fn info(root: &Path, name: &str, entries: Value) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("actor.info"),
        json!({"format_version":1,"kind":"actor","name":name,"filmography":entries}).to_string(),
    )
    .unwrap();
}
fn entry(folder: &str, files: &[&str]) -> Value {
    json!({"folder_name":folder,"video_files":files})
}
fn choice(key: &str, id: &str) -> ScanResolution {
    ScanResolution {
        key: key.into(),
        actor_id: Some(id.into()),
        create_new: false,
    }
}
fn links(s: &Storage, id: &str) -> i64 {
    s.with_connection(|db| {
        db.query_row(
            "SELECT COUNT(*) FROM film_actors WHERE actor_id=?1",
            [id],
            |r| r.get(0),
        )
    })
    .unwrap()
}

#[test]
fn exported_filmography_restores_new_ids_profiles_photos_and_two_actor_relations_after_relocation()
{
    let h = tempfile::tempdir().unwrap();
    let original = Storage::open(&h.path().join("original-db")).unwrap();
    scan(
        &original,
        &h.path().join("H/classification"),
        "old",
        &[("Movie Folder", &["episode2.MP4", "episode1.mkv"])],
    );
    assert!(actors::list_actors(&original).unwrap().is_empty());
    // Matching reads saved scan evidence, so synthetic Windows drives need no real mount.
    original
        .with_connection(|db| {
            db.execute(
                "UPDATE libraries SET root_path='H:\\old-classification' WHERE id='old'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let old_film = catalog::list_films(&original).unwrap()[0].id.clone();
    let photo = h.path().join("photo.png");
    image::RgbImage::from_pixel(8, 10, image::Rgb([42, 0, 0]))
        .save(&photo)
        .unwrap();
    let a = managed::save(
        &original,
        &input("Actor A", vec![old_film.clone()], Some(&photo)),
    )
    .unwrap();
    let b = managed::save(&original, &input("Actor B", vec![old_film.clone()], None)).unwrap();
    let dest = h.path().join("outside-software-data");
    fs::create_dir(&dest).unwrap();
    let p = managed::preview_export_to(&original, &[a.clone(), b.clone()], &dest).unwrap();
    managed::confirm_export_to(&original, &[a.clone(), b], &dest, &p.token, false).unwrap();
    let doc: Value = serde_json::from_slice(
        &fs::read(Path::new(&p.items[0].directory).join("actor.info")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        doc["filmography"],
        json!([entry("Movie Folder", &["episode1.mkv", "episode2.MP4"])])
    );
    let fresh = Storage::open(&h.path().join("fresh-db")).unwrap();
    scan(
        &fresh,
        &h.path().join("E/new-category"),
        "new",
        &[("Movie Folder", &["episode1.mkv", "episode2.MP4"])],
    );
    assert!(actors::list_actors(&fresh).unwrap().is_empty());
    fresh
        .with_connection(|db| {
            db.execute(
                "UPDATE libraries SET root_path='E:\\new-classification' WHERE id='new'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let new_film = catalog::list_films(&fresh).unwrap()[0].id.clone();
    assert_ne!(old_film, new_film);
    let root = dest.join("ActorExports");
    let q = managed::preview_import(&fresh, &root).unwrap();
    let result = managed::confirm_scan_add(&fresh, &root, &q.token, &[]).unwrap();
    let result = serde_json::to_value(result).unwrap();
    assert_eq!(result["relationships"]["available"], 2);
    assert_eq!(result["relationships"]["existing"], 0);
    assert_eq!(result["relationships"]["notFound"], 0);
    let restored = actors::list_actors(&fresh).unwrap();
    assert_eq!(restored.len(), 2);
    for actor in &restored {
        assert_eq!(actor.birth_month, "1993");
        assert_eq!(links(&fresh, &actor.id), 1);
        assert_ne!(actor.id, a);
    }
    let restored_a = restored
        .iter()
        .find(|actor| actor.name == "Actor A")
        .unwrap();
    assert_eq!(
        actors::thumbnail(&fresh, &restored_a.id, true).unwrap(),
        actors::thumbnail(&original, &a, true).unwrap()
    );
    assert_eq!(catalog::list_films(&fresh).unwrap()[0].actors.len(), 2);
}

#[test]
fn exact_full_lists_are_order_independent_and_duplicate_cards_are_not_chosen() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("db")).unwrap();
    scan(
        &s,
        &h.path().join("media"),
        "s",
        &[
            ("Exact", &["one.mp4", "two.mkv"]),
            ("category1/Duplicate", &["same.mp4"]),
            ("category2/Duplicate", &["same.mp4"]),
        ],
    );
    let root = h.path().join("info");
    info(
        &root,
        "Actor",
        json!([
            entry("Exact", &["two.mkv", "one.mp4"]),
            entry("exact", &["one.mp4", "two.mkv"]),
            entry("Exact", &["ONE.mp4", "two.mkv"]),
            entry("Exact", &["one.mp4"]),
            entry("Exact", &["one.mp4", "two.mkv", "extra.avi"]),
            entry("Duplicate", &["same.mp4"])
        ]),
    );
    let q = managed::preview_import(&s, &root).unwrap();
    assert_eq!(q.items[0].status, "new");
    let r =
        serde_json::to_value(managed::confirm_scan_add(&s, &root, &q.token, &[]).unwrap()).unwrap();
    assert_eq!(r["relationships"]["available"], 1);
    assert_eq!(r["relationships"]["notFound"], 5);
    assert_eq!(
        r["relationships"]["unmatched"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["folderName"] == "Duplicate")
            .unwrap()["reason"],
        "找到多个相同条目，未关联"
    );
    assert_eq!(links(&s, &actors::list_actors(&s).unwrap()[0].id), 1);
}

#[test]
fn unchanged_profile_adds_only_missing_links_preserves_manual_cast_and_legacy_empty_lists() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("db")).unwrap();
    scan(
        &s,
        &h.path().join("media"),
        "s",
        &[
            ("One", &["one.mp4"]),
            ("Two", &["two.mkv"]),
            ("Other", &["other.avi"]),
        ],
    );
    let films = catalog::list_films(&s).unwrap();
    let film = |name: &str| {
        films
            .iter()
            .find(|f| f.videos[0].observed_path.contains(name))
            .unwrap()
            .id
            .clone()
    };
    let id = managed::save(&s, &input("Actor", vec![film("One"), film("Other")], None)).unwrap();
    let root = h.path().join("info");
    info(
        &root,
        "Actor",
        json!([
            entry("One", &["one.mp4"]),
            entry("Two", &["two.mkv"]),
            entry("Two", &["two.mkv"]),
            entry("Missing", &["missing.mp4"])
        ]),
    );
    let q = managed::preview_import(&s, &root).unwrap();
    let r = serde_json::to_value(
        managed::confirm_scan_add(&s, &root, &q.token, &[]).unwrap(),
    )
    .unwrap();
    assert_eq!(r["relationships"]["available"], 1);
    assert_eq!(r["relationships"]["existing"], 1);
    assert_eq!(r["relationships"]["notFound"], 1);
    assert_eq!(links(&s, &id), 3);
    let q = managed::preview_import(&s, &root).unwrap();
    let r = serde_json::to_value(
        managed::confirm_scan_add(&s, &root, &q.token, &[choice(&q.items[0].key, &id)]).unwrap(),
    )
    .unwrap();
    assert_eq!(r["relationships"]["available"], 0);
    assert_eq!(r["relationships"]["existing"], 2);
    for doc in [
        json!({"format_version":1,"kind":"actor","name":"Actor","film_ids":[film("One")]}),
        json!({"format_version":1,"kind":"actor","name":"Actor","filmography":[]}),
    ] {
        fs::write(root.join("actor.info"), doc.to_string()).unwrap();
        let q = managed::preview_import(&s, &root).unwrap();
        managed::confirm_scan_add(&s, &root, &q.token, &[choice(&q.items[0].key, &id)]).unwrap();
        assert_eq!(links(&s, &id), 3);
    }
}

#[test]
fn filmography_validation_rejects_paths_empty_lists_bad_types_and_unknown_keys() {
    assert!(actor_info::parse(&json!({"format_version":1,"kind":"actor","name":"Actor","filmography":[entry(" Movie ", &[" clip.MP4 "])]}).to_string()).is_ok());
    for value in [
        json!(null),
        json!({}),
        json!([entry("", &["one.mp4"])]),
        json!([entry("../Movie", &["one.mp4"])]),
        json!([entry("Movie", &[])]),
        json!([entry("Movie", &["sub/one.mp4"])]),
        json!([entry("Movie", &["one.mp4", "one.mp4"])]),
        json!([{"folder_name":"Movie","video_files":[42]}]),
        json!([{"folder_name":"Movie","video_files":["one.mp4"],"id":"x"}]),
    ] {
        assert_eq!(
            actor_info::parse(
                &json!({"format_version":1,"kind":"actor","name":"Actor","filmography":value})
                    .to_string()
            )
            .unwrap_err(),
            "info_invalid"
        );
    }
}

#[test]
fn catalog_snapshot_changes_expire_preview_and_transaction_failure_rolls_back_profiles_photos_and_cast()
 {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("db")).unwrap();
    let media = h.path().join("media");
    scan(&s, &media, "s", &[("Movie", &["main.mp4"])]);
    let root = h.path().join("info");
    info(&root, "New Actor", json!([entry("Movie", &["main.mp4"])]));
    image::RgbImage::from_pixel(8, 10, image::Rgb([21, 0, 0]))
        .save(root.join("actor.png"))
        .unwrap();
    let q = managed::preview_import(&s, &root).unwrap();
    fs::write(media.join("Movie/extra.mkv"), []).unwrap();
    scan_into_library(&s, "s", &media).unwrap();
    assert_eq!(
        managed::confirm_scan_add(&s, &root, &q.token, &[]).unwrap_err(),
        "actor_import_changed"
    );
    assert!(actors::list_actors(&s).unwrap().is_empty());
    info(
        &root,
        "New Actor",
        json!([entry("Movie", &["extra.mkv", "main.mp4"])]),
    );
    let q = managed::preview_import(&s, &root).unwrap();
    s.with_connection(|db| { db.execute_batch("CREATE TRIGGER fail_cast BEFORE INSERT ON film_actors BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")?; Ok(()) }).unwrap();
    let protections = s.list_protections().unwrap().len();
    assert!(managed::confirm_scan_add(&s, &root, &q.token, &[]).is_err());
    s.with_connection(|db| {
        for table in ["actors", "actor_manual", "actor_image_sources", "film_actors"] {
            assert_eq!(
                db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
                0
            );
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(s.list_protections().unwrap().len(), protections + 1);
}

#[test]
fn source_and_relationship_changes_expire_preview_and_protection_failure_writes_nothing() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("db")).unwrap();
    scan(
        &s,
        &h.path().join("media"),
        "s",
        &[("Movie", &["main.mp4"])],
    );
    let root = h.path().join("info");
    let id = managed::save(&s, &input("New Actor", vec![], None)).unwrap();
    info(&root, "New Actor", json!([entry("Movie", &["main.mp4"])]));
    let q = managed::preview_import(&s, &root).unwrap();
    assert_eq!(q.items[0].default_actor_id.as_deref(), Some(id.as_str()));
    s.with_connection(|db| {
        db.execute("UPDATE libraries SET enabled=0 WHERE id='s'", [])?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        managed::confirm_scan_add(&s, &root, &q.token, &[]).unwrap_err(),
        "actor_import_changed"
    );
    s.with_connection(|db| {
        db.execute("UPDATE libraries SET enabled=1 WHERE id='s'", [])?;
        Ok(())
    })
    .unwrap();
    let q = managed::preview_import(&s, &root).unwrap();
    fs::remove_dir_all(s.data_directory().join("protections")).unwrap();
    fs::write(s.data_directory().join("protections"), b"block directory").unwrap();
    assert_eq!(
        managed::confirm_scan_add(&s, &root, &q.token, &[]).unwrap_err(),
        "protection_failed"
    );
    assert_eq!(actors::list_actors(&s).unwrap().len(), 1);
    assert_eq!(links(&s, &id), 0);
}

#[test]
fn root_direct_videos_and_nested_card_lists_are_not_combined() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("db")).unwrap();
    scan(
        &s,
        &h.path().join("Root Movie"),
        "s",
        &[("", &["root.MP4"]), ("Child Movie", &["child.mkv"])],
    );
    let root = h.path().join("info");
    info(
        &root,
        "Actor",
        json!([
            entry("Root Movie", &["root.MP4"]),
            entry("Child Movie", &["child.mkv"]),
            entry("Root Movie", &["root.MP4", "child.mkv"])
        ]),
    );
    let q = managed::preview_import(&s, &root).unwrap();
    let r =
        serde_json::to_value(managed::confirm_scan_add(&s, &root, &q.token, &[]).unwrap()).unwrap();
    assert_eq!(r["relationships"]["available"], 2);
    assert_eq!(r["relationships"]["notFound"], 1);
    assert_eq!(catalog::list_films(&s).unwrap().len(), 2);
}

#[test]
fn export_uses_current_complete_directory_only_and_rejects_missing_evidence_without_publishing() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("db")).unwrap();
    let media = h.path().join("media");
    scan(&s, &media, "s", &[("Old", &["main.mp4", "part2.mkv"])]);
    let film = catalog::list_films(&s).unwrap()[0].id.clone();
    let id = managed::save(&s, &input("Actor", vec![film], None)).unwrap();
    fs::rename(media.join("Old"), media.join("New")).unwrap();
    scan_into_library(&s, "s", &media).unwrap();
    let p = managed::preview_export(&s, &[id.clone()]).unwrap();
    managed::confirm_export(&s, &[id.clone()], &p.token, false).unwrap();
    let dir = Path::new(&p.items[0].directory);
    let doc: Value = serde_json::from_slice(&fs::read(dir.join("actor.info")).unwrap()).unwrap();
    assert_eq!(
        doc["filmography"],
        json!([entry("New", &["main.mp4", "part2.mkv"])])
    );
    let exported = fs::read(dir.join("actor.info")).unwrap();
    fs::remove_file(media.join("New/part2.mkv")).unwrap();
    scan_into_library(&s, "s", &media).unwrap();
    fs::remove_file(media.join("New/main.mp4")).unwrap();
    scan_into_library(&s, "s", &media).unwrap();
    assert_eq!(
        managed::preview_export(&s, &[id]).unwrap_err(),
        "actor_export_film_unavailable"
    );
    assert_eq!(fs::read(dir.join("actor.info")).unwrap(), exported);
}

#[test]
fn unselected_actor_with_unavailable_film_does_not_block_selected_export() {
    let h = tempfile::tempdir().unwrap();
    let s = Storage::open(&h.path().join("db")).unwrap();
    s.with_connection(|db| {
        db.execute("INSERT INTO films(id) VALUES('no-scan-evidence')", [])?;
        Ok(())
    })
    .unwrap();
    managed::save(
        &s,
        &input("Missing Actor", vec!["no-scan-evidence".into()], None),
    )
    .unwrap();
    let selected = managed::save(&s, &input("Selected Actor", vec![], None)).unwrap();
    assert!(managed::preview_export(&s, &[selected]).is_ok());
}
