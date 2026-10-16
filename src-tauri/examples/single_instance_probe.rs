// Native verification uses a unique identifier, a blank offscreen window and only its test folder.
// It never starts the application storage, credentials, scanner or tray.
#[cfg(windows)]
fn main() {
    use std::{fs, io::Write, time::Duration};
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
    let root = std::path::PathBuf::from(std::env::var_os("THEARCHIVE_PROBE_DIR").expect("test folder required"));
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = std::env::var("THEARCHIVE_PROBE_ID").expect("isolated identifier required");
    context.config_mut().app.windows.clear();
    let startup_guard = local_film_library_preview_lib::single_instance::StartupGuard::acquire(&context.config().identifier).unwrap();
    // The native test launches a second process while the first has a mutex but no plugin window.
    if let Ok(delay) = std::env::var("THEARCHIVE_PROBE_DELAY_MS") {
        fs::write(root.join("locked"), std::process::id().to_string()).unwrap();
        std::thread::sleep(Duration::from_millis(delay.parse().unwrap()));
    }
    let app = tauri::Builder::default()
        .plugin(local_film_library_preview_lib::single_instance::init())
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External("about:blank".parse().unwrap()))
                .title("TheArchive isolated single-instance test")
                .position(-20000.0, -20000.0).inner_size(128.0, 80.0)
                .visible(false).skip_taskbar(true).build()?;
            let mut starts = fs::OpenOptions::new().append(true).create(true).open(root.join("starts.log"))?;
            writeln!(starts, "{}", std::process::id())?;
            fs::write(root.join("ready"), std::process::id().to_string())?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last = String::new();
                loop {
                    std::thread::sleep(Duration::from_millis(25));
                    let Ok(command) = fs::read_to_string(root.join("control")) else { continue };
                    if command == last { continue }
                    last = command.clone();
                    let Some((sequence, action)) = command.split_once(':') else { continue };
                    let sequence = sequence.to_owned();
                    let action = action.to_owned();
                    let output = root.clone();
                    let app = handle.clone();
                    let _ = handle.run_on_main_thread(move || {
                        let window = app.get_webview_window("main").unwrap();
                        match action.as_str() {
                            "hide" => { window.hide().unwrap(); }
                            "minimize" => { window.minimize().unwrap(); }
                            "exit" => { app.exit(0); }
                            _ => {}
                        }
                        let state = serde_json::json!({"sequence":sequence,"pid":std::process::id(),"visible":window.is_visible().unwrap_or(false),"minimized":window.is_minimized().unwrap_or(false),"focused":window.is_focused().unwrap_or(false)});
                        let _ = fs::write(output.join("state.json"), state.to_string());
                    });
                }
            });
            Ok(())
        })
        .build(context).expect("isolated native probe failed");
    drop(startup_guard);
    app.run(|_, _| {});
}
#[cfg(not(windows))]
fn main() {}
