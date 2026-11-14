//! Ciała Układu Słonecznego: parametry fizyczne i obrotu (z plików konfiguracyjnych Orbitera),
//! źródła położeń (VSOP87, teoria Księżyca, elementy keplerowskie) i orientacja osi.

use std::sync::OnceLock;

use glam::{DMat3, DVec3};
use serde::Deserialize;

use super::kepler::Elements;
use super::time::J2000;
use super::{AU, G, GM_SUN};

pub type BodyId = usize;

pub const SUN: BodyId = 0;
pub const MERCURY: BodyId = 1;
pub const VENUS: BodyId = 2;
pub const EARTH: BodyId = 3;
pub const MOON: BodyId = 4;
pub const MARS: BodyId = 5;
pub const JUPITER: BodyId = 8;
pub const SATURN: BodyId = 13;
pub const L1: BodyId = 18;
pub const L2: BodyId = 19;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Star,
    Planet,
    Dwarf,
    Moon,
    Small,
    /// Punkt libracyjny (bez bryły).
    Point,
}

#[derive(Clone, Debug)]
pub enum Orbit {
    Fixed,
    Vsop(usize),
    Moon,
    /// Orbita kołowa w płaszczyźnie równika rodzica.
    Circular { radius: f64, period_d: f64, l0_deg: f64 },
    /// Elementy średnie z zmianami na stulecie (Standish, JPL) – Pluton.
    Standish { el: [f64; 6], rate: [f64; 6] },
    Helio(Elements),
    Lagrange(f64),
}

/// Obrót według Orbitera: oś odniesienia precesji (PrecessionLAN/Obliquity), węzeł osi obrotu
/// precesujący z okresem PrecessionPeriod, nachylenie Obliquity, obrót gwiazdowy SidRotPeriod.
#[derive(Clone, Copy, Debug)]
pub struct Rot {
    pub period_s: f64,
    pub offset: f64,
    pub obl: f64,
    pub lan: f64,
    pub prec_period_d: f64,
    pub prec_obl: f64,
    pub prec_lan: f64,
}

#[derive(Clone, Debug)]
pub struct Body {
    pub id: String,
    pub name_pl: String,
    pub name_en: String,
    pub kind: Kind,
    pub parent: Option<BodyId>,
    pub radius: f64,
    pub gm: f64,
    pub orbit: Orbit,
    pub rot: Option<Rot>,
    pub texture: Option<&'static [u8]>,
    pub color: [f32; 3],
    /// Atmosfera (barwa poświaty na krawędzi tarczy).
    pub atmosphere: Option<[f32; 3]>,
    pub clouds: Option<&'static [u8]>,
}

impl Body {
    pub fn name(&self) -> &str {
        if crate::i18n::current() == "pl" { &self.name_pl } else { &self.name_en }
    }
}

fn rot(period_s: f64, offset: f64, obl: f64, lan: f64, prec_period_d: f64, prec_obl: f64, prec_lan: f64) -> Option<Rot> {
    Some(Rot { period_s, offset, obl, lan, prec_period_d, prec_obl, prec_lan })
}

#[derive(Deserialize)]
struct SmallJson {
    id: String,
    name: String,
    epoch_jd: f64,
    a_au: f64,
    e: f64,
    i: f64,
    node: f64,
    peri: f64,
    m: f64,
    diameter_km: Option<f64>,
}

#[allow(clippy::too_many_arguments)]
fn body(
    id: &str,
    pl: &str,
    en: &str,
    kind: Kind,
    parent: Option<BodyId>,
    radius: f64,
    gm: f64,
    orbit: Orbit,
    rot: Option<Rot>,
    texture: Option<&'static [u8]>,
    color: [f32; 3],
) -> Body {
    Body {
        id: id.into(),
        name_pl: pl.into(),
        name_en: en.into(),
        kind,
        parent,
        radius,
        gm,
        orbit,
        rot,
        texture,
        color,
        atmosphere: None,
        clouds: None,
    }
}

