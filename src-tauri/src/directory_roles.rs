//! One bounded directory contract shared by scans, actors and current problems.
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Source,
    Actor,
    Film,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DirectoryRole {
    pub directory: String,
    pub role: Role,
}

pub fn key(path: &Path) -> String {
    let value = path
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_owned();
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

pub(crate) fn identity(value: &str) -> String {
    let value = value.replace('\\', "/").trim_end_matches('/').to_owned();
    let value = if let Some(unc) = value
        .strip_prefix("//?/UNC/")
        .or_else(|| value.strip_prefix("//?/unc/"))
    {
        format!("//{unc}")
    } else {
        value.strip_prefix("//?/").unwrap_or(&value).to_owned()
    };
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

pub fn equivalent(left: &str, right: &str) -> bool {
    identity(left) == identity(right)
}
