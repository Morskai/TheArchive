use local_film_library_preview_lib::{actor_management::{self, SaveActorInput}, actors, catalog, observation::{scan_into_library, ScanResult}, problems, storage::Storage};
use std::{fs, path::Path};

fn touch(root: &Path, path: &str) { let p = root.join(path); fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, []).unwrap(); }
fn photo(root: &Path, path: &str) { let p = root.join(path); fs::create_dir_all(p.parent().unwrap()).unwrap(); image::RgbImage::new(12,18).save(p).unwrap(); }
fn bind(storage: &Storage, root: &Path) {
    storage.with_connection(|db| { db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('source','虚构来源',?1)", [root.to_string_lossy().as_ref()])?; Ok(()) }).unwrap();
    assert!(matches!(scan_into_library(storage,"source",root).unwrap(), ScanResult::Complete(_)));
}

fn saved_actor(storage: &Storage, name: &str, aliases: Vec<String>, portrait: Option<&Path>, films: Vec<String>) -> String {
    actor_management::save(storage, &SaveActorInput {
        actor_id: None, name: name.into(), aliases, profile: actors::Profile::default(),
        photo_path: portrait.map(|path|path.to_string_lossy().into_owned()), remove_photo: false, film_ids: films,
    }).unwrap()
}
#[test]
fn saved_photo_state_keeps_identity_manual_data_without_actor_issues() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source"); let data = home.path().join("data");
    photo(&root,"甲/actor.jpg"); touch(&root,"甲/film/main.mp4");
    let storage = Storage::open(&data).unwrap(); bind(&storage,&root);
    let film = catalog::list_films(&storage).unwrap().remove(0);
    let actor = saved_actor(&storage,"甲",vec![],Some(&root.join("甲/actor.jpg")),vec![film.id.clone()]);
    actors::set_favorite(&storage,&actor,true).unwrap(); catalog::set_favorite(&storage,&film.id,true).unwrap();
    let jpeg = actors::thumbnail(&storage,&actor,false).unwrap();
    fs::remove_file(root.join("甲/actor.jpg")).unwrap();
    scan_into_library(&storage,"source",&root).unwrap();
    assert_eq!(catalog::list_films(&storage).unwrap()[0].id,film.id);
    assert_eq!(actors::thumbnail(&storage,&actor,false).unwrap(),jpeg);
    assert!(!problems::list(&storage).unwrap().iter().any(|p|p.kind=="actor_missing_photo"));
    storage.with_connection(|db| {db.execute("DELETE FROM actor_image_sources WHERE actor_id=?1",[&actor])?;Ok(())}).unwrap();
    assert!(!actors::list_actors(&storage).unwrap()[0].has_portrait);
    assert!(!problems::list(&storage).unwrap().iter().any(|p|p.kind=="actor_missing_photo"));
    drop(storage); let storage = Storage::open(&data).unwrap();
    assert!(problems::list(&storage).unwrap().iter().filter(|p|p.kind=="actor_missing_photo").all(|p|p.handled));
    fs::rename(&root,home.path().join("offline-source")).unwrap();
    let offline = actors::list_actors(&storage).unwrap().remove(0);
    assert_eq!(offline.id,actor); assert!(offline.favorite && !offline.has_portrait);
    assert_eq!(actors::thumbnail(&storage,&actor,false).unwrap_err(),"cover_missing");
    assert!(problems::list(&storage).unwrap().iter().filter(|p|p.kind=="actor_missing_photo").all(|p|p.handled));
}
#[test]
fn recursive_film_problems_do_not_create_actor_records() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source");
    photo(&root,"甲/actor.jpg"); touch(&root,"甲/film/main.mp4"); touch(&root,"甲/stray.mp4"); touch(&root,"root.mp4");
    for path in ["[TEST-01] first", "[TEST-01] second"] { fs::create_dir_all(root.join(path)).unwrap(); }
    touch(&root,"甲/film/photos/[TEST-01] ignored/main.mp4"); photo(&root,"甲/film/photos/actor.jpg"); photo(&root,"甲/film/photos/poster.jpg");
    let storage = Storage::open(&home.path().join("data")).unwrap(); bind(&storage,&root);
    assert_eq!(catalog::list_films(&storage).unwrap().len(),4); assert!(actors::list_actors(&storage).unwrap().is_empty());
    let issues = problems::list(&storage).unwrap();
    for (kind,count) in [("film_missing_cover",4),("film_missing_video",0),("film_code_conflict",0),("actor_missing_photo",0)] {
        assert_eq!(issues.iter().filter(|p| p.kind==kind).count(),count,"{kind}");
    }
    assert!(!issues.iter().any(|p| p.directories.iter().any(|d| d.contains("photos"))));
    storage.with_connection(|db| { assert_eq!(db.query_row("SELECT COUNT(*) FROM file_observations WHERE observed_path LIKE '%photos%'",[],|r|r.get::<_,i64>(0))?,3); Ok(()) }).unwrap();
}

