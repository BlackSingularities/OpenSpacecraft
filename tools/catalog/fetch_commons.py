"""Pobiera z Wikimedia Commons autora i licencję zdjęć użytych w katalogu → raw/commons.json.

python -I tools/catalog/fetch_commons.py   (po build_catalog.py; potem build_catalog.py jeszcze raz)
"""
import html
import json
import os
import re
import sys
import urllib.parse

sys.path.insert(0, os.path.dirname(__file__))
from common import OUT, RAW, fetch  # noqa: E402

API = "https://commons.wikimedia.org/w/api.php?action=query&format=json&prop=imageinfo&iiprop=extmetadata&titles="


def plain(s):
    s = re.sub(r"<[^>]+>", "", s or "")
    return re.sub(r"\s+", " ", html.unescape(s)).strip()


def main():
    sc = json.loads((OUT / "spacecraft.json").read_text(encoding="utf-8"))
    files = sorted({r["img"] for r in sc if r.get("img")})
    out = {}
    for i in range(0, len(files), 50):
        part = files[i:i + 50]
        titles = "|".join("File:" + f for f in part)
        t = fetch(API + urllib.parse.quote(titles), cache_key=f"commons/{i:05d}.json", delay=0.5)
        if not t:
            continue
        d = json.loads(t)
        norm = {n["to"]: n["from"] for n in d.get("query", {}).get("normalized", [])}
        for p in d.get("query", {}).get("pages", {}).values():
            title = norm.get(p["title"], p["title"])
            name = title.split(":", 1)[1]
            meta = ((p.get("imageinfo") or [{}])[0]).get("extmetadata") or {}
            g = lambda k: plain((meta.get(k) or {}).get("value"))  # noqa: E731
            rec = {"author": g("Artist")[:160] or g("Credit")[:160], "license": g("LicenseShortName"), "url": g("LicenseUrl")}
            out[name] = {k: v for k, v in rec.items() if v}
        print(f"{min(i + 50, len(files))}/{len(files)}", flush=True)
    (RAW / "commons.json").write_text(json.dumps(out, ensure_ascii=False), encoding="utf-8")
    print(f"razem {len(out)}, z licencją {sum(1 for v in out.values() if v.get('license'))}")


if __name__ == "__main__":
    sys.exit(main())
