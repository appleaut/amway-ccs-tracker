# Promotion Downloader Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an in-app "ดาวน์โหลดโปรโมชัน" page that downloads the current month's amway.co.th promotion images with one click, running the embedded Python+Playwright script in a background thread and reporting progress in the UI.

**Architecture:** A proven Python+Playwright downloader is embedded in the binary (`include_str!`), materialized to `%APPDATA%\AmwayCCSTracker\promo_downloader\` at run time, and executed via the `py` launcher. A worker thread streams the script's stdout/stderr back to the egui UI over an `mpsc` channel; the UI polls each frame and never blocks. Images save to `Downloads\amway-promotion-<YYYY-MM>\`.

**Tech Stack:** Rust, eframe/egui 0.28, chrono, std::process + std::thread + std::sync::mpsc; Python 3 + Playwright + Chrome (runtime deps, detected at run time).

**Standing constraints:**
- Do NOT run `cargo fmt` (repo is hand-formatted; no rustfmt.toml). Verify with `cargo build` / `cargo test` only.
- Commit messages end with `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- egui emoji glyphs must exist in egui's bundled font subset or they render as tofu — verify by running the app.

---

## File Structure

- Create: `tools/promo_downloader/download_promotions.py` — the parameterized downloader script (embedded into the binary).
- Create: `src/promo.rs` — non-UI runner: `PromoMsg`, `start_download`, pure helpers (`month_folder_name`, `parse_result_line`, `detect_chrome`, `downloads_dir`). Unit-tested.
- Create: `src/ui/promo_download.rs` — the thin view.
- Modify: `src/main.rs` — add `mod promo;`.
- Modify: `src/ui/mod.rs` — add `pub mod promo_download;` and `View::PromoDownload`.
- Modify: `src/app.rs` — `AppState` fields + init + sidebar entry + dispatch + channel poll in `update()` + Settings copy tweak.

---

## Task 1: Parameterized downloader script in the repo

**Files:**
- Create: `tools/promo_downloader/download_promotions.py`

This is the working script (already verified end-to-end) made portable: it takes `--out-dir`, `--profile-dir`, `--chrome` instead of hardcoded paths, clears stale numbered files before a run, converts `.avif`→`.jpg`, and prints a final machine-readable `__RESULT__ saved=<n> dir=<path>` line.

- [ ] **Step 1: Write the script**