#[test]
fn bounded_role_problem_failure_rolls_back_scan_snapshot_and_identity() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source"); touch(&root,"film/main.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| { db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('source','虚构来源',?1)",[root.to_string_lossy().as_ref()])?; db.execute_batch("CREATE TRIGGER reject_problem BEFORE INSERT ON issue_records WHEN NEW.subject_type='film_missing_cover' BEGIN SELECT RAISE(ABORT,'synthetic'); END")?; Ok(()) }).unwrap();
    assert!(scan_into_library(&storage,"source",&root).is_err());
    storage.with_connection(|db| { for table in ["file_observations","films","issue_records"] { assert_eq!(db.query_row(&format!("SELECT COUNT(*) FROM {table}"),[],|r|r.get::<_,i64>(0))?,0); } Ok(()) }).unwrap();
}

#[test]
fn ordinary_category_named_film_folder_keeps_all_versions_in_one_identity() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source");
    for path in ["多人/main.mp4", "多人/second.mkv", "日本/main.mp4", "日本/second.mkv", "单体/main.mp4", "单体/second.mkv", "FC2/main.mp4", "FC2/second.mkv"] { touch(&root,path); }
    let storage = Storage::open(&home.path().join("data")).unwrap(); bind(&storage,&root);
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(),4);
    assert!(films.iter().all(|film| film.videos.len()==2));
    for name in ["多人", "日本", "单体", "FC2"] {
        let film = films.iter().find(|film| film.videos.iter().any(|video| video.observed_path == format!("{name}/main.mp4"))).unwrap();
        assert_eq!(film.title, name, "confirmed Film directory {name}");
    }
    assert!(actors::list_actors(&storage).unwrap().is_empty());
}

#[test]
fn confirmed_film_titles_remove_markers_while_legacy_category_titles_keep_file_fallback() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("source");
    touch(&root, "00-日本/unrelated-file.mp4");
    touch(&root, "[TEST-01] 真实片名【虚构标签】/unrelated-file.mkv");
    let storage = Storage::open(&home.path().join("data")).unwrap();
    bind(&storage, &root);
    let films = catalog::list_films(&storage).unwrap();
    let category = films.iter().find(|film| film.videos[0].observed_path.starts_with("00-日本/")).unwrap();
    let marked = films.iter().find(|film| film.videos[0].observed_path.starts_with("[TEST-01]")).unwrap();
    assert_eq!(category.title, "00-日本");
    assert_eq!(marked.title, "真实片名");
    let category_id = category.id.clone();
    storage.with_connection(|db| {
        // A legacy snapshot has no saved directory roles; preserve its display rules.
        db.execute("DELETE FROM issue_records WHERE subject_type='library_scan_details'", [])?;
        Ok(())
    }).unwrap();
    let legacy = catalog::list_films(&storage).unwrap();
    assert_eq!(legacy.iter().find(|film| film.id == category_id).unwrap().title, "unrelated-file");
    storage.with_connection(|db| {
        db.execute("INSERT INTO film_manual(film_id,title) VALUES(?1,'人工片名')", [&category_id])?;
        Ok(())
    }).unwrap();
    assert_eq!(catalog::list_films(&storage).unwrap().iter().find(|film| film.id == category_id).unwrap().title, "unrelated-file");
}

