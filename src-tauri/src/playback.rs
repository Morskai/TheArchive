//! External launch requests, never media editing or playback-progress inference.
use crate::{catalog, storage::Storage};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSettings {
    pub player_mode: String,
    pub player_path: String,
    pub launch_arguments: String,
    #[serde(default = "default_display")]
    pub internal_display: String,
}
fn default_display() -> String { "overlay".into() }

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            player_mode: "internal".into(),
            player_path: String::new(),
            launch_arguments: String::new(),
            internal_display: default_display(),
        }
    }
}

type Result<T> = std::result::Result<T, String>;

pub fn load_settings(storage: &Storage) -> Result<PlayerSettings> {
    storage
        .with_connection(|db| {
            db.query_row(
                "SELECT mode,program,arguments,internal_display FROM player_settings WHERE singleton=1",
                [],
                |r| {
                    Ok(PlayerSettings {
                        player_mode: r.get(0)?,
                        player_path: r.get(1)?,
                        launch_arguments: r.get(2)?,
                        internal_display: r.get(3)?,
                    })
                },
            )
            .optional()
        })
        .map(|s| {
            let mut settings = s.unwrap_or_default();
            // Retired UI choice: preserve the stored custom program and arguments.
            if settings.player_mode == "system" { settings.player_mode = "internal".into(); }
            settings
        })
        .map_err(|_| "settings_load_failed".into())
}

fn custom_program(settings: &PlayerSettings) -> Result<PathBuf> {
    let path = Path::new(settings.player_path.trim());
    if !path.is_absolute()
        || !path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
    {
        return Err("invalid_program".into());
    }
    let path = path.canonicalize().map_err(|_| "program_unavailable")?;
    if !path.is_file() {
        return Err("invalid_program".into());
    }
    File::open(&path).map_err(|_| "program_unavailable")?;
    Ok(path)
}

pub fn save_settings(storage: &Storage, settings: &PlayerSettings) -> Result<()> {
    if !matches!(settings.player_mode.as_str(), "internal" | "system" | "custom") {
        return Err("invalid_mode".into());
    }
    if !matches!(settings.internal_display.as_str(), "overlay" | "window") { return Err("invalid_mode".into()); }
    if settings.player_mode == "custom" {
        custom_program(settings)?;
        parse_arguments(&settings.launch_arguments)?;
    }
    storage.with_transaction(|tx| {
        tx.execute("INSERT INTO player_settings(singleton,mode,program,arguments,internal_display) VALUES(1,?1,?2,?3,?4) ON CONFLICT(singleton) DO UPDATE SET mode=excluded.mode,program=excluded.program,arguments=excluded.arguments,internal_display=excluded.internal_display",
            params![settings.player_mode, settings.player_path.trim(), settings.launch_arguments, settings.internal_display])?;
        Ok(())
    }).map_err(|_| "settings_save_failed".into())
}

// Windows CRT argument quoting: backslashes only escape immediately before a quote.
// An unmatched quote is rejected in the settings UI instead of silently swallowing argv.
pub fn parse_arguments(value: &str) -> Result<Vec<String>> {
    if value.contains('\0') || value.len() > 16000 {
        return Err("invalid_arguments".into());
    }
    let chars: Vec<char> = value.chars().collect();
    let mut args = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && matches!(chars[i], ' ' | '\t') {
            i += 1;
        }
        if i == chars.len() {
            break;
        }
        let mut arg = String::new();
        let mut quoted = false;
        while i < chars.len() && (quoted || !matches!(chars[i], ' ' | '\t')) {
            let mut slashes = 0;
            while i < chars.len() && chars[i] == '\\' {
                slashes += 1;
                i += 1;
            }
            if i < chars.len() && chars[i] == '"' {
                arg.extend(std::iter::repeat_n('\\', slashes / 2));
                if slashes % 2 == 1 {
                    arg.push('"');
                    i += 1;
                } else if quoted && chars.get(i + 1) == Some(&'"') {
                    arg.push('"');
                    i += 2;
                } else {
                    quoted = !quoted;
                    i += 1;
                }
            } else {
                arg.extend(std::iter::repeat_n('\\', slashes));
                if i < chars.len() && (quoted || !matches!(chars[i], ' ' | '\t')) {
                    arg.push(chars[i]);
                    i += 1;
                }
            }
        }
        if quoted {
            return Err("invalid_arguments".into());
        }
        args.push(arg);
    }
    Ok(args)
}

