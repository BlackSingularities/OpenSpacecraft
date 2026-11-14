//! Trajektorie misji.
//!
//! Dwa źródła:
//!  - **próbki JPL Horizons** (`assets/trajectories.bin`, narzędzie `tools/fetch_horizons.py`) –
//!    rzeczywiste położenia i prędkości, interpolowane wielomianem Hermite'a; fazy orbitalne
//!    krótsze od odstępu próbek są odtwarzane propagacją Keplera od najbliższej próbki,
//!  - **model fizyczny** z opisu misji: wznoszenie z kosmodromu, orbita parkingowa, łuki
//!    Lamberta między ciałami (zagadnienie dwóch ciał), hiperbole przelotów i przechwycenia,
//!    orbity wokół celu, lądowanie i pobyt na powierzchni.
//!
//! Położenia: względem Słońca [m], ekliptyka J2000.

use std::sync::{Arc, OnceLock};

use glam::{DMat3, DVec3};

use super::{Final, Mission, TrajSpec};
use crate::astro::bodies::{self, BodyId, EARTH, MOON, SUN};
use crate::astro::kepler::{lambert, propagate};
use crate::astro::{self, time, AU, GM_SUN};

fn gm(b: BodyId) -> f64 {
    if b == SUN { GM_SUN } else { bodies::get(b).gm }
}

/// Kierunek z szerokości i długości (stopnie) w układzie związanym z ciałem.
fn geo_dir(lat: f64, lon: f64) -> DVec3 {
    let (la, lo) = (lat.to_radians(), lon.to_radians());
    DVec3::new(la.cos() * lo.cos(), la.cos() * lo.sin(), la.sin())
}

fn slerp(a: DVec3, b: DVec3, t: f64) -> DVec3 {
    let d = a.dot(b).clamp(-1.0, 1.0);
    let th = d.acos();
    if th < 1e-6 {
        return a.lerp(b, t).normalize_or_zero();
    }
    let s = th.sin();
    (a * ((1.0 - t) * th).sin() / s + b * (t * th).sin() / s).normalize_or_zero()
}

fn smooth(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/* ── próbki Horizons ─────────────────────────────────────────────────── */

pub struct Samples {
    pub center: BodyId,
    pub t: Vec<f64>,
    pub p: Vec<DVec3>,
    pub v: Vec<DVec3>,
    /// Faza związana z ciałem o okresie krótszym niż odstęp próbek: (ciało, r, v względem ciała).
    bound: Vec<Option<(BodyId, DVec3, DVec3)>>,
}

impl Samples {
    pub fn range(&self) -> (f64, f64) {
        (self.t[0], *self.t.last().unwrap_or(&self.t[0]))
    }

    fn index(&self, jd: f64) -> usize {
        match self.t.binary_search_by(|x| x.total_cmp(&jd)) {
            Ok(i) => i.min(self.t.len().saturating_sub(2)),
            Err(i) => i.saturating_sub(1).min(self.t.len().saturating_sub(2)),
        }
    }

    pub fn is_bound(&self, jd: f64) -> Option<BodyId> {
        let i = self.index(jd);
        let k = if (jd - self.t[i]) < (self.t[(i + 1).min(self.t.len() - 1)] - jd) { i } else { (i + 1).min(self.t.len() - 1) };
        self.bound[k].map(|b| b.0)
    }

    /// Położenie względem środka próbek (bez położenia środka).
    fn rel(&self, jd: f64) -> DVec3 {
        let n = self.t.len();
        if n == 1 {
            return self.p[0];
        }
        let i = self.index(jd);
        let j = i + 1;
        if self.bound[i].is_some() || self.bound[j].is_some() {
            let k = if (jd - self.t[i]) < (self.t[j] - jd) { i } else { j };
            let (b, r, v) = self.bound[k].or(self.bound[i]).or(self.bound[j]).unwrap_or((self.center, self.p[k], self.v[k]));
            let (rr, _) = propagate(r, v, (jd - self.t[k]) * 86_400.0, gm(b));
            // względem środka próbek
            return rr + bodies::position(b, jd) - bodies::position(self.center, jd);
        }
        let h = (self.t[j] - self.t[i]) * 86_400.0;
        let s = ((jd - self.t[i]) / (self.t[j] - self.t[i])).clamp(0.0, 1.0);
        let (s2, s3) = (s * s, s * s * s);
        let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
        let h10 = s3 - 2.0 * s2 + s;
        let h01 = -2.0 * s3 + 3.0 * s2;
        let h11 = s3 - s2;
        self.p[i] * h00 + self.v[i] * (h10 * h) + self.p[j] * h01 + self.v[j] * (h11 * h)
    }

    pub fn pos(&self, jd: f64) -> DVec3 {
        bodies::position(self.center, jd) + self.rel(jd)
    }

    /// Stan na początku albo końcu (do przedłużenia Keplerem).
    fn edge_state(&self, last: bool) -> (f64, DVec3, DVec3) {
        let k = if last { self.t.len() - 1 } else { 0 };
        (self.t[k], self.p[k], self.v[k])
    }
}

/// Ciała, w których strefie wpływu próbka może być „związana”.
const BOUND_CANDIDATES: [(BodyId, f64, f64); 6] = [
    // (ciało, średnia odległość od Słońca [AU], ±zakres [AU] z mimośrodem i strefą wpływu)
    (1, 0.387, 0.09),
    (2, 0.723, 0.01),
    (3, 1.0, 0.024),
    (5, 1.524, 0.15),
    (8, 5.203, 0.6),
    (13, 9.537, 0.9),
];

fn detect_bound(center: BodyId, t: &[f64], p: &[DVec3], v: &[DVec3]) -> Vec<Option<(BodyId, DVec3, DVec3)>> {
    use rayon::prelude::*;
    (0..t.len())
        .into_par_iter()
        .map(|k| {
            let dt_prev = if k > 0 { t[k] - t[k - 1] } else { f64::INFINITY };
            let dt_next = if k + 1 < t.len() { t[k + 1] - t[k] } else { f64::INFINITY };
            let step = dt_prev.min(dt_next) * 86_400.0;
            let check = |b: BodyId, r: DVec3, vel: DVec3| -> Option<(BodyId, DVec3, DVec3)> {
                let mu = gm(b);
                let e = vel.length_squared() / 2.0 - mu / r.length();
                if e >= 0.0 {
                    return None;
                }
                let a = -mu / (2.0 * e);
                let period = std::f64::consts::TAU * (a.powi(3) / mu).sqrt();
                (period < 6.0 * step).then_some((b, r, vel))
            };
            if center == EARTH {
                // Księżyc (strefa ~66 000 km), potem Ziemia
                let mg = astro::moon::geocentric(t[k]);
                let mv = (astro::moon::geocentric(t[k] + 1.0 / 1440.0) - astro::moon::geocentric(t[k] - 1.0 / 1440.0)) / 120.0;
                let rm = p[k] - mg;
                if rm.length() < 6.6e7 {
                    return check(MOON, rm, v[k] - mv);
                }
                return if p[k].length() < 9.0e8 { check(EARTH, p[k], v[k]) } else { None };
            }
            let r_au = p[k].length() / AU;
            for &(b, a, w) in &BOUND_CANDIDATES {
                if (r_au - a).abs() > w {
                    continue;
                }
                let bp = bodies::position(b, t[k]);
                let rel = p[k] - bp;
                if rel.length() < bodies::soi(b, t[k]) {
                    return check(b, rel, v[k] - bodies::velocity(b, t[k]));
                }
            }
            None
        })
        .collect()
}

fn parse_samples() -> Vec<(String, Arc<Samples>)> {
    let b: &[u8] = include_bytes!("../../assets/trajectories.bin");
    let mut raw = Vec::new();
    let mut p = 0usize;
    let rd = |p: &mut usize, n: usize| -> Option<&[u8]> {
        let s = b.get(*p..*p + n)?;
        *p += n;
        Some(s)
    };
    let Some(cnt) = rd(&mut p, 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])) else { return Vec::new() };
    for _ in 0..cnt {
        let Some(l) = rd(&mut p, 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize) else { break };
        let Some(id) = rd(&mut p, l).map(|s| String::from_utf8_lossy(s).into_owned()) else { break };
        let Some(hdr) = rd(&mut p, 5) else { break };
        let center = if hdr[0] == 1 { EARTH } else { SUN };
        let n = u32::from_le_bytes([hdr[1], hdr[2], hdr[3], hdr[4]]) as usize;
        let (mut t, mut pp, mut vv) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
        for _ in 0..n {
            let Some(r) = rd(&mut p, 32) else { break };
            let f = |o: usize| f32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]) as f64;
            let jd = f64::from_le_bytes([r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7]]);
            t.push(jd);
            pp.push(DVec3::new(f(8), f(12), f(16)) * 1000.0);
            vv.push(DVec3::new(f(20), f(24), f(28)) * 1000.0);
        }
        if t.len() >= 2 {
            raw.push((id, (center, t, pp, vv)));
        }
    }
    raw.into_iter()
        .map(|(id, (center, t, p, v))| {
            let bound = detect_bound(center, &t, &p, &v);
            (id, Arc::new(Samples { center, t, p, v, bound }))
        })
        .collect()
}