#[test]
fn actor_catalog_keeps_same_saved_names_separate_and_manual_cast_independent() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source");
    for directory in ["同名（甲）","同名（乙）"] {photo(&root,&format!("{directory}/actor.jpg"));touch(&root,&format!("{directory}/影片/{directory}.mp4"));}
    let storage = Storage::open(&home.path().join("data")).unwrap(); bind(&storage,&root);
    let films = catalog::list_films(&storage).unwrap();
    let first_film = films.iter().find(|film|film.videos[0].observed_path.starts_with("同名（甲）/")).unwrap();
    let second_film = films.iter().find(|film|film.videos[0].observed_path.starts_with("同名（乙）/")).unwrap();
    let first_id = saved_actor(&storage,"同名",vec!["甲".into()],Some(&root.join("同名（甲）/actor.jpg")),vec![first_film.id.clone()]);
    let second_id = saved_actor(&storage,"同名",vec!["乙".into()],Some(&root.join("同名（乙）/actor.jpg")),vec![second_film.id.clone()]);
    assert_ne!(first_id,second_id); actors::set_favorite(&storage,&first_id,true).unwrap();
    actors::add_film_cast_actor(&storage,&first_film.id,&second_id).unwrap();
    scan_into_library(&storage,"source",&root).unwrap();
    let current = actors::list_actors(&storage).unwrap();
    let first = current.iter().find(|actor|actor.id==first_id).unwrap();
    let second = current.iter().find(|actor|actor.id==second_id).unwrap();
    assert_eq!((first.name.as_str(),second.name.as_str()),("同名","同名"));
    assert_eq!((first.aliases.as_deref(),second.aliases.as_deref()),(Some("甲"),Some("乙")));
    assert!(first.favorite && first.has_portrait && first.drawable);
    assert_eq!(first.film_ids,[first_film.id.clone()]);
    assert!(second.drawable && !second.favorite);
    assert_eq!(second.film_ids.iter().collect::<std::collections::BTreeSet<_>>(),[&first_film.id,&second_film.id].into_iter().collect());
    assert_eq!(first.sources.iter().map(|source|source.id.as_str()).collect::<Vec<_>>(),["source"]);
    assert_eq!(second.sources.iter().map(|source|source.id.as_str()).collect::<Vec<_>>(),["source"]);
}
#[test]
fn legacy_cached_problem_keeps_handled_through_pause_resume_without_scan() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source");
    touch(&root,"[TEST] real-film/main.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap(); bind(&storage,&root);
    let missing = problems::list(&storage).unwrap().into_iter().find(|p|p.kind=="film_missing_cover").unwrap();
    problems::mark(&storage,&[missing.id.clone()]).unwrap();
    storage.with_connection(|db| {
        let text: String = db.query_row("SELECT evidence FROM issue_records WHERE subject_type='library_scan_details'",[],|r|r.get(0))?;
        let mut legacy: serde_json::Value = serde_json::from_str(&text).unwrap(); legacy.as_object_mut().unwrap().remove("roles"); legacy.as_object_mut().unwrap().remove("video_sizes");
        db.execute("UPDATE issue_records SET evidence=?1 WHERE subject_type='library_scan_details'",[legacy.to_string()])?; Ok(())
    }).unwrap();
    fs::rename(&root,home.path().join("offline-source")).unwrap();
    local_film_library_preview_lib::source_scan::change_source_state(&storage,"source","pause").unwrap();
    assert!(problems::list(&storage).unwrap().is_empty());
    local_film_library_preview_lib::source_scan::change_source_state(&storage,"source","resume").unwrap();
    let resumed = problems::list(&storage).unwrap().into_iter().find(|p|p.kind=="film_missing_cover").unwrap();
    assert_eq!(resumed.id,missing.id); assert!(resumed.handled);
}

