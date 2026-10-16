//! Actor tags are independent of film tags and never change cast relationships.
use crate::storage::Storage;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::BTreeSet;

type Result<T> = std::result::Result<T, String>;

fn names(db: &Connection) -> rusqlite::Result<Vec<String>> {
    db.prepare("SELECT name FROM actor_tags ORDER BY position,name")?.query_map([], |r| r.get(0))?.collect()
}

fn tag_error(error: Box<dyn std::error::Error + Send + Sync>) -> String {
    match error.downcast_ref::<rusqlite::Error>() {
        Some(rusqlite::Error::InvalidParameterName(code)) if matches!(code.as_str(), "duplicate_tag" | "tag_limit" | "stale_tag_order" | "actor_not_found" | "tag_not_found") => code.clone(),
        _ => "actor_tag_save_failed".into(),
    }
}

fn valid_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 20 { return Err("invalid_tag".into()); }
    Ok(name)
}

fn canonical_tag(db: &Connection, name: &str) -> rusqlite::Result<String> {
    db.query_row("SELECT name FROM actor_tags WHERE name=?1", [name], |r| r.get(0)).optional()?.ok_or_else(|| rusqlite::Error::InvalidParameterName("tag_not_found".into()))
}

pub fn list(storage: &Storage) -> Result<Vec<String>> {
    storage.with_connection(names).map_err(|_| "actor_tags_load_failed".into())
}

pub fn create(storage: &Storage, name: &str) -> Result<()> {
    let name = valid_name(name)?;
    storage.with_transaction(|tx| {
        let existing = names(tx)?;
        if existing.iter().any(|value| value.to_lowercase() == name.to_lowercase()) { return Err(rusqlite::Error::InvalidParameterName("duplicate_tag".into())); }
        if existing.len() >= 30 { return Err(rusqlite::Error::InvalidParameterName("tag_limit".into())); }
        tx.execute("INSERT INTO actor_tags(name,position) VALUES(?1,(SELECT COALESCE(MAX(position),-1)+1 FROM actor_tags))", [name])?;
        Ok(())
    }).map_err(tag_error)
}

pub fn rename(storage: &Storage, name: &str, new_name: &str) -> Result<()> {
    let new_name = valid_name(new_name)?;
    storage.with_transaction(|tx| {
        let old_name = canonical_tag(tx, name)?;
        if names(tx)?.iter().any(|value| value != &old_name && value.to_lowercase() == new_name.to_lowercase()) { return Err(rusqlite::Error::InvalidParameterName("duplicate_tag".into())); }
        tx.execute("UPDATE actor_tags SET name=?1 WHERE name=?2", params![new_name, old_name])?;
        // SQLite NOCASE considers ASCII case-only keys unchanged, so the FK
        // cascade may leave old spelling. Keep link keys canonical explicitly.
        tx.execute("UPDATE actor_tag_links SET tag=?1 WHERE tag=?2 COLLATE NOCASE", params![new_name, old_name])?;
        Ok(())
    }).map_err(tag_error)
}

pub fn delete(storage: &Storage, name: &str) -> Result<()> {
    storage.create_protection().map_err(|_| "protection_failed")?;
    storage.with_transaction(|tx| {
        let name = canonical_tag(tx, name)?;
        tx.execute("DELETE FROM actor_tags WHERE name=?1", [name])?;
        Ok(())
    }).map_err(tag_error)
}

pub fn reorder(storage: &Storage, requested: &[String]) -> Result<()> {
    storage.with_transaction(|tx| {
        let existing = names(tx)?;
        let unique: BTreeSet<_> = requested.iter().collect();
        if requested.len() != existing.len() || unique.len() != requested.len() || existing.iter().any(|name| !unique.contains(name)) { return Err(rusqlite::Error::InvalidParameterName("stale_tag_order".into())); }
        for (position, name) in requested.iter().enumerate() { tx.execute("UPDATE actor_tags SET position=?1 WHERE name=?2", params![position as i64, name])?; }
        Ok(())
    }).map_err(tag_error)
}

