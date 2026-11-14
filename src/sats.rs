//! Satelity i obiekty na orbicie okołoziemskiej (CelesTrak).
//!
//!  - **SATCAT** (`celestrak.org/pub/satcat.csv`) – wszystkie skatalogowane obiekty z datami startu
//!    i deorbitacji oraz parametrami orbity (okres, nachylenie, apogeum, perygeum). Dzięki temu na
//!    orbicie widać populację z dowolnej chwili od 1957 r.
//!  - **GP** (`gp.php?GROUP=active`) – aktualne elementy orbitalne aktywnych satelitów, liczone
//!    modelem SGP4 (dokładne położenia w pobliżu epoki elementów).
//!
//! Obiekty bez aktualnych elementów krążą po orbitach odtworzonych z parametrów katalogu
//! (płaszczyzna i faza są umowne) – rozkład wysokości i nachyleń jest prawdziwy.
//! Pliki są zapisywane w `%LOCALAPPDATA%\OpenSpacecraft\cache` i odświeżane co najwyżej co 12 h.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use glam::DVec3;
use rayon::prelude::*;

use crate::astro::{self, time};

const SATCAT_URL: &str = "https://celestrak.org/pub/satcat.csv";
const GP_URL: &str = "https://celestrak.org/NORAD/elements/gp.php?GROUP=active&FORMAT=json";
const MU: f64 = 3.986_004_418e14;
const RE_KM: f64 = 6378.137;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ObjType {
    Payload,
    RocketBody,
    Debris,
    Unknown,
}

impl ObjType {
    pub const ALL: [ObjType; 4] = [ObjType::Payload, ObjType::RocketBody, ObjType::Debris, ObjType::Unknown];

    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            ObjType::Payload => "Satelity",
            ObjType::RocketBody => "Człony rakiet",
            ObjType::Debris => "Odłamki",
            ObjType::Unknown => "Nieznane",
        })
    }

    pub fn color(self, active: bool) -> [f32; 3] {
        match self {
            ObjType::Payload if active => [0.45, 0.95, 0.75],
            ObjType::Payload => [0.55, 0.75, 1.0],
            ObjType::RocketBody => [1.0, 0.72, 0.35],
            ObjType::Debris => [0.85, 0.42, 0.42],
            ObjType::Unknown => [0.7, 0.7, 0.7],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum OrbitClass {
    Leo,
    Meo,
    Geo,
    Heo,
}

impl OrbitClass {
    pub const ALL: [OrbitClass; 4] = [OrbitClass::Leo, OrbitClass::Meo, OrbitClass::Geo, OrbitClass::Heo];

    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            OrbitClass::Leo => "Niska (LEO)",
            OrbitClass::Meo => "Średnia (MEO)",
            OrbitClass::Geo => "Geostacjonarna (GEO)",
            OrbitClass::Heo => "Wysoce eliptyczna (HEO)",
        })
    }
}

#[derive(Clone, Debug)]
pub struct SatObj {
    pub norad: u32,
    pub name: String,
    pub intl: String,
    pub kind: ObjType,
    pub owner: String,
    /// Kod kosmodromu z SATCAT (np. AFETR, TYMSC).
    pub site: String,
    pub launch_jd: f64,
    pub decay_jd: Option<f64>,
    pub period_min: f64,
    pub inc_deg: f64,
    pub apo_km: f64,
    pub peri_km: f64,
    /// Działający (kod statusu „+”, „P”, „B”, „S”, „X”).
    pub active: bool,
    pub class: OrbitClass,
    /// Indeks elementów GP (SGP4).
    pub gp: Option<usize>,
    /// Kąty umowne dla orbity przybliżonej: węzeł, perygeum, anomalia w chwili startu.
    angles: [f64; 3],
}

pub struct Gp {
    pub constants: sgp4::Constants,
    pub epoch_jd: f64,
}

