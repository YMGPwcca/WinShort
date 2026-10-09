//! Build script: embed the application manifest (PerMonitorV2 DPI awareness,
//! asInvoker execution level, supported-OS entries), application icon, and
//! version metadata into the executable (#26, #50).

fn main() {
    let build = emit_build_info();
    println!("cargo:rerun-if-changed=winshort.manifest");
    println!("cargo:rerun-if-changed=assets/winshort.ico");
    if cfg!(target_os = "windows") {
        use winresource::WindowsResource;
        let mut res = WindowsResource::new();
        res.set_icon("assets/winshort.ico");
        res.set_manifest_file("winshort.manifest");
        res.set("FileDescription", "WinShort");
        res.set("ProductName", "WinShort");
        let version = format!(
            "{}.{}.{}.0",
            env!("CARGO_PKG_VERSION_MAJOR"),
            env!("CARGO_PKG_VERSION_MINOR"),
            env!("CARGO_PKG_VERSION_PATCH")
        );
        res.set("FileVersion", &version);
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        res.set(
            "Comments",
            &format!("Build {} | {} | {}", build.2, build.0, build.1),
        );
        res.compile().expect("embed Windows resources and manifest");
    }
}

fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn emit_build_info() -> (String, String, String) {
    for path in [
        "src",
        "Cargo.toml",
        "Cargo.lock",
        ".git/HEAD",
        ".git/index",
        ".git/refs/heads",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    let revision = command_text("git", &["rev-parse", "--short=12", "HEAD"])
        .unwrap_or_else(|| "unknown revision".into());
    let modified =
        command_text("git", &["status", "--porcelain"]).is_some_and(|status| !status.is_empty());
    let revision = format!("{revision}{}", if modified { " (modified)" } else { "" });
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string();
    let date = command_text(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss')",
        ],
    )
    .map_or_else(|| format!("timestamp {id}"), |value| format!("{value} UTC"));
    println!("cargo:rustc-env=WINSHORT_BUILD_REVISION={revision}");
    println!("cargo:rustc-env=WINSHORT_BUILD_DATE={date}");
    println!("cargo:rustc-env=WINSHORT_BUILD_ID={id}");
    (revision, date, id)
}
