//! Read-only evidence collection for an explicitly selected directory.
//! This module has no database access or media writes.
use crate::directory_roles::{DirectoryRole, Role};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub struct InventoryReport {
    pub scan_mode: String,
    pub videos: Vec<VideoEntry>,
    pub assets: Vec<AssetEntry>,
    pub containers: Vec<CandidateContainer>,
    pub issues: Vec<ScanIssue>,
    pub directories: Vec<String>,
    pub cover_checks: Vec<CoverCheck>,
    pub roles: Vec<DirectoryRole>,
    pub video_sizes: BTreeMap<String, u64>,
    pub directory_created_at: BTreeMap<String, u64>,
    pub directory_modified_at: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoverCheck {
    pub relative_path: String,
    pub state: String,
    pub digest: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct VideoEntry {
    pub relative_path: String,
    pub directory: String,
    pub extension: String,
    pub clues: NameClues,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NameClues {
    pub directory_names: Vec<String>,
    pub file_stem: String,
    pub bracketed: Vec<String>,
    /// Unverified literal tokens; no film identity or equivalence is inferred.
    pub code_candidates: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Image,
    Subtitle,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AssetEntry {
    pub relative_path: String,
    pub directory: String,
    pub kind: AssetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Prompt {
    MultipleVideos,
    NoBracketedCode,
    NoCodeCandidate,
    ConflictingCodes,
    MultipleSubtitles,
    RepeatedCodeAcrossDirectories,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CandidateContainer {
    pub directory: String,
    pub video_paths: Vec<String>,
    pub asset_paths: Vec<String>,
    pub prompts: Vec<Prompt>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ScanIssue {
    pub relative_path: String,
    pub kind: String,
    pub detail: String,
}

pub(crate) fn is_video_extension(extension: &str) -> bool {
    matches!(
        extension,
        "mp4" | "mkv" | "mov" | "avi" | "wmv" | "m4v" | "webm" | "ts" | "mpg" | "rm" | "rmvb"
    )
}

pub(crate) fn asset_kind(extension: &str) -> Option<AssetKind> {
    match extension {
        "jpg" | "jpeg" | "png" | "webp" | "avif" => Some(AssetKind::Image),
        "srt" | "ass" | "ssa" | "vtt" | "sub" => Some(AssetKind::Subtitle),
        _ => None,
    }
}

pub fn inventory(root: &Path) -> io::Result<InventoryReport> {
    inventory_with_rules(root, &BTreeSet::new(), &BTreeSet::new())
}

pub fn inventory_with_rules(
    root: &Path,
    known: &BTreeSet<String>,
    ignored: &BTreeSet<String>,
) -> io::Result<InventoryReport> {
    collect(root, known, ignored).inspect_err(|error| crate::diagnostics::record_error("scan.inventory", error))
}

fn collect(
    root: &Path,
    known: &BTreeSet<String>,
    ignored: &BTreeSet<String>,
) -> io::Result<InventoryReport> {
    // The unused argument preserves the injected-reader signature used by scan
    // concurrency checks. Film scanning has no actor identity input.
    let _ = known;
    let root = root.canonicalize()?;
    if !root.is_dir() {
        return Err(io::Error::other("root is not a directory"));
    }
    let mut report = empty_report();
    let mut pending = vec![root.clone()];
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(e) => e,
            Err(e) => {
                issue(
                    &mut report,
                    &root,
                    &directory,
                    "read_directory",
                    e.to_string(),
                );
                continue;
            }
        };
        let mut files = Vec::new();
        for item in entries {
            let entry = match item {
                Ok(e) => e,
                Err(e) => {
                    issue(&mut report, &root, &directory, "read_entry", e.to_string());
                    continue;
                }
            };
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let kind = match entry.file_type() {
                Ok(k) => k,
                Err(e) => {
                    issue(&mut report, &root, &path, "file_type", e.to_string());
                    continue;
                }
            };
            if (kind.is_dir() || kind.is_symlink()) && ignored.contains(&name.to_lowercase()) {
                continue;
            }
            if kind.is_symlink() {
                issue(
                    &mut report,
                    &root,
                    &path,
                    "link_skipped",
                    "link or junction was not followed".into(),
                );
                continue;
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if fs::symlink_metadata(&path)?.file_attributes() & 0x400 != 0 {
                    issue(
                        &mut report,
                        &root,
                        &path,
                        "link_skipped",
                        "junction was not followed".into(),
                    );
                    continue;
                }
            }
            if kind.is_dir() {
                pending.push(path);
                continue;
            }
            if kind.is_file() {
                files.push(path);
            }
        }
        files.sort();
        let dir = relative(&root, &directory);
        report.directories.push(dir.clone());
        if let Ok(metadata) = fs::symlink_metadata(&directory) {
            for (time, target) in [
                (metadata.created(), &mut report.directory_created_at),
                (metadata.modified(), &mut report.directory_modified_at),
            ] {
                if let Some(t) = time
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                {
                    target.insert(dir.clone(), t.as_millis() as u64);
                }
            }
        }
        let videos: Vec<_> = files
            .iter()
            .filter(|p| {
                is_video_extension(
                    &p.extension()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_ascii_lowercase(),
                )
            })
            .collect();
        report.roles.push(DirectoryRole {
            directory: dir.clone(),
            role: if videos.is_empty() {
                Role::Source
            } else {
                Role::Film
            },
        });
        let stems: BTreeSet<_> = videos
            .iter()
            .map(|p| {
                p.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
            })
            .collect();
        for path in files {
            let rel = relative(&root, &path);
            let ext = path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            if is_video_extension(&ext) {
                let metadata = match fs::metadata(&path) {
                    Ok(m) => m,
                    Err(e) => {
                        issue(&mut report, &root, &path, "video_metadata", e.to_string());
                        continue;
                    }
                };
                report.video_sizes.insert(rel.clone(), metadata.len());
                let directory_names = directory
                    .strip_prefix(&root)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                let folder = directory.file_name().unwrap_or_default().to_string_lossy();
                let bracketed = bracket_tokens(&folder);
                let code_candidates = bracketed
                    .iter()
                    .find(|t| !t.trim().is_empty())
                    .map(|t| vec![t.trim().to_owned()])
                    .unwrap_or_default();
                report.videos.push(VideoEntry {
                    relative_path: rel,
                    directory: dir.clone(),
                    extension: ext,
                    clues: NameClues {
                        directory_names,
                        file_stem: path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        bracketed,
                        code_candidates,
                    },
                });
            } else if let Some(kind) = asset_kind(&ext) {
                if kind == AssetKind::Image {
                    if crate::artwork::named(&rel, crate::artwork::FILM_COVER)
                        || stems.contains(
                            &path
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_lowercase(),
                        )
                    {
                        report
                            .cover_checks
                            .push(crate::covers::inspect_cover(&root, &rel));
                    }
                }
                report.assets.push(AssetEntry {
                    relative_path: rel,
                    directory: dir.clone(),
                    kind,
                });
            }
        }
    }
    report
        .videos
        .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    report
        .assets
        .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    report.directories.sort();
    report.roles.sort_by(|a, b| a.directory.cmp(&b.directory));
    report
        .cover_checks
        .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    build_containers(&mut report);
    Ok(report)
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn issue(report: &mut InventoryReport, root: &Path, path: &Path, kind: &str, detail: String) {
    report.issues.push(ScanIssue {
        relative_path: relative(root, path),
        kind: kind.into(),
        detail,
    });
}

fn bracket_tokens(name: &str) -> Vec<String> {
    name.split('[')
        .skip(1)
        .filter_map(|part| part.split_once(']').map(|(token, _)| token.to_owned()))
        .collect()
}

fn build_containers(report: &mut InventoryReport) {
    let mut by_dir: BTreeMap<String, Vec<&VideoEntry>> = BTreeMap::new();
    for video in &report.videos {
        by_dir
            .entry(video.directory.clone())
            .or_default()
            .push(video);
    }
    let mut occurrences: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (directory, videos) in &by_dir {
        for video in videos {
            for code in &video.clues.code_candidates {
                occurrences
                    .entry(code.clone())
                    .or_default()
                    .insert(directory.clone());
            }
        }
    }
    for (directory, videos) in by_dir {
        let mut prompts = BTreeSet::new();
        if videos.len() > 1 {
            prompts.insert(Prompt::MultipleVideos);
        }
        if !videos.iter().any(|v| {
            v.clues.directory_names.last().is_some_and(|name| {
                bracket_tokens(name)
                    .iter()
                    .any(|token| !token.trim().is_empty())
            })
        }) {
            prompts.insert(Prompt::NoBracketedCode);
        }
        let codes: BTreeSet<&str> = videos
            .iter()
            .flat_map(|v| v.clues.code_candidates.iter().map(String::as_str))
            .collect();
        if codes.is_empty() {
            prompts.insert(Prompt::NoCodeCandidate);
        }
        if codes.len() > 1 {
            prompts.insert(Prompt::ConflictingCodes);
        }
        if codes
            .iter()
            .any(|code| occurrences.get(*code).is_some_and(|dirs| dirs.len() > 1))
        {
            prompts.insert(Prompt::RepeatedCodeAcrossDirectories);
        }
        let assets: Vec<_> = report
            .assets
            .iter()
            .filter(|a| a.directory == directory)
            .collect();
        if assets
            .iter()
            .filter(|a| a.kind == AssetKind::Subtitle)
            .count()
            > 1
        {
            prompts.insert(Prompt::MultipleSubtitles);
        }
        report.containers.push(CandidateContainer {
            directory,
            video_paths: videos.iter().map(|v| v.relative_path.clone()).collect(),
            asset_paths: assets.iter().map(|a| a.relative_path.clone()).collect(),
            prompts: prompts.into_iter().collect(),
        });
    }
}

fn empty_report() -> InventoryReport {
    InventoryReport {
        // Kept in serialized scan evidence for historical backup readability.
        scan_mode: "general".into(),
        videos: vec![],
        assets: vec![],
        containers: vec![],
        issues: vec![],
        directories: vec![],
        cover_checks: vec![],
        roles: vec![],
        video_sizes: BTreeMap::new(),
        directory_created_at: BTreeMap::new(),
        directory_modified_at: BTreeMap::new(),
    }
}
