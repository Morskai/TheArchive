//! Bounded, local diagnostic records. Never format request payloads, errors or panic payloads.
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    error::Error,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const MAX_FILE_BYTES: u64 = 5_000_000;
pub const MAX_FILES: usize = 5;
const MAX_EVENT_BYTES: usize = 4096;
const MAX_REPEAT: usize = 5;
static SERIAL: AtomicU64 = AtomicU64::new(1);
static RUNTIME: OnceLock<RuntimeLog> = OnceLock::new();
thread_local! { static CONTEXT: RefCell<Option<String>> = const { RefCell::new(None) }; }

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
pub fn operation_id() -> String {
    format!(
        "op-{:x}-{:x}",
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    )
}
fn valid_id(value: &str) -> bool {
    value.starts_with("op-")
        && value.len() <= 80
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

// Only fixed application identifiers can cross the JS boundary or appear in an exported record.
fn known_operation(value: &str) -> bool {
    if is_image_operation(value) {
        return true;
    }
    if matches!(
        value,
        "load_application_options"
            | "save_application_options"
            | "resolve_window_close"
            | "load_translation_settings"
            | "save_translation_settings"
            | "save_translation_key"
            | "load_translation_key"
            | "delete_translation_key"
            | "preview_title_translations"
            | "edit_film_title"
            | "translation_job_status"
            | "cancel_title_translation"
            | "window_titlebar_action"
            | "load_interface_preferences"
            | "save_interface_preferences"
            | "load_interface_language"
            | "save_interface_language"
            | "load_ignored_directory_names"
            | "save_ignored_directory_names"
            | "open_problem_directory"
            | "mark_library_problems"
            | "restore_library_problems"
            | "clear_library_problems"
            | "list_actor_tags"
            | "create_actor_tag"
            | "rename_actor_tag"
            | "delete_actor_tag"
            | "reorder_actor_tags"
            | "apply_actor_bulk_tag"
            | "choose_actor_photo"
            | "choose_actor_import_root"
            | "choose_actor_export_destination"
            | "set_actor_favorite"
            | "list_cast_candidates"
            | "add_film_cast_actor"
            | "remove_film_cast_actor"
            | "open_library_source_directory"
            | "save_actor_profile"
            | "set_film_code"
            | "load_video_size"
            | "load_video_resolution"
            | "load_directory_facts"
            | "load_player_settings"
            | "application_data_directory"
            | "save_player_settings"
            | "launch_film_video"
            | "open_film_folder"
            | "set_preferred_video"
            | "load_launch_history"
            | "clear_launch_history"
            | "list_real_tags"
            | "read_edit_clipboard"
            | "write_edit_clipboard"
            | "create_real_tag"
            | "reorder_real_tags"
            | "delete_real_tag"
            | "rename_real_tag"
            | "set_real_favorite"
            | "set_real_rating"
            | "set_real_film_tags"
            | "apply_real_bulk_tag"
            | "list_data_backups"
            | "delete_data_backup"
            | "list_library_sources"
            | "choose_library_root"
            | "cancel_library_preview"
    ) {
        return true;
    }
    matches!(
        value,
        "startup"
            | "database.open"
            | "database.read"
            | "database.transaction"
            | "database.migration"
            | "database.backup"
            | "database.restore"
            | "scan.inventory"
            | "scan.recovery"
            | "scan.artwork"
            | "actor.import"
            | "actor.export"
            | "translation.batch"
            | "window.tray"
            | "window.title"
            | "interface_language.load"
            | "interface_language.event"
            | "application_options.restore"
            | "shutdown"
            | "application"
            | "initialize_library_session"
            | "preview_library_source"
            | "confirm_library_source"
            | "change_library_source_state"
            | "preview_actor_import"
            | "confirm_actor_scan_add"
            | "preview_actor_export"
            | "confirm_actor_export"
            | "save_managed_actor"
            | "create_data_backup"
            | "restore_data_backup"
            | "clear_user_data"
            | "clear_generated_cache"
            | "start_title_translation"
            | "test_translation_connection"
            | "clear_translation_memory"
            | "frontend_exception"
            | "load_real_films"
            | "load_actor_catalog"
            | "load_library_problems"
            | "load_film_cover"
            | "load_actor_portrait"
            | "load_video_cover"
            | "read_image_file"
            | "launch_film"
            | "delete_actors"
            | "delete_real_films"
            | "ipc"
    )
}
fn known_count(value: &str) -> bool {
    matches!(
        value,
        "video_count"
            | "issue_count"
            | "unreadable_directory_count"
            | "video_directory_count"
            | "image_count"
            | "subtitle_count"
            | "discovered"
            | "inserted"
            | "missing_count"
            | "related_retained"
            | "artwork_failed"
            | "imported"
            | "updated"
            | "skipped"
            | "failed"
            | "conflicts"
            | "available"
            | "existing"
            | "not_found"
            | "exported"
            | "items"
            | "total"
            | "completed"
            | "succeeded"
            | "reused"
            | "api_requests"
            | "savedSources"
            | "schema"
            | "suppressed"
            | "cancelled"
    )
}
fn is_image_operation(value: &str) -> bool {
    matches!(
        value,
        "load_film_cover"
            | "load_actor_portrait"
            | "load_video_cover"
            | "read_image_file"
            | "load_actor_thumbnail"
            | "load_film_actor_thumbnail"
            | "load_cover_thumbnail"
            | "load_version_cover_thumbnail"
    )
}
pub fn safe_error_code(value: &str) -> &'static str {
    match value {
        "translation_auth" => "translation_auth",
        "translation_quota" => "translation_quota",
        "translation_rate_limit" => "translation_rate_limit",
        "translation_timeout" => "translation_timeout",
        "translation_network" => "translation_network",
        "translation_server" => "translation_server",
        "translation_invalid_response" => "translation_invalid_response",
        "translation_invalid_key" => "translation_invalid_key",
        "translation_key_missing" => "translation_key_missing",
        "translation_credentials_error" => "translation_credentials_error",
        "translation_credentials_unsupported" => "translation_credentials_unsupported",
        "translation_cancelled" => "translation_cancelled",
        "translation_busy" => "translation_busy",
        "translation_failed" => "translation_failed",
        "protection_failed" => "protection_failed",
        "storage_unavailable" => "storage_unavailable",
        "interface_language_load_failed" => "interface_language_load_failed",
        "interface_language_save_failed" => "interface_language_save_failed",
        "invalid_interface_language" => "invalid_interface_language",
        "catalog_failed" => "catalog_failed",
        "startup_failed" => "startup_failed",
        "scan_failed" => "scan_failed",
        "scan_changed" => "scan_changed",
        "incomplete" => "incomplete",
        "root_unavailable" => "root_unavailable",
        "source_changed" => "source_changed",
        "preview_expired" => "preview_expired",
        "import_failed" => "import_failed",
        "state_failed" => "state_failed",
        "actor_import_failed" => "actor_import_failed",
        "actor_export_failed" => "actor_export_failed",
        "info_invalid" => "info_invalid",
        "info_invalid_json" => "info_invalid_json",
        "actor_directory_unavailable" => "actor_directory_unavailable",
        "actor_unsafe_path" => "actor_unsafe_path",
        "actor_photo_invalid" => "actor_photo_invalid",
        "actor_photo_too_large" => "actor_photo_too_large",
        "actor_import_changed" => "actor_import_changed",
        "actor_import_selection_invalid" => "actor_import_selection_invalid",
        "actor_identity_conflict" => "actor_identity_conflict",
        "actor_duplicate_identity" => "actor_duplicate_identity",
        "actor_batch_conflict" => "actor_batch_conflict",
        "actor_not_found" => "actor_not_found",
        "invalid_actor_profile" => "invalid_actor_profile",
        "actor_invalid_name" => "actor_invalid_name",
        "actor_export_photo_unavailable" => "actor_export_photo_unavailable",
        "actor_export_film_unavailable" => "actor_export_film_unavailable",
        "actor_export_photo_in_use" => "actor_export_photo_in_use",
        "actor_export_unsafe" => "actor_export_unsafe",
        "actor_export_changed" => "actor_export_changed",
        "actor_export_conflict" => "actor_export_conflict",
        "actor_export_destination_invalid" => "actor_export_destination_invalid",
        "actor_photo_unreadable" => "actor_photo_unreadable",
        "actor_save_failed" => "actor_save_failed",
        "actor_delete_failed" => "actor_delete_failed",
        "reset_failed" => "reset_failed",
        "reset_credentials_failed" => "reset_credentials_failed",
        "reset_cleanup_pending" => "reset_cleanup_pending",
        "reset_recovery_required" => "reset_recovery_required",
        "cache_clear_failed" => "cache_clear_failed",
        "launch_failed" => "launch_failed",
        "launch_result_unknown" => "launch_result_unknown",
        "cover_load_failed" => "cover_load_failed",
        "cover_unavailable" => "cover_unavailable",
        "cover_changed" => "cover_changed",
        "file_unavailable" => "file_unavailable",
        "actors_load_failed" => "actors_load_failed",
        "problems_load_failed" => "problems_load_failed",
        "javascript_type_error" => "javascript_type_error",
        "javascript_reference_error" => "javascript_reference_error",
        "javascript_range_error" => "javascript_range_error",
        "javascript_syntax_error" => "javascript_syntax_error",
        "javascript_error" => "javascript_error",
        "resource_load_error" => "resource_load_error",
        "rust_panic" => "rust_panic",
        "task_panicked" => "task_panicked",
        _ => "unknown_error",
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrorDetail {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sqlite_code: Option<i32>,
}
pub fn error_detail(error: &(dyn Error + 'static)) -> ErrorDetail {
    let mut next = Some(error);
    for _ in 0..8 {
        let Some(cause) = next else {
            break;
        };
        if let Some(sql) = cause.downcast_ref::<rusqlite::Error>() {
            return ErrorDetail {
                kind: "sqlite".into(),
                sqlite_code: match sql {
                    rusqlite::Error::SqliteFailure(code, _) => Some(code.extended_code),
                    _ => None,
                },
                ..Default::default()
            };
        }
        if let Some(io) = cause.downcast_ref::<io::Error>() {
            return ErrorDetail {
                kind: match io.kind() {
                    io::ErrorKind::PermissionDenied => "permission_denied",
                    io::ErrorKind::NotFound => "not_found",
                    io::ErrorKind::TimedOut => "timed_out",
                    io::ErrorKind::InvalidData => "invalid_data",
                    _ => "io",
                }
                .into(),
                os_code: io.raw_os_error(),
                ..Default::default()
            };
        }
        next = cause.source();
    }
    ErrorDetail {
        kind: "other".into(),
        ..Default::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Event {
    #[serde(default)]
    pub timestamp_unix_ms: u64,
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub source_revision: String,
    pub origin: String,
    pub operation: String,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<ErrorDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub counts: BTreeMap<String, u64>,
}
impl Event {
    pub fn new(origin: &str, operation: &str, event: &str, operation_id: Option<String>) -> Self {
        Self {
            timestamp_unix_ms: now(),
            session: String::new(),
            version: env!("CARGO_PKG_VERSION").into(),
            source_revision: option_env!("THEARCHIVE_SOURCE_REVISION")
                .unwrap_or("unknown")
                .into(),
            origin: origin.into(),
            operation: operation.into(),
            event: event.into(),
            operation_id,
            duration_ms: None,
            error_code: None,
            detail: None,
            source: None,
            line: None,
            column: None,
            counts: BTreeMap::new(),
        }
    }
    fn sanitize(mut self) -> Self {
        if !matches!(self.origin.as_str(), "backend" | "frontend") {
            self.origin = "frontend".into();
        }
        if !known_operation(&self.operation) {
            self.operation = "ipc".into();
        }
        if !matches!(
            self.event.as_str(),
            "start"
                | "complete"
                | "failed"
                | "cancelled"
                | "panic"
                | "error"
                | "render_error"
                | "unhandled_rejection"
                | "resource_error"
        ) {
            self.event = "error".into();
        }
        self.operation_id = self.operation_id.filter(|s| valid_id(s));
        self.session = if self.session.starts_with("session-")
            && self.session.len() <= 80
            && self
                .session
                .bytes()
                .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase() || c == b'-')
        {
            self.session
        } else {
            String::new()
        };
        self.version = env!("CARGO_PKG_VERSION").into();
        if self.source_revision.len() != 40
            || !self.source_revision.bytes().all(|c| c.is_ascii_hexdigit())
        {
            self.source_revision = "unknown".into();
        }
        self.error_code = self.error_code.map(|s| safe_error_code(&s).into());
        if let Some(detail) = &mut self.detail {
            if !matches!(
                detail.kind.as_str(),
                "sqlite"
                    | "permission_denied"
                    | "not_found"
                    | "timed_out"
                    | "invalid_data"
                    | "io"
                    | "other"
            ) {
                detail.kind = "other".into();
            }
        }
        self.source = self.source.filter(|s| {
            s == "frontend_bundle"
                || (s.starts_with("src/")
                    && s.ends_with(".rs")
                    && s.len() <= 120
                    && !s.contains("..")
                    && s.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"/_-.".contains(&c)))
        });
        self.counts.retain(|key, _| known_count(key));
        self
    }
}

fn check_node(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err(io::Error::other("redirected log node"));
                }
            }
            if meta.file_type().is_symlink() {
                return Err(io::Error::other("redirected log node"));
            }
            if meta.is_file() {
                check_file(&fs::File::open(path)?)?;
            }
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
fn check_directory_path(path: &Path) -> io::Result<()> {
    for ancestor in path.ancestors() {
        check_node(ancestor)?;
    }
    Ok(())
}
fn check_file(file: &fs::File) -> io::Result<()> {
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("not a regular log file"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut information) } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if information.nNumberOfLinks != 1 || information.dwFileAttributes & 0x400 != 0 {
            return Err(io::Error::other("redirected log file"));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if file.metadata()?.nlink() != 1 {
            return Err(io::Error::other("redirected log file"));
        }
    }
    Ok(())
}
pub struct LogFiles {
    root: PathBuf,
    max_bytes: u64,
    max_files: usize,
}
impl LogFiles {
    pub fn open(root: &Path, max_bytes: u64, max_files: usize) -> io::Result<Self> {
        check_directory_path(root)?;
        fs::create_dir_all(root)?;
        if max_files == 0 || max_files > MAX_FILES || max_bytes > MAX_FILE_BYTES {
            return Err(io::Error::other("invalid log limits"));
        }
        Ok(Self {
            root: root.to_path_buf(),
            max_bytes,
            max_files,
        })
    }
    fn path(&self, index: usize) -> PathBuf {
        self.root.join(if index == 0 {
            "runtime.log".into()
        } else {
            format!("runtime.{index}.log")
        })
    }
    fn check(&self) -> io::Result<()> {
        check_directory_path(&self.root)?;
        for n in 0..self.max_files {
            check_node(&self.path(n))?;
        }
        Ok(())
    }
    pub fn write(&mut self, event: Event) -> io::Result<()> {
        self.check()?;
        let mut line = serde_json::to_vec(&event.sanitize())?;
        line.push(b'\n');
        if line.len() > MAX_EVENT_BYTES || line.len() as u64 > self.max_bytes {
            return Err(io::Error::other("log record too large"));
        }
        let active = self.path(0);
        let existing = match fs::metadata(&active) {
            Ok(m) => m.len(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => 0,
            Err(e) => return Err(e),
        };
        if existing + line.len() as u64 > self.max_bytes {
            let oldest = self.path(self.max_files - 1);
            if oldest.exists() {
                fs::remove_file(oldest)?;
            }
            for n in (0..self.max_files - 1).rev() {
                let from = self.path(n);
                if from.exists() {
                    fs::rename(from, self.path(n + 1))?;
                }
            }
        }
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x00200000);
        } // FILE_FLAG_OPEN_REPARSE_POINT
        let mut file = options.open(active)?;
        check_file(&file)?;
        file.write_all(&line)?;
        file.flush()
    }
    pub fn snapshot(&self) -> io::Result<String> {
        self.check()?;
        let mut result = String::new();
        for n in (0..self.max_files).rev() {
            let mut bytes = Vec::new();
            match fs::File::open(self.path(n)) {
                Ok(file) => {
                    check_file(&file)?;
                    file.take(self.max_bytes).read_to_end(&mut bytes)?;
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            }
            // Re-validate every line. No raw file content is ever blindly exported.
            for line in bytes.split(|c| *c == b'\n') {
                if line.len() > MAX_EVENT_BYTES {
                    continue;
                }
                if let Ok(event) = serde_json::from_slice::<Event>(line) {
                    result.push_str(&serde_json::to_string(&event.sanitize())?);
                    result.push('\n');
                }
            }
        }
        Ok(result)
    }
}

enum Message {
    Event(Event),
    Flush(mpsc::Sender<()>),
}
struct RuntimeLog {
    sender: mpsc::SyncSender<Message>,
    files: Arc<Mutex<LogFiles>>,
    healthy: Arc<std::sync::atomic::AtomicBool>,
    session: String,
}
pub fn initialize(root: &Path) -> io::Result<()> {
    if RUNTIME.get().is_some() {
        return Ok(());
    }
    let files = Arc::new(Mutex::new(LogFiles::open(root, MAX_FILE_BYTES, MAX_FILES)?));
    let healthy = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let (sender, receiver) = mpsc::sync_channel(256);
    let worker_files = files.clone();
    let worker_health = healthy.clone();
    std::thread::Builder::new()
        .name("diagnostic-writer".into())
        .spawn(move || {
            let mut repeats: BTreeMap<String, usize> = BTreeMap::new();
            while let Ok(message) = receiver.recv() {
                match message {
                    Message::Flush(done) => {
                        let _ = done.send(());
                    }
                    Message::Event(event) => {
                        let event = event.sanitize();
                        if event.error_code.is_some()
                            && (event.detail.is_some()
                                || event.operation == "frontend_exception"
                                || event.operation == "actor.import"
                                || is_image_operation(&event.operation))
                        {
                            let key = format!(
                                "{}:{}:{}",
                                event.origin,
                                event.operation,
                                event.error_code.as_deref().unwrap_or("")
                            );
                            if !repeats.contains_key(&key) && repeats.len() >= 128 {
                                continue;
                            }
                            let count = repeats.entry(key).or_default();
                            *count += 1;
                            if *count > MAX_REPEAT {
                                continue;
                            }
                        }
                        let ok = worker_files
                            .lock()
                            .map(|mut files| files.write(event).is_ok())
                            .unwrap_or(false);
                        worker_health.store(ok, Ordering::Relaxed);
                    }
                }
            }
        })?;
    let _ = RUNTIME.set(RuntimeLog {
        sender,
        files,
        healthy,
        session: format!("session-{}-{}", std::process::id(), now()),
    });
    Ok(())
}
pub fn record(mut event: Event) {
    if let Some(runtime) = RUNTIME.get() {
        event.timestamp_unix_ms = now();
        event.session = runtime.session.clone();
        event.version = env!("CARGO_PKG_VERSION").into();
        event.source_revision = option_env!("THEARCHIVE_SOURCE_REVISION")
            .unwrap_or("unknown")
            .into();
        if event.operation_id.is_none() {
            CONTEXT.with(|context| event.operation_id = context.borrow().clone());
        }
        if runtime.sender.try_send(Message::Event(event)).is_err() {
            runtime.healthy.store(false, Ordering::Relaxed);
        }
    }
}
pub fn flush() -> bool {
    let Some(runtime) = RUNTIME.get() else {
        return false;
    };
    let (send, receive) = mpsc::channel();
    runtime.sender.try_send(Message::Flush(send)).is_ok()
        && receive.recv_timeout(Duration::from_secs(3)).is_ok()
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStatus {
    pub available: bool,
    pub max_file_bytes: u64,
    pub max_files: usize,
}
pub fn status() -> LogStatus {
    LogStatus {
        available: RUNTIME
            .get()
            .is_some_and(|r| r.healthy.load(Ordering::Relaxed)),
        max_file_bytes: MAX_FILE_BYTES,
        max_files: MAX_FILES,
    }
}
pub fn snapshot() -> io::Result<String> {
    if !flush() {
        return Err(io::Error::other("logging unavailable"));
    }
    RUNTIME
        .get()
        .ok_or_else(|| io::Error::other("logging unavailable"))?
        .files
        .lock()
        .map_err(|_| io::Error::other("logging unavailable"))?
        .snapshot()
}
pub fn log_directory() -> Option<PathBuf> {
    RUNTIME
        .get()?
        .files
        .lock()
        .ok()
        .map(|files| files.root.clone())
}
pub fn export_to(directory: &Path) -> io::Result<PathBuf> {
    check_directory_path(directory)?;
    if !directory.is_dir() {
        return Err(io::Error::other("export directory unavailable"));
    }
    let content = snapshot()?;
    let destination = directory.join(format!("TheArchive-diagnostics-{}.log", operation_id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)?;
    let result = file
        .write_all(content.as_bytes())
        .and_then(|_| file.sync_all());
    drop(file);
    if let Err(error) = result {
        let _ = fs::remove_file(&destination);
        return Err(error);
    }
    Ok(destination)
}
pub fn startup_failure(error: &(dyn Error + 'static)) {
    record_error("database.open", error);
    flush();
}

pub struct Scope {
    previous: Option<String>,
    operation: &'static str,
    id: String,
    started: Instant,
    finished: bool,
}
impl Scope {
    pub fn start(operation: &'static str, id: Option<String>) -> Self {
        let id = id
            .filter(|id| valid_id(id))
            .or_else(|| CONTEXT.with(|context| context.borrow().clone()))
            .unwrap_or_else(operation_id);
        let previous = CONTEXT.with(|context| context.replace(Some(id.clone())));
        record(Event::new("backend", operation, "start", Some(id.clone())));
        Self {
            previous,
            operation,
            id,
            started: Instant::now(),
            finished: false,
        }
    }
    pub fn finish(&mut self, error: Option<&str>, counts: BTreeMap<String, u64>) {
        let mut event = Event::new(
            "backend",
            self.operation,
            if error == Some("translation_cancelled") {
                "cancelled"
            } else if error.is_some() {
                "failed"
            } else {
                "complete"
            },
            Some(self.id.clone()),
        );
        event.duration_ms = Some(self.started.elapsed().as_millis().min(u64::MAX as u128) as u64);
        event.error_code = error.map(|e| safe_error_code(e).into());
        event.counts = counts;
        record(event);
        self.finished = true;
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(
                Some(if std::thread::panicking() {
                    "rust_panic"
                } else {
                    "unknown_error"
                }),
                BTreeMap::new(),
            );
        }
        CONTEXT.with(|context| {
            context.replace(self.previous.take());
        });
    }
}
pub fn spawn_blocking<T, E>(
    operation: &'static str,
    id: Option<String>,
    action: impl FnOnce() -> Result<T, E> + Send + 'static,
) -> tauri::async_runtime::JoinHandle<Result<T, E>>
where
    T: Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let mut scope = Scope::start(operation, id);
        let result = action();
        scope.finish(
            result.as_ref().err().map(|e| e.to_string()).as_deref(),
            BTreeMap::new(),
        );
        result
    })
}
#[track_caller]
pub fn record_error(operation: &'static str, error: &(dyn Error + 'static)) {
    let location = std::panic::Location::caller();
    let mut event = Event::new("backend", operation, "failed", None);
    event.detail = Some(error_detail(error));
    event.error_code = Some("unknown_error".into());
    event.source = Some(location.file().replace('\\', "/"));
    event.line = Some(location.line());
    event.column = Some(location.column());
    record(event);
}
#[track_caller]
pub fn record_code(operation: &'static str, code: &str) {
    let location = std::panic::Location::caller();
    let mut event = Event::new("backend", operation, "failed", None);
    event.error_code = Some(safe_error_code(code).into());
    event.source = Some(location.file().replace('\\', "/"));
    event.line = Some(location.line());
    event.column = Some(location.column());
    record(event);
}
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let mut event = Event::new("backend", "application", "panic", None);
        event.error_code = Some("rust_panic".into());
        if let Some(location) = info.location() {
            event.source = Some(location.file().replace('\\', "/"));
            event.line = Some(location.line());
            event.column = Some(location.column());
        }
        // Do not invoke the default hook: its panic payload can contain a credential.
        if let Some(runtime) = RUNTIME.get() {
            event.timestamp_unix_ms = now();
            event.session = runtime.session.clone();
            CONTEXT.with(|context| event.operation_id = context.borrow().clone());
            match runtime.files.try_lock() {
                Ok(mut files) => {
                    let _ = files.write(event);
                }
                Err(_) => {
                    let _ = runtime.sender.try_send(Message::Event(event));
                }
            }
        }
    }));
}
