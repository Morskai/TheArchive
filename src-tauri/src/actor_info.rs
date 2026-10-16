//! Validation for actor.info JSON. Archival IDs are ignored; filmography uses exact filenames.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

type Result<T> = std::result::Result<T, String>;
const LIMIT: usize = 1024 * 1024;

fn invalid() -> String { "info_invalid".into() }

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilmReference {
    pub folder_name: String,
    pub video_files: Vec<String>,
}

/// Preserve filename case and whitespace. Only the order of the complete list is irrelevant.
pub fn filmography(map: &Map<String, Value>) -> Result<Vec<FilmReference>> {
    let Some(value) = map.get("filmography") else { return Ok(vec![]); };
    let mut entries: Vec<FilmReference> = serde_json::from_value(value.clone()).map_err(|_| invalid())?;
    let filename = |name: &str| !name.trim().is_empty() && name.chars().count() <= 255
        && !name.chars().any(char::is_control) && !name.contains(['/', '\\', ':']) && name != "." && name != "..";
    for entry in &mut entries {
        if !filename(&entry.folder_name) || entry.video_files.is_empty() || entry.video_files.iter().any(|name| !filename(name)) {
            return Err(invalid());
        }
        entry.video_files.sort();
        if entry.video_files.windows(2).any(|pair| pair[0] == pair[1]) { return Err(invalid()); }
    }
    entries.sort();
    entries.dedup();
    Ok(entries)
}

pub fn parse(text: &str) -> Result<Map<String, Value>> {
    if text.len() > LIMIT { return Err("info_too_large".into()); }
    let value: Value = serde_json::from_str(text).map_err(|_| invalid())?;
    let map = value.as_object().ok_or_else(invalid)?;
    if map.get("format_version") != Some(&json!(1)) || map.get("kind").and_then(Value::as_str) != Some("actor") {
        return Err(invalid());
    }
    let fields = ["format_version", "kind", "id", "name", "aliases", "birth_month", "height_cm", "cup", "bust_cm", "waist_cm", "hip_cm", "photo", "film_ids", "filmography"];
    if map.keys().any(|key| !fields.contains(&key.as_str())) { return Err(invalid()); }
    for key in ["name", "birth_month", "cup"] {
        if let Some(value) = map.get(key) { string(value, 1000)?; }
    }
    if let Some(value) = map.get("aliases") { strings(value, 200)?; }
    for key in ["height_cm", "bust_cm", "waist_cm", "hip_cm"] {
        if let Some(value) = map.get(key) {
            if !value.is_null() && value.as_str() != Some("") && value.as_u64().is_none() { return Err(invalid()); }
        }
    }
    filmography(map)?;
    Ok(map.clone())
}

fn string(value: &Value, max: usize) -> Result<String> {
    if value.is_null() { return Ok(String::new()); }
    let text = value.as_str().ok_or_else(invalid)?.trim();
    if text.chars().count() > max || text.chars().any(char::is_control) { return Err(invalid()); }
    Ok(text.into())
}

fn strings(value: &Value, max: usize) -> Result<Vec<String>> {
    if value.is_null() { return Ok(vec![]); }
    let array = value.as_array().ok_or_else(invalid)?;
    if array.len() > 500 { return Err(invalid()); }
    let mut seen = BTreeSet::new();
    let mut result = vec![];
    for value in array {
        let text = string(value, max)?;
        if text.is_empty() { return Err(invalid()); }
        if seen.insert(text.clone()) { result.push(text); }
    }
    Ok(result)
}
