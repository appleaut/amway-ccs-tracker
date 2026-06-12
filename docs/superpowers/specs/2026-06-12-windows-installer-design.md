# Windows Installer Implementation Design

**Date:** 2026-06-12
**Status:** Approved (design), pending plan

## Goal

Produce a polished Windows installer — `AmwayCCSTracker-Setup.exe` — that installs
the Amway CCS Tracker desktop app per-user (no admin prompt), with a branded icon,
Start Menu/Desktop shortcuts, and a proper uninstaller, while never touching the
user's existing data.

## Background

The app is a self-contained Rust/eframe-egui binary (`amway_ccs_tracker.exe`):
fonts are embedded, SQLite is bundled, and the database lives in
`%APPDATA%\AmwayCCSTracker\data.db`. `cargo build --release` already produces a
runnable exe (release builds use `windows_subsystem = "windows"`, so no console
window appears). What is missing for distribution is: an application icon, exe
version metadata, and an installer that places the app, creates shortcuts, and
registers an uninstaller.

The Promotion Downloader feature shells out to Python (Playwright + Pillow) and
Google Chrome. Those cannot be reasonably bundled, so the installer only *informs*
the user they are prerequisites for that one feature; the rest of the app needs
nothing extra.

## Locked Decisions

1. **Deliverable:** A real installer (option A) built with **Inno Setup**, not a
   bare portable exe.
2. **Scope/privileges:** **Per-user**, `PrivilegesRequired=lowest` — installs to
   `{localappdata}\Programs\AmwayCCSTracker`, no UAC/admin prompt.
3. **Icon:** **Generated** — a branded multi-resolution `.ico` (a teal rounded-
   square tile with a "CCS" monogram), produced with Pillow.

## Architecture

Four units with clear boundaries:

### Unit 1 — Branded icon assets (generated, committed)

- A small, committed generator script `tools/icon/make_icon.py` (Pillow) renders
  the app mark and writes:
  - `assets/icons/app.ico` — multi-resolution (16, 32, 48, 64, 128, 256 px).
  - `assets/icons/app.png` — 256×256 RGBA, used for the runtime window icon.
- Design: a rounded-square tile filled with the app's accent teal `#00BCD4`
  (the in-app `ACCENT` value, hardcoded in the generator), with "CCS" centered in
  white using the bundled Kanit font (`assets/fonts/Kanit-Medium.ttf`).
  Generation is deterministic and uses only Pillow (already installed); the
  script is kept so the icon can be regenerated, but the committed `.ico`/`.png`
  are the source of truth for builds.

### Unit 2 — Embed the icon + version metadata in the exe (`build.rs`)

- Add a `build.rs` that, **only when targeting Windows**, uses the `winresource`
  crate to:
  - Set the application icon from `assets/icons/app.ico` (drives the Explorer /
    taskbar / shortcut icon).
  - Stamp version-info resource fields: `ProductName = "Amway CCS Tracker"`,
    `FileDescription = "Amway CCS Prospect & Downline Tracker"`,
    `CompanyName = "Amway CCS Tracker"`, `ProductVersion`/`FileVersion` derived
    from the crate version (`0.1.0`), `LegalCopyright`.
- `Cargo.toml` gains `[build-dependencies] winresource = "0.1"`. On non-Windows
  hosts `build.rs` is a no-op so the crate still builds/tests elsewhere (CI, etc.).

### Unit 3 — Runtime window icon (`src/main.rs`)

- Embed the PNG with `include_bytes!("../assets/icons/app.png")` and attach it to
  the viewport so the running window's title bar / taskbar shows the icon:
  `ViewportBuilder::default().with_icon(eframe::icon_data::from_png_bytes(...))`.
- The icon load is best-effort: if decoding ever fails, fall back to no icon
  rather than panicking (the app must always start).

### Unit 4 — Inno Setup installer (`installer/amway_ccs_tracker.iss`)

