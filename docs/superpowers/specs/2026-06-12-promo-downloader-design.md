# Promotion Downloader Implementation Design

**Date:** 2026-06-12
**Status:** Approved (design), pending plan

## Goal

Add an in-app tool to the Amway CCS Tracker that downloads the current month's
promotion gallery images from `amway.co.th` with one click, saving them to
`Downloads\amway-promotion-<YYYY-MM>\` named `0001.jpg`, `0002.jpg`, ….

## Background

`amway.co.th` is protected by DataDome bot protection: a plain HTTP request only
returns a JavaScript challenge page. A working downloader already exists as a
Python + Playwright script that drives a real, visible Chrome window (the user
solves a CAPTCHA in the window if one appears; a persistent Chrome profile keeps
the DataDome cookie between runs). The script:

1. Opens `https://amway.co.th/promo-listing/monthly-promotion`.
2. Collects every `a.content-card-link*` href (the promotion-item-pages).
3. On each item page, collects every `img.imageComp-product-gallery*` image at
   full resolution.
4. Downloads them as one continuous sequence; `.avif` images are converted to
   `.jpg` in memory.

This design embeds that proven script into the Rust app rather than reimplementing
browser automation in Rust.

## Locked Decisions

1. **Scope:** A launch button only (option A) — NOT a full in-app promotion
   catalog. The app does not store promotions in the database or display the
   images itself; it runs the downloader and opens the output folder.
2. **Execution:** Run in the background and report progress + result inside the
   app (option B) — a worker thread streams the script's stdout to the UI via a
   channel; the UI never blocks. The visible Chrome window still appears for
   CAPTCHA solving.
3. **Placement:** A new left-sidebar view (option A), `🖼  ดาวน์โหลดโปรโมชัน`,
   consistent with the app's per-feature view architecture.