impl SatObj {
    /// Położenie względem środka Ziemi [m], ekliptyka J2000; drugi element – czy dokładne (SGP4).
    pub fn position(&self, jd: f64, gp: &[Gp]) -> Option<(DVec3, bool)> {
        if let Some(g) = self.gp.and_then(|i| gp.get(i)) {
            if (jd - g.epoch_jd).abs() < 120.0 {
                if let Ok(p) = g.constants.propagate(sgp4::MinutesSinceEpoch((jd - g.epoch_jd) * 1440.0)) {
                    let r = DVec3::new(p.position[0], p.position[1], p.position[2]) * 1000.0;
                    if r.is_finite() && r.length() > 6.3e6 {
                        return Some((astro::equatorial_to_ecliptic(r), true));
                    }
                }
            }
        }
        self.approx_position(jd).map(|p| (p, false))
    }

    fn approx_position(&self, jd: f64) -> Option<DVec3> {
        if self.period_min <= 0.0 {
            return None;
        }
        let rp = (RE_KM + self.peri_km.max(120.0)) * 1000.0;
        let ra = (RE_KM + self.apo_km.max(self.peri_km).max(120.0)) * 1000.0;
        let a = (rp + ra) / 2.0;
        let e = ((ra - rp) / (ra + rp)).clamp(0.0, 0.97);
        let n = std::f64::consts::TAU / (self.period_min * 60.0);
        let el = crate::astro::kepler::Elements {
            a,
            e,
            i: self.inc_deg.to_radians(),
            // precesja węzła J2 – orbity słoneczno-synchroniczne i inne obracają się jak w rzeczywistości
            node: self.angles[0] - 1.5 * n * 1.082_63e-3 * (RE_KM * 1000.0 / (a * (1.0 - e * e))).powi(2) * self.inc_deg.to_radians().cos() * (jd - self.launch_jd) * 86_400.0,
            peri: self.angles[1],
            m0: self.angles[2],
            epoch: self.launch_jd,
            mu: MU,
        };
        let el = crate::astro::kepler::Elements { a: (MU / (n * n)).cbrt().max(a * 0.5).min(a * 2.0), ..el };
        Some(astro::equatorial_to_ecliptic(el.state(jd).0))
    }

    pub fn alive_at(&self, jd: f64) -> bool {
        jd >= self.launch_jd && self.decay_jd.is_none_or(|d| jd < d)
    }

    pub fn owner_name(&self) -> String {
        owner_name(&self.owner)
    }
}

fn classify(period: f64, apo: f64, peri: f64) -> OrbitClass {
    let e = (apo - peri) / (apo + peri + 2.0 * RE_KM);
    if e > 0.25 {
        OrbitClass::Heo
    } else if (1_380.0..1_500.0).contains(&period) {
        OrbitClass::Geo
    } else if period < 128.0 {
        OrbitClass::Leo
    } else {
        OrbitClass::Meo
    }
}