fn build() -> Vec<Body> {
    use Kind::*;
    let tex = |b: &'static [u8]| Some(b);
    let mut v = vec![
        body("sun", "Słońce", "Sun", Star, None, 6.96e8, GM_SUN, Orbit::Fixed, rot(2_192_832.0, 0.0, 0.1265, 1.3, 0.0, 0.0, 0.0), None, [1.0, 0.86, 0.55]),
        body("mercury", "Merkury", "Mercury", Planet, Some(SUN), 2.440e6, G * 3.301_846_591_953e23, Orbit::Vsop(0),
            rot(5_067_034.508, -0.830_659_257_8, 0.001_887_623_7, 2.777_423_146, 1_509_264.686, 0.124_591_524, 3.977_837_755_5), tex(include_bytes!("../../assets/textures/mercury.jpg")), [0.62, 0.58, 0.54]),
        body("venus", "Wenus", "Venus", Planet, Some(SUN), 6.051_84e6, G * 4.868_553_170_5e24, Orbit::Vsop(1),
            rot(-20_996_797.016, 3.884_253_452_8, 0.021_624_781_5, 5.239_246_958, 0.0, 0.0, 0.0), tex(include_bytes!("../../assets/textures/venus.jpg")), [0.93, 0.80, 0.55]),
        body("earth", "Ziemia", "Earth", Planet, Some(SUN), 6.371_01e6, 3.986_004_418e14, Orbit::Vsop(2),
            rot(86_164.101_32, 4.889_487_54, 0.409_092_802_3, 0.0, 0.0, 0.0, 0.0), tex(include_bytes!("../../assets/textures/earth.jpg")), [0.3, 0.5, 0.9]),
        body("moon", "Księżyc", "Moon", Moon, Some(EARTH), 1.738_2e6, 4.904_869_5e12, Orbit::Moon,
            rot(2_360_588.140, 4.769_810_896, 0.026_945_107, 1.745_639_497, -6_793.679_659_7, 5.635_681_773e-5, 0.436_856_603), tex(include_bytes!("../../assets/textures/moon.jpg")), [0.7, 0.7, 0.7]),
        body("mars", "Mars", "Mars", Planet, Some(SUN), 3.389_92e6, G * 6.418_553_158_6e23, Orbit::Vsop(3),
            rot(88_642.664_346, 5.468_523_600_8, 0.439_105_815_7, 0.621_248_073_5, -63_265_627.68, 0.033_008_071_9, 4.005_792_765), tex(include_bytes!("../../assets/textures/mars.jpg")), [0.8, 0.45, 0.3]),
        body("phobos", "Fobos", "Phobos", Moon, Some(MARS), 11.27e3, G * 1.062e16, Orbit::Circular { radius: 9.376e6, period_d: 0.318_910_2, l0_deg: 35.0 },
            None, None, [0.5, 0.45, 0.4]),
        body("deimos", "Dejmos", "Deimos", Moon, Some(MARS), 6.5e3, G * 1.441e15, Orbit::Circular { radius: 23.463e6, period_d: 1.263_0, l0_deg: 200.0 },
            None, None, [0.55, 0.5, 0.45]),
        body("jupiter", "Jowisz", "Jupiter", Planet, Some(SUN), 6.991_1e7, G * 1.898_611_062_6e27, Orbit::Vsop(4),
            rot(35_729.711_352, 4.193_162_722_7, -0.016_948_574_6, 1.589_860_219_8, 151_989_599.06, -0.035_084_151_7, -0.840_867_693_8), tex(include_bytes!("../../assets/textures/jupiter.jpg")), [0.85, 0.75, 0.6]),
        body("io", "Io", "Io", Moon, Some(JUPITER), 1.821e6, G * 8.931_937e22, Orbit::Circular { radius: 421.8e6, period_d: 1.769_137_786, l0_deg: 106.08 },
            None, tex(include_bytes!("../../assets/textures/io.jpg")), [0.9, 0.85, 0.5]),
        body("europa", "Europa", "Europa", Moon, Some(JUPITER), 1.565e6, G * 4.799_845e22, Orbit::Circular { radius: 671.1e6, period_d: 3.551_181, l0_deg: 175.73 },
            None, tex(include_bytes!("../../assets/textures/europa.jpg")), [0.85, 0.8, 0.7]),
        body("ganymede", "Ganimedes", "Ganymede", Moon, Some(JUPITER), 2.634e6, G * 1.481_856e23, Orbit::Circular { radius: 1_070.4e6, period_d: 7.154_553, l0_deg: 120.56 },
            None, tex(include_bytes!("../../assets/textures/ganymede.jpg")), [0.6, 0.58, 0.55]),
        body("callisto", "Kallisto", "Callisto", Moon, Some(JUPITER), 2.403e6, G * 1.075_94e23, Orbit::Circular { radius: 1_882.7e6, period_d: 16.689_018, l0_deg: 84.44 },
            None, tex(include_bytes!("../../assets/textures/callisto.jpg")), [0.45, 0.42, 0.38]),
        body("saturn", "Saturn", "Saturn", Planet, Some(SUN), 5.823_2e7, G * 5.684_630_13e26, Orbit::Vsop(5),
            rot(38_362.400_045, 3.039_502_303_9, 0.269_079_507, 0.178_673_558, -613_896_348.49, 0.222_505_496_6, 5.999_769_477), tex(include_bytes!("../../assets/textures/saturn.jpg")), [0.9, 0.82, 0.62]),
        body("titan", "Tytan", "Titan", Moon, Some(SATURN), 2.575e6, G * 1.345_525e23, Orbit::Circular { radius: 1_221.87e6, period_d: 15.945_421, l0_deg: 15.0 },
            None, tex(include_bytes!("../../assets/textures/titan.jpg")), [0.85, 0.65, 0.35]),
        body("uranus", "Uran", "Uranus", Planet, Some(SUN), 2.536_2e7, G * 8.683_211_95e25, Orbit::Vsop(6),
            rot(-62_064.0, 0.503_852_406_6, 1.436_025_519, 2.925_986_534_8, 0.0, 0.0, 0.0), tex(include_bytes!("../../assets/textures/uranus.jpg")), [0.6, 0.85, 0.9]),
        body("neptune", "Neptun", "Neptune", Planet, Some(SUN), 2.462_4e7, G * 1.024_355_2e26, Orbit::Vsop(7),
            rot(57_996.0, 1.061_165_488_5, 0.591_115_007_7, 3.901_190_689_9, 15_951_423.904, 0.141_168_307_2, -0.084_040_863_3), tex(include_bytes!("../../assets/textures/neptune.jpg")), [0.35, 0.5, 0.95]),
        body("pluto", "Pluton", "Pluto", Dwarf, Some(SUN), 1.188e6, 8.696e11, Orbit::Standish {
            el: [39.482_116_75, 0.248_827_30, 17.140_012_06, 238.929_038_33, 224.068_916_29, 110.303_936_84],
            rate: [-0.000_315_96, 0.000_051_70, 0.000_048_18, 145.207_805_15, -0.040_629_42, -0.011_834_82],
        }, rot(-551_856.7, 0.0, 2.1386, 3.86, 0.0, 0.0, 0.0), tex(include_bytes!("../../assets/textures/pluto.jpg")), [0.8, 0.7, 0.6]),
        body("l1", "Punkt L1 Słońce–Ziemia", "Sun–Earth L1", Point, Some(EARTH), 0.0, 0.0, Orbit::Lagrange(-1.5e9), None, None, [0.6, 0.6, 0.7]),
        body("l2", "Punkt L2 Słońce–Ziemia", "Sun–Earth L2", Point, Some(EARTH), 0.0, 0.0, Orbit::Lagrange(1.5e9), None, None, [0.6, 0.6, 0.7]),
    ];
    v[EARTH].atmosphere = Some([0.35, 0.6, 1.0]);
    v[EARTH].clouds = Some(include_bytes!("../../assets/textures/earth_clouds.jpg"));
    v[VENUS].atmosphere = Some([0.95, 0.85, 0.6]);
    v[MARS].atmosphere = Some([0.85, 0.55, 0.4]);
    v[14].atmosphere = Some([0.9, 0.65, 0.3]);
    for b in [JUPITER, SATURN, 15, 16] {
        v[b].atmosphere = Some([v[b].color[0], v[b].color[1], v[b].color[2]]);
    }
    // planetoidy i komety celów misji (elementy oskulacyjne JPL SBDB)
    let small: Vec<SmallJson> = serde_json::from_str(include_str!("../../data/small_bodies.json")).unwrap_or_default();
    for s in small {
        let el = Elements {
            a: s.a_au * AU,
            e: s.e,
            i: s.i.to_radians(),
            node: s.node.to_radians(),
            peri: s.peri.to_radians(),
            m0: s.m.to_radians(),
            epoch: s.epoch_jd,
            mu: GM_SUN,
        };
        let short = s.name.split_once(' ').filter(|(n, _)| n.chars().all(|c| c.is_ascii_digit())).map(|(_, r)| r.split(" (").next().unwrap_or(r).to_string());
        let name = short.unwrap_or_else(|| s.name.split(" (").next().unwrap_or(&s.name).to_string());
        let comet = s.id.chars().next().is_some_and(|c| c.is_ascii_digit()) && s.id.ends_with('p') || s.id.contains("tempel") || s.id.contains("wild");
        let mut b = body(&s.id, &name, &name, Small, Some(SUN), s.diameter_km.unwrap_or(1.0) * 500.0, 0.0, Orbit::Helio(el),
            rot(36_000.0, 0.0, 0.3, 0.0, 0.0, 0.0, 0.0), None, if comet { [0.75, 0.8, 0.85] } else { [0.62, 0.6, 0.56] });
        b.gm = G * 2000.0 * 4.0 / 3.0 * std::f64::consts::PI * b.radius.powi(3);
        v.push(b);
    }
    v
}

