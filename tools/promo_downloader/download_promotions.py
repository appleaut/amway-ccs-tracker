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


def run_session(out_dir: Path, profile_dir: str, chrome: str) -> int:
    """Drive the browser and download all gallery images; return the count saved."""
    with sync_playwright() as p:
        ctx = p.chromium.launch_persistent_context(
            user_data_dir=profile_dir,
            executable_path=chrome,
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
    return counter


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

    try:
        counter = run_session(out_dir, a.profile_dir, a.chrome)
    except PWTimeout:
        print(
            "หมดเวลารอโหลดโปรโมชัน — อาจมีหน้า CAPTCHA ที่ยังไม่ได้ยืนยันในหน้าต่าง Chrome",
            file=sys.stderr,
            flush=True,
        )
        return 1
    except Exception as e:  # noqa: BLE001 - surface a concise reason, no traceback
        print(f"ดาวน์โหลดล้มเหลว: {e}", file=sys.stderr, flush=True)
        return 1

    log(f"Done. {counter} image(s) saved to {out_dir}")
    log(f"__RESULT__ saved={counter} dir={out_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