```python
"""Download Amway monthly-promotion gallery images.

The site (amway.co.th) sits behind DataDome bot protection, so this drives a
real, visible Chrome window via Playwright. If a CAPTCHA appears, solve it in
the window and the script continues automatically once the cards load.

Invoked by the Amway CCS Tracker app with explicit paths:
    py download_promotions.py --out-dir <dir> --profile-dir <dir> [--chrome <exe>]

Flow:
  1. Open the monthly-promotion listing page.
  2. Collect every  a.content-card-link*  href (the promotion-item-pages).
  3. On each item page, collect every  img.imageComp-product-gallery*  image
     (highest resolution available).
  4. Download all images into <out-dir> named 0001.jpg, 0002.jpg, ... as one
     continuous sequence; .avif images are converted to .jpg.

Prints human-readable progress, then a final line:
    __RESULT__ saved=<n> dir=<out-dir>
"""

import argparse
import io
import mimetypes
import re
import sys
import time
from pathlib import Path
from urllib.parse import urljoin, urlparse

from PIL import Image
from playwright.sync_api import sync_playwright, TimeoutError as PWTimeout

LISTING_URL = "https://amway.co.th/promo-listing/monthly-promotion"

CARD_SELECTOR = 'a[class*="content-card-link"]'
GALLERY_SELECTOR = 'img[class*="imageComp-product-gallery"]'

DEFAULT_CHROME = r"C:\Program Files\Google\Chrome\Application\chrome.exe"

# How long to wait for the user to clear a DataDome CAPTCHA (seconds).
CAPTCHA_WAIT = 240

# Files this tool owns and may delete before a fresh run.
NUMBERED_RE = re.compile(r"^\d{4}\.(jpg|jpeg|png|gif|webp|bmp|avif)$", re.IGNORECASE)


def log(msg: str) -> None:
    print(msg, flush=True)


def clean_numbered(folder: Path) -> None:
    """Remove this tool's previously-downloaded NNNN.<ext> files so a re-run does
    not leave stale images from a prior run behind."""
    if not folder.exists():
        return
    for f in folder.iterdir():
        if f.is_file() and NUMBERED_RE.match(f.name):
            f.unlink()


def wait_for_cards(page) -> None:
    try:
        page.wait_for_selector(CARD_SELECTOR, timeout=20_000)
        return
    except PWTimeout:
        pass
    log("")
    log("  >> Cards not visible yet. If a CAPTCHA / 'verify you are human'")
    log("     page is showing in the Chrome window, please solve it now.")
    log(f"     Waiting up to {CAPTCHA_WAIT}s for the promotions to load...")
    page.wait_for_selector(CARD_SELECTOR, timeout=CAPTCHA_WAIT * 1000)


def scroll_to_load_all(page) -> None:
    last = -1
    for _ in range(30):
        count = page.locator(CARD_SELECTOR).count()
        if count == last:
            break
        last = count
        page.mouse.wheel(0, 4000)
        time.sleep(1.2)
    page.evaluate("window.scrollTo(0, 0)")


def collect_card_links(page) -> list[str]:
    hrefs = page.eval_on_selector_all(
        CARD_SELECTOR,
        "els => els.map(e => e.getAttribute('href')).filter(Boolean)",
    )
    seen, out = set(), []
    for h in hrefs:
        url = urljoin(LISTING_URL, h)
        if url not in seen:
            seen.add(url)
            out.append(url)
    return out


def best_from_srcset(srcset: str) -> str | None:
    best_url, best_w = None, -1
    for part in srcset.split(","):
        part = part.strip()
        if not part:
            continue
        bits = part.split()
        url = bits[0]
        w = 0
        if len(bits) > 1 and bits[1].endswith("w"):
            try:
                w = int(bits[1][:-1])
            except ValueError:
                w = 0
        if w >= best_w:
            best_w, best_url = w, url
    return best_url


def collect_gallery_images(page, base_url: str) -> list[str]:
    try:
        page.wait_for_selector(GALLERY_SELECTOR, timeout=20_000)
    except PWTimeout:
        return []
    raw = page.eval_on_selector_all(
        GALLERY_SELECTOR,
        """els => els.map(e => ({
            src: e.getAttribute('src') || e.getAttribute('data-src') || '',
            srcset: e.getAttribute('srcset') || e.getAttribute('data-srcset') || ''
        }))""",
    )
    seen, out = set(), []
    for item in raw:
        pick = best_from_srcset(item["srcset"]) if item["srcset"] else None
        pick = pick or item["src"]
        if not pick:
            continue
        url = urljoin(base_url, pick)
        if url not in seen:
            seen.add(url)
            out.append(url)
    return out


def avif_to_jpeg(body: bytes) -> bytes:
    """Decode AVIF bytes and re-encode as JPEG (flattening any alpha onto white)."""
    im = Image.open(io.BytesIO(body))
    if im.mode in ("RGBA", "LA", "P"):
        bg = Image.new("RGB", im.size, (255, 255, 255))
        im = im.convert("RGBA")
        bg.paste(im, mask=im.split()[-1])
        im = bg
    else:
        im = im.convert("RGB")
    buf = io.BytesIO()
    im.save(buf, "JPEG", quality=95)
    return buf.getvalue()


def ext_for(url: str, content_type: str | None) -> str:
    if content_type:
        ct = content_type.split(";")[0].strip().lower()
        guessed = mimetypes.guess_extension(ct)
        if guessed:
            return ".jpg" if guessed == ".jpe" else guessed
    path_ext = Path(urlparse(url).path).suffix.lower()
    if re.fullmatch(r"\.(jpg|jpeg|png|gif|webp|bmp)", path_ext):
        return path_ext
    return ".jpg"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--profile-dir", required=True)
    ap.add_argument("--chrome", default=DEFAULT_CHROME)
    a = ap.parse_args()

    out_dir = Path(a.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    Path(a.profile_dir).mkdir(parents=True, exist_ok=True)
    clean_numbered(out_dir)
    log(f"Output folder: {out_dir}")

    with sync_playwright() as p:
        ctx = p.chromium.launch_persistent_context(
            user_data_dir=a.profile_dir,
            executable_path=a.chrome,
            headless=False,
            viewport={"width": 1366, "height": 900},
            args=["--disable-blink-features=AutomationControlled"],
        )
        page = ctx.pages[0] if ctx.pages else ctx.new_page()

        log("Opening promotion listing...")
        page.goto(LISTING_URL, wait_until="domcontentloaded", timeout=60_000)
        wait_for_cards(page)
        scroll_to_load_all(page)

        links = collect_card_links(page)
        log(f"Found {len(links)} promotion item page(s).")

        counter = 0
        for i, link in enumerate(links, 1):
            log(f"[{i}/{len(links)}] {link}")
            try:
                page.goto(link, wait_until="domcontentloaded", timeout=60_000)
            except PWTimeout:
                log("   ! page load timed out, skipping")
                continue
            images = collect_gallery_images(page, link)
            log(f"   {len(images)} gallery image(s)")
            for img_url in images:
                try:
                    resp = ctx.request.get(img_url, timeout=60_000)
                    if not resp.ok:
                        log(f"   ! HTTP {resp.status} for {img_url}")
                        continue
                    body = resp.body()
                    ext = ext_for(img_url, resp.headers.get("content-type"))
                    counter += 1
                    if ext == ".avif":
                        body = avif_to_jpeg(body)
                        ext = ".jpg"
                    fname = out_dir / f"{counter:04d}{ext}"
                    fname.write_bytes(body)
                    log(f"   saved {fname.name}  ({len(body):,} bytes)")
                except Exception as e:  # noqa: BLE001 - report and continue
                    log(f"   ! error downloading {img_url}: {e}")

        ctx.close()

    log(f"Done. {counter} image(s) saved to {out_dir}")
    log(f"__RESULT__ saved={counter} dir={out_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Verify it parses and the CLI is wired**

Run: `py -m py_compile tools/promo_downloader/download_promotions.py`
Expected: exit 0, no output.

Run: `py tools/promo_downloader/download_promotions.py --help`
Expected: usage text listing `--out-dir`, `--profile-dir`, `--chrome`.

- [ ] **Step 3: Commit**

```bash
git add tools/promo_downloader/download_promotions.py
git commit -m "Add parameterized promotion-downloader script"
```

---

## Task 2: `src/promo.rs` runner + unit tests

**Files:**
- Create: `src/promo.rs`
- Modify: `src/main.rs` (add `mod promo;`)
- Test: inline `#[cfg(test)]` in `src/promo.rs`

