"""Łączy surowe dane (Wikidata, NSSDCA, Launch Library 2) z ręcznie opracowanymi misjami
w statyczne pliki data/catalog/*.json wbudowywane w program.

  spacecraft.json – statki kosmiczne (COSPAR), fakty, opisy EN, zdjęcie (Commons), źródła
  launches.json   – historia startów orbitalnych (LL2)
  terms.json      – pojęcia do przetłumaczenia (operatorzy, typy, rakiety, orbity…)

Tłumaczenia PL są w data/catalog/pl.json (edytor: python -I tools/translate/server.py).
python -I tools/catalog/build_catalog.py
"""
import csv
import json
import os
import re
import sys
import unicodedata
from collections import Counter

sys.path.insert(0, os.path.dirname(__file__))
from common import RAW, ROOT, load_json, save_json  # noqa: E402

KG = {"http://www.wikidata.org/entity/Q11570": 1.0, "http://www.wikidata.org/entity/Q191118": 1000.0,
      "http://www.wikidata.org/entity/Q100995": 0.45359237, "http://www.wikidata.org/entity/Q41803": 0.001}


def norm(s):
    s = unicodedata.normalize("NFKD", s or "").encode("ascii", "ignore").decode().lower()
    return re.sub(r"[^a-z0-9]+", "", s)


def split(s):
    return [x for x in (s or "").split("|") if x]


def day(t):
    return (t or "")[:10]


def commons(url):
    # http://commons.wikimedia.org/wiki/Special:FilePath/Voyager_spacecraft.jpg → nazwa pliku
    if not url:
        return None
    from urllib.parse import unquote
    return unquote(url.rsplit("/", 1)[-1])


def short_author(a):
    """Autor zdjęcia z Commons bywa całym zdaniem – zostawiamy krótką nazwę."""
    if not a:
        return None
    a = re.sub(r"\s+", " ", a).strip()
    a = re.sub(r"^(Photo|Image|Credit|Author)\s*:\s*", "", a, flags=re.I)
    if len(a) > 60:
        m = re.search(r"(?:from|by|courtesy of)\s+([A-Z][^.,;()]{2,50})", a)
        a = m.group(1).strip() if m else a[:57].rsplit(" ", 1)[0] + "…"
    return a


