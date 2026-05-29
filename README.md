# Amway CCS Tracker

Windows desktop app (Rust) for managing Amway business operations following the
**CCS Guide (Crown Center Success)** methodology — tracking prospects, VIP
customers, and ABO downline through the 8-step sponsor flow, scoring sheets, the
BK1/BK2/C1 follow-up checklist, and the network org chart.

Thai UI labels throughout; English code identifiers.

## Tech stack

| Concern      | Choice                                   |
|--------------|------------------------------------------|
| Language     | Rust 2021                                |
| GUI          | `egui` + `eframe` 0.28 (immediate mode)  |
| Database     | `rusqlite` 0.31 (bundled SQLite, sync)   |
| Date/time    | `chrono` 0.4                             |
| Errors       | `thiserror` 1                            |
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
* The **Settings** screen has a *Load sample data* button that seeds a 3-level
  ABO hierarchy plus example prospects/customers for a quick tour.

## Thai font

egui ships no Thai glyphs, so the app loads a Thai-capable system font at startup
(tries `Leelawadee UI` → `Leelawadee` → `Tahoma` from `C:\Windows\Fonts`). The
chosen font is shown on the Settings screen.

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
│   └── enums.rs        Gender, NetworkCategory, ContactType, Rank, SponsorStep
├── ui/
│   ├── dashboard.rs    summary cards + customer target + flow overview
│   ├── prospect_list.rs   Sponsor List table (score-sorted, step badge, advance)
│   ├── customer_list.rs   Customer Name List table
│   ├── followup.rs        per-ABO checklist with progress bar
│   ├── downline_tree.rs   recursive network org chart
│   └── forms.rs           add/edit modal with scoring inputs
└── utils/
    └── scoring.rs      score totals, rank/bonus tiers, transition validation + unit tests
```

See [docs/SYSTEM_ANALYSIS.md](docs/SYSTEM_ANALYSIS.md) for the domain model and
[docs/TEST_REPORT.md](docs/TEST_REPORT.md) for the test report and manual QA checklist.
