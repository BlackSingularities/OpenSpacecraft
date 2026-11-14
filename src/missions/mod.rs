//! Baza misji, rakiet i kosmodromów (pliki `data/*.json` wbudowane w program).

pub mod traj;

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::astro::time;

#[derive(Deserialize, Clone, Debug)]
pub struct Event {
    pub t: String,
    pub pl: String,
    pub en: String,
    /// Rodzaj wydarzenia (ikona): launch, stage, sep, dock, undock, burn, orbit, land, liftoff,
    /// eva, entry, splash, flyby, science, end.
    #[serde(default)]
    pub k: Option<String>,
    /// Części, które w tej chwili odłączają się od statku.
    #[serde(default)]
    pub sep: Vec<String>,
    /// Części, które w tej chwili łączą się (dokują) ze statkiem.
    #[serde(default)]
    pub dock: Vec<String>,
    #[serde(skip)]
    pub jd: f64,
}

impl Event {
    pub fn text(&self) -> &str {
        if crate::i18n::current() == "pl" { &self.pl } else { &self.en }
    }

    pub fn icon(&self) -> &'static str {
        use crate::ui::theme::icon;
        match self.k.as_deref().unwrap_or("") {
            "launch" => icon::ROCKET_LAUNCH,
            "stage" => icon::STACK,
            "sep" => icon::ARROWS_SPLIT,
            "dock" => icon::PLUGS_CONNECTED,
            "undock" => icon::PLUGS,
            "burn" => icon::FIRE,
            "orbit" => icon::CIRCLE_DASHED,
            "land" => icon::ARROW_LINE_DOWN,
            "liftoff" => icon::ARROW_LINE_UP,
            "eva" => icon::PERSON_SIMPLE_WALK,
            "entry" => icon::FLAME,
            "splash" => icon::PARACHUTE,
            "flyby" => icon::PATH,
            "science" => icon::FLASK,
            "end" => icon::FLAG_CHECKERED,
            _ => icon::DOT_OUTLINE,
        }
    }
}

/// Gdzie jest część po odłączeniu (znacznik w widoku 3D).
#[derive(Deserialize, Clone, Debug)]
pub struct PartPos {
    /// `surface` – na powierzchni (domyślnie miejsce lądowania misji), `orbit` – na orbicie
    /// (domyślnie orbita misji wokół celu).
    pub at: String,
    pub body: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub peri_km: Option<f64>,
    pub apo_km: Option<f64>,
    pub inc: Option<f64>,
}

/// Moduł, człon albo inna część zestawu startowego.
#[derive(Deserialize, Clone, Debug)]
pub struct Part {
    pub id: String,
    pub pl: String,
    pub en: String,
    /// Co się z nią stało po ostatnim odłączeniu.
    pub fate_pl: Option<String>,
    pub fate_en: Option<String>,
    pub pos: Option<PartPos>,
    /// Początek znacznika w 3D (np. sonda dociera na powierzchnię dopiero po locie).
    pub from: Option<String>,
    /// Koniec znacznika w 3D (np. orbiter zakończył pracę).
    pub until: Option<String>,
    /// Dołącza dopiero później (np. moduł dowieziony na stację).
    #[serde(default)]
    pub later: bool,
}

impl Part {
    pub fn name(&self) -> &str {
        if crate::i18n::current() == "pl" { &self.pl } else { &self.en }
    }

    pub fn fate(&self) -> Option<&str> {
        if crate::i18n::current() == "pl" { self.fate_pl.as_deref() } else { self.fate_en.as_deref().or(self.fate_pl.as_deref()) }
    }
}

/// Stan części w danej chwili.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PartState {
    /// Jeszcze jej nie ma w zestawie.
    NotYet,
    /// W zestawie (połączona ze statkiem).
    Attached,
    /// Odłączona przy wydarzeniu `event`; `last` – już nie wróci do zestawu.
    Separated { event: usize, last: bool },
}