def main():
    wd = load_json(RAW / "wikidata.json", {})
    ns = load_json(RAW / "nssdca.json", {})
    ll = load_json(RAW / "ll2.json", [])
    missions = load_json(ROOT / "data" / "missions.json", [])
    commons_meta = load_json(RAW / "commons.json", {})  # fetch_commons.py: autor i licencja zdjęć
    satcat = {}
    if (RAW / "satcat.csv").exists():  # https://celestrak.org/pub/satcat.csv
        with open(RAW / "satcat.csv", encoding="utf-8") as f:
            satcat = {r["OBJECT_ID"]: r for r in csv.DictReader(f)}

    # ── starty (LL2) ───────────────────────────────────────────────────
    launches, by_desig = [], {}
    for r in ll:
        m = r.get("mission") or {}
        pad = r.get("pad") or {}
        loc = pad.get("location") or {}
        cfg = ((r.get("rocket") or {}).get("configuration") or {})
        img = r.get("image") or {}
        lic = (img.get("license") or {})
        if lic.get("name") in (None, "", "Unknown"):
            img = {}  # zdjęcia bez znanej licencji pomijamy
        rec = {
            "id": r["id"], "name": r["name"], "t": r["net"],
            "status": (r.get("status") or {}).get("abbrev"),
            "provider": (r.get("launch_service_provider") or {}).get("name"),
            "rocket": cfg.get("full_name") or cfg.get("name"),
            "pad": pad.get("name"), "site": loc.get("name"),
            "cc": ((pad.get("country") or {}).get("alpha_2_code")),
            "lat": float(pad["latitude"]) if pad.get("latitude") else None,
            "lon": float(pad["longitude"]) if pad.get("longitude") else None,
            "mission": m.get("name"), "type": m.get("type"),
            "orbit": ((m.get("orbit") or {}).get("name")),
            "desc": (m.get("description") or "").strip() or None,
            "fail": (r.get("failreason") or "").strip() or None,
            "img": img.get("image_url"), "img_credit": img.get("credit"),
            "img_lic": lic.get("name") if img else None, "img_lic_url": lic.get("link") if img else None,
            "desig": r.get("launch_designator"), "wiki": pad.get("wiki_url") and None,
        }
        rec = {k: v for k, v in rec.items() if v not in (None, "")}
        launches.append(rec)
        if rec.get("desig"):
            by_desig[rec["desig"]] = rec["id"]

    # ── powiązanie misji opracowanych ręcznie ───────────────────────────
    m_by_cospar = {m["cospar"]: m["id"] for m in missions if m.get("cospar")}
    m_by_norad = {str(m["norad"]): m["id"] for m in missions if m.get("norad")}
    m_by_name = {}
    for m in missions:
        m_by_name.setdefault((norm(m["name"]), day(m["launch"])), m["id"])
        m_by_name.setdefault((norm(m["name"]), None), m["id"])

    # ── statki kosmiczne ───────────────────────────────────────────────
    sc, linked = [], {}
    for qid, w in wd.items():
        cospar = w.get("cospar", "")
        if not re.fullmatch(r"\d{4}-\d{3}[A-Z]{0,3}", cospar):
            continue
        n = ns.get(cospar, {})
        name = w.get("l_en") or n.get("name") or cospar
        mass = None
        if w.get("m"):
            try:
                mass = round(float(w["m"]) * KG.get(w.get("mu"), 1.0), 2)
            except ValueError:
                pass
        if mass is None and n.get("facts", {}).get("Mass"):
            mm = re.match(r"([\d.,]+)\s*kg", n["facts"]["Mass"])
            if mm:
                mass = float(mm.group(1).replace(",", ""))
        t0 = w.get("t0") or (n.get("facts", {}).get("Launch Date"))
        sc_row = satcat.get(cospar, {})
        sources = ["wikidata"] + (["nssdca"] if n else []) + (["satcat"] if sc_row else [])
        rec = {
            "id": cospar, "qid": qid, "name": name,
            "name_pl": w.get("l_pl") if w.get("l_pl") and w.get("l_pl") != name else None,
            "t0": t0, "t1": w.get("t1") or (sc_row.get("DECAY_DATE") or None),
            "owner": sc_row.get("OWNER") if sc_row.get("OWNER") not in (None, "", "TBD") else None,
            "cc": sorted(set(split(w.get("countries")))) or None,
            "op": split(w.get("operators"))[:4] or n.get("agency") or None,
            "maker": split(w.get("makers"))[:3] or None,
            "type": split(w.get("types"))[:3] or None,
            "veh": (split(w.get("vehicles")) or [n.get("facts", {}).get("Launch Vehicle")])[0],
            "site": (split(w.get("sites")) or split(w.get("dests")) or [n.get("facts", {}).get("Launch Site")])[0],
            "mass": mass, "power": n.get("facts", {}).get("Nominal Power"),
            "norad": int(w["norad"]) if (w.get("norad") or "").isdigit() else (int(sc_row["NORAD_CAT_ID"]) if sc_row.get("NORAD_CAT_ID") else None),
            "disc": n.get("disciplines"), "alt": (n.get("alt") or [])[:6] or None,
            "short": w.get("d_en"), "short_pl": w.get("d_pl"),
            "desc": n.get("desc"),
            "img": commons(w.get("image")) if commons_meta.get(commons(w.get("image")) or "", {}).get("license") or not commons_meta else None,
            "img_author": short_author(commons_meta.get(commons(w.get("image")) or "", {}).get("author")),
            "img_lic": commons_meta.get(commons(w.get("image")) or "", {}).get("license"),
            "img_lic_url": commons_meta.get(commons(w.get("image")) or "", {}).get("url"),
            "wiki_en": w.get("enwiki"), "wiki_pl": w.get("plwiki"),
            "launch": by_desig.get(cospar[:8]),
            "src": sources,
        }
        mid = m_by_cospar.get(cospar) or (m_by_norad.get(str(rec["norad"])) if rec["norad"] else None) \
            or m_by_name.get((norm(name), day(t0))) \
            or (m_by_name.get((norm(name), None)) if cospar.endswith("A") else None)
        if mid and mid not in linked:
            rec["m"] = mid
            linked[mid] = cospar
        sc.append({k: v for k, v in rec.items() if v not in (None, "", [])})
    # druga runda: ten sam dzień startu (misje o innej nazwie niż statek, np. Curiosity = MSL)
    by_day = {}
    for r in sc:
        by_day.setdefault(day(r.get("t0")), []).append(r)
    for m in missions:
        if m["id"] in linked:
            continue
        cands = [r for r in by_day.get(day(m["launch"]), []) if "m" not in r]
        if not cands:
            continue
        key = norm(m["name"].split()[0])
        pick = [r for r in cands if key and key in norm(r["name"])] or                ([r for r in cands if r["id"].endswith("A")] if len({r["id"][:8] for r in cands}) == 1 else [])
        if pick:
            pick.sort(key=lambda r: r["id"])
            # główny ładunek startu (oznaczenie …A) ma pierwszeństwo przed członami rakiety
            main = [r for r in cands if r["id"] == pick[0]["id"][:8] + "A"]
            if main:
                pick = main
            pick[0]["m"] = m["id"]
            linked[m["id"]] = pick[0]["id"]
    sc.sort(key=lambda r: (r.get("t0") or "9999", r["id"]))

    # ── pojęcia do tłumaczenia ─────────────────────────────────────────
    terms = Counter()
    for r in sc:
        for k in ("type", "disc"):
            for t in r.get(k, []):
                terms[t] += 1
    for r in launches:
        for k in ("type", "orbit", "status"):
            if r.get(k):
                terms[r[k]] += 1
    term_list = [{"en": t, "n": c} for t, c in terms.most_common() if c >= 2]

    save_json("spacecraft.json", sc)
    save_json("launches.json", launches)
    save_json("terms.json", term_list)
    print(f"misje z bazy powiązane z katalogiem: {len(linked)}/{len(missions)}")
    missing = [m["id"] for m in missions if m["id"] not in linked]
    print("niepowiązane:", ", ".join(missing))


if __name__ == "__main__":
    sys.exit(main())
