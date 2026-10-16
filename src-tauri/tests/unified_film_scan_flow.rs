use local_film_library_preview_lib::{
    actors, catalog, directory_roles::Role, inventory, observation::scan_into_library,
    storage::Storage,
};
use std::fs;

#[test]
fn mixed_recursive_sources_and_explicit_rescans_only_collect_films() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("source");
    for path in [
        "one.mp4",
        "two.mkv",
        "演员甲/影片甲/a.mp4",
        "演员甲/影片甲/b.mkv",
        "分类/影片乙/a.mp4",
        "分类/direct.mp4",
        "影片丙/a.mp4",
        "演员甲/影片甲/子片/a.mp4",
    ] {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, []).unwrap();
    }
    image::RgbImage::new(12, 16)
        .save(root.join("演员甲/actor.png"))
        .unwrap();
    fs::write(
        root.join("演员甲/actor.info.json"),
        r#"{"format_version":1,"kind":"actor","id":"actor-fictional","name":"演员甲"}"#,
    )
    .unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO libraries(id,display_name,root_path) VALUES('s','合成片库',?1)",
                [root.to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
        .unwrap();
    scan_into_library(&storage, "s", &root).unwrap();
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 6);
    assert_eq!(films.iter().map(|film| film.videos.len()).sum::<usize>(), 8);
    assert!(
        actors::list_actors(&storage).unwrap().is_empty(),
        "film scans must never import actors"
    );
    assert!(films.iter().all(|film| film.actors.is_empty()));
    let report = inventory::inventory(&root).unwrap();
    assert!(report.roles.iter().all(|role| role.role != Role::Actor));
    assert!(
        report.cover_checks.iter().all(|check| !check.relative_path.ends_with("actor.jpg")),
        "actor photo must not participate in film scans"
    );
    scan_into_library(&storage, "s", &root).unwrap();
    assert!(actors::list_actors(&storage).unwrap().is_empty());
}

#[test]
fn saved_actor_photo_paths_are_independent_of_film_scan_problem_evidence() {
    use local_film_library_preview_lib::problems;
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    let unavailable = home.path().join("external-missing-photo.jpg");
    storage.with_transaction(|db| {
        db.execute_batch("INSERT INTO actors(id,observed_name) VALUES('mapped','有照片路径'),('unset','未选择照片');")?;
        db.execute("INSERT INTO actor_image_sources(actor_id,path,fingerprint) VALUES('mapped',?1,'synthetic')", [unavailable.to_string_lossy().as_ref()])?;
        Ok(())
    }).unwrap();
    let missing = problems::list(&storage).unwrap().into_iter().filter(|problem| problem.kind == "actor_missing_photo").collect::<Vec<_>>();
    assert!(missing.is_empty());
    let actors = actors::list_actors(&storage).unwrap();
    assert!(actors.iter().find(|a|a.id=="mapped").unwrap().has_portrait);
    assert!(!actors.iter().find(|a|a.id=="unset").unwrap().has_portrait);
    assert!(!unavailable.exists());
}