pub fn resolve_video(storage: &Storage, film_id: &str, video_id: &str) -> Result<PathBuf> {
    let (root, relative, enabled, removed): (Option<String>, String, bool, bool) = storage.with_connection(|db| {
        db.query_row("SELECT l.root_path,o.observed_path,l.enabled,l.removed FROM file_observations o JOIN libraries l ON l.id=o.library_id WHERE o.id=?1 AND o.film_id=?2 AND o.kind='video'", params![video_id,film_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))
    }).map_err(|_| "video_not_linked")?;
    if removed {
        return Err("source_removed".into());
    }
    if !enabled {
        return Err("source_paused".into());
    }
    let root = PathBuf::from(root.ok_or("root_unavailable")?)
        .canonicalize()
        .map_err(|_| "root_unavailable")?;
    let relative = Path::new(&relative);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("path_outside_source".into());
    }
    let file = root
        .join(relative)
        .canonicalize()
        .map_err(|_| "file_unavailable")?;
    if !file.starts_with(&root) {
        return Err("path_outside_source".into());
    }
    if !file.is_file() {
        return Err("file_unavailable".into());
    }
    Ok(file)
}

fn saved_video(
    storage: &Storage,
    film_id: &str,
    video_id: &str,
) -> Result<(String, String, String)> {
    let (source,root,relative,enabled,removed):(String,String,String,bool,bool)=storage.with_connection(|db| db.query_row(
        "SELECT l.id,l.root_path,o.observed_path,l.enabled,l.removed FROM file_observations o JOIN libraries l ON l.id=o.library_id WHERE o.id=?1 AND o.film_id=?2 AND o.kind='video'",
        params![video_id,film_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))
    )).map_err(|_|"video_not_linked")?;
    if removed {
        return Err("source_removed".into());
    }
    if !enabled {
        return Err("source_paused".into());
    }
    if !Path::new(&relative).is_relative()
        || relative.contains('\\')
        || Path::new(&relative)
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err("path_outside_source".into());
    }
    let current = storage
        .with_connection(|db| {
            Ok(crate::artwork::ScanSnapshot::read(db)?.contains(&source, video_id))
        })
        .map_err(|_| "video_not_linked")?;
    if !current {
        return Err("video_not_linked".into());
    }
    Ok((source, root, relative))
}

pub fn current_video_path(storage:&Storage,film_id:&str,video_id:&str)->Result<PathBuf> {
    saved_video(storage,film_id,video_id)?;
    resolve_video(storage,film_id,video_id)
}

pub fn video_size(storage: &Storage, film_id: &str, video_id: &str) -> Result<u64> {
    let (source, _, relative) = saved_video(storage, film_id, video_id)?;
    storage
        .with_connection(|db| {
            let details = crate::artwork::scan_details(db)?;
            Ok(details
                .get(&source)
                .and_then(|d| d.video_sizes.get(&relative))
                .copied())
        })
        .map_err(|_| "size_unavailable")?
        .ok_or_else(|| "size_unavailable".into())
}

#[derive(Debug, Serialize)]
pub struct DirectoryFacts {
    pub path: String,
    pub videos: usize,
    pub images: usize,
    pub subtitles: usize,
}