pub fn all() -> &'static [Body] {
    static B: OnceLock<Vec<Body>> = OnceLock::new();
    B.get_or_init(build)
}

pub fn get(id: BodyId) -> &'static Body {
    &all()[id]
}

pub fn find(id: &str) -> Option<BodyId> {
    all().iter().position(|b| b.id.eq_ignore_ascii_case(id))
}

const S_OBL: f64 = -1.0;
const S_LAN: f64 = 1.0;

/// Oś odniesienia i węzeł osi obrotu (bez obrotu własnego): z układu równika ciała do ekliptyki J2000.
pub fn equator_frame(id: BodyId, jd: f64) -> DMat3 {
    let Some(r) = get(id).rot else { return DMat3::IDENTITY };
    let mjd = jd - 2_400_000.5;
    let lan = if r.prec_period_d != 0.0 { r.lan + std::f64::consts::TAU * (mjd - 51_544.5) / r.prec_period_d } else { r.lan };
    // Orbiter liczy w układzie lewoskrętnym (y – północ ekliptyki); w prawoskrętnej ekliptyce
    // kąty nachylenia zmieniają znak
    DMat3::from_rotation_z(S_LAN * r.prec_lan)
        * DMat3::from_rotation_x(S_OBL * r.prec_obl)
        * DMat3::from_rotation_z(S_LAN * lan)
        * DMat3::from_rotation_x(S_OBL * r.obl)
}