fn hash_angles(norad: u32) -> [f64; 3] {
    let mut x = (norad as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    let mut next = || {
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        (x % 1_000_000) as f64 / 1_000_000.0 * std::f64::consts::TAU
    };
    [next(), next(), next()]
}

/// Start orbitalny z katalogu.
#[derive(Clone, Debug)]
pub struct Launch {
    pub cospar: String,
    pub jd: f64,
    pub site: String,
    pub owner: String,
    /// Indeksy obiektów (ładunki najpierw).
    pub objects: Vec<u32>,
    pub payloads: u32,
}

/// Kosmodrom z kodu SATCAT: nazwa (w języku programu) i położenie (szerokość, długość).
pub fn site_info(code: &str) -> Option<(&'static str, f64, f64)> {
    site_raw(code).map(|(n, a, b)| (crate::i18n::tr(n), a, b))
}

fn site_raw(code: &str) -> Option<(&'static str, f64, f64)> {
    Some(match code {
        "AFETR" => ("Cape Canaveral / KSC", 28.52, -80.60),
        "AFWTR" => ("Vandenberg", 34.70, -120.60),
        "TYMSC" => ("Bajkonur", 45.92, 63.34),
        "PKMTR" => ("Plesieck", 62.93, 40.58),
        "KYMSC" => ("Kapustin Jar", 48.57, 45.80),
        "VOSTO" => ("Wostocznyj", 51.88, 128.33),
        "DLS" => ("Jasnyj", 51.21, 59.85),
        "YSLA" => ("Morze Żółte (start z morza)", 34.90, 121.20),
        "SVOB" => ("Swobodnyj", 51.80, 128.40),
        "FRGUI" => ("Kourou", 5.24, -52.77),
        "HGSTR" => ("Hammaguir", 30.88, -3.04),
        "JSC" => ("Jiuquan", 40.96, 100.29),
        "TSC" | "TAISC" => ("Taiyuan", 38.85, 111.61),
        "XSC" | "XICLF" => ("Xichang", 28.25, 102.03),
        "WSC" => ("Wenchang", 19.61, 110.95),
        "SEAL" => ("Sea Launch", 0.0, -154.0),
        "TNSTA" => ("Tanegashima", 30.40, 130.97),
        "KSCUT" => ("Uchinoura", 31.25, 131.08),
        "SRILR" => ("Sriharikota", 13.72, 80.23),
        "RLLB" => ("Mahia (Rocket Lab)", -39.26, 177.86),
        "WLPIS" => ("Wallops", 37.94, -75.46),
        "KODAK" => ("Kodiak", 57.43, -152.34),
        "NSC" => ("Naro", 34.43, 127.53),
        "SEMLS" => ("Semnan", 35.23, 53.92),
        "YUN" => ("Sohae", 39.66, 124.70),
        "PMRF" => ("Barking Sands", 22.02, -159.78),
        "SNMLP" => ("San Marco", -2.94, 40.21),
        "WOMRA" => ("Woomera", -30.96, 136.50),
        "PALMA" => ("Palmachim", 31.88, 34.68),
        "SMTS" => ("Shahrud", 36.20, 55.33),
        "STARB" | "SPXBC" => ("Starbase", 25.99, -97.15),
        _ => return None,
    })
}

pub fn owner_name(code: &str) -> String {
    let pl = crate::i18n::current() == "pl";
    let s = match code {
        "US" => "USA",
        "CIS" => if pl { "ZSRR / Rosja" } else { "USSR / Russia" },
        "PRC" => if pl { "Chiny" } else { "China" },
        "ESA" => "ESA",
        "JPN" => if pl { "Japonia" } else { "Japan" },
        "IND" => if pl { "Indie" } else { "India" },
        "FR" => if pl { "Francja" } else { "France" },
        "UK" => if pl { "Wielka Brytania" } else { "United Kingdom" },
        "GER" => if pl { "Niemcy" } else { "Germany" },
        "IT" => if pl { "Włochy" } else { "Italy" },
        "CA" => if pl { "Kanada" } else { "Canada" },
        "SKOR" => if pl { "Korea Płd." } else { "South Korea" },
        "ISS" => if pl { "ISS (międzynarodowa)" } else { "ISS (international)" },
        "POL" => if pl { "Polska" } else { "Poland" },
        "SES" => "SES",
        "ITSO" => "Intelsat",
        "GLOB" => "Globalstar",
        "O3B" => "O3b",
        "ORB" => "Orbcomm",
        "IRID" => "Iridium",
        "EUTE" => "Eutelsat",
        "AB" => if pl { "Arabsat" } else { "Arabsat" },
        other => other,
    };
    s.to_string()
}

/* ── wczytywanie ─────────────────────────────────────────────────────── */

fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut q = false;
    for c in line.chars() {
        match c {
            '"' => q = !q,
            ',' if !q => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

fn parse_satcat(text: &str) -> Vec<SatObj> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else { return Vec::new() };
    let cols: HashMap<String, usize> = split_csv(header).into_iter().enumerate().map(|(i, c)| (c.trim().to_string(), i)).collect();
    let col = |name: &str| cols.get(name).copied();
    let (Some(c_name), Some(c_id), Some(c_norad), Some(c_type), Some(c_status), Some(c_owner), Some(c_launch), Some(c_decay), Some(c_period), Some(c_inc), Some(c_apo), Some(c_peri)) = (
        col("OBJECT_NAME"),
        col("OBJECT_ID"),
        col("NORAD_CAT_ID"),
        col("OBJECT_TYPE"),
        col("OPS_STATUS_CODE"),
        col("OWNER"),
        col("LAUNCH_DATE"),
        col("DECAY_DATE"),
        col("PERIOD"),
        col("INCLINATION"),
        col("APOGEE"),
        col("PERIGEE"),
    ) else {
        return Vec::new();
    };
    let c_center = col("ORBIT_CENTER");
    let c_site = col("LAUNCH_SITE");
    let mut out = Vec::new();
    for line in lines {
        let f = split_csv(line);
        let get = |i: usize| f.get(i).map(|s| s.trim()).unwrap_or("");
        if let Some(cc) = c_center {
            let center = get(cc);
            if !center.is_empty() && center != "EA" {
                continue;
            }
        }
        let num = |i: usize| get(i).parse::<f64>().ok();
        let (Some(period), Some(inc), Some(apo), Some(peri)) = (num(c_period), num(c_inc), num(c_apo), num(c_peri)) else { continue };
        let Some(launch_jd) = time::parse(get(c_launch)) else { continue };
        let Ok(norad) = get(c_norad).parse::<u32>() else { continue };
        let kind = match get(c_type) {
            "PAY" => ObjType::Payload,
            "R/B" => ObjType::RocketBody,
            "DEB" => ObjType::Debris,
            _ => ObjType::Unknown,
        };
        let status = get(c_status);
        out.push(SatObj {
            norad,
            name: get(c_name).to_string(),
            intl: get(c_id).to_string(),
            kind,
            owner: get(c_owner).to_string(),
            site: c_site.map(|c| get(c).to_string()).unwrap_or_default(),
            launch_jd,
            decay_jd: time::parse(get(c_decay)),
            period_min: period,
            inc_deg: inc,
            apo_km: apo,
            peri_km: peri,
            active: matches!(status, "+" | "P" | "B" | "S" | "X"),
            class: classify(period, apo, peri),
            gp: None,
            angles: hash_angles(norad),
        });
    }
    out
}

fn parse_gp(text: &str) -> Vec<(u32, Gp)> {
    let Ok(list) = serde_json::from_str::<Vec<sgp4::Elements>>(text) else { return Vec::new() };
    list.into_iter()
        .filter_map(|e| {
            let epoch = time::from_datetime(&e.datetime);
            let c = sgp4::Constants::from_elements(&e).ok()?;
            Some((e.norad_id as u32, Gp { constants: c, epoch_jd: epoch }))
        })
        .collect()
}

/* ── katalog ─────────────────────────────────────────────────────────── */

pub struct Catalog {
    pub objs: Vec<SatObj>,
    pub gp: Vec<Gp>,
    pub by_norad: HashMap<u32, usize>,
    /// Liczba obiektów na orbicie na początku każdego roku: (rok, [satelity, człony, odłamki, inne]).
    pub per_year: Vec<(i32, [u32; 4])>,
    /// Starty (nowe ładunki) w każdym roku.
    pub launches_per_year: Vec<(i32, u32)>,
    /// Starty orbitalne (grupy obiektów o wspólnym numerze COSPAR), w kolejności chronologicznej.
    pub launches: Vec<Launch>,
    /// Liczba startów w każdym roku.
    pub launch_count: Vec<(i32, u32)>,
    pub satcat_age_h: Option<f64>,
    pub gp_age_h: Option<f64>,
}

impl Catalog {
    fn build(mut objs: Vec<SatObj>, gp: Vec<(u32, Gp)>, satcat_age_h: Option<f64>, gp_age_h: Option<f64>) -> Self {
        objs.sort_by_key(|o| o.norad);
        let mut by_norad: HashMap<u32, usize> = objs.iter().enumerate().map(|(i, o)| (o.norad, i)).collect();
        let mut gps = Vec::with_capacity(gp.len());
        for (norad, g) in gp {
            let idx = gps.len();
            gps.push(g);
            match by_norad.get(&norad) {
                Some(&i) => objs[i].gp = Some(idx),
                None => {
                    // obiekt bez wpisu w katalogu (np. katalog jeszcze nie pobrany): minimalny wpis
                    let o = SatObj {
                        norad,
                        name: format!("NORAD {norad}"),
                        intl: String::new(),
                        kind: ObjType::Payload,
                        owner: String::new(),
                        site: String::new(),
                        launch_jd: 0.0,
                        decay_jd: None,
                        period_min: 0.0,
                        inc_deg: 0.0,
                        apo_km: 0.0,
                        peri_km: 0.0,
                        active: true,
                        class: OrbitClass::Leo,
                        gp: Some(idx),
                        angles: [0.0; 3],
                    };
                    by_norad.insert(norad, objs.len());
                    objs.push(o);
                }
            }
        }
        let now_y = time::year(time::now());
        let mut per_year = Vec::new();
        let mut launches_per_year = Vec::new();
        for y in 1957..=now_y {
            let jd = time::year_start(y + 1).min(time::now());
            let mut c = [0u32; 4];
            for o in objs.iter().filter(|o| o.alive_at(jd)) {
                c[o.kind as usize] += 1;
            }
            per_year.push((y, c));
            let (a, b) = (time::year_start(y), time::year_start(y + 1));
            let n = objs.iter().filter(|o| o.kind == ObjType::Payload && o.launch_jd >= a && o.launch_jd < b).count() as u32;
            launches_per_year.push((y, n));
        }
        // starty: obiekty o wspólnym prefiksie COSPAR „RRRR-NNN”
        let mut groups: HashMap<String, Launch> = HashMap::new();
        for (i, o) in objs.iter().enumerate() {
            if o.intl.len() < 8 || o.launch_jd <= 0.0 {
                continue;
            }
            let key = o.intl[..8].to_string();
            let l = groups.entry(key.clone()).or_insert_with(|| Launch { cospar: key, jd: o.launch_jd, site: o.site.clone(), owner: o.owner.clone(), objects: Vec::new(), payloads: 0 });
            l.objects.push(i as u32);
            if o.kind == ObjType::Payload {
                l.payloads += 1;
            }
        }
        let mut launches: Vec<Launch> = groups.into_values().filter(|l| l.payloads > 0).collect();
        launches.sort_by(|a, b| a.jd.total_cmp(&b.jd).then(a.cospar.cmp(&b.cospar)));
        for l in &mut launches {
            // ładunki najpierw
            l.objects.sort_by_key(|&i| (objs[i as usize].kind != ObjType::Payload, objs[i as usize].norad));
        }
        let mut launch_count = Vec::new();
        for y in 1957..=now_y {
            let (a, b) = (time::year_start(y), time::year_start(y + 1));
            launch_count.push((y, launches.iter().filter(|l| l.jd >= a && l.jd < b).count() as u32));
        }
        Self { objs, gp: gps, by_norad, per_year, launches_per_year, launches, launch_count, satcat_age_h, gp_age_h }
    }

    pub fn find(&self, norad: u32) -> Option<&SatObj> {
        self.by_norad.get(&norad).map(|&i| &self.objs[i])
    }

    pub fn empty() -> Self {
        Self::build(Vec::new(), Vec::new(), None, None)
    }
}

/// Stan pobierania (0 – bezczynne, 1 – pobieranie, 2 – błąd).
pub struct Loader {
    pub catalog: Arc<Mutex<Arc<Catalog>>>,
    pub state: Arc<AtomicU8>,
    pub error: Arc<Mutex<Option<String>>>,
}

pub fn cache_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("OpenSpacecraft")
        .join("cache")
}

fn age_hours(p: &std::path::Path) -> Option<f64> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(m.elapsed().map(|d| d.as_secs_f64() / 3600.0).unwrap_or(0.0))
}