/// Próbki Horizons misji (wczytywane i analizowane leniwie, raz).
pub fn samples_for(id: &str) -> Option<Arc<Samples>> {
    static ALL: OnceLock<Vec<(String, Arc<Samples>)>> = OnceLock::new();
    ALL.get_or_init(parse_samples).iter().find(|(m, _)| m == id).map(|(_, s)| s.clone())
}

/* ── odcinki modelu ───────────────────────────────────────────────────── */

#[derive(Clone)]
pub enum Seg {
    /// Stan względem ciała w chwili `t0`, propagowany w zagadnieniu dwóch ciał.
    Conic { center: BodyId, t0: f64, r0: DVec3, v0: DVec3 },
    /// Punkt na powierzchni (obraca się z ciałem).
    Surface { body: BodyId, lat: f64, lon: f64, alt: f64 },
    /// Przy ciele, w stałym odstępie od jego środka (po stronie Słońca).
    Station { body: BodyId, dist: f64 },
    /// Orbita halo wokół punktu libracyjnego.
    Halo { point: BodyId, ay: f64, az: f64, period_d: f64, t0: f64 },
    /// Orbita okołosłoneczna podążająca za Ziemią i powoli się od niej oddalająca.
    Trailing { t0: f64, drift_deg_y: f64 },
    /// Odcinek prosty (awaryjnie, gdy zagadnienie Lamberta nie ma rozwiązania).
    Line { p0: DVec3, p1: DVec3, t0: f64, t1: f64 },
    Samples(Arc<Samples>),
    /// Płynne przejście między odcinkami w przedziale [t0, t1].
    Blend { a: Box<Seg>, b: Box<Seg>, t0: f64, t1: f64 },
    /// Przejście po łuku nad ciałem (wznoszenie, lądowanie): kierunek po wielkim kole,
    /// wysokość zmienia się szybciej niż przesunięcie (bez przecinania powierzchni).
    Arc { body: BodyId, a: Box<Seg>, b: Box<Seg>, t0: f64, t1: f64 },
}

impl Seg {
    pub fn pos(&self, jd: f64) -> DVec3 {
        match self {
            Seg::Conic { center, t0, r0, v0 } => bodies::position(*center, jd) + propagate(*r0, *v0, (jd - t0) * 86_400.0, gm(*center)).0,
            Seg::Surface { body, lat, lon, alt } => {
                let b = bodies::get(*body);
                bodies::position(*body, jd) + bodies::orientation(*body, jd) * geo_dir(*lat, *lon) * (b.radius + alt)
            }
            Seg::Station { body, dist } => {
                let p = bodies::position(*body, jd);
                p - p.normalize_or_zero() * *dist
            }
            Seg::Halo { point, ay, az, period_d, t0 } => {
                let l = bodies::position(*point, jd);
                let e = bodies::position(EARTH, jd);
                let x = e.normalize();
                let y = DVec3::Z.cross(x).normalize();
                let ph = std::f64::consts::TAU * (jd - t0) / period_d;
                l + y * (ay * ph.cos()) + DVec3::Z * (az * ph.sin())
            }
            Seg::Trailing { t0, drift_deg_y } => {
                let ang = -(drift_deg_y * (jd - t0) / 365.25).to_radians();
                let e = bodies::position(EARTH, jd);
                DMat3::from_rotation_z(ang) * e * (1.0 + 0.01 * ((jd - t0) / 365.25).min(1.0))
            }
            Seg::Line { p0, p1, t0, t1 } => p0.lerp(*p1, ((jd - t0) / (t1 - t0)).clamp(0.0, 1.0)),
            Seg::Samples(s) => s.pos(jd),
            Seg::Arc { body, a, b, t0, t1 } => {
                let c = bodies::position(*body, jd);
                let (ra, rb) = (a.pos(jd) - c, b.pos(jd) - c);
                let x = ((jd - t0) / (t1 - t0)).clamp(0.0, 1.0);
                let (la, lb) = (ra.length(), rb.length());
                // wysokość: szybko na początku wznoszenia, wolno na końcu lądowania
                let wr = if lb >= la { 1.0 - (1.0 - x).powi(2) } else { x * x };
                let dir = slerp(ra / la.max(1.0), rb / lb.max(1.0), smooth(x));
                c + dir * (la + (lb - la) * wr)
            }
            Seg::Blend { a, b, t0, t1 } => {
                let w = smooth((jd - t0) / (t1 - t0));
                if w <= 0.0 {
                    a.pos(jd)
                } else if w >= 1.0 {
                    b.pos(jd)
                } else {
                    a.pos(jd).lerp(b.pos(jd), w)
                }
            }
        }
    }