/// Kąt obrotu własnego (od węzła) w chwili `jd`.
pub fn spin(id: BodyId, jd: f64) -> f64 {
    // Ziemia: średni czas gwiazdowy (zgodnie z układem TEME, w którym liczy SGP4)
    if id == EARTH {
        return super::time::gmst(jd);
    }
    let Some(r) = get(id).rot else { return 0.0 };
    let mjd = jd - 2_400_000.5;
    let mut a = r.offset + std::f64::consts::TAU * (mjd - 51_544.5) * 86_400.0 / r.period_s;
    // precesja węzła przesuwa południk zerowy (jak w Orbiterze: obrót mierzony od kierunku stałego)
    if r.prec_period_d != 0.0 {
        a -= std::f64::consts::TAU * (mjd - 51_544.5) / r.prec_period_d;
    }
    a
}

/// Orientacja ciała: z układu związanego z ciałem (x – południk 0, z – biegun północny) do ekliptyki.
pub fn orientation(id: BodyId, jd: f64) -> DMat3 {
    equator_frame(id, jd) * DMat3::from_rotation_z(spin(id, jd))
}

/// Położenie środka ciała względem Słońca [m], ekliptyka J2000.
pub fn position(id: BodyId, jd: f64) -> DVec3 {
    let b = get(id);
    match &b.orbit {
        Orbit::Fixed => DVec3::ZERO,
        Orbit::Vsop(i) => super::vsop::position(*i, jd),
        Orbit::Moon => position(EARTH, jd) + super::moon::geocentric(jd),
        Orbit::Circular { radius, period_d, l0_deg } => {
            let p = b.parent.unwrap_or(SUN);
            let l = (l0_deg + 360.0 * (jd - J2000) / period_d).to_radians();
            position(p, jd) + equator_frame(p, jd) * DVec3::new(radius * l.cos(), radius * l.sin(), 0.0)
        }
        Orbit::Standish { el, rate } => {
            let t = (jd - J2000) / 36_525.0;
            let e: Vec<f64> = (0..6).map(|k| el[k] + rate[k] * t).collect();
            let (a, ecc, inc, l, varpi, node) = (e[0] * AU, e[1], e[2].to_radians(), e[3].to_radians(), e[4].to_radians(), e[5].to_radians());
            let elements = Elements { a, e: ecc, i: inc, node, peri: varpi - node, m0: l - varpi, epoch: jd, mu: GM_SUN };
            elements.state(jd).0
        }
        Orbit::Helio(el) => el.state(jd).0,
        Orbit::Lagrange(d) => {
            let e = position(EARTH, jd);
            e + e.normalize() * *d
        }
    }
}

