//! Build script: on Windows targets, embed the app icon and version metadata
//! into the executable's resources (shows in Explorer, taskbar, shortcuts, and
//! Properties → Details). No-op on other targets so the crate still builds there.

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/app.ico");
        res.set("ProductName", "Amway CCS Tracker");
        res.set("FileDescription", "Amway CCS Prospect & Downline Tracker");
        res.set("CompanyName", "Amway CCS Tracker");
        res.set("LegalCopyright", "Copyright (C) 2026 Amway CCS Tracker");
        // ProductVersion / FileVersion default from CARGO_PKG_VERSION.
        if let Err(e) = res.compile() {
            println!("cargo:warning=winresource failed to embed exe resources: {e}");
            std::process::exit(1);
        }
    }
}
