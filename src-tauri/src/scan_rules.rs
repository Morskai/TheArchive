//! Persisted scan settings. Names exclude child directories, never library originals.
use crate::storage::Storage;
use std::collections::BTreeSet;

pub fn load(storage: &Storage) -> Result<Vec<String>, String> {
    storage
        .with_connection(|db| {
            let mut query =
                db.prepare("SELECT name FROM ignored_directory_names ORDER BY position")?;
            query.query_map([], |row| row.get(0))?.collect()
        })
        .map_err(|_| "scan_rules_load_failed".into())
}

pub fn keys(storage: &Storage) -> Result<BTreeSet<String>, String> {
    load(storage).map(|names| names.into_iter().map(|name| name.to_lowercase()).collect())
}

pub fn save(storage: &Storage, names: &[String]) -> Result<(), String> {
    if names.len() > 100 {
        return Err("scan_rules_invalid".into());
    }
    let mut unique = BTreeSet::new();
    let mut normalized = Vec::new();
    for name in names {
        let name = name.trim();
        let stem = name.split('.').next().unwrap_or_default().to_uppercase();
        let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'));
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.ends_with('.')
            || name.encode_utf16().count() > 255
            || name
                .chars()
                .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
            || reserved
            || !unique.insert(name.to_lowercase())
        {
            return Err("scan_rules_invalid".into());
        }
        normalized.push(name.to_owned());
    }
    storage
        .with_transaction(|tx| {
            tx.execute("DELETE FROM ignored_directory_names", [])?;
            for (position, name) in normalized.iter().enumerate() {
                tx.execute(
                    "INSERT INTO ignored_directory_names(position,name,name_key) VALUES(?1,?2,?3)",
                    rusqlite::params![position as i64, name, name.to_lowercase()],
                )?;
            }
            Ok(())
        })
        .map_err(|_| "scan_rules_save_failed".into())
}
