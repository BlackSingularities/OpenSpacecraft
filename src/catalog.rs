//! Statyczny katalog statków kosmicznych i startów (pliki `data/catalog/*.json` wbudowane w program).
//!
//! Źródła: Wikidata (CC0), NASA NSSDCA Master Catalog (domena publiczna), Launch Library 2
//! (The Space Devs). Dane buduje `tools/catalog/build_catalog.py`; tłumaczenia polskie są
//! w `data/catalog/pl.json` (edytor: `tools/translate/server.py`). Z sieci dociągane są tylko zdjęcia.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::astro::time;

#[derive(Deserialize, Clone, Debug)]
pub struct Craft {
    pub id: String,
    pub qid: String,
    pub name: String,
    pub name_pl: Option<String>,
    pub t0: Option<String>,
    pub t1: Option<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    /// Kod właściciela z SATCAT (CelesTrak), gdy Wikidata nie podaje kraju.
    pub owner: Option<String>,
    #[serde(default)]
    pub op: Vec<String>,
    #[serde(default)]
    pub maker: Vec<String>,
    #[serde(default, rename = "type")]
    pub types: Vec<String>,
    pub veh: Option<String>,
    pub site: Option<String>,
    pub mass: Option<f64>,
    pub power: Option<String>,
    pub norad: Option<u32>,
    #[serde(default)]
    pub disc: Vec<String>,
    #[serde(default)]
    pub alt: Vec<String>,
    short: Option<String>,
    short_pl: Option<String>,
    desc: Option<String>,
    pub img: Option<String>,
    pub img_author: Option<String>,
    pub img_lic: Option<String>,
    pub img_lic_url: Option<String>,
    pub wiki_en: Option<String>,
    pub wiki_pl: Option<String>,
    pub launch: Option<String>,
    #[serde(default)]
    pub src: Vec<String>,
    /// Identyfikator misji z `data/missions.json`, jeśli to ten sam statek.
    pub m: Option<String>,
    #[serde(skip)]
    pub jd: Option<f64>,
    #[serde(skip)]
    pub end_jd: Option<f64>,
    #[serde(skip)]
    pub search: String,
    /// Statek główny startu, gdy ten obiekt jest tylko jego częścią (człon, odłamek, inny ładunek).
    #[serde(skip)]
    pub parent: Option<usize>,
    /// Liczba pozostałych obiektów startu w archiwum (dla statku głównego).
    #[serde(skip)]
    pub parts: usize,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Launch {
    pub id: String,
    pub name: String,
    pub t: String,
    pub status: Option<String>,
    pub provider: Option<String>,
    pub rocket: Option<String>,
    pub pad: Option<String>,
    pub site: Option<String>,
    pub cc: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub mission: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub orbit: Option<String>,
    desc: Option<String>,
    pub fail: Option<String>,
    pub img: Option<String>,
    pub img_credit: Option<String>,
    pub img_lic: Option<String>,
    pub img_lic_url: Option<String>,
    pub desig: Option<String>,
    #[serde(skip)]
    pub jd: f64,
}

/// Tekst w bieżącym języku; `translated == false` oznacza angielski oryginał przy polskim interfejsie.
pub struct Text<'a> {
    pub text: &'a str,
    pub translated: bool,
}

pub struct Catalog {
    pub crafts: Vec<Craft>,
    pub launches: Vec<Launch>,
    pl: HashMap<String, String>,
    by_mission: HashMap<String, usize>,
    by_launch: HashMap<String, usize>,
    by_desig: HashMap<String, usize>,
    by_norad: HashMap<u32, usize>,
    mission_of_desig: HashMap<String, String>,
}