4. **Integration:** Embed the script in the binary and version it in the repo
   (option A). The script is committed under `tools/promo_downloader/`, embedded
   via `include_str!`, materialized to `%APPDATA%\AmwayCCSTracker\promo_downloader\`
   at run time, and executed with the `py` launcher.

All three options still require **Python + Playwright + Chrome** on the machine;
the app detects their absence and shows a friendly Thai message.

## Architecture

Three layers with clear boundaries:

### `tools/promo_downloader/download_promotions.py` (moved into repo, parameterized)

- Accepts CLI arguments instead of hardcoded paths:
  - `--out-dir <path>` — output folder (the app passes
    `Downloads\amway-promotion-<YYYY-MM>`).
  - `--profile-dir <path>` — Chrome user-data dir for the persistent profile.
  - `--chrome <path>` — Chrome executable (the app detects it; optional, falls
    back to the standard install path).
- Before downloading, deletes existing numbered image files (`NNNN.jpg`/`.avif`/…)
  in the output folder so a re-run does not leave stale files from a prior run.
- Keeps the existing behavior: scroll to load all cards, collect links, collect
  full-resolution gallery images, download via the browser's request context
  (shares the DataDome cookie), convert `.avif` → `.jpg`.
- Prints human-readable progress lines (unchanged) AND a final machine-readable
  line: `__RESULT__ saved=<n> dir=<path>`.

### `src/promo.rs` (non-UI logic, unit-tested)

- `enum PromoMsg { Line(String), Done { saved: usize, dir: String }, Failed(String) }`.
- `fn start_download(out_dir, profile_dir, chrome) -> Receiver<PromoMsg>`:
  materializes the embedded script, verifies dependencies, spawns a worker thread
  that runs `py <script> --out-dir … --profile-dir … --chrome …`, reads child
  stdout line by line forwarding `PromoMsg::Line`, and on EOF parses the result
  line into `Done` or sends `Failed` (with the tail of output) on any error.
- Pure helpers for tests:
  - `month_folder_name(date: NaiveDate) -> String` → `"amway-promotion-2026-06"`.
  - `parse_result_line(&str) -> Option<(usize, String)>` → parses
    `__RESULT__ saved=<n> dir=<path>`.
  - `detect_chrome() -> Option<PathBuf>` (checks the standard install paths).
- The embedded script is included via
  `include_str!("../tools/promo_downloader/download_promotions.py")`.

### `src/ui/promo_download.rs` (thin view)

- Heading + an "ℹ วิธีทำงาน" card explaining: opens real Chrome, solve CAPTCHA in
  the window if shown, saves to `Downloads\amway-promotion-<month>`, converts
  avif→jpg, names `0001…`.
- Button `⬇ ดาวน์โหลดโปรโมชันเดือนนี้`, disabled while a run is in progress.
- While running: a spinner and a scrollable log area showing streamed progress.
- On completion: `ดาวน์โหลด N รูปแล้ว` with the folder path shown as a clickable
  link (reusing the existing status-bar mechanism that opens a path with
  `explorer`, which works for folders). The last result also stays shown in the
  view.

### `src/app.rs` wiring

- New `View::PromoDownload` variant + sidebar entry `🖼  ดาวน์โหลดโปรโมชัน` +
  dispatch arm.
- New `AppState` fields:
  - `promo_running: bool`
  - `promo_rx: Option<std::sync::mpsc::Receiver<crate::promo::PromoMsg>>`
  - `promo_log: Vec<String>` (capped to the last ~200 lines)
  - `promo_last_result: Option<String>`
- `update()` polls the receiver non-blocking every frame (like the existing
  chart-export poll), appends `Line`s to `promo_log`, and on `Done`/`Failed`
  clears `promo_running`, sets the status bar, and (on `Done`) stores the folder
  path as the clickable link. Calls `ctx.request_repaint()` while running so the
  UI keeps updating.

## Data Flow

Button click → app computes `out_dir` (`Downloads\amway-promotion-<current month>`),
`profile_dir` (under appdata), detects Chrome → `promo::start_download(...)` returns
a `Receiver`, stored in `AppState`; `promo_running = true` → worker thread runs the
Python process, streaming stdout lines → `update()` drains the channel each frame →
on `__RESULT__` → `Done { saved, dir }` → view shows result, button re-enabled,
folder link set.

## Error Handling

All messages are in Thai and surfaced both in the view and the status bar; the
button always returns to a clickable state:

- `py` not found / `import playwright` fails / Chrome not found → an instructive
  message on how to install the missing dependency.
- Non-zero process exit or a missing `__RESULT__` line → `Failed` with the tail of
  the captured output as the reason.
- Worker thread error (spawn failure, I/O error) → `Failed` with the error text.

## Behavior Details

- **Month:** always the current calendar month; the folder name is derived from
  the system date at click time.
- **Re-run:** writing into the same month folder is safe — the script clears the
  existing numbered files first, then writes a fresh `0001…` sequence.
- **Output location:** `C:\Users\<user>\Downloads\amway-promotion-<YYYY-MM>\`
  (resolved from the user profile, not hardcoded to one account).

## Testing

- **Unit tests** (`src/promo.rs`): `month_folder_name` formatting,
  `parse_result_line` (well-formed, malformed, missing fields), command-argument
  assembly, and `detect_chrome` path logic.
- **Manual verification:** run the app, open the new view, click the button,
  confirm the Chrome window opens, progress streams into the log, files land in
  the month folder named `0001…` (all `.jpg`), and the folder link opens. Verify
  the `🖼` glyph renders (not tofu) per the egui bundled-font-subset constraint.

## Related Adjustment

The Settings screen currently states *"ไม่มีการเชื่อมต่อเครือข่าย"* (no network
connection), which will no longer be strictly true. Update the copy to clarify
that contact data stays fully local while the promotion downloader connects to
`amway.co.th` only when explicitly triggered.

## Out of Scope (YAGNI)

- Storing promotions or images in the database.
- Displaying downloaded images inside the app.
- Linking promotions to contacts, meetings, or activities.
- Choosing an arbitrary month or custom output folder.
- Reimplementing browser automation natively in Rust.