- [ ] **Step 1: Write `src/promo.rs`**

```rust
//! Promotion downloader: runs the embedded Python + Playwright script in a
//! background thread and streams its progress back to the UI over a channel.
//!
//! The script is embedded so the binary stays self-contained; it is written out
//! to `%APPDATA%\AmwayCCSTracker\promo_downloader\` at run time and executed via
//! the `py` launcher. Images are saved to `Downloads\amway-promotion-<YYYY-MM>\`.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};

use chrono::NaiveDate;

use crate::error::{AppError, Result};

/// The downloader script, embedded so the binary stays self-contained.
const SCRIPT: &str = include_str!("../tools/promo_downloader/download_promotions.py");

/// A message from the download worker thread to the UI.
pub enum PromoMsg {
    /// One line of progress output from the script.
    Line(String),
    /// The run finished successfully: `saved` images in `dir`.
    Done { saved: usize, dir: String },
    /// The run failed; the string is a Thai-readable reason.
    Failed(String),
}

/// Folder name for `date`'s month, e.g. `"amway-promotion-2026-06"`.
pub fn month_folder_name(date: NaiveDate) -> String {
    date.format("amway-promotion-%Y-%m").to_string()
}

/// Parse the script's final machine-readable line
/// `__RESULT__ saved=<n> dir=<path>` into `(saved, dir)`. Returns `None` if the
/// line is not a well-formed result line.
pub fn parse_result_line(line: &str) -> Option<(usize, String)> {
    let rest = line.strip_prefix("__RESULT__")?.trim_start();
    let after_saved = rest.strip_prefix("saved=")?;
    let (saved_str, dir_part) = after_saved.split_once(' ')?;
    let saved: usize = saved_str.parse().ok()?;
    let dir = dir_part.trim().strip_prefix("dir=")?.to_string();
    if dir.is_empty() {
        return None;
    }
    Some((saved, dir))
}

/// `%USERPROFILE%\Downloads`.
pub fn downloads_dir() -> Result<PathBuf> {
    let base = std::env::var("USERPROFILE")
        .map_err(|_| AppError::validation("ไม่พบโฟลเดอร์โปรไฟล์ผู้ใช้ (USERPROFILE)"))?;
    Ok(PathBuf::from(base).join("Downloads"))
}

/// `%APPDATA%\AmwayCCSTracker\promo_downloader`.
fn runtime_dir() -> Result<PathBuf> {
    let base = std::env::var("APPDATA")
        .map_err(|_| AppError::validation("ไม่พบโฟลเดอร์ข้อมูลแอป (APPDATA)"))?;
    Ok(PathBuf::from(base)
        .join("AmwayCCSTracker")
        .join("promo_downloader"))
}

/// Locate a real Chrome install (best for passing DataDome). `None` if absent.
pub fn detect_chrome() -> Option<PathBuf> {
    let candidates = [
        std::env::var("ProgramFiles").ok(),
        std::env::var("ProgramFiles(x86)").ok(),
        std::env::var("LOCALAPPDATA").ok(),
    ];
    candidates
        .into_iter()
        .flatten()
        .map(|base| PathBuf::from(base).join(r"Google\Chrome\Application\chrome.exe"))
        .find(|p| p.exists())
}

/// Write the embedded script to `dir`, returning its path.
fn materialize_script(dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("download_promotions.py");
    std::fs::write(&path, SCRIPT)?;
    Ok(path)
}

/// Find a Python interpreter that can import playwright; returns the program
/// name to invoke (`py` / `python` / `python3`).
fn find_python() -> Result<String> {
    for prog in ["py", "python", "python3"] {
        let ok = Command::new(prog)
            .args(["-c", "import playwright"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Ok(prog.to_string());
        }
    }
    Err(AppError::validation(
        "ไม่พบ Python ที่ติดตั้ง Playwright — ติดตั้งด้วยคำสั่ง: pip install playwright",
    ))
}

/// Spawn the downloader for `out_dir` on a worker thread and return the receiver
/// the UI polls. All setup (dependency checks, Chrome detection, script
/// materialization) happens on the thread; any failure arrives as
/// `PromoMsg::Failed`, so this never blocks the caller.
pub fn start_download(out_dir: PathBuf) -> Receiver<PromoMsg> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let tx_run = tx.clone();
        let run = move || -> Result<(usize, String)> {
            let python = find_python()?;
            let chrome = detect_chrome().ok_or_else(|| {
                AppError::validation("ไม่พบ Google Chrome — กรุณาติดตั้ง Chrome ก่อนใช้งาน")
            })?;
            let rt = runtime_dir()?;
            let script = materialize_script(&rt)?;
            let profile = rt.join("chrome-profile");
            std::fs::create_dir_all(&out_dir)?;

            let mut child = Command::new(&python)
                .arg(&script)
                .arg("--out-dir")
                .arg(&out_dir)
                .arg("--profile-dir")
                .arg(&profile)
                .arg("--chrome")
                .arg(&chrome)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;

            // Drain stderr concurrently so a chatty child can't deadlock on a
            // full stderr pipe while we read stdout.
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| AppError::validation("อ่าน output ไม่ได้"))?;
            let err_tx = tx_run.clone();
            let err_handle = std::thread::spawn(move || {
                let mut tail = String::new();
                let mut reader = BufReader::new(stderr);
                let mut buf = String::new();
                let _ = reader.read_to_string(&mut buf);
                for line in buf.lines() {
                    let _ = err_tx.send(PromoMsg::Line(line.to_string()));
                }
                tail.push_str(buf.trim());
                tail
            });

            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| AppError::validation("อ่าน output ไม่ได้"))?;
            let mut result: Option<(usize, String)> = None;
            let mut tail: Vec<String> = Vec::new();
            for line in BufReader::new(stdout).lines() {
                let line = line?;
                if let Some(r) = parse_result_line(&line) {
                    result = Some(r);
                } else {
                    let _ = tx_run.send(PromoMsg::Line(line.clone()));
                    tail.push(line);
                    if tail.len() > 20 {
                        tail.remove(0);
                    }
                }
            }
            let status = child.wait()?;
            let err_tail = err_handle.join().unwrap_or_default();
            match result {
                Some(r) if status.success() => Ok(r),
                _ => {
                    let detail = if !err_tail.trim().is_empty() {
                        err_tail.trim().to_string()
                    } else {
                        tail.join("\n")
                    };
                    Err(AppError::validation(format!("ดาวน์โหลดไม่สำเร็จ: {detail}")))
                }
            }
        };
        match run() {
            Ok((saved, dir)) => {
                let _ = tx.send(PromoMsg::Done { saved, dir });
            }
            Err(e) => {
                let _ = tx.send(PromoMsg::Failed(e.to_string()));
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn month_folder_name_formats_year_month() {
        let d = NaiveDate::from_ymd_opt(2026, 6, 9).unwrap();
        assert_eq!(month_folder_name(d), "amway-promotion-2026-06");
        let d2 = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        assert_eq!(month_folder_name(d2), "amway-promotion-2026-12");
    }

    #[test]
    fn parse_result_line_well_formed() {
        let line = r"__RESULT__ saved=22 dir=C:\Users\Aut\Downloads\amway-promotion-2026-06";
        let (n, dir) = parse_result_line(line).unwrap();
        assert_eq!(n, 22);
        assert_eq!(dir, r"C:\Users\Aut\Downloads\amway-promotion-2026-06");
    }

    #[test]
    fn parse_result_line_zero_saved() {
        let (n, dir) = parse_result_line(r"__RESULT__ saved=0 dir=C:\tmp").unwrap();
        assert_eq!(n, 0);
        assert_eq!(dir, r"C:\tmp");
    }

    #[test]
    fn parse_result_line_rejects_malformed() {
        assert!(parse_result_line("just a normal log line").is_none());
        assert!(parse_result_line("__RESULT__ saved=abc dir=C:\\x").is_none());
        assert!(parse_result_line("__RESULT__ saved=5").is_none());
        assert!(parse_result_line("__RESULT__ saved=5 dir=").is_none());
    }
}
```

