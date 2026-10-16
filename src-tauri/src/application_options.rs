use crate::storage::Storage;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationOptions {
    pub close_window_exits: bool,
    // Compatibility only: old clients and backups cannot revive removed monitoring.
    #[serde(skip_serializing, deserialize_with = "ignore_legacy_monitor")]
    pub monitor_libraries: bool,
}
fn ignore_legacy_monitor<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    let _ = bool::deserialize(deserializer)?;
    Ok(false)
}
impl Default for ApplicationOptions {
    fn default() -> Self {
        Self {
            close_window_exits: false,
            monitor_libraries: false,
        }
    }
}
pub fn load(storage: &Storage) -> Result<ApplicationOptions, String> {
    storage.with_connection(|db| db.query_row("SELECT close_window_exits FROM application_options WHERE singleton=1", [], |r| Ok(ApplicationOptions { close_window_exits:r.get(0)?,monitor_libraries:false })).optional())
        .map(|row| row.unwrap_or_default()).map_err(|_| "options_load_failed".into())
}
pub fn save(storage: &Storage, options: &ApplicationOptions) -> Result<(), String> {
    storage.with_transaction(|tx| {
        tx.execute("INSERT INTO application_options(singleton,close_window_exits,monitor_libraries) VALUES(1,?1,0) ON CONFLICT(singleton) DO UPDATE SET close_window_exits=excluded.close_window_exits,monitor_libraries=0", rusqlite::params![options.close_window_exits])?;
        Ok(())
    }).map_err(|_| "options_save_failed".into())
}
