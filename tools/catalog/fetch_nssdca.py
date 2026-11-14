"""Pobiera opisy z NASA NSSDCA Master Catalog (domena publiczna) dla obiektów z raw/wikidata.json
→ raw/nssdca.json.  Strony są cache'owane w raw/nssdca/, więc przerwane pobieranie można wznowić.

python -I tools/catalog/fetch_nssdca.py
"""
import html
import json
import os
import re
import sys
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, os.path.dirname(__file__))
from common import RAW, fetch  # noqa: E402

URL = "https://nssdc.gsfc.nasa.gov/nmc/spacecraft/display.action?id="


def clean(s):
    s = re.sub(r"<br\s*/?>", "\n", s)
    s = re.sub(r"<[^>]+>", "", s)
    s = html.unescape(s).replace("\xa0", " ")
    return re.sub(r"[ \t\r\f\v]+", " ", s).strip()


def parse(t):
    rec = {}
    m = re.search(r"<h1>(.*?)</h1>", t, re.S)
    if not m:
        return None
    rec["name"] = clean(m.group(1))
    m = re.search(r"<h2>Description</h2>(.*?)<div class=\"urtwo\">", t, re.S)
    if m:
        body = m.group(1)
        parts = []
        for tag, txt in re.findall(r"<(h4|p)[^>]*>(.*?)(?=<h4|<p|</div>|$)", body, re.S):
            c = clean(txt)
            if c:
                parts.append(("## " + c) if tag == "h4" else c)
        desc = "\n\n".join(parts).strip()
        if desc:
            rec["desc"] = desc
    facts = re.search(r"<h2>Facts in Brief</h2><p>(.*?)</p>", t, re.S)
    if facts:
        for k, v in re.findall(r"<strong>(.*?):</strong>(.*?)(?:<br\s*/?>|$)", facts.group(1), re.S):
            rec.setdefault("facts", {})[clean(k)] = clean(v)
    for key, h in (("alt", "Alternate Names"), ("agency", "Funding Agency"), ("disciplines", "Disciplines")):
        m = re.search(rf"<h2>{h}</h2><ul>(.*?)</ul>", t, re.S)
        if m:
            rec[key] = [clean(x) for x in re.findall(r"<li>(.*?)</li>", m.group(1), re.S) if clean(x)]
    return rec


def one(cospar):
    t = fetch(URL + cospar, cache_key=f"nssdca/{cospar}.html", delay=0.2)
    if not t or "<h1>" not in t:
        return cospar, None
    return cospar, parse(t)


def main():
    wd = json.loads((RAW / "wikidata.json").read_text(encoding="utf-8"))
    ids = sorted({r["cospar"] for r in wd.values() if re.fullmatch(r"\d{4}-\d{3}[A-Z]{1,3}", r.get("cospar", ""))})
    print(f"{len(ids)} identyfikatorów COSPAR")
    out = {}
    with ThreadPoolExecutor(4) as ex:
        for n, (c, rec) in enumerate(ex.map(one, ids), 1):
            if rec:
                out[c] = rec
            if n % 250 == 0:
                print(f"{n}/{len(ids)}, z opisem: {sum(1 for r in out.values() if 'desc' in r)}", flush=True)
    (RAW / "nssdca.json").write_text(json.dumps(out, ensure_ascii=False), encoding="utf-8")
    print(f"razem {len(out)}, z opisem {sum(1 for r in out.values() if 'desc' in r)}")


if __name__ == "__main__":
    sys.exit(main())