fn download(url: &str) -> Result<String, String> {
    crate::net::agent()
        .get(url)
        .timeout(Duration::from_secs(90))
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
}

impl Loader {
    pub fn new(ctx: eframe::egui::Context) -> Self {
        let l = Self {
            catalog: Arc::new(Mutex::new(Arc::new(Catalog::empty()))),
            state: Arc::new(AtomicU8::new(1)),
            error: Arc::new(Mutex::new(None)),
        };
        l.load(ctx, false);
        l
    }

    pub fn current(&self) -> Arc<Catalog> {
        self.catalog.lock().map(|c| c.clone()).unwrap_or_else(|_| Arc::new(Catalog::empty()))
    }

    pub fn busy(&self) -> bool {
        self.state.load(Ordering::Relaxed) == 1
    }

    /// Wczytuje pamięć podręczną, a w razie potrzeby (albo na żądanie) pobiera nowe dane.
    pub fn load(&self, ctx: eframe::egui::Context, force: bool) {
        let (cat, state, err) = (self.catalog.clone(), self.state.clone(), self.error.clone());
        state.store(1, Ordering::Relaxed);
        std::thread::spawn(move || {
            let dir = cache_dir();
            let _ = std::fs::create_dir_all(&dir);
            let satcat_f = dir.join("satcat.csv");
            let gp_f = dir.join("gp-active.json");
            let read = |p: &PathBuf| std::fs::read_to_string(p).ok();
            let publish = |sc: Option<String>, g: Option<String>| {
                let objs = sc.as_deref().map(parse_satcat).unwrap_or_default();
                let gp = g.as_deref().map(parse_gp).unwrap_or_default();
                let c = Catalog::build(objs, gp, age_hours(&satcat_f), age_hours(&gp_f));
                log::info!("katalog satelitów: {} obiektów, {} z elementami GP", c.objs.len(), c.gp.len());
                if let Ok(mut g) = cat.lock() {
                    *g = Arc::new(c);
                }
                ctx.request_repaint();
            };
            let (mut sc, mut g) = (read(&satcat_f), read(&gp_f));
            if sc.is_some() || g.is_some() {
                publish(sc.clone(), g.clone());
            }
            let stale = |p: &PathBuf, h: f64| age_hours(p).is_none_or(|a| a > h);
            let mut problem = None;
            let mut changed = false;
            if force || stale(&satcat_f, 72.0) {
                match download(SATCAT_URL) {
                    Ok(t) if t.starts_with("OBJECT_NAME") => {
                        let _ = std::fs::write(&satcat_f, &t);
                        sc = Some(t);
                        changed = true;
                    }
                    Ok(_) => problem = Some(crate::i18n::tr("CelesTrak zwrócił nieoczekiwaną odpowiedź.").to_string()),
                    Err(e) => problem = Some(e),
                }
            }
            if force || stale(&gp_f, 12.0) {
                match download(GP_URL) {
                    Ok(t) if t.trim_start().starts_with('[') => {
                        let _ = std::fs::write(&gp_f, &t);
                        g = Some(t);
                        changed = true;
                    }
                    Ok(t) => problem = Some(t.chars().take(160).collect()),
                    Err(e) => problem = Some(e),
                }
            }
            if changed {
                publish(sc, g);
            }
            if let Some(p) = &problem {
                log::warn!("satelity: {p}");
            }
            if let Ok(mut e) = err.lock() {
                *e = problem.clone();
            }
            state.store(if problem.is_some() { 2 } else { 0 }, Ordering::Relaxed);
            ctx.request_repaint();
        });
    }
}

