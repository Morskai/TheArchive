//! Problem IDs and saved directory choices only. Never accepts a caller-provided path.
use crate::{artwork, problems, storage::Storage};
use rusqlite::OptionalExtension;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;

#[derive(Serialize)]
pub struct ProblemWithDirectories {
    #[serde(flatten)]
    pub problem: problems::Problem,
    pub directory_options: Vec<String>,
    pub directory_targets: Vec<DirectoryTarget>,
}
#[derive(Serialize)]
pub struct DirectoryTarget {
    pub source_id: String,
    pub relative: String,
    pub label: String,
}
struct Target {
    source_id: String,
    root: String,
    relative: String,
    label: String,
}

fn normal_relative(relative: &str) -> bool {
    !relative.contains('\\')
        && !relative.contains(':')
        && !Path::new(relative).is_absolute()
        && Path::new(relative)
            .components()
            .all(|p| matches!(p, Component::Normal(_)))
}
fn targets_from_saved(
    db: &rusqlite::Connection,
    problem: &problems::Problem,
    snapshots: &artwork::ScanSnapshot,
    details: &BTreeMap<String, artwork::ScanDetails>,
) -> rusqlite::Result<Vec<Target>> {
    let mut result = BTreeMap::new();
    for source in &problem.sources {
        let root: Option<String> = db
            .query_row(
                "SELECT root_path FROM libraries WHERE id=?1 AND enabled=1 AND removed=0",
                [&source.id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let Some(root) = root else { continue };
        let mut relatives = Vec::new();
        match problem.kind.as_str() {
            "film_missing_cover" => {
                if let Some(detail) = details.get(&source.id) {
                    relatives.extend(problem.directories.iter().filter(|dir| {
                        detail.roles.iter().any(|role| {
                            role.directory == **dir
                                && role.role == crate::directory_roles::Role::Film
                        })
                    }).cloned());
                }
                for video in &problem.videos {
                    let saved:Option<String>=db.query_row("SELECT observed_path FROM file_observations WHERE id=?1 AND library_id=?2 AND kind='video'",rusqlite::params![video.id,source.id],|r|r.get(0)).optional()?;
                    if snapshots.contains(&source.id, &video.id) {
                        if let Some(path) = saved {
                            relatives.push(
                                path.rsplit_once('/')
                                    .map(|(parent, _)| parent.to_owned())
                                    .unwrap_or_default(),
                            );
                        }
                    }
                }
            }
            "film_code_conflict" => {
                if let Some(detail) = details.get(&source.id) {
                    for directory in &problem.directories {
                        if detail.directories.contains(directory) {
                            relatives.push(directory.clone());
                        } else if detail.directories.iter().any(|d| d.is_empty())
                            && Path::new(&root)
                                .file_name()
                                .is_some_and(|name| name.to_string_lossy() == *directory)
                        {
                            relatives.push(String::new());
                        }
                    }
                }
            }
            _ => {}
        }
        for relative in relatives {
            if !normal_relative(&relative) {
                continue;
            }
            let label = if relative.is_empty() {
                source.name.clone()
            } else {
                relative.clone()
            };
            result.insert(
                (source.id.clone(), relative.clone()),
                Target {
                    source_id: source.id.clone(),
                    root: root.clone(),
                    relative,
                    label,
                },
            );
        }
    }
    Ok(result.into_values().collect())
}
fn targets(storage: &Storage, problem: &problems::Problem) -> Result<Vec<Target>> {
    storage
        .with_connection(|db| {
            let snapshots = artwork::ScanSnapshot::read(db)?;
            let details = artwork::scan_details(db)?;
            targets_from_saved(db, problem, &snapshots, &details)
        })
        .map_err(|_| "problems_load_failed".into())
}
pub fn list(storage: &Storage) -> Result<Vec<ProblemWithDirectories>> {
    let problems = problems::list(storage)?;
    if problems.is_empty() {
        return Ok(Vec::new());
    }
    storage
        .with_connection(|db| {
            // The entire batch shares saved evidence; opening a directory still revalidates afresh.
            let snapshots = artwork::ScanSnapshot::read(db)?;
            let details = artwork::scan_details(db)?;
            problems
                .into_iter()
                .map(|problem| {
                    let saved_targets = targets_from_saved(db, &problem, &snapshots, &details)?;
                    let directory_options = saved_targets.iter().map(|t| t.label.clone()).collect();
                    let directory_targets = saved_targets
                        .into_iter()
                        .map(|t| DirectoryTarget {
                            source_id: t.source_id,
                            relative: t.relative,
                            label: t.label,
                        })
                        .collect();
                    Ok(ProblemWithDirectories {
                        problem,
                        directory_options,
                        directory_targets,
                    })
                })
                .collect()
        })
        .map_err(|_| "problems_load_failed".into())
}
fn redirected(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
fn resolve(target: &Target) -> Result<PathBuf> {
    let root = Path::new(&target.root);
    let meta = fs::symlink_metadata(root).map_err(|_| "root_unavailable")?;
    if redirected(&meta) || !meta.is_dir() {
        return Err("path_outside_source".into());
    }
    let bound = root.canonicalize().map_err(|_| "root_unavailable")?;
    let mut folder = root.to_path_buf();
    for part in Path::new(&target.relative).components() {
        let Component::Normal(part) = part else {
            return Err("path_outside_source".into());
        };
        folder.push(part);
        let meta = fs::symlink_metadata(&folder).map_err(|_| "directory_unavailable")?;
        if redirected(&meta) {
            return Err("path_outside_source".into());
        }
        if !meta.is_dir() {
            return Err("directory_unavailable".into());
        }
    }
    let folder = folder.canonicalize().map_err(|_| "directory_unavailable")?;
    if !folder.starts_with(bound) {
        return Err("path_outside_source".into());
    }
    Ok(folder)
}
pub fn open_directory_with(
    storage: &Storage,
    id: &str,
    index: usize,
    opener: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let problem = problems::list(storage)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("problem_not_found")?;
    let choices = targets(storage, &problem)?;
    let target = choices.get(index).ok_or("directory_unavailable")?;
    opener(&resolve(target)?)
}
pub fn open_directory(storage: &Storage, id: &str, index: usize) -> Result<()> {
    open_directory_with(storage, id, index, |folder| {
        #[cfg(windows)]
        {
            std::process::Command::new("explorer.exe")
                .arg(folder)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map(|_| ())
                .map_err(|_| "launch_failed".into())
        }
        #[cfg(not(windows))]
        {
            let _ = folder;
            Err("launch_failed".into())
        }
    })
}
