use local_film_library_preview_lib::{
    catalog, observation::scan_into_library, playback::directory_facts, storage::Storage,
};
use std::fs;

#[test]
fn facts_count_only_regular_root_files_and_keep_nested_files_out() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("media");
    let folder = root.join("[TEST-01] 片名【标签】");
    fs::create_dir_all(folder.join("nested")).unwrap();
    for file in [
        "one.mp4",
        "two.MKV",
        "poster.JPG",
        "text.srt",
        "notes.txt",
        "nested/extra.mp4",
        "nested/extra.png",
        "nested/extra.ass",
    ] {
        fs::write(folder.join(file), b"fictional").unwrap();
    }
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO libraries(id,display_name,root_path) VALUES('source','虚构',?1)",
                [root.to_string_lossy().as_ref()],
            )
        })
        .unwrap();
    scan_into_library(&storage, "source", &root).unwrap();
    let film = catalog::list_films(&storage)
        .unwrap()
        .into_iter()
        .find(|f| {
            f.videos
                .iter()
                .any(|v| v.observed_path.ends_with("one.mp4"))
        })
        .unwrap();
    let video = &film
        .videos
        .iter()
        .find(|v| v.observed_path.ends_with("one.mp4"))
        .unwrap()
        .id;
    let facts = directory_facts(&storage, &film.id, video).unwrap();
    assert_eq!((facts.videos, facts.images, facts.subtitles), (2, 1, 1));
    assert_eq!(std::path::Path::new(&facts.path), folder);
    assert!(directory_facts(&storage, "wrong-film", video).is_err());
    fs::remove_file(folder.join("one.mp4")).unwrap(); // Isolated fake file only.
    assert_eq!(
        directory_facts(&storage, &film.id, video).unwrap().videos,
        2
    );
    fs::rename(&root, root.with_extension("offline")).unwrap();
    let facts = directory_facts(&storage, &film.id, video).unwrap();
    assert_eq!((facts.videos, facts.images, facts.subtitles), (2, 1, 1));
    assert_eq!(
        local_film_library_preview_lib::playback::video_size(&storage, &film.id, video).unwrap(),
        9
    );
    storage
        .with_connection(|db| db.execute("UPDATE libraries SET enabled=0", []))
        .unwrap();
    assert_eq!(
        directory_facts(&storage, &film.id, video).unwrap_err(),
        "source_paused"
    );
    assert_eq!(
        fs::read(
            root.with_extension("offline")
                .join(folder.strip_prefix(&root).unwrap())
                .join("poster.JPG")
        )
        .unwrap(),
        b"fictional"
    );
}
