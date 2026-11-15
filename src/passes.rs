//! Przeloty satelitów nad miejscem obserwatora: kiedy satelita wznosi się nad horyzont,
//! jak wysoko i czy da się go zobaczyć gołym okiem (satelita w słońcu, obserwator po zmroku).

use glam::{DMat3, DVec3};

use crate::astro::bodies::{self, EARTH, SUN};
use crate::sats::{Gp, SatObj};

/// Najmniejsza wysokość nad horyzontem, od której liczymy przelot [°].
pub const MIN_EL: f64 = 10.0;

#[derive(Clone, Debug)]
pub struct Pass {
    pub start: f64,
    pub end: f64,
    pub max_jd: f64,
    pub max_el: f64,
    /// Azymut wschodu i zachodu nad horyzontem [°] (0 – północ, 90 – wschód).
    pub az_start: f64,
    pub az_end: f64,
    /// Widoczny gołym okiem (w słońcu, gdy u obserwatora jest ciemno).
    pub visible: bool,
}

/// Układ lokalny obserwatora (inercjalny, względem środka Ziemi): położenie [m], zenit, wschód, północ.
pub fn observer(lat: f64, lon: f64, jd: f64) -> (DVec3, DVec3, DVec3, DVec3) {
    let rot: DMat3 = bodies::orientation(EARTH, jd);
    let (la, lo) = (lat.to_radians(), lon.to_radians());
    let up = rot * DVec3::new(la.cos() * lo.cos(), la.cos() * lo.sin(), la.sin());
    let pole = rot * DVec3::Z;
    let east = pole.cross(up).normalize_or(DVec3::X);
    let north = up.cross(east);
    (up * bodies::get(EARTH).radius, up, east, north)
}

/// Wysokość nad horyzontem i azymut [°] punktu `p` (względem środka Ziemi).
pub fn el_az(p: DVec3, obs: &(DVec3, DVec3, DVec3, DVec3)) -> (f64, f64) {
    let d = (p - obs.0).normalize_or(DVec3::Z);
    let el = d.dot(obs.1).clamp(-1.0, 1.0).asin().to_degrees();
    let az = d.dot(obs.2).atan2(d.dot(obs.3)).to_degrees().rem_euclid(360.0);
    (el, az)
}

/// Czy punkt na orbicie jest oświetlony przez Słońce (poza cieniem Ziemi – walec).
fn sunlit(r: DVec3, sun_dir: DVec3) -> bool {
    let along = r.dot(sun_dir);
    along > 0.0 || (r - sun_dir * along).length() > bodies::get(EARTH).radius
}


/// Przeloty w ciągu `days` dni od `jd0` (najwyżej `max` przelotów).
pub fn predict(o: &SatObj, gp: &[Gp], lat: f64, lon: f64, jd0: f64, days: f64, max: usize) -> Vec<Pass> {
    let step = 20.0 / 86_400.0;
    let n = (days / step) as usize;
    let mut out: Vec<Pass> = Vec::new();
    let mut cur: Option<Pass> = None;
    let mut prev: Option<(f64, f64, f64)> = None; // (jd, el, az)
    for k in 0..=n {
        let jd = jd0 + k as f64 * step;
        if !o.alive_at(jd) {
            if o.decay_jd.is_some_and(|d| jd >= d) {
                break;
            }
            continue;
        }
        let Some((r, _)) = o.position(jd, gp) else { break };
        let obs = observer(lat, lon, jd);
        let (el, az) = el_az(r, &obs);
        if el >= MIN_EL {
            let sun_dir = (bodies::position(SUN, jd) - bodies::position(EARTH, jd)).normalize();
            let vis = sunlit(r, sun_dir) && el_az(sun_dir * 1.5e11, &obs).0 < -6.0;
            let c = cur.get_or_insert_with(|| {
                // początek: interpolacja przejścia przez próg
                let start = match prev {
                    Some((pj, pe, _)) if pe < MIN_EL => pj + (jd - pj) * ((MIN_EL - pe) / (el - pe)).clamp(0.0, 1.0),
                    _ => jd,
                };
                Pass { start, end: jd, max_jd: jd, max_el: el, az_start: az, az_end: az, visible: false }
            });
            c.end = jd;
            c.az_end = az;
            c.visible |= vis;
            if el > c.max_el {
                c.max_el = el;
                c.max_jd = jd;
            }
        } else if let Some(mut c) = cur.take() {
            if let Some((pj, pe, _)) = prev {
                c.end = pj + (jd - pj) * ((pe - MIN_EL) / (pe - el)).clamp(0.0, 1.0);
            }
            out.push(c);
            if out.len() >= max {
                return out;
            }
        }
        prev = Some((jd, el, az));
    }
    if let Some(c) = cur {
        out.push(c);
    }
    out.truncate(max);
    out
}

/// Kierunek świata z azymutu (8 stron, skróty w języku programu).
pub fn compass(az: f64) -> &'static str {
    let k = ((az / 45.0).round() as usize) % 8;
    if crate::i18n::current() == "pl" {
        ["pn.", "pn.-wsch.", "wsch.", "pd.-wsch.", "pd.", "pd.-zach.", "zach.", "pn.-zach."][k]
    } else {
        ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][k]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zenith_is_up() {
        let jd = crate::astro::time::J2000;
        let obs = observer(52.23, 21.01, jd);
        let (el, _) = el_az(obs.0 * 2.0, &obs);
        assert!((el - 90.0).abs() < 1e-6, "{el}");
        // punkt na północ od obserwatora (styczna) – azymut 0
        let (_, az) = el_az(obs.0 + obs.3 * 1.0e5 + obs.1 * 10.0, &obs);
        assert!(az < 1.0 || az > 359.0, "{az}");
    }
}
