use local_film_library_preview_lib::storage::Storage;

#[test]
fn deleting_one_backup_preserves_current_data_other_backups_and_protection() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).unwrap();
    storage
        .with_connection(|db| {
            db.execute("INSERT INTO films (id) VALUES ('fictional')", [])?;
            db.execute(
                "INSERT INTO film_manual (film_id, title) VALUES ('fictional', '人工资料')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let first = storage.create_backup().unwrap();
    let second = storage.create_backup().unwrap();
    storage.create_protection().unwrap();
    let protections = storage.list_protections().unwrap();
    assert!(storage.delete_backup("../library").is_err());
    assert!(
        storage
            .delete_backup("00000000000000000000000000000000")
            .is_err()
    );
    assert_eq!(storage.list_backups().unwrap().len(), 2);
    storage.delete_backup(&first).unwrap();
    assert_eq!(storage.list_backups().unwrap()[0].id, second);
    assert_eq!(storage.list_protections().unwrap(), protections);
    assert!(storage.delete_backup(&first).is_err());
    storage
        .with_connection(|db| {
            let title: String = db.query_row(
                "SELECT title FROM film_manual WHERE film_id='fictional'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(title, "人工资料");
            Ok(())
        })
        .unwrap();
    drop(storage);
    assert_eq!(
        Storage::open(dir.path())
            .unwrap()
            .list_backups()
            .unwrap()
            .len(),
        1
    );
}
