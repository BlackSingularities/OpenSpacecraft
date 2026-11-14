"""Lokalny edytor tłumaczeń katalogu EN → PL.

python -I tools/translate/server.py   →  http://127.0.0.1:8765
Zapisuje do data/catalog/pl.json (klucz → tekst polski). Działa tylko lokalnie, bez zależności.
"""
import json
import os
import sys
import threading
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
CAT = ROOT / "data" / "catalog"
PL = CAT / "pl.json"
HERE = Path(__file__).resolve().parent
LOCK = threading.Lock()


def load(name, default):
    p = CAT / name
    return json.loads(p.read_text(encoding="utf-8")) if p.exists() else default


def save_pl(pl):
    tmp = PL.with_suffix(".tmp")
    with open(tmp, "w", encoding="utf-8", newline="\n") as f:
        f.write("{\n")
        items = sorted(pl.items())
        for i, (k, v) in enumerate(items):
            f.write(f" {json.dumps(k, ensure_ascii=False)}: {json.dumps(v, ensure_ascii=False)}{',' if i < len(items) - 1 else ''}\n")
        f.write("}\n")
    os.replace(tmp, PL)


def build_items():
    sc = load("spacecraft.json", [])
    ln = load("launches.json", [])
    terms = load("terms.json", [])
    items = []
    for t in terms:
        items.append({"k": "term:" + t["en"], "c": "term", "en": t["en"], "title": t["en"], "n": t["n"]})
    for r in sc:
        prio = (3 if r.get("m") else 0) + (1 if r.get("img") else 0) + (1 if r.get("wiki_pl") else 0) + (1 if r.get("desc") else 0)
        src = [f"https://www.wikidata.org/wiki/{r['qid']}"]
        if "nssdca" in r.get("src", []):
            src.append(f"https://nssdc.gsfc.nasa.gov/nmc/spacecraft/display.action?id={r['id']}")
        if r.get("wiki_en"):
            src.append(r["wiki_en"])
        if r.get("wiki_pl"):
            src.append(r["wiki_pl"])
        title = f"{r['name']} · {r['id']} · {(r.get('t0') or '')[:10]}"
        img = r.get("img")
        if r.get("short"):
            items.append({"k": f"sc:{r['id']}:short", "c": "short", "en": r["short"], "hint": r.get("short_pl"), "title": title, "src": src, "p": prio, "img": img})
        if r.get("desc"):
            items.append({"k": f"sc:{r['id']}:desc", "c": "desc", "en": r["desc"], "title": title, "src": src, "p": prio, "img": img})
    for r in ln:
        if r.get("desc"):
            items.append({"k": f"ln:{r['id']}:desc", "c": "launch", "en": r["desc"], "title": f"{r['name']} · {r['t'][:10]}",
                          "src": [f"https://ll.thespacedevs.com/2.3.0/launches/{r['id']}/"], "p": 0})
    return items


class H(BaseHTTPRequestHandler):
    items = None

    def log_message(self, *a):
        pass

    def send(self, code, body, ctype="application/json; charset=utf-8"):
        data = body if isinstance(body, bytes) else body.encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path in ("/", "/index.html"):
            return self.send(200, (HERE / "index.html").read_bytes(), "text/html; charset=utf-8")
        if self.path == "/api/data":
            with LOCK:
                pl = load("pl.json", {})
            return self.send(200, json.dumps({"items": H.items, "pl": pl}, ensure_ascii=False))
        self.send(404, "{}")

    def do_POST(self):
        if self.path != "/api/save" or self.headers.get("Origin") not in (None, f"http://127.0.0.1:{PORT}", f"http://localhost:{PORT}"):
            return self.send(403, "{}")
        req = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        key, text = req.get("k"), (req.get("pl") or "").strip()
        if not isinstance(key, str) or not key:
            return self.send(400, "{}")
        with LOCK:
            pl = load("pl.json", {})
            if text:
                pl[key] = text
            else:
                pl.pop(key, None)
            save_pl(pl)
        self.send(200, json.dumps({"ok": True, "n": len(pl)}))


PORT = int(os.environ.get("PORT", 8765))

if __name__ == "__main__":
    H.items = build_items()
    print(f"{len(H.items)} pozycji do tłumaczenia · http://127.0.0.1:{PORT}  (Ctrl+C kończy)")
    srv = ThreadingHTTPServer(("127.0.0.1", PORT), H)
    if "--no-browser" not in sys.argv:
        threading.Timer(0.6, lambda: webbrowser.open(f"http://127.0.0.1:{PORT}")).start()
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass
