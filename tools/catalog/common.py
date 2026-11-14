"""Wspólne narzędzia skryptów katalogu: pobieranie z cache (raw/), JSON, opóźnienia."""
import hashlib
import json
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
RAW = ROOT / "tools" / "catalog" / "raw"          # surowe odpowiedzi (poza gitem)
OUT = ROOT / "data" / "catalog"                    # dane statyczne wbudowywane w program
UA = "OpenSpacecraft-catalog/1.0 (+https://github.com/BlackSingularities/OpenSpacecraft)"


def fetch(url, *, cache_key=None, delay=0.0, accept=None, retries=4):
    """Pobiera URL z cache na dysku. Zwraca tekst albo None (404)."""
    key = cache_key or hashlib.sha1(url.encode()).hexdigest()
    path = RAW / key
    if path.exists():
        data = path.read_bytes()
        return None if data == b"\x00404" else data.decode("utf-8", "replace")
    path.parent.mkdir(parents=True, exist_ok=True)
    headers = {"User-Agent": UA}
    if accept:
        headers["Accept"] = accept
    for attempt in range(retries):
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=90) as r:
                data = r.read()
            path.write_bytes(data)
            if delay:
                time.sleep(delay)
            return data.decode("utf-8", "replace")
        except urllib.error.HTTPError as e:
            if e.code == 404:
                path.write_bytes(b"\x00404")
                return None
            if e.code == 429:
                wait = int(e.headers.get("Retry-After") or 60)
                print(f"  429, czekam {wait}s"); time.sleep(wait); continue
            print(f"  HTTP {e.code} {url} (próba {attempt + 1})"); time.sleep(5 * (attempt + 1))
        except Exception as e:  # noqa: BLE001
            print(f"  błąd {e} {url} (próba {attempt + 1})"); time.sleep(5 * (attempt + 1))
    return None


def save_json(name, obj):
    OUT.mkdir(parents=True, exist_ok=True)
    p = OUT / name
    with open(p, "w", encoding="utf-8", newline="\n") as f:
        json.dump(obj, f, ensure_ascii=False, separators=(",", ":"), sort_keys=False)
        f.write("\n")
    print(f"zapisano {p.relative_to(ROOT)} ({p.stat().st_size // 1024} KiB, {len(obj)} rekordów)")


def load_json(path, default=None):
    p = Path(path)
    if not p.exists():
        return default
    with open(p, encoding="utf-8") as f:
        return json.load(f)
