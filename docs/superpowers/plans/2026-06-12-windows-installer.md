# Windows Installer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a per-user Windows installer `dist\AmwayCCSTracker-Setup.exe` (Inno Setup) for the app, with a branded icon embedded in the exe, the running window, and the installer, plus Start Menu/Desktop shortcuts and an uninstaller that preserves user data.

**Architecture:** A Pillow script generates a committed `.ico`/`.png` icon. A `build.rs` (winresource) embeds the icon + version metadata into the exe; `main.rs` sets the runtime window icon. An Inno Setup `.iss` script + a `build_installer.ps1` wrapper turn `cargo build --release` output into the setup exe.

**Tech Stack:** Rust, eframe/egui 0.28, Pillow (icon generation), `winresource` (build-dep, exe resources), Inno Setup 6 (`iscc`).

**Conventions for every task:**
- This repo is **hand-formatted** — NEVER run `cargo fmt` (no `rustfmt.toml`; it reformats every file). Verify only with `cargo build` / `cargo test`.
- Every commit message must end with the line:
  `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`
- This is a single-developer Windows machine. Work from `D:\Projects\amway\space-to-grow\amway_ccs_tracker` on branch `windows-installer`.

---

### Task 1: Generate the branded icon assets

**Files:**
- Create: `tools/icon/make_icon.py`
- Create (generated, committed): `assets/icons/app.ico`, `assets/icons/app.png`

- [ ] **Step 1: Write the icon generator**

Create `tools/icon/make_icon.py`:

```python
"""Generate the Amway CCS Tracker app icon: a teal rounded-square tile with a
white "CCS" monogram. Writes assets/icons/app.png (256) and app.ico (multi-size).
Run: py tools/icon/make_icon.py"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "assets" / "icons"
FONT = ROOT / "assets" / "fonts" / "Kanit-Medium.ttf"

SIZE = 256
TEAL = (0x00, 0xBC, 0xD4, 255)  # in-app ACCENT
WHITE = (255, 255, 255, 255)
RADIUS = 52
TEXT = "CCS"


def render(size: int) -> Image.Image:
    # Render at 256 then downscale for crisp small sizes.
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, SIZE - 1, SIZE - 1], radius=RADIUS, fill=TEAL)
    font = ImageFont.truetype(str(FONT), 92)
    box = d.textbbox((0, 0), TEXT, font=font)
    tw, th = box[2] - box[0], box[3] - box[1]
    pos = ((SIZE - tw) / 2 - box[0], (SIZE - th) / 2 - box[1])
    d.text(pos, TEXT, font=font, fill=WHITE)
    if size != SIZE:
        img = img.resize((size, size), Image.LANCZOS)
    return img


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    base = render(SIZE)
    base.save(OUT / "app.png")
    sizes = [16, 32, 48, 64, 128, 256]
    base.save(OUT / "app.ico", sizes=[(s, s) for s in sizes])
    print(f"wrote {OUT / 'app.png'} and {OUT / 'app.ico'}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Generate the icon**

Run: `py tools/icon/make_icon.py`
Expected output: `wrote ...\assets\icons\app.png and ...\assets\icons\app.ico`, and both files exist.

- [ ] **Step 3: Visually verify the icon**

Open `assets/icons/app.png` and confirm: a teal rounded tile with a legible white "CCS". (Optional: open `app.ico` in an image viewer.) If the text is clipped or off-center, adjust the font size (92) / `RADIUS` and re-run Step 2 before committing.

- [ ] **Step 4: Commit**

```
git add tools/icon/make_icon.py assets/icons/app.ico assets/icons/app.png
git commit -m "Add branded app icon and its Pillow generator

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Embed the icon + version metadata into the exe (`build.rs`)

**Files:**
- Modify: `Cargo.toml` (add `[build-dependencies]`)
- Create: `build.rs`

- [ ] **Step 1: Add the build dependency**

In `Cargo.toml`, after the `[profile.release]` block (end of file), add:

```toml
[build-dependencies]
winresource = "0.1"
```

- [ ] **Step 2: Create `build.rs`**

Create `build.rs` at the repo root:

```rust
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
            eprintln!("cargo:warning=winresource failed to embed exe resources: {e}");
            std::process::exit(1);
        }
    }
}
```

- [ ] **Step 3: Build the release binary**

