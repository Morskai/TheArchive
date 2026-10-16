fn main() {
    // Tauri embeds Common Controls v6 for the main binary; native test examples need it too.
    if std::env::var("TARGET").is_ok_and(|target| target.ends_with("windows-msvc")) {
        println!("cargo:rustc-link-arg-examples=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-examples=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
    // Rebuild embedded Windows icon resources when program artwork changes.
    println!("cargo:rerun-if-changed=icons");
    // Same product version can have several local deliveries; keep their source provenance in logs.
    if let Ok(head) = std::process::Command::new("git").args(["rev-parse", "HEAD"]).output() {
        let revision = String::from_utf8_lossy(&head.stdout).trim().to_owned();
        if head.status.success() && revision.len() == 40 && revision.bytes().all(|c| c.is_ascii_hexdigit()) {
            println!("cargo:rustc-env=THEARCHIVE_SOURCE_REVISION={revision}");
        }
    }
    for reference in ["HEAD", "refs", "packed-refs"] {
        if let Ok(path) = std::process::Command::new("git").args(["rev-parse", "--git-path", reference]).output() {
            if path.status.success() { println!("cargo:rerun-if-changed={}", String::from_utf8_lossy(&path.stdout).trim()); }
        }
    }
    tauri_build::build()
}
