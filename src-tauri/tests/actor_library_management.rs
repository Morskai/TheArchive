use local_film_library_preview_lib::{actors, storage::Storage};
use serde_json::json;

fn fixture() -> (tempfile::TempDir, Storage) {
    let root = tempfile::tempdir().unwrap();
    let storage = Storage::open(root.path()).unwrap();
    storage.with_connection(|db| db.execute_batch("INSERT INTO actors(id,observed_name) VALUES('a','甲'),('c','丙'); INSERT INTO actor_tags(name,position) VALUES('第一',0),('第二',1); INSERT INTO actor_tag_links(actor_id,tag) VALUES('a','第二'),('a','第一'),('c','第一'); INSERT INTO actor_image_sources(actor_id,path,fingerprint) VALUES('a','synthetic-original.jpg','path');")).unwrap();
    (root, storage)
}

#[test]
fn actor_catalog_returns_independent_ordered_tags_and_path_presence() {
    let (_root, storage) = fixture();
    let catalog = actors::list_actors(&storage).unwrap();
    let actor = catalog.iter().find(|actor| actor.id == "a").unwrap();
    assert_eq!(actor.tags, vec!["第一", "第二"]);
    assert!(actor.has_portrait);
    let other = catalog.iter().find(|actor| actor.id == "c").unwrap();
    assert_eq!(other.tags, vec!["第一"]);
    assert!(!other.has_portrait);
}

#[test]
fn deleting_actor_retires_only_owned_disk_cache_and_keeps_shared_sizes_and_originals() {
    let (root, storage) = fixture();
    let original = root.path().join("synthetic-original.jpg");
    std::fs::write(&original, b"preserved external original").unwrap();
    let cache = root.path().join("cover-cache");
    std::fs::create_dir(&cache).unwrap();
    let shared = "1".repeat(64);
    let exclusive = "2".repeat(64);
    let shared_file = cache.join(format!("v1-{shared}-480.jpg"));
    let exclusive_file = cache.join(format!("v1-{exclusive}-1200.jpg"));
    std::fs::write(&shared_file, b"shared thumbnail").unwrap();
    std::fs::write(&exclusive_file, b"exclusive thumbnail").unwrap();
    std::fs::write(cache.join("index-v1.json"), serde_json::to_vec(&json!({"version":1,"observations":{"actor:a":{"480":shared,"1200":exclusive},"actor:c":{"480":shared}}})).unwrap()).unwrap();
    assert_eq!(actors::delete_actors(&storage, &["a".into()]).unwrap(), 1);
    assert!(shared_file.is_file());
    assert!(!exclusive_file.exists());
    assert_eq!(std::fs::read(original).unwrap(), b"preserved external original");
    let index: serde_json::Value = serde_json::from_slice(&std::fs::read(cache.join("index-v1.json")).unwrap()).unwrap();
    assert!(index["observations"].get("actor:a").is_none());
    assert!(index["observations"].get("actor:c").is_some());
}

#[test]
fn cache_failure_stops_actor_deletion_before_database_changes() {
    let (root, storage) = fixture();
    std::fs::write(root.path().join("cover-cache"), b"cache directory blocker").unwrap();
    assert_eq!(actors::delete_actors(&storage, &["a".into(), "c".into()]).unwrap_err(), "actor_delete_failed");
    for (table, expected) in [("actors", 2), ("actor_tag_links", 3), ("actor_image_sources", 1)] {
        assert_eq!(storage.with_connection(|db| db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0))).unwrap(), expected);
    }
}