/// Filtr widocznej populacji.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Filter {
    pub types: [bool; 4],
    pub classes: [bool; 4],
    pub only_active: bool,
    pub group: Group,
}

impl Default for Filter {
    fn default() -> Self {
        Self { types: [true, true, true, false], classes: [true; 4], only_active: false, group: Group::All }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Group {
    All,
    Starlink,
    OneWeb,
    Gnss,
    Stations,
    Weather,
}

impl Group {
    pub const ALL: [Group; 6] = [Group::All, Group::Starlink, Group::OneWeb, Group::Gnss, Group::Stations, Group::Weather];

    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            Group::All => "Wszystkie",
            Group::Starlink => "Starlink",
            Group::OneWeb => "OneWeb",
            Group::Gnss => "Nawigacja (GNSS)",
            Group::Stations => "Stacje",
            Group::Weather => "Meteo",
        })
    }

    fn matches(self, o: &SatObj) -> bool {
        let n = o.name.as_str();
        match self {
            Group::All => true,
            Group::Starlink => n.starts_with("STARLINK"),
            Group::OneWeb => n.starts_with("ONEWEB"),
            Group::Gnss => n.starts_with("NAVSTAR") || n.starts_with("GPS ") || n.starts_with("GSAT0") || n.contains("GALILEO") || n.starts_with("COSMOS") && o.class == OrbitClass::Meo || n.starts_with("BEIDOU") || n.starts_with("QZS") || n.starts_with("IRNSS"),
            Group::Stations => (n.starts_with("ISS") || n.starts_with("CSS") || n.contains("TIANHE") || n.starts_with("MIR") || n.starts_with("SALYUT") || n.starts_with("SKYLAB") || n.starts_with("TIANGONG")) && o.kind == ObjType::Payload,
            Group::Weather => n.starts_with("NOAA") || n.starts_with("METEOSAT") || n.starts_with("GOES") || n.starts_with("METEOR") || n.starts_with("FENGYUN") || n.starts_with("HIMAWARI") || n.starts_with("METOP") || n.starts_with("DMSP"),
        }
    }
}