- [ ] **Step 2: Register the module in `src/main.rs`**

Add `mod promo;` alongside the other `mod` lines (after `mod models;`):

```rust
mod app;
mod db;
mod error;
mod models;
mod promo;
mod ui;
mod utils;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test promo::`
Expected: the 4 `promo::tests::*` tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/promo.rs src/main.rs
git commit -m "Add promotion-downloader runner (background thread + channel)"
```

---

## Task 3: `src/ui/promo_download.rs` view + module/View registration

**Files:**
- Create: `src/ui/promo_download.rs`
- Modify: `src/ui/mod.rs` (add `pub mod promo_download;` and `View::PromoDownload`)

This task references `AppState` fields added in Task 4 (`promo_running`, `promo_log`, `promo_last_result`, `promo_rx`). It will not compile until Task 4 adds them — that is expected; build/verify happens at the end of Task 4.

- [ ] **Step 1: Write `src/ui/promo_download.rs`**

```rust
//! Promotion downloader page: one button that runs the embedded Python+Playwright
//! downloader in the background and streams its progress here. Images save to
//! `Downloads\amway-promotion-<current month>\`.

use crate::app::AppState;
use crate::promo;
use crate::ui::{ACCENT, ACCENT_STRONG};

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("ดาวน์โหลดโปรโมชัน / Promotions");
    ui.label(
        egui::RichText::new("ดาวน์โหลดรูปโปรโมชันประจำเดือนจาก amway.co.th")
            .weak()
            .small(),
    );
    ui.add_space(8.0);

    // How-it-works card.
    egui::Frame::group(ui.style())
        .rounding(8.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("ℹ วิธีทำงาน").strong().color(ACCENT_STRONG));
            ui.add_space(4.0);
            ui.label("• เปิดหน้าต่าง Chrome จริง — ถ้ามีหน้า CAPTCHA ให้ยืนยันในหน้าต่างนั้น");
            ui.label("• บันทึกไปที่ Downloads\\amway-promotion-<เดือนปัจจุบัน>");
            ui.label("• ตั้งชื่อรูปเรียง 0001, 0002, … และแปลงไฟล์ .avif เป็น .jpg ให้");
            ui.label("• ต้องมี Python (+Playwright) และ Google Chrome ติดตั้งบนเครื่อง");
        });
    ui.add_space(10.0);

    let today = chrono::Local::now().date_naive();
    let folder = promo::month_folder_name(today);

    ui.horizontal(|ui| {
        let btn = egui::Button::new(
            egui::RichText::new("⬇  ดาวน์โหลดโปรโมชันเดือนนี้").size(16.0),
        )
        .fill(ACCENT);
        if ui.add_enabled(!app.promo_running, btn).clicked() {
            start(app);
        }
        if app.promo_running {
            ui.add(egui::Spinner::new());
            ui.label("กำลังดาวน์โหลด…");
        }
    });
    ui.label(
        egui::RichText::new(format!("โฟลเดอร์ปลายทาง: Downloads\\{folder}"))
            .small()
            .weak(),
    );

    if let Some(result) = &app.promo_last_result {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(result).strong().color(ACCENT_STRONG));
    }

    // Streamed progress log.
    if !app.promo_log.is_empty() {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(egui::RichText::new("ความคืบหน้า").small().weak());
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &app.promo_log {
                    ui.label(egui::RichText::new(line).monospace().size(12.5));
                }
            });
    }
}