- A committed Inno Setup script producing `dist\AmwayCCSTracker-Setup.exe`:
  - `PrivilegesRequired=lowest`; `DefaultDirName={localappdata}\Programs\AmwayCCSTracker`.
  - Stable `AppId` GUID (fixed in the script so upgrades/uninstall match).
  - `AppName`, `AppVersion=0.1.0`, `AppPublisher="Amway CCS Tracker"`,
    `SetupIconFile=..\assets\icons\app.ico`, `UninstallDisplayIcon={app}\amway_ccs_tracker.exe`.
  - `[Files]`: `target\release\amway_ccs_tracker.exe`, `assets\icons\app.ico`,
    `LICENSE.md`.
  - `[Icons]`: Start Menu shortcut (always); Desktop shortcut behind an opt-in
    `[Tasks]` checkbox (default checked).
  - `[Run]`: optional "launch the app now" checkbox on the finish page.
  - A custom **prerequisites info page** (an `InfoBefore` page or a `Code` message)
    stating: the app runs as-is; the *Promotion Downloader* feature additionally
    needs Python (with Playwright + Pillow) and Google Chrome.
  - **No data deletion:** the script never lists `%APPDATA%\AmwayCCSTracker` in
    `[Files]`/`[UninstallDelete]`, so uninstall leaves the database and backups
    intact.
  - Output: `OutputDir=..\dist`, `OutputBaseFilename=AmwayCCSTracker-Setup`.
- Installer wizard language: English (Inno's bundled `Default.isl`); the app UI
  remains Thai.

### Build glue (`build_installer.ps1`, `.gitignore`, README)

- `build_installer.ps1` (committed): runs `cargo build --release`, locates `iscc`
  (Inno Setup compiler), compiles the `.iss`, and reports the output path
  `dist\AmwayCCSTracker-Setup.exe`. Fails with a clear message if `iscc` is not
  found, pointing at the install step.
- `.gitignore`: add `/dist/`.
- README: a "Building the Windows installer" section documenting the one-time
  `winget install JRSoftware.InnoSetup` and running `build_installer.ps1`.

## Data Flow (build pipeline)

`tools/icon/make_icon.py` → `assets/icons/app.{ico,png}` (committed) →
`cargo build --release` (build.rs embeds icon+version; main.rs embeds window icon)
→ `target\release\amway_ccs_tracker.exe` → `iscc installer\amway_ccs_tracker.iss`
→ `dist\AmwayCCSTracker-Setup.exe`. `build_installer.ps1` runs the last two steps.

## Error Handling

- `build.rs`: guard the `winresource` call behind a Windows-target check; any
  resource-compile error fails the build loudly (correct — a broken icon resource
  should not ship).
- Runtime window icon: best-effort; on decode failure, start without an icon.
- `build_installer.ps1`: if `cargo build` fails, stop and surface the error; if
  `iscc` is missing, print the `winget` install hint and exit non-zero.

## Testing / Verification

- `cargo build --release` succeeds with the new `build.rs` (icon + version
  resource compiled in); `cargo test` still passes (95 tests) — `build.rs` must
  not break the existing build.
- Inspect `target\release\amway_ccs_tracker.exe` in Explorer: the custom icon
  shows, and Properties → Details shows the version/product metadata.
- Run the exe: the window title bar / taskbar shows the icon.
- Run `build_installer.ps1` (after installing Inno Setup): it produces
  `dist\AmwayCCSTracker-Setup.exe`.
- Run the setup: installs without a UAC prompt to `%LOCALAPPDATA%\Programs\
  AmwayCCSTracker`, creates the Start Menu (and optional Desktop) shortcut, the
  app launches, and it appears in Add/Remove Programs with the icon. Uninstall
  removes the app but leaves `%APPDATA%\AmwayCCSTracker` (verify the database
  survives).
- Verify the generated icon glyph renders (the "CCS" mark is legible at 16px).

## Out of Scope (YAGNI)

- Bundling Python / Playwright / Pillow / Chrome.
- Code signing (no certificate available) — Windows SmartScreen may warn; noted
  in the README.
- Auto-update, delta updates.
- MSI / WiX, Microsoft Store / MSIX packaging.
- Multi-language installer wizard (Thai installer UI).
- Per-machine (all-users) installation.