impl Filter {
    pub fn accepts(&self, o: &SatObj) -> bool {
        self.types[o.kind as usize] && self.classes[o.class as usize] && (!self.only_active || o.active || o.gp.is_some()) && self.group.matches(o)
    }
}

/// Obiekt do narysowania: położenie względem Ziemi [m] (ekliptyka) i indeks w katalogu.
pub struct Visible {
    pub pos: Vec<[f32; 3]>,
    pub idx: Vec<u32>,
    pub precise: usize,
}

/// Położenia wszystkich obiektów spełniających filtr w chwili `jd` (równolegle).
pub fn compute(cat: &Catalog, filter: &Filter, jd: f64) -> Visible {
    let res: Vec<(u32, [f32; 3], bool)> = cat
        .objs
        .par_iter()
        .enumerate()
        .filter(|(_, o)| (o.alive_at(jd) || o.launch_jd == 0.0) && filter.accepts(o))
        .filter_map(|(i, o)| {
            let (p, exact) = o.position(jd, &cat.gp)?;
            Some((i as u32, [p.x as f32, p.y as f32, p.z as f32], exact))
        })
        .collect();
    let precise = res.iter().filter(|r| r.2).count();
    let (idx, pos) = res.into_iter().map(|(i, p, _)| (i, p)).unzip();
    Visible { pos, idx, precise }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_and_gp_parse() {
        let csv = "OBJECT_NAME,OBJECT_ID,NORAD_CAT_ID,OBJECT_TYPE,OPS_STATUS_CODE,OWNER,LAUNCH_DATE,LAUNCH_SITE,DECAY_DATE,PERIOD,INCLINATION,APOGEE,PERIGEE,RCS,DATA_STATUS_CODE,ORBIT_CENTER,ORBIT_TYPE\n\
ISS (ZARYA),1998-067A,25544,PAY,+,ISS,1998-11-20,TYMSC,,92.9,51.64,422,415,399.0,,EA,ORB\n\
\"SL-1 R/B, X\",1957-001A,1,R/B,D,CIS,1957-10-04,TYMSC,1957-12-01,96.19,65.1,938,214,20.4,,EA,IMP\n";
        let v = parse_satcat(csv);
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].name, "SL-1 R/B, X");
        let iss = &v[0];
        assert!(iss.active && iss.class == OrbitClass::Leo);
        let jd = time::parse("2025-01-01").unwrap();
        let p = iss.approx_position(jd).unwrap().length() / 1000.0;
        assert!((p - 6795.0).abs() < 30.0, "{p}");
        let gp = r#"[{"OBJECT_NAME":"ISS (ZARYA)","OBJECT_ID":"1998-067A","EPOCH":"2025-01-01T12:00:00.000000","MEAN_MOTION":15.5,"ECCENTRICITY":0.0005,"INCLINATION":51.64,"RA_OF_ASC_NODE":100.0,"ARG_OF_PERICENTER":50.0,"MEAN_ANOMALY":10.0,"EPHEMERIS_TYPE":0,"CLASSIFICATION_TYPE":"U","NORAD_CAT_ID":25544,"ELEMENT_SET_NO":999,"REV_AT_EPOCH":1000,"BSTAR":0.0002,"MEAN_MOTION_DOT":0.0001,"MEAN_MOTION_DDOT":0}]"#;
        let g = parse_gp(gp);
        assert_eq!(g.len(), 1);
        let cat = Catalog::build(v, g, None, None);
        let (p, exact) = cat.find(25544).unwrap().position(jd + 0.5, &cat.gp).unwrap();
        assert!(exact && (p.length() / 1000.0 - 6790.0).abs() < 60.0);
    }
}
