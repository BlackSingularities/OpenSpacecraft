"""Pobiera z JPL Horizons rzeczywiste trajektorie sond z data/missions.json i zapisuje je
w assets/trajectories.bin (wbudowywane w program).

Uruchomienie z katalogu repozytorium:  python -I tools/fetch_horizons.py [id misji ...]

Format pliku (little-endian):
  u32 liczba misji
  dla każdej: u16 długość id, id (UTF-8), u8 środek (0 Słońce, 1 Ziemia), u32 liczba próbek,
  próbki: f64 JD (UT), 3×f32 położenie [km], 3×f32 prędkość [km/s] – ekliptyka J2000.

Próbki: równomierna siatka na całym przedziale + gęstsze okna wokół wydarzeń misji (przeloty).
Dane JPL Horizons są w domenie publicznej (NASA/JPL).
"""
import json
import re
import struct
import sys
import time
import urllib.parse
import urllib.request
from datetime import datetime, timedelta, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
API = "https://ssd.jpl.nasa.gov/api/horizons.api"
MONTHS = {m: i + 1 for i, m in enumerate(["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"])}
GEO_KINDS = {"lunar", "orbit", "lagrange"}


def parse_t(s):
    s = s.replace("Z", "")
    for fmt in ("%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d"):
        try:
            return datetime.strptime(s, fmt).replace(tzinfo=timezone.utc)
        except ValueError:
            pass
    raise ValueError(s)


def hz_time(t):
    return t.strftime("%Y-%m-%d %H:%M")


def query(cmd, center, start, stop, steps):
    params = {
        "format": "text", "COMMAND": f"'{cmd}'", "OBJ_DATA": "'NO'", "MAKE_EPHEM": "'YES'",
        "EPHEM_TYPE": "'VECTORS'", "CENTER": f"'{center}'", "START_TIME": f"'{hz_time(start)}'",
        "STOP_TIME": f"'{hz_time(stop)}'", "STEP_SIZE": f"'{steps}'", "REF_PLANE": "'ECLIPTIC'",
        "REF_SYSTEM": "'ICRF'", "VEC_TABLE": "'2'", "OUT_UNITS": "'KM-S'", "CSV_FORMAT": "'YES'",
        "VEC_LABELS": "'NO'", "TIME_TYPE": "'UT'",
    }
    url = API + "?" + urllib.parse.urlencode(params)
    for attempt in range(4):
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "OpenSpacecraft-tools/1.0"}), timeout=120) as r:
                return r.read().decode("utf-8", "replace")
        except Exception as e:  # sieć bywa kapryśna
            print("   ponowienie:", e)
            time.sleep(3 + attempt * 5)
    raise RuntimeError("Horizons nie odpowiada")


def hz_date(s):
    # "1977-SEP-05 14:01:39.2009"
    m = re.match(r"(\d{4})-([A-Z]{3})-(\d{2})\s+(\d{2}):(\d{2})(?::(\d{2}))?", s.strip())
    y, mon, d, hh, mm, ss = m.groups()
    return datetime(int(y), MONTHS[mon], int(d), int(hh), int(mm), int(ss or 0), tzinfo=timezone.utc)


def coverage_error(text):
    lo = re.search(r"prior to A\.D\.\s*([0-9]{4}-[A-Z]{3}-[0-9]{2}\s+[0-9:]+)", text)
    hi = re.search(r"after A\.D\.\s*([0-9]{4}-[A-Z]{3}-[0-9]{2}\s+[0-9:]+)", text)
    return (hz_date(lo.group(1)) if lo else None, hz_date(hi.group(1)) if hi else None)


def samples(text):
    if "$$SOE" not in text:
        return None
    body = text.split("$$SOE", 1)[1].split("$$EOE", 1)[0]
    out = []
    for line in body.strip().splitlines():
        p = [x.strip() for x in line.split(",")]
        if len(p) < 8:
            continue
        out.append((float(p[0]), *map(float, p[2:8])))
    return out


def fetch_range(cmd, center, start, stop, steps):
    """Zapytanie z automatycznym dopasowaniem do zasięgu efemerydy."""
    for _ in range(6):
        if stop <= start:
            return [], start, stop
        text = query(cmd, center, start, stop, steps)
        s = samples(text)
        if s:
            return s, start, stop
        lo, hi = coverage_error(text)
        if lo is None and hi is None:
            print("   błąd Horizons:", text[-400:].strip().replace("\n", " | "))
            return [], start, stop
        if lo and lo >= start:
            start = lo + timedelta(minutes=2)
        if hi and hi <= stop:
            stop = hi - timedelta(minutes=2)
    return [], start, stop


def mission_samples(m):
    kind = m["traj"]["type"]
    center = "500@399" if kind in GEO_KINDS else "500@10"
    code = 1 if kind in GEO_KINDS else 0
    start = parse_t(m["launch"])
    end = parse_t(m["end"]) if m.get("end") else datetime.now(timezone.utc).replace(microsecond=0)
    # przyszłe wydarzenia (planowane przeloty) – efemerydy prognozowane też się przydają
    future = max((parse_t(e["t"]) for e in m.get("events", [])), default=end)
    if m.get("end") is None:
        end = max(end, future) + timedelta(days=30)
    dur_days = (end - start).total_seconds() / 86400
    coarse = 1600 if dur_days > 60 else 1200
    pts, s0, s1 = fetch_range(m["horizons"], center, start, end, coarse)
    if not pts:
        return code, []
    # okna wokół wydarzeń: przeloty trwają godziny, siatka zgrubna ma krok dni
    if dur_days > 60:
        for ev in m.get("events", []):
            t = parse_t(ev["t"])
            w = timedelta(days=4)
            a, b = max(t - w, s0), min(t + w, s1)
            if b - a < timedelta(hours=6):
                continue
            extra, _, _ = fetch_range(m["horizons"], center, a, b, 192)
            pts.extend(extra)
            time.sleep(0.4)
    pts = sorted({round(p[0], 7): p for p in pts}.values())
    return code, pts


def main():
    missions = json.loads((ROOT / "data" / "missions.json").read_text(encoding="utf-8"))
    only = set(sys.argv[1:])
    out_file = ROOT / "assets" / "trajectories.bin"
    cache_dir = ROOT / "tools" / ".cache"
    cache_dir.mkdir(exist_ok=True)
    result = {}
    for m in missions:
        if "horizons" not in m:
            continue
        cache = cache_dir / f"{m['id']}.json"
        if cache.exists() and (not only or m["id"] not in only):
            code, pts = json.loads(cache.read_text())
        elif only and m["id"] not in only:
            continue
        else:
            print(f"{m['id']} ({m['horizons']})…")
            code, pts = mission_samples(m)
            cache.write_text(json.dumps([code, pts]))
        print(f"  {m['id']}: {len(pts)} próbek")
        if pts:
            result[m["id"]] = (code, pts)
    buf = bytearray(struct.pack("<I", len(result)))
    for mid, (code, pts) in result.items():
        b = mid.encode()
        buf += struct.pack("<H", len(b)) + b + struct.pack("<BI", code, len(pts))
        for p in pts:
            buf += struct.pack("<d6f", *p)
    out_file.write_bytes(buf)
    print(f"Zapisano {out_file} ({len(buf) / 1e6:.2f} MB, {len(result)} misji)")


if __name__ == "__main__":
    main()
