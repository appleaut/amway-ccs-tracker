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
            let stderr = match child.stderr.take() {
                Some(s) => s,
                None => {
                    let _ = child.kill();
                    return Err(AppError::validation("อ่าน output ไม่ได้"));
                }
            };
            let err_tx = tx_run.clone();
            let err_handle = std::thread::spawn(move || {
                let mut reader = BufReader::new(stderr);
                let mut buf = String::new();
                let _ = reader.read_to_string(&mut buf);
                for line in buf.lines() {
                    let _ = err_tx.send(PromoMsg::Line(line.to_string()));
                }
                buf.trim().to_string()
            });

            let stdout = match child.stdout.take() {
                Some(s) => s,
                None => {
                    let _ = child.kill();
                    return Err(AppError::validation("อ่าน output ไม่ได้"));
                }
            };
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
            let status = match child.wait() {
                Ok(s) => s,
                Err(e) => {
                    let _ = child.kill();
                    return Err(e.into());
                }
            };
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
    fn parse_result_line_dir_with_spaces() {
        let line = r"__RESULT__ saved=3 dir=C:\Users\John Smith\Downloads\amway-promotion-2026-06";
        let (n, dir) = parse_result_line(line).unwrap();
        assert_eq!(n, 3);
        assert_eq!(dir, r"C:\Users\John Smith\Downloads\amway-promotion-2026-06");
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