/// Prędkość [m/s] (różnica centralna).
pub fn velocity(id: BodyId, jd: f64) -> DVec3 {
    let h = 30.0 / 86_400.0;
    (position(id, jd + h) - position(id, jd - h)) / 60.0
}

/// Okres orbitalny wokół ciała centralnego [dni] – do rysowania orbity.
pub fn orbit_period_days(id: BodyId) -> f64 {
    let b = get(id);
    match &b.orbit {
        Orbit::Fixed | Orbit::Lagrange(_) => 365.25,
        Orbit::Moon => 27.321_661,
        Orbit::Circular { period_d, .. } => *period_d,
        Orbit::Standish { .. } => 90_560.0,
        Orbit::Helio(el) => el.period_s() / 86_400.0,
        Orbit::Vsop(i) => [87.969, 224.701, 365.256, 686.98, 4_332.59, 10_759.22, 30_688.5, 60_182.0][*i],
    }
}

/// Promień strefy wpływu grawitacyjnego (Laplace) [m]; dla Słońca – nieskończoność.
pub fn soi(id: BodyId, jd: f64) -> f64 {
    let b = get(id);
    let Some(p) = b.parent else { return f64::INFINITY };
    let pm = get(p).gm;
    if b.gm <= 0.0 || pm <= 0.0 {
        return 0.0;
    }
    let d = (position(id, jd) - position(p, jd)).length();
    d * (b.gm / pm).powf(0.4)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Biegun Marsa wg IAU: α = 317.68°, δ = 52.89° (J2000).
    #[test]
    fn mars_pole() {
        let jd = J2000;
        let pole = orientation(MARS, jd) * DVec3::Z;
        let eps = 0.409_092_802_3f64;
        let (a, d) = (317.68f64.to_radians(), 52.89f64.to_radians());
        let eq = DVec3::new(d.cos() * a.cos(), d.cos() * a.sin(), d.sin());
        let ecl = DMat3::from_rotation_x(-eps) * eq;
        let ang = pole.dot(ecl).clamp(-1.0, 1.0).acos().to_degrees();
        assert!(ang < 1.5, "kąt {ang}");
    }

    /// Księżyc zwrócony do Ziemi jedną stroną: długość selenograficzna Ziemi ≈ 0 (± libracja).
    #[test]
    fn moon_faces_earth() {
        for k in 0..40 {
            let jd = J2000 - 3_000.0 + k as f64 * 173.3;
            let rel = position(EARTH, jd) - position(MOON, jd);
            let local = orientation(MOON, jd).transpose() * rel.normalize();
            let lon = local.y.atan2(local.x).to_degrees();
            let lat = local.z.asin().to_degrees();
            assert!(lon.abs() < 12.0 && lat.abs() < 10.0, "jd {jd}: lon {lon} lat {lat}");
        }
    }

    /// Obrót Ziemi zgodny z GMST.
    #[test]
    fn earth_rotation() {
        let jd = 2_460_000.3;
        let x = orientation(EARTH, jd) * DVec3::X;
        let eq = DMat3::from_rotation_x(0.409_092_802_3) * x;
        let ang = eq.y.atan2(eq.x).rem_euclid(std::f64::consts::TAU);
        let diff = (ang - super::super::time::gmst(jd)).abs().to_degrees();
        assert!(diff < 1e-6 || (diff - 360.0).abs() < 1e-6, "{diff}");
    }

    #[test]
    fn small_bodies_loaded() {
        let p = find("67p").expect("67P");
        let r = position(p, 2_457_247.589).length() / AU; // peryhelium 2015-08-13: 1.24 AU
        assert!((r - 1.243).abs() < 0.01, "{r}");
    }
}
