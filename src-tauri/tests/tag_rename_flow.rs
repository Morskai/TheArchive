use local_film_library_preview_lib::{catalog::{create_tag, list_tags, rename_tag}, storage::Storage};

#[test]
fn rename_preserves_identity_order_assignments_and_backup_values() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let storage = Storage::open(&data).unwrap();
    create_tag(&storage, "Old").unwrap();
    create_tag(&storage, "Other").unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO films (id) VALUES ('synthetic-film')", [])?;
        db.execute("INSERT INTO film_tags (film_id, tag_id) SELECT 'synthetic-film', id FROM tags WHERE name='Old'", [])?;
        Ok(())
    }).unwrap();
    let identity = storage.with_connection(|db| db.query_row("SELECT id FROM tags WHERE name='Old'", [], |r| r.get::<_, String>(0))).unwrap();
    rename_tag(&storage, "Old", "  New  ").unwrap();
    assert_eq!(list_tags(&storage).unwrap(), vec!["New", "Other"]);
    storage.with_connection(|db| {
        let linked: String = db.query_row("SELECT tags.id FROM tags JOIN film_tags ON tags.id=film_tags.tag_id WHERE film_id='synthetic-film' AND name='New'", [], |r| r.get(0))?;
        assert_eq!(linked, identity);
        Ok(())
    }).unwrap();
    for invalid in ["other", "", "                     ", "123456789012345678901"] {
        assert!(rename_tag(&storage, "New", invalid).is_err());
        assert_eq!(list_tags(&storage).unwrap(), vec!["New", "Other"]);
    }
    assert!(rename_tag(&storage, "missing", "Next").is_err());
    rename_tag(&storage, "New", "NEW").unwrap();
    drop(storage);
    let storage = Storage::open(&data).unwrap();
    assert_eq!(list_tags(&storage).unwrap(), vec!["NEW", "Other"]);
    let backup = storage.create_backup().unwrap();
    rename_tag(&storage, "NEW", "Changed").unwrap();
    storage.restore_backup(&backup).unwrap();
    assert_eq!(list_tags(&storage).unwrap(), vec!["NEW", "Other"]);
}