fn load() -> Catalog {
    let mut crafts: Vec<Craft> = serde_json::from_str(include_str!("../data/catalog/spacecraft.json")).unwrap_or_else(|e| {
        log::error!("katalog statków: {e}");
        Vec::new()
    });
    let mut launches: Vec<Launch> = serde_json::from_str(include_str!("../data/catalog/launches.json")).unwrap_or_else(|e| {
        log::error!("katalog startów: {e}");
        Vec::new()
    });
    let pl: HashMap<String, String> = serde_json::from_str(include_str!("../data/catalog/pl.json")).unwrap_or_else(|e| {
        log::error!("tłumaczenia katalogu: {e}");
        HashMap::new()
    });
    for c in &mut crafts {
        c.jd = c.t0.as_deref().and_then(time::parse);
        c.end_jd = c.t1.as_deref().and_then(time::parse);
        c.search = format!("{} {} {} {} {} {}", c.name, c.name_pl.as_deref().unwrap_or(""), c.id, c.op.join(" "), c.alt.join(" "), c.cc.join(" ")).to_lowercase();
    }
    for l in &mut launches {
        l.jd = time::parse(&l.t).unwrap_or(0.0);
    }
    launches.sort_by(|a, b| a.jd.total_cmp(&b.jd));
    // start = jedna pozycja: statek główny (misja z atlasu, inaczej najniższe oznaczenie) i jego części
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, c) in crafts.iter().enumerate() {
        if c.id.len() > 8 {
            groups.entry(c.id[..8].to_string()).or_default().push(i);
        }
    }
    let mut mission_of_desig: HashMap<String, String> = HashMap::new();
    for (desig, members) in &groups {
        // główny: misja z atlasu, potem ładunek (nie człon rakiety ani odłamek), potem najniższe oznaczenie
        let rank = |i: usize| (crafts[i].m.is_none(), is_part(&crafts[i]), crafts[i].id.clone());
        let main = members.iter().copied().min_by_key(|&i| rank(i));
        let Some(main) = main else { continue };
        if let Some(m) = &crafts[main].m {
            mission_of_desig.insert(desig.clone(), m.clone());
        }
        crafts[main].parts = members.len() - 1;
        for &i in members {
            if i != main {
                crafts[i].parent = Some(main);
                // szukanie po nazwie części znajduje start
                let extra = format!(" {}", crafts[i].search);
                crafts[main].search.push_str(&extra);
            }
        }
    }
    let by_mission = crafts.iter().enumerate().filter_map(|(i, c)| c.m.clone().map(|m| (m, i))).collect();
    let by_launch = launches.iter().enumerate().map(|(i, l)| (l.id.clone(), i)).collect();
    let by_desig = launches.iter().enumerate().filter_map(|(i, l)| l.desig.clone().map(|d| (d, i))).collect();
    let by_norad = crafts.iter().enumerate().filter_map(|(i, c)| c.norad.map(|n| (n, i))).collect();
    Catalog { crafts, launches, pl, by_mission, by_launch, by_desig, by_norad, mission_of_desig }
}

/// Człon rakiety, odłamek albo inny obiekt, który nie jest ładunkiem startu.
fn is_part(c: &Craft) -> bool {
    let n = c.name.to_lowercase();
    n.contains("rocket body") || n.contains("r/b") || n.contains(" deb") || n.contains("debris") || n.contains("stage") || n.contains("sivb") || n.contains("s-ivb") || c.types.iter().any(|t| {
        let t = t.to_lowercase();
        t.contains("rocket stage") || t.contains("upper stage") || t.contains("space debris")
    })
}

pub fn get() -> &'static Catalog {
    static C: OnceLock<Catalog> = OnceLock::new();
    C.get_or_init(load)
}

fn polish() -> bool {
    crate::i18n::current() == "pl"
}

impl Catalog {
    pub fn craft_of_mission(&self, mission_id: &str) -> Option<usize> {
        self.by_mission.get(mission_id).copied()
    }

    pub fn launch(&self, id: &str) -> Option<usize> {
        self.by_launch.get(id).copied()
    }

    /// Misja z atlasu, do której należy obiekt o oznaczeniu COSPAR (cały start to misja).
    pub fn mission_of_cospar(&self, cospar: &str) -> Option<&str> {
        self.mission_of_desig.get(cospar.get(..8)?).map(String::as_str)
    }

    /// Statek główny startu (on sam, jeśli jest główny).
    pub fn main_of(&self, i: usize) -> usize {
        self.crafts[i].parent.unwrap_or(i)
    }

    /// Statek o danym numerze NORAD (satelita z katalogu CelesTrak).
    pub fn craft_by_norad(&self, norad: u32) -> Option<usize> {
        self.by_norad.get(&norad).copied()
    }

    /// Start o oznaczeniu COSPAR „rrrr-nnn”.
    pub fn launch_by_desig(&self, desig: &str) -> Option<usize> {
        self.by_desig.get(desig).copied()
    }

    /// Statki wyniesione w danym starcie (oznaczenie COSPAR startu „rrrr-nnn”).
    pub fn payloads(&self, desig: &str) -> Vec<usize> {
        let mut v: Vec<usize> = self.crafts.iter().enumerate().filter(|(_, c)| c.id.len() > desig.len() && c.id.starts_with(desig)).map(|(i, _)| i).collect();
        v.sort_by(|a, b| self.crafts[*a].id.cmp(&self.crafts[*b].id));
        v
    }

    /// Pojęcie (typ, dyscyplina, orbita…) przetłumaczone w `pl.json`.
    pub fn term<'a>(&'a self, en: &'a str) -> &'a str {
        if polish() {
            if let Some(p) = self.pl.get(&format!("term:{en}")) {
                return p;
            }
        }
        en
    }

