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
| Tables       | `egui_extras` 0.28 (`TableBuilder`)      |
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
│   └── enums.rs        Gender, NetworkCategory, ContactType, Rank, SponsorStep, ActivityKind
├── ui/
│   ├── dashboard.rs    summary cards + customer target + flow overview
│   ├── prospect_list.rs   Sponsor List table (sortable, editable step, advance)
│   ├── customer_list.rs   Customer Name List table (sortable)
│   ├── abo_list.rs        ABO management table (sortable) + 📊 Rank Advisor
│   ├── followup.rs        per-ABO checklist with progress bar
│   ├── downline_tree.rs   radial node chart ("me" centre shows my rank, draggable, auto-arrange, self Rank Advisor)
│   ├── forms.rs           add/edit modal with scoring inputs
│   ├── confirm.rs         shared delete-confirmation modal
│   ├── rank_advisor.rs    rank assessment for an ABO and for "me" (PPV + downline legs → qualified rank)
│   └── activity_log.rs    per-contact interaction history (📝, all tables)
└── utils/
    └── scoring.rs      score totals, rank/bonus tiers, transition validation + unit tests
```

See [docs/SYSTEM_ANALYSIS.md](docs/SYSTEM_ANALYSIS.md) for the domain model and
[docs/TEST_REPORT.md](docs/TEST_REPORT.md) for the test report and manual QA checklist.