/// Kick off a background download for the current month.
fn start(app: &mut AppState) {
    let today = chrono::Local::now().date_naive();
    let folder = promo::month_folder_name(today);
    match promo::downloads_dir() {
        Ok(base) => {
            let out_dir = base.join(folder);
            app.promo_log.clear();
            app.promo_last_result = None;
            app.promo_running = true;
            app.promo_rx = Some(promo::start_download(out_dir));
            app.set_status("เริ่มดาวน์โหลดโปรโมชัน…");
        }
        Err(e) => app.set_error(e),
    }
}
```

- [ ] **Step 2: Register module + View variant in `src/ui/mod.rs`**

Add to the module list (keep alphabetical, after `pub mod prospect_list;`... place near siblings):

```rust
pub mod promo_download;
```

Add a variant to the `View` enum (after `Advances,`):

```rust
pub enum View {
    Dashboard,
    Prospects,
    Customers,
    Abos,
    FollowUp,
    Meetings,
    Todos,
    TodoSchedules,
    Advances,
    PromoDownload,
    Network,
    Activities,
    ActivityKinds,
    Settings,
}
```

- [ ] **Step 3: Commit** (compiles only after Task 4; commit together with Task 4 if preferred, or commit now knowing the build is red until Task 4)

```bash
git add src/ui/promo_download.rs src/ui/mod.rs
git commit -m "Add promotion-downloader view + View::PromoDownload"
```

---

## Task 4: App wiring — state, sidebar, dispatch, channel poll, Settings copy

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add `AppState` fields**

After the `last_gen_check` field, add:

```rust
    /// Promotion-downloader: whether a background run is in progress.
    pub promo_running: bool,
    /// Channel from the download worker thread (`None` when idle).
    pub promo_rx: Option<std::sync::mpsc::Receiver<crate::promo::PromoMsg>>,
    /// Streamed progress lines, capped to the most recent ~200.
    pub promo_log: Vec<String>,
    /// Summary of the last finished run (success or error), shown in the view.
    pub promo_last_result: Option<String>,