#[test]
fn saved_actor_photo_state_is_independent_of_film_scan() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source"); touch(&root,"film/main.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap(); bind(&storage,&root);
    let film = catalog::list_films(&storage).unwrap().remove(0);
    let actor = saved_actor(&storage,"旧根演员",vec![],None,vec![film.id.clone()]);
    actors::set_favorite(&storage,&actor,true).unwrap();
    for _ in 0..2 {
        scan_into_library(&storage,"source",&root).unwrap();
        assert_eq!(catalog::list_films(&storage).unwrap()[0].actors[0].id,actor);
        assert!(!catalog::list_films(&storage).unwrap()[0].actors[0].system);
        assert!(!actors::list_actors(&storage).unwrap().iter().find(|item|item.id==actor).unwrap().has_portrait);
        assert!(!problems::list(&storage).unwrap().iter().any(|p|p.kind=="actor_missing_photo"));
        assert!(actors::list_actors(&storage).unwrap()[0].favorite);
    }
}
#[test]
fn actor_import_photo_changes_reject_stale_confirmation_without_film_scan_side_effects() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source");
    photo(&root,"甲/actor.jpg"); touch(&root,"甲/film/main.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap(); bind(&storage,&root);
    fs::write(root.join("甲/actor.info"),serde_json::json!({"format_version":1,"kind":"actor","name":"甲"}).to_string()).unwrap();
    let preview = actor_management::preview_import(&storage,&root).unwrap();
    let item = preview.items.iter().find(|item|item.name=="甲").unwrap();
    let selection = actor_management::ScanResolution {key:item.key.clone(),actor_id:None,create_new:true};
    image::RgbImage::from_pixel(12,18,image::Rgb([44,55,66])).save(root.join("甲/actor.jpg")).unwrap();
    assert_eq!(actor_management::confirm_scan_add(&storage,&root,&preview.token,&[selection]).unwrap_err(),"actor_import_changed");
    assert!(actors::list_actors(&storage).unwrap().is_empty());
    scan_into_library(&storage,"source",&root).unwrap();
    assert!(actors::list_actors(&storage).unwrap().is_empty());
    fs::write(root.join("甲/actor.jpg"),b"synthetic corrupt photo").unwrap();
    scan_into_library(&storage,"source",&root).unwrap();
    assert!(!problems::list(&storage).unwrap().iter().any(|p|p.kind=="actor_missing_photo"));
}
#[test]
fn saved_actor_photo_state_survives_source_pause_without_actor_problems() {
    let home = tempfile::tempdir().unwrap(); let root = home.path().join("source-a"); let other = home.path().join("source-b");
    photo(&root,"甲/actor.jpg"); touch(&root,"甲/film/main.mp4"); touch(&other,"film/other.mp4");
    let storage = Storage::open(&home.path().join("data")).unwrap(); bind(&storage,&root);
    storage.with_connection(|db| {db.execute("INSERT INTO libraries(id,display_name,root_path) VALUES('other','虚构B',?1)",[other.to_string_lossy().as_ref()])?;Ok(())}).unwrap();
    scan_into_library(&storage,"other",&other).unwrap();
    let film = catalog::list_films(&storage).unwrap().into_iter().find(|f|f.videos[0].source_id=="other").unwrap();
    let actor = saved_actor(&storage,"甲",vec![],None,vec![film.id.clone()]);
    assert!(!actors::list_actors(&storage).unwrap().iter().find(|item|item.id==actor).unwrap().has_portrait);
    assert!(!problems::list(&storage).unwrap().iter().any(|p|p.kind=="actor_missing_photo"));
    for source in ["source","other"] {
        for state in ["pause","resume"] {
            local_film_library_preview_lib::source_scan::change_source_state(&storage,source,state).unwrap();
            assert!(!actors::list_actors(&storage).unwrap().iter().find(|item|item.id==actor).unwrap().has_portrait);
            assert!(!problems::list(&storage).unwrap().iter().any(|p|p.kind=="actor_missing_photo"));
        }
    }
}