pub fn directory_facts(storage: &Storage, film_id: &str, video_id: &str) -> Result<DirectoryFacts> {
    let (source, root, relative) = saved_video(storage, film_id, video_id)?;
    let parent = crate::folder_groups::parent(&relative);
    storage
        .with_connection(|db| {
            let snapshot = crate::artwork::ScanSnapshot::read(db)?;
            let mut facts = DirectoryFacts {
                path: Path::new(&root).join(&parent).to_string_lossy().into(),
                videos: 0,
                images: 0,
                subtitles: 0,
            };
            let mut q = db.prepare(
                "SELECT id,observed_path,kind FROM file_observations WHERE library_id=?1",
            )?;
            for row in q.query_map([&source], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })? {
                let (id, path, kind) = row?;
                if !snapshot.contains(&source, &id) || crate::folder_groups::parent(&path) != parent
                {
                    continue;
                }
                match kind.as_str() {
                    "video" => facts.videos += 1,
                    "image" => facts.images += 1,
                    "subtitle" => facts.subtitles += 1,
                    _ => {}
                }
            }
            Ok(facts)
        })
        .map_err(|_| "directory_unavailable".into())
}
pub fn set_preferred(
    storage: &Storage,
    film_id: &str,
    video_id: &str,
    preferred: bool,
) -> Result<()> {
    // Choosing a stored file changes only its playback preference.
    storage.with_transaction(|tx| {
        tx.query_row("SELECT id FROM file_observations WHERE id=?1 AND film_id=?2 AND kind='video'", params![video_id,film_id], |r| r.get::<_,String>(0))?;
        if preferred {
            tx.execute("UPDATE file_versions SET preferred=0 WHERE observation_id IN (SELECT id FROM file_observations WHERE film_id=?1)", [film_id])?;
        }
        tx.execute("INSERT INTO file_versions(observation_id,preferred) VALUES(?1,?2) ON CONFLICT(observation_id) DO UPDATE SET preferred=excluded.preferred", params![video_id,preferred])?;
        Ok(())
    }).map_err(|_| "preferred_save_failed".into())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchEvent {
    pub id: String,
    pub film_id: String,
    pub video_id: Option<String>,
    pub launched_at: String,
}

#[derive(Serialize)]
pub struct History {
    pub films: Vec<catalog::CatalogFilm>,
    pub events: Vec<LaunchEvent>,
    pub film_count: usize,
    pub source_count: usize,
}

pub fn load_history(storage: &Storage) -> Result<History> {
    let films = catalog::list_all_films(storage).map_err(|_| "history_load_failed")?;
    let events = storage.with_connection(|db| {
        let mut q = db.prepare("SELECT e.id,COALESCE(a.film_id,e.film_id),e.observation_id,e.launched_at FROM player_launch_events e LEFT JOIN film_aliases a ON a.id=e.film_id ORDER BY e.launched_at DESC,e.id")?;
        q.query_map([], |r| Ok(LaunchEvent { id:r.get(0)?,film_id:r.get(1)?,video_id:r.get(2)?,launched_at:r.get(3)? }))?.collect()
    }).map_err(|_| "history_load_failed")?;
    let film_count = catalog::list_films(storage)
        .map_err(|_| "history_load_failed")?
        .len();
    let source_count = storage
        .with_connection(|db| {
            db.query_row(
                "SELECT COUNT(*) FROM libraries WHERE enabled=1 AND removed=0",
                [],
                |r| r.get(0),
            )
        })
        .map_err(|_| "history_load_failed")?;
    Ok(History {
        films,
        events,
        film_count,
        source_count,
    })
}

pub fn clear_history(storage: &Storage) -> Result<()> {
    storage
        .create_protection()
        .map_err(|_| "protection_failed")?;
    storage
        .with_transaction(|tx| {
            tx.execute("DELETE FROM player_launch_events", [])?;
            Ok(())
        })
        .map_err(|_| "history_clear_failed".into())
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOutcome {
    pub history_saved: bool,
}

#[derive(Debug)]
pub enum LaunchTarget {
    Internal { video: PathBuf, display: String },
    System(PathBuf),
    Custom {
        program: PathBuf,
        arguments: Vec<String>,
        video: PathBuf,
    },
}

type CachedRequest = (String, String, Result<LaunchOutcome>);
#[derive(Default)]
pub struct PlaybackSession {
    requests: Mutex<HashMap<String, CachedRequest>>,
}

impl PlaybackSession {
    pub fn reset(&self) { if let Ok(mut requests) = self.requests.lock() { requests.clear(); } }
    pub fn launch(
        &self,
        storage: &Storage,
        request_id: &str,
        film_id: &str,
        video_id: &str,
        launcher: impl FnOnce(&LaunchTarget) -> Result<()>,
    ) -> Result<LaunchOutcome> {
        self.launch_with_display(storage, request_id, film_id, video_id, None, launcher)
    }
    pub fn launch_with_display(
        &self, storage: &Storage, request_id: &str, film_id: &str, video_id: &str,
        internal_display: Option<&str>, launcher: impl FnOnce(&LaunchTarget) -> Result<()>,
    ) -> Result<LaunchOutcome> {
        if request_id.len() < 16
            || request_id.len() > 80
            || !request_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err("invalid_request".into());
        }
        // Serialize validation, launch and event persistence. Replayed IDs never launch twice,
        // including history-save failures and events cleared in this app session.
        let mut requests = self.requests.lock().map_err(|_| "launch_failed")?;
        if let Some((film, video, outcome)) = requests.get(request_id) {
            if film != film_id || video != video_id {
                return Err("invalid_request".into());
            }
            return outcome.clone();
        }
        let outcome = (|| {
            let existing: Option<(String, Option<String>)> = storage
                .with_connection(|db| {
                    db.query_row(
                        "SELECT film_id,observation_id FROM player_launch_events WHERE id=?1",
                        [request_id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()
                })
                .map_err(|_| "history_load_failed")?;
            if let Some((film, video)) = existing {
                if film != film_id || video.as_deref() != Some(video_id) {
                    return Err("invalid_request".into());
                }
                return Ok(LaunchOutcome {
                    history_saved: true,
                });
            }
            let video = resolve_video(storage, film_id, video_id)?;
            let mut settings = load_settings(storage)?;
            if let Some(display) = internal_display { settings.player_mode = "internal".into(); settings.internal_display = display.into(); }
            let target = if settings.player_mode == "internal" {
                LaunchTarget::Internal { video, display: settings.internal_display }
            } else if settings.player_mode == "system" {
                LaunchTarget::System(video)
            } else {
                LaunchTarget::Custom {
                    program: custom_program(&settings)?,
                    arguments: parse_arguments(&settings.launch_arguments)?,
                    video,
                }
            };
            launcher(&target)?;
            let saved = storage.with_transaction(|tx| {
                tx.execute("INSERT INTO player_launch_events(id,film_id,observation_id,launched_at) VALUES(?1,?2,?3,strftime('%Y-%m-%dT%H:%M:%fZ','now'))", params![request_id,film_id,video_id])?;
                Ok(())
            }).is_ok();
            Ok(LaunchOutcome {
                history_saved: saved,
            })
        })();
        requests.insert(
            request_id.into(),
            (film_id.into(), video_id.into(), outcome.clone()),
        );
        outcome
    }
}

pub fn launch_external(target: &LaunchTarget) -> Result<()> {
    match target {
        LaunchTarget::Internal { .. } => Err("internal_player_unavailable".into()),
        LaunchTarget::System(video) => open_with_windows(video),
        LaunchTarget::Custom {
            program,
            arguments,
            video,
        } => std::process::Command::new(program)
            .args(arguments)
            .arg(video)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|_| "launch_failed".into()),
    }
}

pub fn open_folder(storage: &Storage, film_id: &str, video_id: &str) -> Result<()> {
    open_folder_with(storage, film_id, video_id, open_with_windows)
}

pub fn open_data_directory(storage: &Storage) -> Result<()> {
    open_data_directory_with(storage, open_with_windows)
}
pub fn open_data_directory_with(storage: &Storage, opener: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let path = storage.data_directory().canonicalize().map_err(|_| "directory_unavailable")?;
    if !path.is_dir() { return Err("directory_unavailable".into()); }
    opener(&path)
}

pub fn open_folder_with(
    storage: &Storage,
    film_id: &str,
    video_id: &str,
    opener: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let file = resolve_video(storage, film_id, video_id)?;
    opener(file.parent().ok_or("file_unavailable")?)
}

#[cfg(windows)]
fn open_with_windows(path: &Path) -> Result<()> {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: *mut c_void,
            verb: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, mode: u32) -> i32;
        fn CoUninitialize();
    }
    // Shell associations expect ordinary Win32 paths; preserve UTF-16 while
    // converting only the canonical extended prefix, including UNC shares.
    let mut file: Vec<u16> = path.as_os_str().encode_wide().collect();
    let unc: Vec<u16> = "\\\\?\\UNC\\".encode_utf16().collect();
    let extended: Vec<u16> = "\\\\?\\".encode_utf16().collect();
    if file.starts_with(&unc) {
        file.splice(..unc.len(), "\\\\".encode_utf16());
    } else if file.starts_with(&extended) {
        file.drain(..extended.len());
    }
    file.push(0);
    let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    // Initialize COM for this blocking worker; no shell command or media argument string.
    let initialized = unsafe { CoInitializeEx(std::ptr::null_mut(), 2) } >= 0;
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    if initialized {
        unsafe {
            CoUninitialize();
        }
    }
    match result {
        n if n > 32 => Ok(()),
        31 => Err("no_file_association".into()),
        5 => Err("access_denied".into()),
        _ => Err("launch_failed".into()),
    }
}

#[cfg(not(windows))]
fn open_with_windows(_path: &Path) -> Result<()> {
    Err("unsupported_platform".into())
}
