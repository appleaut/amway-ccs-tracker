# Amway CCS Tracker

Windows desktop app (Rust) for managing Amway business operations following the
**CCS Guide (Crown Center Success)** methodology — tracking prospects, VIP
customers, and ABO downline through the 8-step sponsor flow, scoring sheets, the
BK1/BK2/C1 follow-up checklist, the network org chart, and a per-contact to-do
list with due dates.

Thai UI labels throughout; English code identifiers.

## Tech stack

| Concern      | Choice                                   |
|--------------|------------------------------------------|
| Language     | Rust 2021                                |
| GUI          | `egui` + `eframe` 0.28 (immediate mode)  |
| Tables/dates | `egui_extras` 0.28 (`TableBuilder`, `datepicker`) |
| Database     | `rusqlite` 0.31 (bundled SQLite, sync)   |
| Date/time    | `chrono` 0.4                             |
| Errors       | `thiserror` 1                            |
| Image export | `png` 0.17 (network chart → PNG)         |
| Target       | `x86_64-pc-windows-msvc`                 |

No async runtime, no network calls, no installer — a single ~6 MB `.exe`.

## Build & run

```powershell
# Debug build + run
cargo run

# Release build (single self-contained binary)
cargo build --release --target x86_64-pc-windows-msvc
# -> target\x86_64-pc-windows-msvc\release\amway_ccs_tracker.exe

# Tests
cargo test
```

> The MSVC toolchain (Visual Studio Build Tools, "Desktop development with C++")
> is required because SQLite is compiled from C via the `bundled` feature. `cargo`
> locates it automatically — no Developer Command Prompt needed.

## Data

* Stored locally in SQLite at `%APPDATA%\AmwayCCSTracker\data.db`
  (created on first launch).
* Schema is versioned via `PRAGMA user_version` — see [src/db/schema.rs](src/db/schema.rs).
* Network-chart images (💾 บันทึกรูป) are written to
  `%APPDATA%\AmwayCCSTracker\exports\network_<timestamp>.png`.

## Thai font

egui ships no Thai glyphs, so the **Kanit** font (Google Fonts, SIL Open Font
License) is embedded into the binary via `include_bytes!`
(`assets/fonts/Kanit-Regular.ttf` + `Kanit-Medium.ttf`) and set as the primary
face — Regular for body text, Medium for headings/buttons. No system-font
dependency; the license is bundled at `assets/fonts/OFL.txt`.

## Project layout

```
src/
├── main.rs              eframe entry point + window options
├── app.rs              AppState, main render loop, sidebar, settings, fonts/theme
├── error.rs            AppError (thiserror) + Result alias
├── db/
│   ├── mod.rs          DbConnection wrapper (single connection owner)
│   ├── schema.rs       CREATE TABLE + PRAGMA user_version migrations
│   └── queries.rs      typed SQL + business-rule enforcement + integration tests
├── models/
│   ├── contact.rs      Contact, ProspectScore, CustomerScore, SponsorFlowStatus
│   ├── followup.rs     FollowUpSheet (26-item BK1/BK2/C1/Conference checklist)
│   ├── activity.rs     Activity (per-contact interaction history)
│   ├── todo.rs         Todo (a task, optionally tied to a contact, + due date & done)
│   └── enums.rs        Gender, NetworkCategory, ContactType, Rank, SponsorStep
├── ui/
│   ├── dashboard.rs    summary cards (incl. clickable overdue-todos card) + customer target + flow overview
│   ├── prospect_list.rs   Sponsor List table (sortable, editable step, advance)
│   ├── customer_list.rs   Customer Name List table (sortable)
│   ├── abo_list.rs        ABO management table (sortable) + 📊 Rank Advisor
│   ├── followup.rs        per-ABO checklist with progress bar
│   ├── downline_tree.rs   radial node chart ("me" centre shows my rank, draggable, zoom, auto-arrange, self Rank Advisor, PNG export)
│   ├── forms.rs           add/edit modal with scoring inputs
│   ├── confirm.rs         shared delete-confirmation modal
│   ├── rank_advisor.rs    rank assessment for an ABO and for "me" (PPV + downline legs → qualified rank)
│   ├── activity_log.rs    per-contact interaction history modal (📝, all tables)
│   ├── activities.rs      aggregate Activity History page (all contacts, search + kind filter)
│   ├── activity_kinds.rs  manage activity types (CRUD) used by the log + history
│   └── todo.rs            Todo List page — CRUD tasks, due-date picker, status/type filters, overdue highlight
└── utils/
    └── scoring.rs      score totals, rank/bonus tiers, transition validation + unit tests
```

See [docs/SYSTEM_ANALYSIS.md](docs/SYSTEM_ANALYSIS.md) for the domain model and
[docs/TEST_REPORT.md](docs/TEST_REPORT.md) for the test report and manual QA checklist.

## License

Copyright 2026 appleaut. Released under the **PolyForm Noncommercial License
1.0.0** — see [LICENSE.md](LICENSE.md). You may use, modify, and share it for
**noncommercial purposes only**; commercial use is not permitted. This is a
*source-available* licence, **not** an OSI open-source one.

Bundled third-party components keep their own licences and are unaffected: the
**Kanit** font under the SIL Open Font License (`assets/fonts/OFL.txt`), and the
Rust dependencies under their respective (mostly MIT / Apache-2.0) terms.

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
SmartScreen may warn on first run ("More info" -> "Run anyway").

The optional Promotion Downloader feature needs Google Chrome and
`pip install playwright pillow`; the rest of the app needs nothing extra.