#[derive(Deserialize, Clone, Debug)]
pub struct Leg {
    pub t: String,
    pub body: Option<String>,
    pub r_au: Option<f64>,
    pub ra: Option<f64>,
    pub dec: Option<f64>,
    pub ecl_lon: Option<f64>,
    pub ecl_lat: Option<f64>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct OrbitSpec {
    pub body: String,
    pub peri_km: f64,
    pub apo_km: f64,
    #[serde(default)]
    pub inc: f64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SurfaceSpec {
    pub body: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Final {
    pub orbit: Option<OrbitSpec>,
    pub surface: Option<SurfaceSpec>,
    pub surface_from: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Landing {
    pub t: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TrajSpec {
    Orbit {
        body: String,
        peri_km: f64,
        apo_km: f64,
        inc: f64,
    },
    Lunar {
        tli: String,
        loi: Option<String>,
        orbit_km: Option<f64>,
        orbit_apo_km: Option<f64>,
        inc: Option<f64>,
        landing: Option<Landing>,
        liftoff: Option<String>,
        tei: Option<String>,
        entry: Option<String>,
        flyby: Option<String>,
        flyby_km: Option<f64>,
        return_km: Option<f64>,
        #[serde(default)]
        direct: bool,
    },
    Helio {
        legs: Vec<Leg>,
        #[serde(rename = "final")]
        fin: Option<Final>,
    },
    Lagrange {
        point: String,
        arrive: String,
    },
    Trailing {
        drift_deg_per_year: f64,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    Crewed,
    Station,
    Lunar,
    Planetary,
    SmallBody,
    Solar,
    Observatory,
    Satellite,
}

impl Kind {
    pub const ALL: [Kind; 8] = [Kind::Crewed, Kind::Station, Kind::Lunar, Kind::Planetary, Kind::SmallBody, Kind::Solar, Kind::Observatory, Kind::Satellite];

    fn parse(s: &str) -> Kind {
        match s {
            "crewed" => Kind::Crewed,
            "station" => Kind::Station,
            "lunar" => Kind::Lunar,
            "planetary" => Kind::Planetary,
            "small-body" => Kind::SmallBody,
            "solar" => Kind::Solar,
            "observatory" => Kind::Observatory,
            _ => Kind::Satellite,
        }
    }

    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            Kind::Crewed => "Załogowe",
            Kind::Station => "Stacje",
            Kind::Lunar => "Księżycowe",
            Kind::Planetary => "Planetarne",
            Kind::SmallBody => "Komety i planetoidy",
            Kind::Solar => "Słoneczne",
            Kind::Observatory => "Obserwatoria",
            Kind::Satellite => "Satelity",
        })
    }

    pub fn icon(self) -> &'static str {
        use crate::ui::theme::icon;
        match self {
            Kind::Crewed => icon::USER,
            Kind::Station => icon::BUILDINGS,
            Kind::Lunar => icon::MOON,
            Kind::Planetary => icon::PLANET,
            Kind::SmallBody => icon::METEOR,
            Kind::Solar => icon::SUN,
            Kind::Observatory => icon::BINOCULARS,
            Kind::Satellite => icon::BROADCAST,
        }
    }

    /// Barwa misji na mapie (liniowa sRGB 0–1).
    pub fn color(self) -> [f32; 3] {
        match self {
            Kind::Crewed => [1.0, 0.62, 0.35],
            Kind::Station => [0.98, 0.84, 0.4],
            Kind::Lunar => [0.82, 0.86, 0.95],
            Kind::Planetary => [0.45, 0.72, 1.0],
            Kind::SmallBody => [0.6, 0.9, 0.7],
            Kind::Solar => [1.0, 0.78, 0.3],
            Kind::Observatory => [0.78, 0.6, 1.0],
            Kind::Satellite => [0.5, 0.9, 0.95],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Active,
    Completed,
    Failed,
}

impl Status {
    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            Status::Active => "w trakcie",
            Status::Completed => "zakończona",
            Status::Failed => "niepowodzenie",
        })
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Mission {
    pub id: String,
    pub name: String,
    pub agency: String,
    pub country: String,
    #[serde(rename = "kind")]
    kind_s: String,
    #[serde(rename = "status")]
    status_s: String,
    pub launch: String,
    pub end: Option<String>,
    pub vehicle: Option<String>,
    pub site: Option<String>,
    pub mass_kg: Option<f64>,
    #[serde(default)]
    pub crew: Vec<String>,
    pub horizons: Option<String>,
    pub norad: Option<u32>,
    pub pl: String,
    pub en: String,
    #[serde(default)]
    pub ach_pl: Vec<String>,
    #[serde(default)]
    pub ach_en: Vec<String>,
    #[serde(default)]
    pub events: Vec<Event>,
    /// Części zestawu (moduły, człony rakiety) – scenariusz łączenia i odłączania.
    #[serde(default)]
    pub parts: Vec<Part>,
    /// Co przedstawia zdjęcie z archiwum i poprawiony podpis autora.
    pub photo_pl: Option<String>,
    pub photo_en: Option<String>,
    pub photo_author: Option<String>,
    pub traj: TrajSpec,
    #[serde(skip)]
    pub launch_jd: f64,
    #[serde(skip)]
    pub end_jd: Option<f64>,
    #[serde(skip, default = "default_kind")]
    pub kind: Kind,
    #[serde(skip, default = "default_status")]
    pub status: Status,
}

fn default_kind() -> Kind {
    Kind::Satellite
}

fn default_status() -> Status {
    Status::Completed
}

impl Mission {
    pub fn summary(&self) -> &str {
        if crate::i18n::current() == "pl" { &self.pl } else { &self.en }
    }

    pub fn achievements(&self) -> &[String] {
        if crate::i18n::current() == "pl" || self.ach_en.is_empty() { &self.ach_pl } else { &self.ach_en }
    }

    /// Czy w chwili `jd` misja trwa (po starcie, przed końcem).
    pub fn live_at(&self, jd: f64) -> bool {
        jd >= self.launch_jd && self.end_jd.is_none_or(|e| jd <= e)
    }

    pub fn vehicle(&self) -> Option<&'static Vehicle> {
        self.vehicle.as_deref().and_then(vehicle)
    }

    /// Misja z ludźmi na pokładzie (także stacje i loty księżycowe z załogą).
    pub fn crewed(&self) -> bool {
        self.kind == Kind::Crewed || self.kind == Kind::Station || !self.crew.is_empty()
    }

    pub fn photo_caption(&self) -> Option<&str> {
        if crate::i18n::current() == "pl" { self.photo_pl.as_deref() } else { self.photo_en.as_deref().or(self.photo_pl.as_deref()) }
    }

    /// Stan części `p` w chwili `jd` (wg wydarzeń `sep`/`dock`).
    pub fn part_state(&self, p: usize, jd: f64) -> PartState {
        let id = &self.parts[p].id;
        let mut st = if self.parts[p].later { PartState::NotYet } else { PartState::Attached };
        for (k, e) in self.events.iter().enumerate() {
            if e.jd > jd {
                break;
            }
            if e.sep.contains(id) {
                st = PartState::Separated { event: k, last: true };
            }
            if e.dock.contains(id) {
                st = PartState::Attached;
            }
        }
        if let PartState::Separated { event, .. } = st {
            let again = self.events[event + 1..].iter().any(|e| e.dock.contains(id));
            st = PartState::Separated { event, last: !again };
        }
        st
    }

    /// Okresy, w których część leci osobno: (od, do).
    pub fn part_free_intervals(&self, p: usize) -> Vec<(f64, f64)> {
        let id = &self.parts[p].id;
        let mut out = Vec::new();
        let mut from: Option<f64> = None;
        for e in &self.events {
            if e.sep.contains(id) && from.is_none() {
                from = Some(e.jd);
            }
            if e.dock.contains(id) {
                if let Some(f) = from.take() {
                    out.push((f, e.jd));
                }
            }
        }
        if let Some(f) = from {
            out.push((f, f64::INFINITY));
        }
        out
    }

    /// Ostatnie wydarzenie przed chwilą `jd` (bieżąca faza lotu).
    pub fn phase_at(&self, jd: f64) -> Option<&Event> {
        self.events.iter().rev().find(|e| e.jd <= jd)
    }

    pub fn site(&self) -> Option<&'static Site> {
        self.site.as_deref().and_then(|id| db().sites.iter().find(|s| s.id == id))
    }

    pub fn country_name(&self) -> String {
        country_name(&self.country)
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Vehicle {
    pub id: String,
    pub name: String,
    pub family: String,
    pub country: String,
    pub first: String,
    pub last: Option<String>,
    pub height_m: f64,
    pub diameter_m: f64,
    pub mass_t: f64,
    pub leo_t: Option<f64>,
    pub gto_t: Option<f64>,
    pub tli_t: Option<f64>,
    pub stages: f64,
    pub launches: Option<u32>,
    pub successes: Option<u32>,
    #[serde(default)]
    pub reusable: bool,
    pub pl: String,
    pub en: String,
}

impl Vehicle {
    pub fn summary(&self) -> &str {
        if crate::i18n::current() == "pl" { &self.pl } else { &self.en }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Site {
    pub id: String,
    pub name: String,
    pub country: String,
    pub lat: f64,
    pub lon: f64,
}

pub struct Db {
    pub missions: Vec<Mission>,
    pub vehicles: Vec<Vehicle>,
    pub sites: Vec<Site>,
    by_id: HashMap<String, usize>,
}

fn load() -> Db {
    let mut missions: Vec<Mission> = match serde_json::from_str(include_str!("../../data/missions.json")) {
        Ok(m) => m,
        Err(e) => {
            log::error!("baza misji: {e}");
            Vec::new()
        }
    };
    for m in &mut missions {
        m.launch_jd = time::parse(&m.launch).unwrap_or(time::J2000);
        m.end_jd = m.end.as_deref().and_then(time::parse);
        m.kind = Kind::parse(&m.kind_s);
        m.status = match m.status_s.as_str() {
            "active" => Status::Active,
            "failed" => Status::Failed,
            _ => Status::Completed,
        };
        for e in &mut m.events {
            e.jd = time::parse(&e.t).unwrap_or(m.launch_jd);
        }
        m.events.sort_by(|a, b| a.jd.total_cmp(&b.jd));
    }
    missions.sort_by(|a, b| a.launch_jd.total_cmp(&b.launch_jd));
    let vehicles: Vec<Vehicle> = serde_json::from_str(include_str!("../../data/vehicles.json")).unwrap_or_else(|e| {
        log::error!("rakiety: {e}");
        Vec::new()
    });
    let sites: Vec<Site> = serde_json::from_str(include_str!("../../data/sites.json")).unwrap_or_default();
    let by_id = missions.iter().enumerate().map(|(i, m)| (m.id.clone(), i)).collect();
    Db { missions, vehicles, sites, by_id }
}

pub fn db() -> &'static Db {
    static D: OnceLock<Db> = OnceLock::new();
    D.get_or_init(load)
}

pub fn index_of(id: &str) -> Option<usize> {
    db().by_id.get(id).copied()
}

pub fn vehicle(id: &str) -> Option<&'static Vehicle> {
    db().vehicles.iter().find(|v| v.id == id)
}

pub fn missions_with_vehicle(id: &str) -> impl Iterator<Item = &'static Mission> + '_ {
    db().missions.iter().filter(move |m| m.vehicle.as_deref() == Some(id))
}

pub fn country_name(code: &str) -> String {
    let pl = crate::i18n::current() == "pl";
    let n = match code {
        "SU" => if pl { "ZSRR" } else { "USSR" },
        "RU" => if pl { "Rosja" } else { "Russia" },
        "US" => "USA",
        "CN" => if pl { "Chiny" } else { "China" },
        "EU" => if pl { "Europa (ESA)" } else { "Europe (ESA)" },
        "JP" => if pl { "Japonia" } else { "Japan" },
        "IN" => if pl { "Indie" } else { "India" },
        "KR" => if pl { "Korea Płd." } else { "South Korea" },
        "AE" => if pl { "ZEA" } else { "UAE" },
        "NZ" => if pl { "Nowa Zelandia" } else { "New Zealand" },
        "INT" => if pl { "międzynarodowa" } else { "international" },
        "KZ" => if pl { "Kazachstan" } else { "Kazakhstan" },
        "GF" => if pl { "Gujana Francuska" } else { "French Guiana" },
        "FR" => if pl { "Francja" } else { "France" },
        "UK" | "GB" => if pl { "Wielka Brytania" } else { "United Kingdom" },
        other => other,
    };
    n.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_loads() {
        let d = db();
        assert!(d.missions.len() >= 60, "{}", d.missions.len());
        for m in &d.missions {
            assert!(time::parse(&m.launch).is_some(), "{}: start", m.id);
            for e in &m.events {
                assert!(time::parse(&e.t).is_some(), "{}: {}", m.id, e.t);
                for p in e.sep.iter().chain(e.dock.iter()) {
                    assert!(m.parts.iter().any(|x| &x.id == p), "{}: brak części {p}", m.id);
                }
            }
            for p in &m.parts {
                if let Some(u) = &p.from {
                    assert!(time::parse(u).is_some(), "{}: {}", m.id, u);
                }
                if let Some(u) = &p.until {
                    assert!(time::parse(u).is_some(), "{}: {}", m.id, u);
                }
                if let Some(b) = p.pos.as_ref().and_then(|x| x.body.as_ref()) {
                    assert!(crate::astro::bodies::find(b).is_some(), "{}: ciało {b}", m.id);
                }
            }
            if let Some(v) = &m.vehicle {
                assert!(vehicle(v).is_some(), "{}: brak rakiety {v}", m.id);
            }
            if let Some(s) = &m.site {
                assert!(m.site().is_some(), "{}: brak kosmodromu {s}", m.id);
            }
            if let TrajSpec::Helio { legs, .. } = &m.traj {
                for l in legs {
                    assert!(time::parse(&l.t).is_some(), "{}: {}", m.id, l.t);
                    if let Some(b) = &l.body {
                        assert!(crate::astro::bodies::find(b).is_some(), "{}: ciało {b}", m.id);
                    }
                }
            }
        }
    }
}
