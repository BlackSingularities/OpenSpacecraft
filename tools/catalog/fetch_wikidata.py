"""Pobiera z Wikidata (CC0) wszystkie obiekty z identyfikatorem COSPAR (P247) → raw/wikidata.json.

python -I tools/catalog/fetch_wikidata.py
"""
import json
import sys
import urllib.parse

sys.path.insert(0, __import__("os").path.dirname(__file__))
from common import RAW, fetch

EP = "https://query.wikidata.org/sparql?format=json&query="
Q = """
SELECT ?i (SAMPLE(?c) AS ?cospar) (SAMPLE(?len) AS ?l_en) (SAMPLE(?lpl) AS ?l_pl)
  (SAMPLE(?den) AS ?d_en) (SAMPLE(?dpl) AS ?d_pl) (MIN(?launch) AS ?t0) (MAX(?end) AS ?t1)
  (SAMPLE(?mass) AS ?m) (SAMPLE(?munit) AS ?mu) (SAMPLE(?img) AS ?image) (SAMPLE(?scn) AS ?norad)
  (SAMPLE(?ewiki) AS ?enwiki) (SAMPLE(?pwiki) AS ?plwiki)
  (GROUP_CONCAT(DISTINCT ?opl; separator="|") AS ?operators)
  (GROUP_CONCAT(DISTINCT ?ctry; separator="|") AS ?countries)
  (GROUP_CONCAT(DISTINCT ?veh; separator="|") AS ?vehicles)
  (GROUP_CONCAT(DISTINCT ?site; separator="|") AS ?sites)
  (GROUP_CONCAT(DISTINCT ?typ; separator="|") AS ?types)
  (GROUP_CONCAT(DISTINCT ?mfr; separator="|") AS ?makers)
  (GROUP_CONCAT(DISTINCT ?dest; separator="|") AS ?dests)
WHERE {
  ?i wdt:P247 ?c . FILTER(STRSTARTS(?c, "%s"))
  OPTIONAL { ?i rdfs:label ?len FILTER(LANG(?len)="en") }
  OPTIONAL { ?i rdfs:label ?lpl FILTER(LANG(?lpl)="pl") }
  OPTIONAL { ?i schema:description ?den FILTER(LANG(?den)="en") }
  OPTIONAL { ?i schema:description ?dpl FILTER(LANG(?dpl)="pl") }
  OPTIONAL { ?i wdt:P619 ?launch }
  OPTIONAL { { ?i wdt:P582 ?end } UNION { ?i wdt:P730 ?end } UNION { ?i wdt:P576 ?end } UNION { ?i wdt:P2669 ?end } }
  OPTIONAL { ?i p:P2067/psv:P2067 [ wikibase:quantityAmount ?mass; wikibase:quantityUnit ?munit ] }
  OPTIONAL { ?i wdt:P18 ?img }
  OPTIONAL { ?i wdt:P377 ?scn }
  OPTIONAL { ?ewiki schema:about ?i; schema:isPartOf <https://en.wikipedia.org/> }
  OPTIONAL { ?pwiki schema:about ?i; schema:isPartOf <https://pl.wikipedia.org/> }
  OPTIONAL { ?i wdt:P137 ?op . ?op rdfs:label ?opl FILTER(LANG(?opl)="en") }
  OPTIONAL { ?i wdt:P495 ?co . ?co wdt:P297 ?ctry }
  OPTIONAL { ?i wdt:P375 ?v . ?v rdfs:label ?veh FILTER(LANG(?veh)="en") }
  OPTIONAL { ?i wdt:P448 ?s . ?s rdfs:label ?site FILTER(LANG(?site)="en") }
  OPTIONAL { ?i wdt:P31 ?t . ?t rdfs:label ?typ FILTER(LANG(?typ)="en") }
  OPTIONAL { ?i wdt:P176 ?mk . ?mk rdfs:label ?mfr FILTER(LANG(?mfr)="en") }
  OPTIONAL { ?i wdt:P1427 ?ds . ?ds rdfs:label ?dest FILTER(LANG(?dest)="en") }
}
GROUP BY ?i
"""


def val(b, k):
    v = b.get(k)
    return v["value"] if v else None


def main():
    out = {}
    for year in range(1957, 2027):
        txt = fetch(EP + urllib.parse.quote(Q % year), cache_key=f"wikidata/{year}.json", delay=1.0, accept="application/sparql-results+json")
        if not txt:
            print(f"{year}: brak danych"); continue
        rows = json.loads(txt)["results"]["bindings"]
        for b in rows:
            qid = val(b, "i").rsplit("/", 1)[-1]
            rec = {k: val(b, k) for k in b if k != "i"}
            rec = {k: v for k, v in rec.items() if v not in (None, "")}
            out[qid] = rec
        print(f"{year}: {len(rows)}")
    (RAW / "wikidata.json").write_text(json.dumps(out, ensure_ascii=False), encoding="utf-8")
    print(f"razem {len(out)}")


if __name__ == "__main__":
    sys.exit(main())
