use local_film_library_preview_lib::{actor_management as managed, actors, storage::Storage};
use serde_json::json;
use std::fs;

fn input(birth: &str) -> managed::SaveActorInput {
    managed::SaveActorInput {
        actor_id: None,
        name: "合成年份演员".into(),
        aliases: vec![],
        profile: actors::Profile {
            birth_month: birth.into(),
            ..Default::default()
        },
        photo_path: None,
        remove_photo: false,
        film_ids: vec![],
    }
}

#[test]
fn year_precision_survives_manual_scan_export_backup_and_legacy_profile_save() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let id = managed::save(&storage, &input("2003")).unwrap();
    assert_eq!(
        actors::list_actors(&storage).unwrap()[0].birth_month,
        "2003"
    );
    let preview = managed::preview_export(&storage, &[id.clone()]).unwrap();
    managed::confirm_export(&storage, &[id.clone()], &preview.token, false).unwrap();
    let exported: serde_json::Value = serde_json::from_slice(
        &fs::read(std::path::Path::new(&preview.items[0].directory).join("actor.info")).unwrap(),
    )
    .unwrap();
    assert_eq!(exported["birth_month"], "2003");
    let import_root = home.path().join("import");
    fs::create_dir(&import_root).unwrap();
    let info_path = import_root.join("actor.info");
    let bytes =
        json!({"format_version":1,"kind":"actor","name":"合成扫描演员","birth_month":"1993"})
            .to_string();
    fs::write(&info_path, &bytes).unwrap();
    let root = import_root.as_path();
    let preview = managed::preview_import(&storage, root).unwrap();
    assert_eq!(preview.items[0].status, "new");
    managed::confirm_scan_add(&storage, root, &preview.token, &[]).unwrap();
    assert!(
        actors::list_actors(&storage)
            .unwrap()
            .iter()
            .any(|a| a.birth_month == "1993")
    );
    assert_eq!(fs::read_to_string(&info_path).unwrap(), bytes);
    let backup = storage.create_backup().unwrap();
    actors::save_profile(
        &storage,
        &id,
        &actors::Profile {
            birth_month: "1900".into(),
            ..Default::default()
        },
    )
    .unwrap();
    storage.restore_backup(&backup).unwrap();
    assert_eq!(
        actors::list_actors(&storage)
            .unwrap()
            .iter()
            .find(|a| a.id == id)
            .unwrap()
            .birth_month,
        "2003"
    );
}

#[test]
fn both_save_paths_keep_month_validation_and_reject_invalid_years() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let current: String = storage
        .with_connection(|db| {
            db.query_row("SELECT strftime('%Y-%m','now','localtime')", [], |r| {
                r.get(0)
            })
        })
        .unwrap();
    let mut edit = input(&current);
    let id = managed::save(&storage, &edit).unwrap();
    edit.actor_id = Some(id.clone());
    for birth in ["1900", &current[..4], "1993-12", ""] {
        edit.profile.birth_month = birth.into();
        managed::save(&storage, &edit).unwrap();
        actors::save_profile(&storage, &id, &edit.profile).unwrap();
        assert_eq!(actors::list_actors(&storage).unwrap()[0].birth_month, birth);
    }
    let future = (current[..4].parse::<u16>().unwrap() + 1).to_string();
    for birth in [
        "1899",
        future.as_str(),
        "2003-00",
        "2003-13",
        "2003-1",
        "2003-01-01",
        "200",
        "２００３",
    ] {
        edit.profile.birth_month = birth.into();
        assert!(managed::save(&storage, &edit).is_err(), "{birth}");
        assert!(
            actors::save_profile(&storage, &id, &edit.profile).is_err(),
            "{birth}"
        );
    }
    assert_eq!(actors::list_actors(&storage).unwrap()[0].birth_month, "");
}