pub fn apply_bulk_tag(storage: &Storage, actor_ids: &[String], name: &str, add: bool) -> Result<usize> {
    storage.with_transaction(|tx| {
        let tag = canonical_tag(tx, name)?;
        let unique: BTreeSet<_> = actor_ids.iter().collect();
        for id in &unique {
            if !tx.query_row("SELECT EXISTS(SELECT 1 FROM actors WHERE id=?1)", [*id], |r| r.get::<_, bool>(0))? { return Err(rusqlite::Error::InvalidParameterName("actor_not_found".into())); }
        }
        for id in &unique {
            if add { tx.execute("INSERT OR IGNORE INTO actor_tag_links(actor_id,tag) VALUES(?1,?2)", params![id, tag])?; }
            else { tx.execute("DELETE FROM actor_tag_links WHERE actor_id=?1 AND tag=?2", params![id, tag])?; }
        }
        Ok(unique.len())
    }).map_err(tag_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn fixture() -> (tempfile::TempDir, Storage) {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::open(root.path()).unwrap();
        // Remain isolated while the parent integrates schema 19.
        storage.with_connection(|db| db.execute_batch("CREATE TABLE IF NOT EXISTS actor_tags(name TEXT PRIMARY KEY COLLATE NOCASE,position INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS actor_tag_links(actor_id TEXT NOT NULL REFERENCES actors(id) ON DELETE CASCADE,tag TEXT NOT NULL REFERENCES actor_tags(name) ON UPDATE CASCADE ON DELETE CASCADE,PRIMARY KEY(actor_id,tag)); INSERT INTO actors(id,observed_name) VALUES('a','甲'),('b','乙'); INSERT INTO films(id) VALUES('film'); INSERT INTO tags(id,name) VALUES('film-tag','精选'); INSERT INTO film_tags(film_id,tag_id) VALUES('film','film-tag');")).unwrap();
        (root, storage)
    }

    #[test]
    fn actor_tag_crud_preserves_assignments_on_rename_and_film_tags() {
        let (_root, storage) = fixture();
        create(&storage, " 精选 ").unwrap();
        create(&storage, "已看").unwrap();
        assert_eq!(list(&storage).unwrap(), vec!["精选", "已看"]);
        assert_eq!(apply_bulk_tag(&storage, &["a".into(), "a".into(), "b".into()], "精选", true).unwrap(), 2);
        rename(&storage, "精选", "新名字").unwrap();
        let links: Vec<(String, String)> = storage.with_connection(|db| db.prepare("SELECT actor_id,tag FROM actor_tag_links ORDER BY actor_id")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()).unwrap();
        assert_eq!(links, vec![("a".into(), "新名字".into()), ("b".into(), "新名字".into())]);
        delete(&storage, "新名字").unwrap();
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM actor_tag_links", [], |r| r.get::<_, i64>(0))).unwrap(), 0);
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM film_tags JOIN tags ON tags.id=film_tags.tag_id WHERE name='精选'", [], |r| r.get::<_, i64>(0))).unwrap(), 1);
        assert_eq!(list(&storage).unwrap(), vec!["已看"]);
    }

    #[test]
    fn actor_tag_rules_match_film_names_and_inventory_limit() {
        let (_root, storage) = fixture();
        create(&storage, "ÄBC").unwrap();
        assert_eq!(create(&storage, "äbc").unwrap_err(), "duplicate_tag");
        assert_eq!(create(&storage, " ").unwrap_err(), "invalid_tag");
        assert_eq!(create(&storage, &"字".repeat(21)).unwrap_err(), "invalid_tag");
        create(&storage, "second").unwrap();
        assert_eq!(rename(&storage, "second", "äbc").unwrap_err(), "duplicate_tag");
        rename(&storage, "ÄBC", "äbc").unwrap();
        create(&storage, "ASCII").unwrap();
        apply_bulk_tag(&storage, &["a".into()], "ASCII", true).unwrap();
        rename(&storage, "ASCII", "ascii").unwrap();
        apply_bulk_tag(&storage, &["a".into()], "ascii", true).unwrap();
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM actor_tag_links WHERE actor_id='a'", [], |r| r.get::<_, i64>(0))).unwrap(), 1);
        for index in 3..30 { create(&storage, &format!("tag{index}")).unwrap(); }
        assert_eq!(create(&storage, "one-more").unwrap_err(), "tag_limit");
        assert_eq!(list(&storage).unwrap().len(), 30);
    }

    #[test]
    fn actor_bulk_tags_validate_all_ids_before_write_and_remove_only_target_link() {
        let (_root, storage) = fixture();
        create(&storage, "第一").unwrap();
        create(&storage, "第二").unwrap();
        apply_bulk_tag(&storage, &["a".into()], "第一", true).unwrap();
        apply_bulk_tag(&storage, &["a".into()], "第二", true).unwrap();
        assert_eq!(apply_bulk_tag(&storage, &["b".into(), "missing".into()], "第一", true).unwrap_err(), "actor_not_found");
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM actor_tag_links WHERE actor_id='b'", [], |r| r.get::<_, i64>(0))).unwrap(), 0);
        assert_eq!(apply_bulk_tag(&storage, &["a".into(), "a".into()], "第一", false).unwrap(), 1);
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT tag FROM actor_tag_links WHERE actor_id='a'", [], |r| r.get::<_, String>(0))).unwrap(), "第二");
        storage.with_connection(|db| db.execute_batch("CREATE TRIGGER reject_actor_tag BEFORE INSERT ON actor_tag_links WHEN NEW.actor_id='b' BEGIN SELECT RAISE(ABORT,'injected failure'); END;")).unwrap();
        assert!(apply_bulk_tag(&storage, &["a".into(), "b".into()], "第一", true).is_err());
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM actor_tag_links WHERE tag='第一'", [], |r| r.get::<_, i64>(0))).unwrap(), 0);
    }

    #[test]
    fn actor_tag_reorder_rejects_stale_inventory_and_delete_protection_failure_preserves_links() {
        let (root, storage) = fixture();
        create(&storage, "甲标签").unwrap(); create(&storage, "乙标签").unwrap();
        reorder(&storage, &["乙标签".into(), "甲标签".into()]).unwrap();
        assert_eq!(list(&storage).unwrap(), vec!["乙标签", "甲标签"]);
        assert_eq!(reorder(&storage, &["乙标签".into(), "乙标签".into()]).unwrap_err(), "stale_tag_order");
        apply_bulk_tag(&storage, &["a".into()], "甲标签", true).unwrap();
        std::fs::write(root.path().join("protections"), b"blocker").unwrap();
        assert_eq!(delete(&storage, "甲标签").unwrap_err(), "protection_failed");
        assert_eq!(storage.with_connection(|db| db.query_row("SELECT COUNT(*) FROM actor_tag_links WHERE actor_id=?1", params!["a"], |r| r.get::<_, i64>(0))).unwrap(), 1);
    }
}