```

- [ ] **Step 2: Initialize the fields in `AppState::new`**

After `last_gen_check: ...,` in the returned struct literal:

```rust
            promo_running: false,
            promo_rx: None,
            promo_log: Vec::new(),
            promo_last_result: None,
```

- [ ] **Step 3: Add the sidebar entry**

In `sidebar`, add to the `items` array after the Advances line:

```rust
            (View::Advances, "💵  สำรองจ่าย"),
            (View::PromoDownload, "🖼  ดาวน์โหลดโปรโมชัน"),
            (View::Network, "🌳  เครือข่าย"),
```

- [ ] **Step 4: Add the dispatch arm**

In the `CentralPanel` match in `update`, after the `Advances` arm:

```rust
            View::Advances => ui::advances::render(self, ui),
            View::PromoDownload => ui::promo_download::render(self, ui),
            View::Network => ui::downline_tree::render(self, ui),
```

- [ ] **Step 5: Poll the download channel each frame**

At the top of `update`, right after the existing recurring-todo generation block (before the `SidePanel` is shown), add:

```rust
        // Drain promotion-download progress from the worker thread. Take the
        // receiver out so we can mutate `self` while processing messages; put it
        // back unless the run finished.
        if let Some(rx) = self.promo_rx.take() {
            let mut finished = false;
            loop {
                match rx.try_recv() {
                    Ok(crate::promo::PromoMsg::Line(l)) => {
                        self.promo_log.push(l);
                        let n = self.promo_log.len();
                        if n > 200 {
                            self.promo_log.drain(0..n - 200);
                        }
                    }
                    Ok(crate::promo::PromoMsg::Done { saved, dir }) => {
                        self.promo_running = false;
                        self.promo_last_result = Some(format!("ดาวน์โหลด {saved} รูปแล้ว"));
                        self.set_saved_image(
                            format!("ดาวน์โหลดโปรโมชัน {saved} รูปแล้ว"),
                            dir,
                        );
                        finished = true;
                    }
                    Ok(crate::promo::PromoMsg::Failed(reason)) => {
                        self.promo_running = false;
                        self.promo_last_result = Some(format!("ผิดพลาด: {reason}"));
                        self.set_error(reason);
                        finished = true;
                    }
                    Err(_) => break, // empty (keep) or disconnected
                }
            }
            if !finished {
                self.promo_rx = Some(rx);
            }
        }
        if self.promo_running {
            ctx.request_repaint();
        }
