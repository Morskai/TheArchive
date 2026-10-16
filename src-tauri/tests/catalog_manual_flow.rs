use local_film_library_preview_lib::catalog::{
    apply_bulk_tag, create_tag, delete_tag, ensure_film_entries, list_films, list_tags,
    set_favorite, set_film_tags, set_rating,
};
use local_film_library_preview_lib::observation::scan_into_library;
use local_film_library_preview_lib::storage::Storage;
use std::fs::{self, File};

fn setup() -> (tempfile::TempDir, Storage) {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("fictional-media");
    fs::create_dir(&media).unwrap();
    for name in ["one.mp4", "two.mp4"] {
        let folder = media.join(name.trim_end_matches(".mp4"));
        fs::create_dir(&folder).unwrap();
        File::create(folder.join(name)).unwrap();
    }
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO libraries (id, display_name, root_path) VALUES ('source', '虚构来源', ?1)", [media.to_string_lossy().as_ref()])?;
        db.execute("INSERT INTO file_observations (id, library_id, observed_path, kind) VALUES ('one', 'source', 'one/one.mp4', 'video')", [])?;
        db.execute("INSERT INTO file_observations (id, library_id, observed_path, kind) VALUES ('two', 'source', 'two/two.mp4', 'video')", [])?;
        Ok(())
    }).unwrap();
    ensure_film_entries(&storage).unwrap();
    (home, storage)
}

#[test]
fn favorite_rating_tags_survive_reopen_and_rescan_without_changing_identity() {
    let (home, storage) = setup();
    let before = list_films(&storage).unwrap();
    let first = &before[0].id;
    create_tag(&storage, "验收").unwrap();
    set_favorite(&storage, first, true).unwrap();
    set_rating(&storage, first, Some(4)).unwrap();
    set_film_tags(&storage, first, &["验收".into()]).unwrap();
    storage
        .with_connection(|db| {
            db.execute(
                "UPDATE film_manual SET title='人工标题' WHERE film_id=?1",
                [first],
            )?;
            Ok(())
        })
        .unwrap();
    drop(storage);
    let reopened = Storage::open(&home.path().join("data")).unwrap();
    assert_eq!(ensure_film_entries(&reopened).unwrap(), 0);
    scan_into_library(&reopened, "source", &home.path().join("fictional-media")).unwrap();
    let after = list_films(&reopened).unwrap();
    assert_eq!(
        after.iter().map(|film| &film.id).collect::<Vec<_>>(),
        before.iter().map(|film| &film.id).collect::<Vec<_>>()
    );
    let changed = after.iter().find(|film| &film.id == first).unwrap();
    assert_eq!(changed.title, before.iter().find(|film| &film.id == first).unwrap().title);
    assert!(changed.favorite);
    assert_eq!(changed.rating, Some(4.0));
    assert_eq!(changed.tags, ["验收"]);
    assert_eq!(list_tags(&reopened).unwrap(), ["验收"]);
    reopened
        .with_connection(|db| {
            assert_eq!(db.query_row("SELECT COUNT(*) FROM pragma_table_info('film_manual') WHERE name='notes'",[],|r|r.get::<_,i64>(0))?,0);
            Ok(())
        })
        .unwrap();
}

#[test]
fn bulk_updates_only_named_films_and_rolls_back_when_a_later_write_fails() {
    let (_home, storage) = setup();
    let films = list_films(&storage).unwrap();
    let first = films[0].id.clone();
    let second = films[1].id.clone();
    create_tag(&storage, "指定").unwrap();
    assert_eq!(
        apply_bulk_tag(&storage, &[first.clone()], "指定", true).unwrap(),
        1
    );
    let changed = list_films(&storage).unwrap();
    assert_eq!(
        changed.iter().find(|film| film.id == first).unwrap().tags,
        ["指定"]
    );
    assert!(
        changed
            .iter()
            .find(|film| film.id == second)
            .unwrap()
            .tags
            .is_empty()
    );
    assert!(apply_bulk_tag(&storage, &[first.clone(), "missing".into()], "指定", false).is_err());
    assert_eq!(
        list_films(&storage)
            .unwrap()
            .iter()
            .find(|film| film.id == first)
            .unwrap()
            .tags,
        ["指定"]
    );

    storage.with_connection(|db| {
        db.execute_batch("CREATE TRIGGER fail_second BEFORE INSERT ON film_tags WHEN NEW.film_id=(SELECT film_id FROM file_observations WHERE id='two') BEGIN SELECT RAISE(FAIL, 'injected failure'); END;")?;
        Ok(())
    }).unwrap();
    assert!(apply_bulk_tag(&storage, &[first.clone(), second.clone()], "指定", true).is_err());
    let after = list_films(&storage).unwrap();
    assert_eq!(
        after.iter().find(|film| film.id == first).unwrap().tags,
        ["指定"]
    );
    assert!(
        after
            .iter()
            .find(|film| film.id == second)
            .unwrap()
            .tags
            .is_empty()
    );
}

#[test]
fn failed_manual_writes_keep_existing_fields_and_assignments() {
    let (_home, storage) = setup();
    let first = list_films(&storage).unwrap()[0].id.clone();
    create_tag(&storage, "保留").unwrap();
    set_favorite(&storage, &first, true).unwrap();
    set_rating(&storage, &first, Some(5)).unwrap();
    set_film_tags(&storage, &first, &["保留".into()]).unwrap();
    assert!(set_rating(&storage, &first, Some(6)).is_err());
    assert!(set_film_tags(&storage, &first, &["保留".into(), "不存在".into()]).is_err());
    assert!(set_favorite(&storage, "missing", false).is_err());
    let film = list_films(&storage)
        .unwrap()
        .into_iter()
        .find(|film| film.id == first)
        .unwrap();
    assert!(film.favorite);
    assert_eq!(film.rating, Some(5.0));
    assert_eq!(film.tags, ["保留"]);
}

#[test]
fn global_tag_removal_needs_protection_and_preserves_assignments_on_failure() {
    let home = tempfile::tempdir().unwrap();
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage
        .with_connection(|db| {
            db.execute("INSERT INTO films (id) VALUES ('manual')", [])?;
            Ok(())
        })
        .unwrap();
    let first = "manual";
    create_tag(&storage, "保留").unwrap();
    set_film_tags(&storage, first, &["保留".into()]).unwrap();
    File::create(home.path().join("data/protections")).unwrap();
    assert!(delete_tag(&storage, "保留").is_err());
    assert_eq!(list_tags(&storage).unwrap(), ["保留"]);
    storage
        .with_connection(|db| {
            let count: i64 = db.query_row(
                "SELECT COUNT(*) FROM film_tags WHERE film_id='manual'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(count, 1);
            Ok(())
        })
        .unwrap();
}

#[test]
fn global_tag_removal_protects_then_removes_only_that_tag() {
    let (_home, storage) = setup();
    let first = list_films(&storage).unwrap()[0].id.clone();
    create_tag(&storage, "删除").unwrap();
    create_tag(&storage, "保留").unwrap();
    set_film_tags(&storage, &first, &["删除".into(), "保留".into()]).unwrap();
    let prior_protections = storage.list_protections().unwrap().len();
    delete_tag(&storage, "删除").unwrap();
    assert_eq!(
        storage.list_protections().unwrap().len(),
        prior_protections + 1
    );
    assert_eq!(list_tags(&storage).unwrap(), ["保留"]);
    assert_eq!(
        list_films(&storage)
            .unwrap()
            .into_iter()
            .find(|film| film.id == first)
            .unwrap()
            .tags,
        ["保留"]
    );
}
