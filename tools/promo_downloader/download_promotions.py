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

# Files this tool owns and may delete before a fresh run. Matches any purely
# numeric image name (0001.jpg, but also 3-/5-digit names left by older tool
# versions) so a re-run starts from a clean, gap-free 0001 sequence.
NUMBERED_RE = re.compile(r"^\d{1,6}\.(jpg|jpeg|png|gif|webp|bmp|avif)$", re.IGNORECASE)


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


# Card-collection tuning. The listing lazy-loads promotions in batches as you
# scroll, so we must (a) collect links progressively — not once at the end, or a
# batch that scrolls out of view can be missed — and (b) keep scrolling until the
# count stays unchanged for several consecutive passes, not just one, because a
# single plateau often means "the next batch hasn't arrived yet", not "done".
SCROLL_MAX_PASSES = 40       # hard cap on scroll iterations
SCROLL_STABLE_PASSES = 4     # consecutive no-growth passes before we stop
SCROLL_MIN_PASSES = 6        # always scroll at least this many times first
SCROLL_WAIT = 1.5            # seconds to let a batch load after each scroll


def load_and_collect_cards(page) -> list[str]:
    """Scroll the listing to the bottom repeatedly, accumulating every promotion
    card link as batches load. Returns links in first-seen order.

    Stops once the unique-link count has not grown for `SCROLL_STABLE_PASSES`
    consecutive passes (after a minimum number of passes), or at the pass cap."""
    seen, order = set(), []

    def harvest() -> None:
        hrefs = page.eval_on_selector_all(
            CARD_SELECTOR,
            "els => els.map(e => e.getAttribute('href')).filter(Boolean)",
        )
        for h in hrefs:
            url = urljoin(LISTING_URL, h)
            if url not in seen:
                seen.add(url)
                order.append(url)

    stable = 0
    for i in range(SCROLL_MAX_PASSES):
        before = len(seen)
        harvest()
        page.evaluate("window.scrollTo(0, document.body.scrollHeight)")
        time.sleep(SCROLL_WAIT)
        harvest()
        if len(seen) == before:
            stable += 1
            if stable >= SCROLL_STABLE_PASSES and i + 1 >= SCROLL_MIN_PASSES:
                break
        else:
            stable = 0
    page.evaluate("window.scrollTo(0, 0)")
    return order


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


def looks_like_avif(body: bytes) -> bool:
    """Sniff AVIF by its ISO-BMFF 'ftyp' brand, so we convert correctly even when
    the URL ends in .jpg / the content-type is wrong."""
    head = body[:64]
    return b"ftyp" in head and (b"avif" in head or b"avis" in head)


def fetch_image(ctx, url: str, attempts: int = 3):
    """Fetch an image with a few retries for transient failures. Returns
    (body, content_type) on success, or (None, None) after exhausting attempts."""
    for n in range(1, attempts + 1):
        try:
            resp = ctx.request.get(url, timeout=60_000)
            if resp.ok:
                return resp.body(), resp.headers.get("content-type")
            log(f"   ! HTTP {resp.status} (ครั้งที่ {n}/{attempts}) {url}")
        except Exception as e:  # noqa: BLE001 - retry on any transient error
            log(f"   ! โหลดผิดพลาด (ครั้งที่ {n}/{attempts}): {e}")
        if n < attempts:
            time.sleep(1.0)
    return None, None


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

        links = load_and_collect_cards(page)
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
                body, content_type = fetch_image(ctx, img_url)
                if body is None:
                    log(f"   ! ข้ามรูป (โหลดไม่สำเร็จหลังลองหลายครั้ง): {img_url}")
                    continue
                ext = ext_for(img_url, content_type)
                if ext == ".avif" or looks_like_avif(body):
                    try:
                        body = avif_to_jpeg(body)
                        ext = ".jpg"
                    except Exception as e:  # noqa: BLE001 - skip undecodable image
                        log(f"   ! แปลง avif ไม่สำเร็จ ข้ามรูป: {e}")
                        continue
                # Number only after a successful save so failures never leave a
                # gap in the 0001, 0002, ... sequence.
                fname = out_dir / f"{counter + 1:04d}{ext}"
                try:
                    fname.write_bytes(body)
                except OSError as e:
                    log(f"   ! เขียนไฟล์ไม่สำเร็จ ข้ามรูป: {e}")
                    continue
                counter += 1
                log(f"   saved {fname.name}  ({len(body):,} bytes)")

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