    pub fn craft_name<'a>(&self, c: &'a Craft) -> &'a str {
        match (&c.name_pl, polish()) {
            (Some(p), true) => p,
            _ => &c.name,
        }
    }

    fn pick<'a>(&'a self, key: String, pl_fallback: Option<&'a str>, en: Option<&'a str>) -> Option<Text<'a>> {
        if polish() {
            if let Some(p) = self.pl.get(&key) {
                return Some(Text { text: p, translated: true });
            }
            if let Some(p) = pl_fallback {
                return Some(Text { text: p, translated: true });
            }
        }
        en.map(|t| Text { text: t, translated: !polish() })
    }

    pub fn craft_short<'a>(&'a self, c: &'a Craft) -> Option<Text<'a>> {
        self.pick(format!("sc:{}:short", c.id), c.short_pl.as_deref(), c.short.as_deref())
    }

    pub fn craft_desc<'a>(&'a self, c: &'a Craft) -> Option<Text<'a>> {
        self.pick(format!("sc:{}:desc", c.id), None, c.desc.as_deref())
    }

    pub fn launch_desc<'a>(&'a self, l: &'a Launch) -> Option<Text<'a>> {
        self.pick(format!("ln:{}:desc", l.id), None, l.desc.as_deref())
    }
}

impl Craft {
    /// Kraj albo właściciel do wyświetlenia.
    pub fn country(&self) -> String {
        if self.cc.is_empty() {
            self.owner.as_deref().map(crate::sats::owner_name).unwrap_or_default()
        } else {
            self.cc.iter().map(|x| crate::missions::country_name(x)).collect::<Vec<_>>().join(", ")
        }
    }

    pub fn has_desc(&self) -> bool {
        self.desc.is_some()
    }

    /// Adres zdjęcia (miniatura z Wikimedia Commons).
    pub fn photo_url(&self) -> Option<String> {
        self.img.as_deref().map(commons_url)
    }

    pub fn photo_credit(&self) -> crate::photos::Credit<'_> {
        crate::photos::Credit {
            author: self.img_author.as_deref().and_then(crate::photos::clean_author),
            caption: None,
            source: "Wikimedia Commons",
            page: self.img.as_deref().map(|i| format!("https://commons.wikimedia.org/wiki/File:{}", encode(i))),
            license: self.img_lic.as_deref(),
            license_url: self.img_lic_url.as_deref(),
        }
    }

    /// Źródła danych: (etykieta, adres).
    pub fn sources(&self) -> Vec<(&'static str, String)> {
        let mut v = vec![("Wikidata", format!("https://www.wikidata.org/wiki/{}", self.qid))];
        if self.src.iter().any(|s| s == "satcat") {
            v.push(("CelesTrak SATCAT", format!("https://celestrak.org/satcat/table-satcat.php?INTDES={}", self.id)));
        }
        if self.src.iter().any(|s| s == "nssdca") {
            v.push(("NASA NSSDCA", format!("https://nssdc.gsfc.nasa.gov/nmc/spacecraft/display.action?id={}", self.id)));
        }
        if let Some(w) = &self.wiki_pl {
            v.push(("Wikipedia (pl)", w.clone()));
        }
        if let Some(w) = &self.wiki_en {
            v.push(("Wikipedia (en)", w.clone()));
        }
        if let Some(i) = &self.img {
            v.push(("Wikimedia Commons", format!("https://commons.wikimedia.org/wiki/File:{}", encode(i))));
        }
        v
    }
}

impl Launch {
    pub fn success(&self) -> Option<bool> {
        match self.status.as_deref() {
            Some("Success") => Some(true),
            Some("Failure") | Some("Partial Failure") => Some(false),
            _ => None,
        }
    }

    pub fn photo_credit(&self) -> crate::photos::Credit<'_> {
        crate::photos::Credit {
            author: self.img_credit.as_deref().and_then(crate::photos::clean_author),
            caption: None,
            source: "Launch Library 2",
            page: Some(self.source_url()),
            license: self.img_lic.as_deref(),
            license_url: self.img_lic_url.as_deref(),
        }
    }

    pub fn source_url(&self) -> String {
        format!("https://ll.thespacedevs.com/2.3.0/launches/{}/", self.id)
    }
}

pub fn commons_url(file: &str) -> String {
    format!("https://commons.wikimedia.org/wiki/Special:FilePath/{}?width=800", encode(file))
}

fn encode(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for b in s.replace(' ', "_").bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~()".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_loads_and_links() {
        let c = get();
        assert!(c.crafts.len() > 5000, "{}", c.crafts.len());
        assert!(c.launches.len() > 5000, "{}", c.launches.len());
        for k in &c.crafts {
            if let Some(m) = &k.m {
                assert!(crate::missions::index_of(m).is_some(), "{}: brak misji {m}", k.id);
            }
        }
        assert!(c.craft_of_mission("voyager-1").is_some());
        // start Sputnika 3: głównym obiektem jest satelita, nie człon rakiety
        let main = |id: &str| c.crafts.iter().position(|x| x.id == id).map(|i| c.crafts[c.main_of(i)].id.clone());
        assert_eq!(main("1958-004A").as_deref(), Some("1958-004B"));
        assert_eq!(main("1958-002B").as_deref(), Some("1958-002B"));
        assert_eq!(c.mission_of_cospar("1969-059B"), Some("apollo-11"));
        assert_eq!(encode("A b/ł.jpg"), "A_b%2F%C5%82.jpg");
    }
}
