use rusqlite::Connection;

/// Remove schema-19 path storage before synthesizing an older database.
#[allow(dead_code)]
pub fn remove_actor_v19(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("DROP TABLE IF EXISTS actor_tag_links; DROP TABLE IF EXISTS actor_tags; DROP TABLE IF EXISTS actor_image_sources;")
}

/// Recreate both retired structures and photo triggers for a real schema-17 fixture.
#[allow(dead_code)] // Each integration target imports only the fixtures it needs.
pub fn restore_actor_v17(db: &Connection) -> rusqlite::Result<()> {
    remove_actor_v19(db)?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS actor_photos(actor_id TEXT PRIMARY KEY REFERENCES actors(id) ON DELETE CASCADE,jpeg BLOB NOT NULL,fingerprint TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS actor_import_sources(directory_key TEXT PRIMARY KEY,actor_id TEXT NOT NULL REFERENCES actors(id) ON DELETE CASCADE,fingerprint TEXT NOT NULL,external_id TEXT);
        CREATE TRIGGER IF NOT EXISTS actor_photo_insert_time AFTER INSERT ON actor_photos BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.actor_id; END;
        CREATE TRIGGER IF NOT EXISTS actor_photo_update_time AFTER UPDATE ON actor_photos WHEN OLD.fingerprint IS NOT NEW.fingerprint BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=NEW.actor_id; END;
        CREATE TRIGGER IF NOT EXISTS actor_photo_delete_time AFTER DELETE ON actor_photos BEGIN UPDATE actors SET modified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=OLD.actor_id; END;
        PRAGMA user_version=17;")
}

/// Synthetic downgrade fixtures must remove the new schema before exercising a
/// historical migration. This helper is never used against runtime data.
pub fn remove_actor_v17(db: &Connection) -> rusqlite::Result<()> {
    remove_actor_v19(db)?;
    let mut query = db.prepare(
        "SELECT name FROM sqlite_master WHERE type='trigger' AND name LIKE 'actor_%_time'",
    )?;
    let names = query
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(query);
    for name in names {
        db.execute_batch(&format!("DROP TRIGGER \"{}\";", name.replace('"', "\"\"")))?;
    }
    db.execute_batch(
        "DROP TABLE IF EXISTS actor_photos; DROP TABLE IF EXISTS actor_import_sources;",
    )?;
    let present: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('actors') WHERE name='modified_at')",
        [],
        |row| row.get(0),
    )?;
    if present {
        db.execute_batch("ALTER TABLE actors DROP COLUMN modified_at;")?;
    }
    Ok(())
}
