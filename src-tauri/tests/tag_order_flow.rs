#[path="support/legacy_schema.rs"] mod legacy_schema;
use local_film_library_preview_lib::{catalog::{create_tag,list_tags,reorder_tags},storage::Storage};
#[test]
fn order_is_atomic_persistent_and_backup_restorable() {
    let temp=tempfile::tempdir().unwrap();
    let data=temp.path().join("data");
    let storage=Storage::open(&data).unwrap();
    for name in ["旅行","中文字幕","夜景"] { create_tag(&storage,name).unwrap(); }
    let names=vec!["中文字幕".to_owned(),"夜景".to_owned(),"旅行".to_owned()];
    reorder_tags(&storage,&names).unwrap();
    assert_eq!(list_tags(&storage).unwrap(),names);
    assert!(reorder_tags(&storage,&["夜景".into(),"夜景".into(),"旅行".into()]).is_err());
    assert_eq!(list_tags(&storage).unwrap(),names);
    drop(storage);
    let storage=Storage::open(&data).unwrap();
    assert_eq!(list_tags(&storage).unwrap(),names);
    let backup=storage.create_backup().unwrap();
    reorder_tags(&storage,&["旅行".into(),"夜景".into(),"中文字幕".into()]).unwrap();
    storage.restore_backup(&backup).unwrap();
    assert_eq!(list_tags(&storage).unwrap(),names);
}
#[test]
fn thirty_tag_limit_does_not_remove_existing_tags() {
    let temp=tempfile::tempdir().unwrap(); let storage=Storage::open(temp.path()).unwrap();
    for i in 0..30 {create_tag(&storage,&format!("标签{i}")).unwrap();}
    assert!(create_tag(&storage,"第三十一项").is_err());
    assert_eq!(list_tags(&storage).unwrap().len(),30);
}
#[test]
fn schema_thirteen_migration_preserves_tags_and_initial_order() {
    let temp=tempfile::tempdir().unwrap(); let storage=Storage::open(temp.path()).unwrap();
    create_tag(&storage,"B").unwrap(); create_tag(&storage,"A").unwrap();
    storage.with_connection(|db|{ legacy_schema::remove_actor_v17(db)?; db.execute_batch("ALTER TABLE tags DROP COLUMN position; PRAGMA user_version=13;")?; Ok(()) }).unwrap();
    drop(storage); let storage=Storage::open(temp.path()).unwrap();
    assert_eq!(storage.schema_version().unwrap(), 23);
    assert_eq!(list_tags(&storage).unwrap(),vec!["A","B"]);
}