Run: `cargo build --release`
Expected: compiles successfully. (First build downloads `winresource`.) If it fails with a "could not find `rc.exe`" / resource-compiler error, the Windows SDK is missing — install the "Desktop development with C++" workload / Windows SDK (VS Build Tools), then rebuild.

- [ ] **Step 4: Verify the exe carries the icon + metadata**

Run (PowerShell):
```
(Get-Item .\target\release\amway_ccs_tracker.exe).VersionInfo | Format-List ProductName,FileDescription,CompanyName,ProductVersion
```
Expected: `ProductName = Amway CCS Tracker`, `FileDescription = Amway CCS Prospect & Downline Tracker`, `CompanyName = Amway CCS Tracker`, `ProductVersion = 0.1.0`. Also open `target\release\` in Explorer and confirm the exe shows the teal "CCS" icon.

- [ ] **Step 5: Confirm tests still pass**

Run: `cargo test`
Expected: `test result: ok. 95 passed`.

- [ ] **Step 6: Commit**

```
git add Cargo.toml Cargo.lock build.rs
git commit -m "Embed app icon and version metadata in the Windows exe

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Set the runtime window icon (`src/main.rs`)

**Files:**
- Modify: `src/main.rs:16-23` (the `NativeOptions` / viewport construction)

- [ ] **Step 1: Attach the window icon**

In `src/main.rs`, replace the `let options = eframe::NativeOptions { ... };` block:

```rust
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("Amway CCS Tracker"),
        ..Default::default()
    };
```

with this (build the viewport first so we can attach the icon best-effort):

```rust
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([960.0, 640.0])
        .with_title("Amway CCS Tracker");
    // Best-effort window icon (title bar / taskbar); never fail startup over it.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icons/app.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
```

- [ ] **Step 2: Build**

Run: `cargo build --release`
Expected: compiles. (`eframe::icon_data::from_png_bytes` returns `Result<IconData, image::ImageError>`; `with_icon` accepts the `IconData`.)

- [ ] **Step 3: Verify the running window shows the icon**

Run: `.\target\release\amway_ccs_tracker.exe`
Expected: the window opens; its title-bar icon and taskbar button show the teal "CCS" mark (not the generic default). Close the app.

- [ ] **Step 4: Commit**

```
git add src/main.rs
git commit -m "Show the app icon on the running window

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Inno Setup installer + build script + docs

**Files:**
- Create: `installer/amway_ccs_tracker.iss`
- Create: `installer/prerequisites.txt`
- Create: `build_installer.ps1`
- Modify: `.gitignore` (add `/dist/`)
- Modify: `README.md` (add a build section)

- [ ] **Step 1: Create the prerequisites info text**

Create `installer/prerequisites.txt`:

```
Amway CCS Tracker runs as-is - no extra software is required to use the app,
manage contacts, meetings, to-dos, advances, and backups.

The optional "Promotion Downloader" feature additionally requires:

  - Google Chrome (installed normally), and
  - Python 3 with two packages:
        pip install playwright pillow

If those are not present, every other part of the app still works; only the
promotion download button will report the missing prerequisite.
```

- [ ] **Step 2: Create the Inno Setup script**

Create `installer/amway_ccs_tracker.iss`:

```iss
; Inno Setup script for Amway CCS Tracker (per-user, no admin).
; Build: run ..\build_installer.ps1 (or: iscc amway_ccs_tracker.iss)

#define MyAppName "Amway CCS Tracker"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "Amway CCS Tracker"
#define MyAppExeName "amway_ccs_tracker.exe"

[Setup]
AppId={{A1C7F3E2-9B4D-4E6A-8C21-3F5D7E9A1B2C}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={localappdata}\Programs\AmwayCCSTracker
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=AmwayCCSTracker-Setup
SetupIconFile=..\assets\icons\app.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
InfoBeforeFile=prerequisites.txt
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "..\target\release\amway_ccs_tracker.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\icons\app.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\app.ico"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\app.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent
```

Note: the app's data folder `%APPDATA%\AmwayCCSTracker` is intentionally NOT listed in `[Files]` or any `[UninstallDelete]`, so uninstall leaves the database and backups untouched.

- [ ] **Step 3: Create the build wrapper**

Create `build_installer.ps1` at the repo root:

```powershell
# Build the Amway CCS Tracker Windows installer: release exe -> Inno Setup -> dist.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

Write-Host "==> cargo build --release"
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
    foreach ($p in @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
    )) { if (Test-Path $p) { $iscc = $p; break } }
}
if (-not $iscc) {
    throw "Inno Setup compiler (iscc) not found. Install it with: winget install JRSoftware.InnoSetup"
}