```

Note: `set_saved_image` stores the path as the clickable status-bar link, which opens via `explorer` — and `explorer <folder>` opens the folder in Windows Explorer, exactly what we want.

- [ ] **Step 6: Update the Settings "no network" copy**

In `settings`, replace the line:

```rust
        ui.label(
            egui::RichText::new("ข้อมูลถูกบันทึกในเครื่อง (Local SQLite) ไม่มีการเชื่อมต่อเครือข่าย")
                .small()
                .weak(),
        );
```

with:

```rust
        ui.label(
            egui::RichText::new(
                "ข้อมูลรายชื่อถูกบันทึกในเครื่อง (Local SQLite) • ตัวดาวน์โหลดโปรโมชันจะเชื่อมต่อ amway.co.th เฉพาะตอนสั่งงานเท่านั้น",
            )
            .small()
            .weak(),
        );
```

- [ ] **Step 7: Build and test**

Run: `cargo build`
Expected: compiles cleanly (no warnings introduced).

Run: `cargo test`
Expected: all existing tests plus the 4 new `promo::tests::*` pass.

- [ ] **Step 8: Commit**

```bash
git add src/app.rs
git commit -m "Wire promotion-downloader page into the app shell"
```

---

## Task 5: Visual + glyph verification (manual)

**Files:** none (verification only)

- [ ] **Step 1: Verify glyphs render (not tofu)**

Build and run the app:

Run: `cargo run`

Confirm in the running window:
- The sidebar entry `🖼  ดาวน์โหลดโปรโมชัน` shows its icon (not a □ tofu box).
- On the page, the `⬇` on the button and `ℹ` on the card render.

If any glyph is tofu, replace it with one already proven to render in this app's sidebar/buttons (e.g. swap `🖼` for `📅`-style known-good icons, or drop the leading emoji from the button and keep text only). Re-run to confirm. Commit any glyph change:

```bash
git add src/app.rs src/ui/promo_download.rs
git commit -m "Use font-subset-safe glyphs for the promotion page"
```

- [ ] **Step 2: Functional smoke test**

In the running app, open the page and click `⬇ ดาวน์โหลดโปรโมชันเดือนนี้`. Confirm:
- A Chrome window opens (solve a CAPTCHA there if shown).
- Progress lines stream into the log area; the button is disabled while running.
- On completion, `ดาวน์โหลด N รูปแล้ว` appears and the status-bar folder link opens `Downloads\amway-promotion-<month>` in Explorer.
- The folder contains `0001.jpg`, `0002.jpg`, … with no `.avif` files left.

- [ ] **Step 3: Final review + finish the branch**

Dispatch the final code review, then use superpowers:finishing-a-development-branch.

---

## Self-Review

- **Spec coverage:** launch button ✅ (Task 3), background run + in-app progress ✅ (Tasks 2/3/4), new sidebar view ✅ (Tasks 3/4), embedded+versioned script ✅ (Tasks 1/2), avif→jpg ✅ (Task 1), re-run cleans stale files ✅ (Task 1), dependency/Chrome detection + Thai errors ✅ (Task 2), current-month folder ✅ (Tasks 2/3), Settings copy fix ✅ (Task 4), unit tests + manual verification ✅ (Tasks 2/5).
- **Type consistency:** `PromoMsg`, `start_download(PathBuf) -> Receiver<PromoMsg>`, `month_folder_name`, `downloads_dir`, `parse_result_line`, `detect_chrome`, and the four `AppState` fields are named identically across Tasks 2/3/4.
- **Placeholders:** none — every code step contains complete code.
