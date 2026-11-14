"""Pobiera historię startów z Launch Library 2 (The Space Devs) → raw/ll2.json.

Używa serwera deweloperskiego lldev.thespacedevs.com (bez limitu zapytań; dane historyczne te same).
python -I tools/catalog/fetch_ll2.py
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from common import RAW, fetch  # noqa: E402

BASE = "https://lldev.thespacedevs.com/2.3.0/launches/previous/?mode=normal&limit=100&ordering=net&offset="


def main():
    out, off = [], 0
    while True:
        t = fetch(BASE + str(off), cache_key=f"ll2/{off:05d}.json", delay=1.0)
        if not t:
            break
        d = json.loads(t)
        out += d["results"]
        print(f"{len(out)}/{d['count']}", flush=True)
        if not d.get("next"):
            break
        off += 100
    (RAW / "ll2.json").write_text(json.dumps(out, ensure_ascii=False), encoding="utf-8")


if __name__ == "__main__":
    sys.exit(main())