    /// Ciało, wokół którego odcinek krąży po orbicie związanej (do rysowania bieżącej orbity).
    fn bound_body(&self, jd: f64) -> Option<BodyId> {
        match self {
            Seg::Conic { center, r0, v0, .. } if *center != SUN => {
                let mu = gm(*center);
                (v0.length_squared() / 2.0 - mu / r0.length() < 0.0).then_some(*center)
            }
            Seg::Samples(s) => s.is_bound(jd),
            _ => None,
        }
    }

    /// Czy odcinek powtarza się wiele razy (orbita) – wtedy rysujemy tylko bieżący obieg.
    fn repeating(&self, t0: f64, t1: f64) -> bool {
        match self {
            Seg::Conic { center, r0, v0, .. } if *center != SUN => {
                let mu = gm(*center);
                let e = v0.length_squared() / 2.0 - mu / r0.length();
                if e >= 0.0 {
                    return false;
                }
                let a = -mu / (2.0 * e);
                let period = std::f64::consts::TAU * (a.powi(3) / mu).sqrt() / 86_400.0;
                (t1 - t0) > 1.3 * period
            }
            Seg::Halo { .. } => true,
            Seg::Surface { .. } | Seg::Station { .. } => true,
            _ => false,
        }
    }
}

#[derive(Clone)]
pub struct Piece {
    pub t0: f64,
    pub t1: f64,
    pub seg: Seg,
}

/// Linia trasy do narysowania: punkty względem ciała odniesienia i chwile.
pub struct Strip {
    pub t: Vec<f64>,
    pub p: Vec<DVec3>,
}

pub struct Trajectory {
    /// Odcinki modelu (ostatni może trwać bez końca: `t1` = ∞).
    pub pieces: Vec<Piece>,
    /// Krótkie odcinki nadrzędne (przeloty, przejścia) – mają pierwszeństwo przed `pieces`.
    pub overrides: Vec<Piece>,
    pub samples: Option<Arc<Samples>>,
    pub start: f64,
    /// Po tej chwili obiekt nie istnieje (spłonął, wodował, zderzył się).
    pub gone_after: Option<f64>,
    /// Ciało, względem którego rysujemy trasę (Ziemia dla misji okołoziemskich i księżycowych).
    pub frame: BodyId,
    /// Model kończy się powierzchnią albo orbitą od tej chwili (po próbkach Horizons).
    model_final_from: Option<f64>,
    pub strips: Vec<Strip>,
    /// Części lecące osobno (np. moduł dowodzenia na orbicie, gdy lądownik jest na powierzchni).
    pub companions: Vec<Companion>,
}

/// Część misji z własnym położeniem w przedziale [t0, t1].
pub struct Companion {
    /// Indeks w `Mission::parts`.
    pub part: usize,
    pub t0: f64,
    pub t1: f64,
    pub seg: Seg,
}

impl Companion {
    pub fn position(&self, jd: f64) -> Option<DVec3> {
        (jd >= self.t0 && jd < self.t1).then(|| self.seg.pos(jd))
    }
}

impl Trajectory {
    fn model_at(&self, jd: f64) -> Option<&Seg> {
        self.overrides
            .iter()
            .find(|p| jd >= p.t0 && jd < p.t1)
            .or_else(|| self.pieces.iter().find(|p| jd >= p.t0 && jd < p.t1))
            .or_else(|| self.pieces.last().filter(|p| jd >= p.t1 && p.t1.is_infinite()))
            .map(|p| &p.seg)
    }

    /// Położenie [m] względem Słońca albo `None` przed startem i po zniknięciu obiektu.
    pub fn position(&self, jd: f64) -> Option<DVec3> {
        if jd < self.start {
            // godzinę przed startem rakieta stoi na stanowisku
            if self.start - jd < 1.0 / 24.0 {
                // pierwszy odcinek to wznoszenie: przed jego początkiem – punkt na kosmodromie
                return self.pieces.first().map(|p| p.seg.pos(jd));
            }
            return None;
        }
        if self.gone_after.is_some_and(|g| jd > g) {
            return None;
        }
        if let Some(s) = &self.samples {
            let (s0, s1) = s.range();
            if jd >= s0 && jd <= s1 {
                return Some(s.pos(jd));
            }
            if jd < s0 {
                // od startu do pierwszej próbki: przejście od modelu do próbek cofniętych Keplerem
                let (t, r, v) = s.edge_state(false);
                let back = bodies::position(s.center, jd) + propagate(r, v, (jd - t) * 86_400.0, gm(s.center)).0;
                let model = self.model_at(jd).map(|m| m.pos(jd)).unwrap_or(back);
                let w = smooth((jd - self.start) / (s0 - self.start).max(1e-6));
                return Some(model.lerp(back, w));
            }
            if self.model_final_from.is_some_and(|f| jd >= f) {
                return self.model_at(jd).map(|m| m.pos(jd));
            }
            let (t, r, v) = s.edge_state(true);
            if s.center == EARTH && (jd - t) > 30.0 {
                return None;
            }
            return Some(bodies::position(s.center, jd) + propagate(r, v, (jd - t) * 86_400.0, gm(s.center)).0);
        }
        self.model_at(jd).map(|m| m.pos(jd))
    }

    /// Ciało, wokół którego obiekt krąży w chwili `jd` (do rysowania bieżącej orbity).
    pub fn bound_body(&self, jd: f64) -> Option<BodyId> {
        if let Some(s) = &self.samples {
            let (s0, s1) = s.range();
            if jd >= s0 && jd <= s1 {
                return s.is_bound(jd).or_else(|| {
                    // próbki co kilka minut: orbita wokół Ziemi albo Księżyca widoczna wprost
                    None
                });
            }
        }
        let p = self.overrides.iter().chain(self.pieces.iter()).find(|p| jd >= p.t0 && jd < p.t1)?;
        if p.seg.repeating(p.t0, p.t1) { p.seg.bound_body(jd) } else { None }
    }

    /// Najbliższe ciało „dominujące” (do odczytów wysokości i prędkości względnej).
    pub fn nearest_body(&self, jd: f64) -> BodyId {
        let Some(p) = self.position(jd) else { return SUN };
        nearest_body(p, jd)
    }
}

/// Ciało, w którego strefie wpływu leży punkt (najmniejsza strefa wygrywa).
pub fn nearest_body(p: DVec3, jd: f64) -> BodyId {
    let mut best = (SUN, f64::INFINITY);
    for (id, b) in bodies::all().iter().enumerate() {
        if id == SUN || b.gm <= 0.0 || b.kind == bodies::Kind::Point || b.kind == bodies::Kind::Small {
            continue;
        }
        let soi = bodies::soi(id, jd);
        let d = (p - bodies::position(id, jd)).length();
        if d < soi && soi < best.1 {
            best = (id, soi);
        }
    }
    best.0
}

/* ── budowa modelu ───────────────────────────────────────────────────── */

struct Builder<'a> {
    m: &'a Mission,
    pieces: Vec<Piece>,
    overrides: Vec<Piece>,
    /// Od tej chwili model kończy się orbitą albo powierzchnią celu.
    model_final: Option<f64>,
    /// Orbita wokół celu (księżycowa albo końcowa) – dla części, które na niej zostają.
    target_orbit: Option<Seg>,
    /// Miejsce lądowania: (ciało, szerokość, długość).
    landing: Option<(BodyId, f64, f64)>,
}

