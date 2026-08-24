//! Build script: embed the application manifest (PerMonitorV2 DPI awareness,
//! asInvoker execution level, supported-OS entries) and version metadata
//! into the executable (#26).

fn main() {
    println!("cargo:rerun-if-changed=winshort.manifest");
    if cfg!(target_os = "windows") {
        use winresource::WindowsResource;
        let mut res = WindowsResource::new();
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
        res.compile()
            .expect("embed Windows resources and manifest");
    }
}