Write-Host "==> compiling installer with $iscc"
& $iscc "installer\amway_ccs_tracker.iss"
if ($LASTEXITCODE -ne 0) { throw "iscc failed" }

Write-Host "==> done: dist\AmwayCCSTracker-Setup.exe"
```

- [ ] **Step 4: Ignore the dist output**

In `.gitignore`, add a line (keep existing entries):

```
/dist/
```

- [ ] **Step 5: Document it in the README**

In `README.md`, add this section near the bottom (before any license footer; if unsure, append it at the end):

```markdown
## Building the Windows installer

One-time: install Inno Setup 6 (`winget install JRSoftware.InnoSetup`).

Then from the repo root:

```powershell
./build_installer.ps1
```

This builds the release binary and produces `dist\AmwayCCSTracker-Setup.exe` — a
per-user installer (no admin prompt) that installs to
`%LOCALAPPDATA%\Programs\AmwayCCSTracker`, adds Start Menu / optional Desktop
shortcuts, and registers an uninstaller. Uninstalling leaves your data
(`%APPDATA%\AmwayCCSTracker`) intact. The installer is unsigned, so Windows
SmartScreen may warn on first run ("More info" → "Run anyway").

The optional Promotion Downloader feature needs Google Chrome and
`pip install playwright pillow`; the rest of the app needs nothing extra.
```

- [ ] **Step 6: Commit**

```
git add installer/amway_ccs_tracker.iss installer/prerequisites.txt build_installer.ps1 .gitignore README.md
git commit -m "Add Inno Setup installer, build script, and docs

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: Build the installer and verify end-to-end

**Files:** none (verification; only re-touch earlier files if a defect is found).

- [ ] **Step 1: Ensure Inno Setup is available**

Run: `iscc /?` (or check `"${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"`).
If missing, install once: `winget install --id JRSoftware.InnoSetup -e` and re-open the shell.

- [ ] **Step 2: Build the installer**

Run: `./build_installer.ps1`
Expected: ends with `==> done: dist\AmwayCCSTracker-Setup.exe`, and `dist\AmwayCCSTracker-Setup.exe` exists.

- [ ] **Step 3: Install and verify (manual)**

Run `dist\AmwayCCSTracker-Setup.exe` and confirm:
- No UAC / admin prompt appears.
- The prerequisites information page is shown.
- It installs to `%LOCALAPPDATA%\Programs\AmwayCCSTracker`.
- A Start Menu entry "Amway CCS Tracker" exists (and a Desktop shortcut if the checkbox was left on), both with the teal "CCS" icon.
- Launching from the shortcut opens the app and the existing data (contacts) is still present (data lives in `%APPDATA%`, untouched).
- "Amway CCS Tracker" appears in Settings → Apps (Add/Remove Programs) with the icon, version `0.1.0`, publisher.

- [ ] **Step 4: Verify uninstall preserves data (manual)**

Uninstall via Add/Remove Programs, then confirm `%APPDATA%\AmwayCCSTracker\data.db` still exists. Reinstall + relaunch shows the data intact. (If you don't want to disturb the installed copy, this step may be done with care or noted as verified by inspection of the `.iss` — it lists no `[UninstallDelete]` for the data folder.)

- [ ] **Step 5: Finish the branch**

Use the **superpowers:finishing-a-development-branch** skill: confirm `cargo test` passes, then present the merge/PR options. (Per project rule, do NOT merge to main without explicit user approval.)

---

## Notes for the implementer

- **winresource needs a resource compiler.** On the MSVC toolchain it uses the Windows SDK `rc.exe`; since `rusqlite`'s bundled SQLite already compiles C here, the toolchain is present, but `rc.exe` specifically ships with the Windows SDK. If `cargo build` fails on the resource step, install the Windows SDK (VS Build Tools "Desktop development with C++").
- **Icon is the single source for all three surfaces** (exe resource via build.rs, runtime window via `with_icon`, installer via `SetupIconFile`/shortcut `IconFilename`). Regenerate with `py tools/icon/make_icon.py` if the design changes, then rebuild.
- **No code-signing** — the setup is unsigned; SmartScreen may warn. Documented in the README; out of scope to fix.
- **Inno Setup is not committed/bundled** — it's a build-time tool installed once via winget. Only the `.iss` script and `build_installer.ps1` live in the repo.