impl Builder<'_> {
    fn push(&mut self, t0: f64, t1: f64, seg: Seg) {
        if t1 > t0 {
            self.pieces.push(Piece { t0, t1, seg });
        }
    }

    fn over(&mut self, t0: f64, t1: f64, seg: Seg) {
        if t1 > t0 {
            self.overrides.push(Piece { t0, t1, seg });
        }
    }

    fn site(&self) -> (f64, f64) {
        self.m.site().map(|s| (s.lat, s.lon)).unwrap_or((28.5, -80.6))
    }

    /// Orbita okołoziemska przechodząca w chwili `t` nad kosmodromem (wznoszenie na wschód).
    fn earth_orbit_over_site(&self, t: f64, peri_km: f64, apo_km: f64, inc_deg: f64) -> Seg {
        let (lat, lon) = self.site();
        let re = bodies::get(EARTH).radius;
        let mu = gm(EARTH);
        // kierunek kosmodromu w układzie równikowym
        let s_eq = DMat3::from_rotation_z(time::gmst(t)) * geo_dir(lat, lon);
        let ra = s_eq.y.atan2(s_eq.x);
        let dec = lat.to_radians();
        let mut inc = inc_deg.to_radians();
        let retro = inc > std::f64::consts::FRAC_PI_2;
        if !retro {
            inc = inc.max(dec.abs() + 0.002);
        }
        let si = inc.sin().abs().max(1e-6);
        let u = (dec.sin() / si).clamp(-1.0, 1.0).asin();
        let node = ra - (inc.cos() * u.sin()).atan2(u.cos());
        let n = DVec3::new(inc.sin() * node.sin(), -inc.sin() * node.cos(), inc.cos());
        let rp = re + peri_km * 1000.0;
        let ra_ = re + apo_km.max(peri_km) * 1000.0;
        let vp = (mu * (2.0 / rp - 2.0 / (rp + ra_))).sqrt();
        let r = s_eq * rp;
        let v = n.cross(s_eq).normalize() * vp;
        Seg::Conic { center: EARTH, t0: t, r0: astro::equatorial_to_ecliptic(r), v0: astro::equatorial_to_ecliptic(v) }
    }

    fn launch_site_seg(&self) -> Seg {
        let (lat, lon) = self.site();
        Seg::Surface { body: EARTH, lat, lon, alt: 0.0 }
    }

    /// Start: wznoszenie z kosmodromu na orbitę parkingową; zwraca odcinek orbity i chwilę wejścia na nią.
    fn ascent(&mut self, until: f64, peri_km: f64, apo_km: f64, inc: f64) -> (Seg, f64) {
        let launch = self.m.launch_jd;
        let t_ins = (launch + 9.0 / 1440.0).min(until.max(launch + 2.0 / 1440.0));
        // Orbita przechodzi nad kosmodromem tuż po starcie, więc w czasie wznoszenia jej punkt leży
        // stale przed rakietą (w kierunku lotu). Odniesienie w chwili wejścia na orbitę kładło ten punkt
        // za kosmodromem – tor startu cofał się i zawracał (haczyk).
        let t_ref = launch;
        let orbit = self.earth_orbit_over_site(t_ref, peri_km, apo_km, inc);
        self.push(launch, t_ins, Seg::Arc { body: EARTH, a: Box::new(self.launch_site_seg()), b: Box::new(orbit.clone()), t0: launch, t1: t_ins });
        (orbit, t_ins)
    }

    fn build_orbit(&mut self, body: &str, peri: f64, apo: f64, inc: f64) {
        let end = self.m.end_jd.unwrap_or(f64::INFINITY);
        if bodies::find(body) != Some(EARTH) {
            return;
        }
        let (orbit, t_ins) = self.ascent(end, peri, apo, inc);
        self.push(t_ins, end, orbit);
    }

    fn moon_normal(t: f64) -> DVec3 {
        let a = astro::moon::geocentric(t);
        let b = astro::moon::geocentric(t + 0.01);
        a.cross(b).normalize()
    }

    /// Punkt wejścia w atmosferę Ziemi (względem Ziemi) dla powrotu w chwili `te`.
    fn entry_point(t_from: f64, r: f64) -> DVec3 {
        let m = astro::moon::geocentric(t_from).normalize();
        DMat3::from_axis_angle(Self::moon_normal(t_from), 2.6) * m * r
    }

    fn earth_rel(seg: &Seg, t: f64) -> DVec3 {
        seg.pos(t) - bodies::position(EARTH, t)
    }

    fn lambert_geo(&mut self, from: DVec3, t0: f64, to: DVec3, t1: f64) -> Seg {
        match lambert(from, to, (t1 - t0) * 86_400.0, gm(EARTH), true) {
            Some((v1, _)) => Seg::Conic { center: EARTH, t0, r0: from, v0: v1 },
            None => {
                let e0 = bodies::position(EARTH, t0);
                Seg::Line { p0: e0 + from, p1: bodies::position(EARTH, t1) + to, t0, t1 }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_lunar(
        &mut self,
        tli: f64,
        loi: Option<f64>,
        orbit_km: f64,
        orbit_apo_km: Option<f64>,
        landing: Option<(f64, f64, f64)>,
        liftoff: Option<f64>,
        tei: Option<f64>,
        entry: Option<f64>,
        flyby: Option<(f64, f64)>,
        return_km: Option<f64>,
        direct: bool,
    ) {
        let end = self.m.end_jd.unwrap_or(f64::INFINITY);
        let (lat, _) = self.site();
        let (park, t_ins) = self.ascent(tli, 185.0, 185.0, lat.abs() + 0.5);
        self.push(t_ins, tli, park.clone());
        let r_tli = Self::earth_rel(&park, tli);
        let rm = bodies::get(MOON).radius;
        let re = bodies::get(EARTH).radius;
        let r_entry = return_km.map(|k| k * 1000.0).unwrap_or(re + 120_000.0);
        let splash = |b: &mut Self, from: Seg, t_from: f64, te: f64| {
            let p_from = Self::earth_rel(&from, t_from);
            let target = Self::entry_point(t_from, r_entry);
            let back = b.lambert_geo(p_from, t_from, target, te);
            b.push(t_from, te, back.clone());
            if end.is_finite() && end > te {
                // pod punktem wejścia: wodowanie
                let local = bodies::orientation(EARTH, te).transpose() * target.normalize();
                let (la, lo) = (local.z.asin().to_degrees(), local.y.atan2(local.x).to_degrees());
                b.push(te, end, Seg::Blend { a: Box::new(back), b: Box::new(Seg::Surface { body: EARTH, lat: la, lon: lo, alt: 0.0 }), t0: te, t1: end });
            }
        };
        // bezpośrednie lądowanie / uderzenie
        if let (true, Some((tl, la, lo))) = (direct, landing) {
            let target = astro::moon::geocentric(tl) + bodies::orientation(MOON, tl) * geo_dir(la, lo) * rm;
            let arc = self.lambert_geo(r_tli, tli, target, tl);
            self.push(tli, tl, arc);
            self.landing = Some((MOON, la, lo));
            self.push(tl, if end > tl + 1e-3 { end } else { f64::INFINITY }, Seg::Surface { body: MOON, lat: la, lon: lo, alt: 0.0 });
            return;
        }
        // przelot (swobodny powrót)
        if let Some((tf, km)) = flyby {
            let mg = astro::moon::geocentric(tf);
            let p_f = mg + mg.normalize() * (rm + km * 1000.0);
            let out = self.lambert_geo(r_tli, tli, p_f, tf);
            self.push(tli, tf, out.clone());
            if let Some(te) = entry {
                splash(self, out, tf, te);
            }
            return;
        }
        let Some(t_loi) = loi else { return };
        let mg = astro::moon::geocentric(t_loi);
        let mv = (astro::moon::geocentric(t_loi + 1.0 / 1440.0) - astro::moon::geocentric(t_loi - 1.0 / 1440.0)) / 120.0;
        let r_lo = rm + orbit_km * 1000.0;
        let dir = (mg.normalize() + mv.normalize()).normalize();
        let arr = mg + dir * r_lo;
        let Some((v1, v2)) = lambert(r_tli, arr, (t_loi - tli) * 86_400.0, gm(EARTH), true) else {
            self.push(tli, t_loi, Seg::Line { p0: bodies::position(EARTH, tli) + r_tli, p1: bodies::position(EARTH, t_loi) + arr, t0: tli, t1: t_loi });
            return;
        };
        self.push(tli, t_loi, Seg::Conic { center: EARTH, t0: tli, r0: r_tli, v0: v1 });
        // orbita księżycowa w płaszczyźnie przylotu
        let r_rel = dir * r_lo;
        let v_rel = v2 - mv;
        let h = r_rel.cross(v_rel);
        let h = if h.length() > 1.0 { h.normalize() } else { Self::moon_normal(t_loi) };
        let apo = orbit_apo_km.map(|k| rm + k * 1000.0).unwrap_or(r_lo);
        let vp = (gm(MOON) * (2.0 / r_lo - 2.0 / (r_lo + apo))).sqrt();
        let lunar_orbit = Seg::Conic { center: MOON, t0: t_loi, r0: r_rel, v0: h.cross(r_rel).normalize() * vp };
        let orbit_end = landing.map(|l| l.0 - 12.0 / 1440.0).or(tei).unwrap_or(end);
        self.push(t_loi, orbit_end, lunar_orbit.clone());
        self.target_orbit = Some(lunar_orbit.clone());
        let mut last = lunar_orbit.clone();
        if let Some((tl, la, lo)) = landing {
            self.landing = Some((MOON, la, lo));
            let surf = Seg::Surface { body: MOON, lat: la, lon: lo, alt: 0.0 };
            let d0 = tl - 12.0 / 1440.0;
            self.push(d0, tl, Seg::Arc { body: MOON, a: Box::new(lunar_orbit.clone()), b: Box::new(surf.clone()), t0: d0, t1: tl });
            match liftoff {
                Some(tlo) => {
                    self.push(tl, tlo, surf.clone());
                    let t_orb = tlo + 8.0 / 1440.0;
                    self.push(tlo, t_orb, Seg::Arc { body: MOON, a: Box::new(surf), b: Box::new(lunar_orbit.clone()), t0: tlo, t1: t_orb });
                    self.push(t_orb, tei.unwrap_or(end), lunar_orbit.clone());
                    last = lunar_orbit.clone();
                }
                None => {
                    self.push(tl, f64::INFINITY, surf);
                    return;
                }
            }
        }
        if let (Some(t_ei), Some(te)) = (tei, entry) {
            splash(self, last, t_ei, te);
        }
    }

    /// Hiperbola przelotu/przylotu/odlotu wokół ciała; `dir_in`/`dir_out` – kierunki prędkości
    /// względem ciała daleko przed i po (jednostkowe), `vinf` – prędkość nadmiarowa.
    fn hyperbola(body: BodyId, t: f64, dir_in: Option<DVec3>, dir_out: Option<DVec3>, vinf: f64, rp_min: f64) -> (Seg, f64) {
        let mu = gm(body).max(1.0);
        let r_body = bodies::get(body).radius.max(1.0);
        let (p_hat, q_hat, rp) = match (dir_in, dir_out) {
            (Some(a), Some(b)) => {
                let turn = a.dot(b).clamp(-1.0, 1.0).acos();
                let p = (a - b).try_normalize().unwrap_or_else(|| a.any_orthonormal_vector());
                let q = (a + b).try_normalize().unwrap_or(a);
                let e = 1.0 / (turn / 2.0).sin().max(1e-3);
                let rp = (mu * (e - 1.0) / (vinf * vinf)).max(rp_min).min(r_body * 60.0);
                (p, q, rp)
            }
            (Some(d), None) | (None, Some(d)) => {
                let rp = rp_min;
                let e = 1.0 + rp * vinf * vinf / mu;
                let th = (-1.0 / e).acos();
                let w = DVec3::Z.cross(d).try_normalize().unwrap_or(DVec3::X).cross(d).normalize();
                if dir_out.is_some() {
                    (th.cos() * d - th.sin() * w, th.sin() * d + th.cos() * w, rp)
                } else {
                    (-th.cos() * d + th.sin() * w, th.sin() * d + th.cos() * w, rp)
                }
            }
            (None, None) => (DVec3::X, DVec3::Y, rp_min),
        };
        let vp = (vinf * vinf + 2.0 * mu / rp).sqrt();
        let soi = bodies::soi(body, t).min(1.0e10);
        let half = (0.3 * soi / vinf.max(100.0) / 86_400.0).clamp(0.2, 8.0);
        (Seg::Conic { center: body, t0: t, r0: p_hat * rp, v0: q_hat * vp }, half)
    }

    fn waypoint(leg: &super::Leg) -> Option<(f64, Option<BodyId>, DVec3)> {
        let t = time::parse(&leg.t)?;
        if let Some(b) = leg.body.as_deref().and_then(bodies::find) {
            return Some((t, Some(b), bodies::position(b, t)));
        }
        let r = leg.r_au? * AU;
        let dir = match (leg.ra, leg.dec, leg.ecl_lon, leg.ecl_lat) {
            (Some(a), Some(d), _, _) => astro::radec_to_ecliptic(a, d),
            (_, _, Some(l), Some(b)) => astro::ecliptic_dir(l, b),
            _ => return None,
        };
        Some((t, None, dir * r))
    }

    fn build_helio(&mut self, legs: &[super::Leg], fin: Option<&Final>) {
        let end = self.m.end_jd.unwrap_or(f64::INFINITY);
        let wps: Vec<_> = legs.iter().filter_map(Self::waypoint).collect();
        if wps.is_empty() {
            return;
        }
        // start z Ziemi: wznoszenie i orbita parkingowa do pierwszego punktu
        let (lat, _) = self.site();
        let (park, t_ins) = self.ascent(wps[0].0, 200.0, 200.0, lat.abs() + 0.5);
        self.push(t_ins, wps[0].0, park.clone());
        // łuki Lamberta
        let mut legs_v: Vec<Option<(DVec3, DVec3)>> = Vec::new();
        for w in wps.windows(2) {
            let (t0, b0, p0) = w[0];
            let (t1, b1, p1) = w[1];
            if b0.is_some() && b0 == b1 {
                let body = b0.unwrap_or(SUN);
                let dist = (bodies::get(body).radius * 6.0).max(20_000.0);
                self.push(t0, t1, Seg::Station { body, dist });
                legs_v.push(None);
                continue;
            }
            match lambert(p0, p1, (t1 - t0) * 86_400.0, GM_SUN, true) {
                Some((v1, v2)) => {
                    self.push(t0, t1, Seg::Conic { center: SUN, t0, r0: p0, v0: v1 });
                    legs_v.push(Some((v1, v2)));
                }
                None => {
                    self.push(t0, t1, Seg::Line { p0, p1, t0, t1 });
                    legs_v.push(None);
                }
            }
        }
        // przeloty, odlot i przylot: hiperbole względem ciał
        let n = wps.len();
        let fin_orbit = fin.and_then(|f| f.orbit.as_ref());
        let fin_surface = fin.and_then(|f| f.surface.as_ref());
        let mut final_from: Option<f64> = None;
        for k in 0..n {
            let (t, Some(b), _) = wps[k] else { continue };
            if b == SUN {
                continue;
            }
            let vb = bodies::velocity(b, t);
            let vin = if k > 0 { legs_v[k - 1].map(|(_, v2)| v2 - vb) } else { None };
            let vout = if k + 1 < n { legs_v[k].map(|(v1, _)| v1 - vb) } else { None };
            let last = k + 1 == n;
            let radius = bodies::get(b).radius;
            if k == 0 {
                let Some(vo) = vout else { continue };
                let (hyp, half) = Self::hyperbola(b, t, None, vo.try_normalize(), vo.length(), radius + 200_000.0);
                let leg = self.pieces.iter().find(|p| (p.t0 - t).abs() < 1e-9 && matches!(p.seg, Seg::Conic { center: SUN, .. })).map(|p| p.seg.clone());
                self.over(t - 5.0 / 1440.0, t, Seg::Blend { a: Box::new(park.clone()), b: Box::new(hyp.clone()), t0: t - 5.0 / 1440.0, t1: t });
                self.over(t, t + half / 2.0, hyp.clone());
                if let Some(leg) = leg {
                    self.over(t + half / 2.0, t + half, Seg::Blend { a: Box::new(hyp), b: Box::new(leg), t0: t + half / 2.0, t1: t + half });
                }
                continue;
            }
            let prev_leg = self.pieces.iter().find(|p| (p.t1 - t).abs() < 1e-9 && matches!(p.seg, Seg::Conic { center: SUN, .. })).map(|p| p.seg.clone());
            let next_leg = self.pieces.iter().find(|p| (p.t0 - t).abs() < 1e-9 && matches!(p.seg, Seg::Conic { center: SUN, .. })).map(|p| p.seg.clone());
            let vinf = match (vin, vout) {
                (Some(a), Some(b)) => (a.length() + b.length()) / 2.0,
                (Some(a), None) | (None, Some(a)) => a.length(),
                _ => continue,
            };
            let capture = last && (fin_orbit.is_some() || fin_surface.is_some());
            let rp_min = if capture {
                fin_orbit.map(|o| radius + o.peri_km * 1000.0).unwrap_or(radius * 1.02)
            } else {
                radius * 1.3
            };
            let (hyp, half) = Self::hyperbola(b, t, vin.and_then(|v| v.try_normalize()), if capture { None } else { vout.and_then(|v| v.try_normalize()) }, vinf, rp_min);
            if let Some(prev) = prev_leg {
                self.over(t - half, t - half / 2.0, Seg::Blend { a: Box::new(prev), b: Box::new(hyp.clone()), t0: t - half, t1: t - half / 2.0 });
            }
            if capture {
                self.over(t - half / 2.0, t, hyp.clone());
                if let Seg::Conic { r0, v0, .. } = &hyp {
                    let rp = r0.length();
                    let mu = gm(b);
                    let mut surf_from = None;
                    if let Some(o) = fin_orbit {
                        let ra = radius + o.apo_km.max(o.peri_km) * 1000.0;
                        let vp = (mu * (2.0 / rp - 2.0 / (rp + ra))).sqrt();
                        let orbit = Seg::Conic { center: b, t0: t, r0: *r0, v0: v0.normalize() * vp };
                        self.target_orbit = Some(orbit.clone());
                        let s_from = fin.and_then(|f| f.surface_from.as_deref()).and_then(time::parse);
                        surf_from = s_from;
                        let orbit_end = s_from.map(|s| s - 15.0 / 1440.0).unwrap_or(end.max(t));
                        self.push(t, orbit_end, orbit.clone());
                        if let (Some(sf), Some(s)) = (s_from, fin_surface) {
                            let sb = bodies::find(&s.body).unwrap_or(b);
                            self.landing = Some((sb, s.lat, s.lon));
                            let surf = Seg::Surface { body: sb, lat: s.lat, lon: s.lon, alt: 0.0 };
                            self.push(orbit_end, sf, Seg::Blend { a: Box::new(orbit), b: Box::new(surf.clone()), t0: orbit_end, t1: sf });
                            self.push(sf, f64::INFINITY, surf);
                        }
                        final_from = Some(t);
                    }
                    if surf_from.is_none() && fin_orbit.is_none() {
                        if let Some(s) = fin_surface {
                            let sb = bodies::find(&s.body).unwrap_or(b);
                            self.landing = Some((sb, s.lat, s.lon));
                            let surf = Seg::Surface { body: sb, lat: s.lat, lon: s.lon, alt: 0.0 };
                            let d0 = t - 10.0 / 1440.0;
                            self.over(d0, t, Seg::Blend { a: Box::new(hyp.clone()), b: Box::new(surf.clone()), t0: d0, t1: t });
                            self.push(t, f64::INFINITY, surf);
                            final_from = Some(t);
                        }
                    }
                }
            } else {
                self.over(t - half / 2.0, t + half / 2.0, hyp.clone());
                if let Some(next) = next_leg {
                    self.over(t + half / 2.0, t + half, Seg::Blend { a: Box::new(hyp), b: Box::new(next), t0: t + half / 2.0, t1: t + half });
                }
            }
        }
        // po ostatnim punkcie bez celu: lot swobodny po ostatnim łuku
        if final_from.is_none() {
            if let (Some(Some((_, v2))), Some(&(t, b, p))) = (legs_v.last(), wps.last()) {
                if b.is_none() || self.m.end_jd.is_none() {
                    self.push(t, f64::INFINITY, Seg::Conic { center: SUN, t0: t, r0: p, v0: *v2 });
                }
            }
        }
        self.model_final = final_from;
    }

    fn build_lagrange(&mut self, point: &str, arrive: f64) {
        let pid = bodies::find(point).unwrap_or(bodies::L2);
        let (lat, _) = self.site();
        let (park, t_ins) = self.ascent(arrive, 200.0, 200.0, lat.abs() + 0.5);
        let dep = t_ins + 40.0 / 1440.0;
        self.push(t_ins, dep, park.clone());
        let (ay, az, period) = match self.m.id.as_str() {
            "soho" => (650.0e6, 200.0e6, 178.0),
            "gaia" => (340.0e6, 90.0e6, 180.0),
            _ => (800.0e6, 250.0e6, 180.0),
        };
        let halo = Seg::Halo { point: pid, ay, az, period_d: period, t0: arrive };
        self.push(dep, arrive, Seg::Blend { a: Box::new(park), b: Box::new(halo.clone()), t0: dep, t1: arrive });
        self.push(arrive, f64::INFINITY, halo);
    }

    fn build_trailing(&mut self, drift: f64) {
        let (park, t_ins) = self.ascent(self.m.launch_jd + 1.0, 200.0, 200.0, 28.5);
        let t1 = t_ins + 3.0;
        let trail = Seg::Trailing { t0: self.m.launch_jd, drift_deg_y: drift };
        self.push(t_ins, t1, Seg::Blend { a: Box::new(park), b: Box::new(trail.clone()), t0: t_ins, t1 });
        self.push(t1, f64::INFINITY, trail);
    }
}

/// Buduje trajektorię misji (model + próbki Horizons, jeśli są).
pub fn build(m: &Mission) -> Trajectory {
    let mut b = Builder { m, pieces: Vec::new(), overrides: Vec::new(), model_final: None, target_orbit: None, landing: None };
    b.run();
    let companions = companions(m, b.target_orbit.as_ref(), b.landing);
    let Builder { mut pieces, overrides, model_final, .. } = b;
    pieces.sort_by(|a, b| a.t0.total_cmp(&b.t0));
    let samples = m.horizons.as_ref().and_then(|_| samples_for(&m.id));
    let frame = match (&m.traj, &samples) {
        (_, Some(s)) if s.center == EARTH => EARTH,
        (TrajSpec::Orbit { .. } | TrajSpec::Lunar { .. } | TrajSpec::Lagrange { .. }, None) => EARTH,
        _ => SUN,
    };
    let persists = match &m.traj {
        TrajSpec::Helio { .. } | TrajSpec::Trailing { .. } => true,
        TrajSpec::Lunar { landing, liftoff, .. } => landing.is_some() && liftoff.is_none(),
        _ => false,
    };
    let fin_orbit = matches!(&m.traj, TrajSpec::Helio { fin: Some(f), .. } if f.orbit.is_some() && f.surface.is_none());
    let gone_after = match m.end_jd {
        Some(e) if !persists || fin_orbit => Some(e),
        _ => None,
    };
    let mut t = Trajectory { pieces, overrides, samples, start: m.launch_jd, gone_after, frame, model_final_from: model_final, strips: Vec::new(), companions };
    t.strips = make_strips(&t, m);
    t
}

/// Orbita wokół ciała w jego płaszczyźnie równikowej nachylonej o `inc` (węzeł umowny).
fn orbit_around(body: BodyId, t0: f64, peri_km: f64, apo_km: f64, inc_deg: f64) -> Seg {
    let r = bodies::get(body).radius;
    let rp = r + peri_km * 1000.0;
    let ra = r + apo_km.max(peri_km) * 1000.0;
    let mu = gm(body);
    let vp = (mu * (2.0 / rp - 2.0 / (rp + ra))).sqrt();
    let rot = bodies::orientation(body, t0);
    let i = inc_deg.to_radians();
    let r0 = rot * DVec3::X * rp;
    let v0 = rot * DVec3::new(0.0, i.cos(), i.sin()) * vp;
    Seg::Conic { center: body, t0, r0, v0 }
}

/// Znaczniki części lecących osobno, z opisu `parts[].pos` i wydarzeń `sep`/`dock`.
fn companions(m: &Mission, target_orbit: Option<&Seg>, landing: Option<(BodyId, f64, f64)>) -> Vec<Companion> {
    let mut out = Vec::new();
    for (k, part) in m.parts.iter().enumerate() {
        let Some(pos) = &part.pos else { continue };
        let until = part.until.as_deref().and_then(time::parse).unwrap_or(f64::INFINITY);
        let from = part.from.as_deref().and_then(time::parse).unwrap_or(f64::NEG_INFINITY);
        for (t0, t1) in m.part_free_intervals(k) {
            let (t0, t1) = (t0.max(from), t1.min(until));
            if t1 <= t0 {
                continue;
            }
            let body = pos.body.as_deref().and_then(bodies::find);
            let seg = match pos.at.as_str() {
                "surface" => match (body, pos.lat, pos.lon, landing) {
                    (b, Some(la), Some(lo), l) => Seg::Surface { body: b.or(l.map(|x| x.0)).unwrap_or(MOON), lat: la, lon: lo, alt: 0.0 },
                    (_, _, _, Some((b, la, lo))) => Seg::Surface { body: b, lat: la, lon: lo, alt: 0.0 },
                    _ => continue,
                },
                "orbit" => match (pos.peri_km, body, target_orbit) {
                    (Some(pe), Some(b), _) => orbit_around(b, t0, pe, pos.apo_km.unwrap_or(pe), pos.inc.unwrap_or(0.0)),
                    (_, _, Some(o)) => o.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            out.push(Companion { part: k, t0, t1, seg });
        }
    }
    out
}

impl Builder<'_> {
    fn run(&mut self) {
        let m = self.m;
        let p = |s: &Option<String>| s.as_deref().and_then(time::parse);
        match &m.traj {
            TrajSpec::Orbit { body, peri_km, apo_km, inc } => self.build_orbit(body, *peri_km, *apo_km, *inc),
            TrajSpec::Lunar { tli, loi, orbit_km, orbit_apo_km, landing, liftoff, tei, entry, flyby, flyby_km, return_km, direct, .. } => {
                let Some(tli) = time::parse(tli) else { return };
                let landing = landing.as_ref().and_then(|l| time::parse(&l.t).map(|t| (t, l.lat, l.lon)));
                let flyby = p(flyby).map(|t| (t, flyby_km.unwrap_or(1000.0)));
                self.build_lunar(tli, p(loi), orbit_km.unwrap_or(110.0), *orbit_apo_km, landing, p(liftoff), p(tei), p(entry), flyby, *return_km, *direct);
            }
            TrajSpec::Helio { legs, fin } => {
                self.build_helio(legs, fin.as_ref());
            }
            TrajSpec::Lagrange { point, arrive } => {
                let a = time::parse(arrive).unwrap_or(m.launch_jd + 30.0);
                self.build_lagrange(point, a);
            }
            TrajSpec::Trailing { drift_deg_per_year } => self.build_trailing(*drift_deg_per_year),
        }
    }
}

/* ── linia trasy ─────────────────────────────────────────────────────── */

/// Punkty trasy względem ciała odniesienia `frame`; fazy orbitalne (powtarzające się) pomijamy –
/// bieżąca orbita rysuje się osobno.
fn make_strips(t: &Trajectory, m: &Mission) -> Vec<Strip> {
    let frame = t.frame;
    let mut strips: Vec<Strip> = Vec::new();
    let mut cur = Strip { t: Vec::new(), p: Vec::new() };
    let flush = |cur: &mut Strip, strips: &mut Vec<Strip>| {
        if cur.t.len() >= 2 {
            strips.push(std::mem::replace(cur, Strip { t: Vec::new(), p: Vec::new() }));
        } else {
            cur.t.clear();
            cur.p.clear();
        }
    };
    let horizon_end = m.end_jd.unwrap_or_else(|| time::now() + 3_650.0).min(time::now() + 40.0 * 365.25);
    let mut times: Vec<f64> = Vec::new();
    if let Some(s) = &t.samples {
        for (i, &jd) in s.t.iter().enumerate() {
            if s.bound[i].is_some() {
                times.push(f64::NAN);
            } else {
                times.push(jd);
            }
        }
        let (s0, s1) = s.range();
        // przed pierwszą próbką
        let mut pre: Vec<f64> = (0..24).map(|k| t.start + (s0 - t.start) * k as f64 / 24.0).collect();
        pre.extend(times);
        times = pre;
        if t.model_final_from.is_none() && m.end_jd.is_none() && s.center == SUN {
            let tail = horizon_end.max(s1);
            for k in 1..=60 {
                times.push(s1 + (tail - s1) * k as f64 / 60.0);
            }
        }
    } else {
        let mut all: Vec<&Piece> = t.pieces.iter().chain(t.overrides.iter()).collect();
        all.sort_by(|a, b| a.t0.total_cmp(&b.t0));
        // punkty co krok zależny od typu odcinka; fazy powtarzające się przerywają linię
        let mut jd = t.start;
        let stop = t.gone_after.unwrap_or(horizon_end).min(horizon_end);
        while jd < stop {
            let piece = t
                .overrides
                .iter()
                .find(|p| jd >= p.t0 && jd < p.t1)
                .or_else(|| t.pieces.iter().find(|p| jd >= p.t0 && jd < p.t1));
            let Some(p) = piece else {
                // przerwa: przeskok do następnego odcinka
                match all.iter().map(|p| p.t0).filter(|&x| x > jd).fold(f64::INFINITY, f64::min) {
                    x if x.is_finite() => {
                        jd = x;
                        continue;
                    }
                    _ => break,
                }
            };
            if p.seg.repeating(p.t0, p.t1) {
                times.push(f64::NAN);
                jd = p.t1.min(stop);
                if p.t1.is_infinite() {
                    break;
                }
                continue;
            }
            let span = (p.t1.min(stop) - p.t0).max(1e-6);
            let step = match &p.seg {
                Seg::Blend { .. } | Seg::Arc { .. } => span / 40.0,
                Seg::Conic { center, .. } if *center != SUN => (span / 240.0).max(30.0 / 86_400.0),
                _ => (span / 300.0).max(0.02),
            };
            times.push(jd);
            jd += step.max(1.0 / 86_400.0);
        }
        times.push(stop);
    }
    for jd in times {
        if !jd.is_finite() {
            flush(&mut cur, &mut strips);
            continue;
        }
        match t.position(jd) {
            Some(p) => {
                cur.t.push(jd);
                cur.p.push(p - bodies::position(frame, jd));
            }
            None => flush(&mut cur, &mut strips),
        }
    }
    flush(&mut cur, &mut strips);
    strips
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::missions::{db, index_of};

    fn traj(id: &str) -> (Trajectory, &'static Mission) {
        let m = &db().missions[index_of(id).unwrap()];
        (build(m), m)
    }

    #[test]
    fn apollo_11_reaches_moon_and_lands() {
        let (t, m) = traj("apollo-11");
        let landing = time::parse("1969-07-20T21:00Z").unwrap();
        let p = t.position(landing).unwrap();
        let d = (p - bodies::position(MOON, landing)).length();
        assert!((d - bodies::get(MOON).radius).abs() < 2_000.0, "odległość od środka Księżyca {d}");
        let lunar_orbit = time::parse("1969-07-20T05:00Z").unwrap();
        let d = (t.position(lunar_orbit).unwrap() - bodies::position(MOON, lunar_orbit)).length();
        assert!(d < bodies::get(MOON).radius + 200_000.0, "orbita księżycowa {d}");
        assert!(t.position(m.end_jd.unwrap() + 1.0).is_none());
        assert!(!t.strips.is_empty());
    }

    #[test]
    fn voyager_samples_near_jupiter() {
        let (t, _) = traj("voyager-1");
        assert!(t.samples.is_some());
        let jd = time::parse("1979-03-05T12:05Z").unwrap();
        let d = (t.position(jd).unwrap() - bodies::position(bodies::JUPITER, jd)).length();
        assert!(d < 1.0e9, "Voyager 1 przy Jowiszu: {d}");
    }

    #[test]
    fn model_flyby_close_to_planet() {
        // Pioneer 10 bez próbek (model): przelot obok Jowisza
        let m = db().missions.iter().find(|m| m.id == "pioneer-10").unwrap().clone();
        let mut m = m;
        m.horizons = None;
        let t = build(&m);
        let jd = time::parse("1973-12-04T02:26Z").unwrap();
        let d = (t.position(jd).unwrap() - bodies::position(bodies::JUPITER, jd)).length();
        assert!(d < 1.0e9, "model przelotu: {d}");
    }

    #[test]
    fn ascent_moves_away_from_site() {
        for id in ["explorer-1", "apollo-11", "sputnik-1", "voyager-1"] {
            let (tr, m) = traj(id);
            let site = |jd: f64| {
                let s = m.site().unwrap();
                Seg::Surface { body: EARTH, lat: s.lat, lon: s.lon, alt: 0.0 }.pos(jd)
            };
            let mut last = -1.0;
            for k in 1..=16 {
                let jd = m.launch_jd + k as f64 * 0.5 / 1440.0;
                let p = tr.position(jd).unwrap() - bodies::position(EARTH, jd);
                let s = site(jd) - bodies::position(EARTH, jd);
                let ang = p.normalize().angle_between(s.normalize());
                assert!(ang >= last - 1e-4, "{id}: tor startu zawraca w {k}. pół minucie ({ang} < {last})");
                last = ang;
            }
            assert!(last.to_degrees() > 10.0, "{id}: po 8 min tylko {:.1}° od kosmodromu", last.to_degrees());
        }
    }

    #[test]
    fn every_mission_has_position_after_launch() {
        for m in &db().missions {
            let t = build(m);
            let jd = m.launch_jd + 0.5 / 24.0;
            let p = t.position(jd);
            assert!(p.is_some_and(|p| p.is_finite()), "{}: brak położenia po starcie", m.id);
        }
    }
}
