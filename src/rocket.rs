//! Rakieta przy starcie: prosty model złożony z członów opisanych w scenariuszu misji
//! (`parts` + wydarzenia `sep`). Człony, osłony i boczne bloki odpadają w chwili odłączenia
//! i przez chwilę widać, jak zostają w tyle i koziołkują.

use std::sync::OnceLock;

use glam::Vec3;

use crate::missions::{self, Mission};
use crate::models::{self, Mesh};

/// Do kiedy po starcie szukamy odłączeń członów rakiety [d].
const ASCENT_D: f64 = 8.0 / 24.0;
/// Jak długo widać odpadający człon [s czasu symulacji].
pub const FALL_S: f64 = 150.0;

/// Części, które nie są członami rakiety, choć odłączają się wkrótce po starcie.
const NOT_LAUNCHER: &[&str] = &["po", "sa", "airlock", "retro", "marco", "xrism"];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Role {
    /// Człon w osi rakiety (od dołu).
    Stage,
    /// Boczne bloki przyspieszające.
    Side,
    /// Osłona aerodynamiczna albo wieża ratunkowa na szczycie.
    Nose,
}

/// Jeden element rakiety do narysowania osobno (człon, blok boczny, osłona).
struct Piece {
    part: usize,
    sep_jd: f64,
    /// Wierzchołki względem środka elementu (oś rakiety wzdłuż −Y, ładunek w zerze).
    mesh_center: Vec3,
    mesh: Vec<([f32; 3], [f32; 3])>,
    idx: Vec<u32>,
    /// Kierunek rozchodzenia się (bloki boczne odsuwają się na boki).
    out: Vec3,
}

pub struct Plan {
    pub mission: usize,
    pub length_m: f32,
    /// Ostatnie odłączenie członu – potem widać już sam statek.
    pub end_jd: f64,
    pieces: Vec<Piece>,
    /// Ładunek (statek) na szczycie – zawsze w zestawie.
    payload: (Vec<([f32; 3], [f32; 3])>, Vec<u32>),
}

/// Element rakiety do narysowania w danej chwili.
pub struct Draw {
    pub key: String,
    /// Położenie środka elementu względem statku (w układzie modelu).
    pub offset: Vec3,
    /// Odsunięcie do tyłu wzdłuż osi lotu [m] i obrót koziołkowania [rad].
    pub back_m: f32,
    pub tumble: f32,
    /// Odsunięcie na boki w układzie modelu [m].
    pub drift: Vec3,
}

fn role(id: &str) -> Role {
    match id {
        "boosters" | "srb" | "gem" | "eap" | "side" | "s200" => Role::Side,
        "fairing" | "les" | "las" | "tower" | "shroud" | "shield" | "sla" => Role::Nose,
        _ => Role::Stage,
    }
}

#[derive(Default)]
struct Geo {
    v: Vec<([f32; 3], [f32; 3])>,
    i: Vec<u32>,
}

impl Geo {
    /// Ścięty stożek wokół osi (cx, cy) od wysokości h0 (promień r0) do h1 (promień r1).
    fn frustum(&mut self, c: (f32, f32), r0: f32, h0: f32, r1: f32, h1: f32, col: [f32; 3]) {
        const N: u32 = 28;
        let base = self.v.len() as u32;
        for k in 0..N {
            let a = k as f32 / N as f32 * std::f32::consts::TAU;
            let (s, co) = a.sin_cos();
            self.v.push(([c.0 + r0 * co, c.1 + r0 * s, h0], col));
            self.v.push(([c.0 + r1 * co, c.1 + r1 * s, h1], col));
        }
        for k in 0..N {
            let (a, b) = (base + 2 * k, base + 2 * ((k + 1) % N));
            self.i.extend([a, b, a + 1, b, b + 1, a + 1]);
        }
        // denka
        let c0 = self.v.len() as u32;
        self.v.push(([c.0, c.1, h0], col));
        self.v.push(([c.0, c.1, h1], col));
        for k in 0..N {
            let (a, b) = (base + 2 * k, base + 2 * ((k + 1) % N));
            self.i.extend([c0, b, a, c0 + 1, a + 1, b + 1]);
        }
    }

    /// Przeniesienie do układu modelu: oś h → −Y (nos w przód lotu), środek w `center`.
    fn into_model(self, h_payload: f32, center: Vec3) -> (Vec<([f32; 3], [f32; 3])>, Vec<u32>) {
        let v = self.v.into_iter().map(|(p, c)| ([p[0] - center.x, -(p[2] - h_payload) - center.y, p[1] - center.z], c)).collect();
        (v, self.i)
    }
}

fn to_model(h_payload: f32, x: f32, y: f32, h: f32) -> Vec3 {
    Vec3::new(x, -(h - h_payload), y)
}

fn build(mi: usize, m: &Mission) -> Option<Plan> {
    let v = m.vehicle()?;
    // części odłączane w czasie wznoszenia, w kolejności odłączania
    let mut seq: Vec<(usize, f64)> = Vec::new();
    for e in &m.events {
        if e.jd > m.launch_jd + ASCENT_D {
            break;
        }
        for s in &e.sep {
            if NOT_LAUNCHER.contains(&s.as_str()) {
                continue;
            }
            if let Some(p) = m.parts.iter().position(|p| &p.id == s && !p.later) {
                if !seq.iter().any(|x| x.0 == p) {
                    seq.push((p, e.jd));
                }
            }
        }
    }
    if seq.is_empty() {
        return None;
    }
    let h_total = v.height_m as f32;
    let soviet = matches!(m.country.as_str(), "SU" | "RU");
    let steel = v.id == "starship";
    let white = if soviet { [0.50, 0.55, 0.48] } else if steel { [0.70, 0.71, 0.73] } else { [0.86, 0.86, 0.84] };
    let dark = [0.07, 0.07, 0.08];
    let stages: Vec<(usize, f64)> = seq.iter().copied().filter(|(p, _)| role(&m.parts[*p].id) == Role::Stage).collect();
    let sides: Vec<(usize, f64)> = seq.iter().copied().filter(|(p, _)| role(&m.parts[*p].id) == Role::Side).collect();
    let noses: Vec<(usize, f64)> = seq.iter().copied().filter(|(p, _)| role(&m.parts[*p].id) == Role::Nose).collect();
    let nose_f = if noses.iter().any(|(p, _)| matches!(m.parts[*p].id.as_str(), "fairing" | "shroud" | "shield" | "sla")) { 0.12 } else { 0.0 };
    // Starship: statek to prawie połowa wysokości zestawu
    let payload_f = if steel { 0.42 } else { 0.09 };
    let mut r = (v.diameter_m as f32 / 2.0) / if sides.is_empty() { 1.0 } else { 3.0 };
    r = r.min(h_total / 14.0).max(0.4);
    let weights: Vec<f32> = (0..stages.len()).map(|k| 0.6f32.powi(k as i32)).collect();
    let wsum: f32 = weights.iter().sum::<f32>().max(1e-3);
    let avail = h_total * (1.0 - nose_f - payload_f);
    let mut pieces: Vec<Piece> = Vec::new();
    let mut h = 0.0f32;
    let mut stage_h0 = 0.0f32;
    let mut top_r = r;
    // ładunek: wysokość środka ładunku znana po ułożeniu członów
    let mut spans: Vec<(usize, f64, f32, f32, f32)> = Vec::new();
    for (k, &(p, t)) in stages.iter().enumerate() {
        let len = avail * weights[k] / wsum;
        let rk = if k >= 2 { r * 0.72 } else { r };
        if k == 0 {
            stage_h0 = len;
        }
        spans.push((p, t, h, h + len, rk));
        h += len;
        top_r = rk;
    }
    let h_payload0 = h;
    let h_payload = h + h_total * payload_f * 0.5;
    for &(p, t, h0, h1, rk) in &spans {
        let id = m.parts[p].id.as_str();
        let col = match id {
            "et" => [0.72, 0.36, 0.14],
            "core" if v.id == "sls" => [0.80, 0.45, 0.18],
            _ => white,
        };
        let mut g = Geo::default();
        g.frustum((0.0, 0.0), rk, h0, rk, h1, col);
        // ciemny pierścień między członami i dysze na dole
        g.frustum((0.0, 0.0), rk * 1.005, h0, rk * 1.005, h0 + (h1 - h0) * 0.06, dark);
        if h0 == 0.0 {
            g.frustum((0.0, 0.0), rk * 0.55, -h_total * 0.035, rk * 0.85, 0.0, [0.18, 0.18, 0.2]);
        }
        let center = to_model(h_payload, 0.0, 0.0, (h0 + h1) / 2.0);
        let (mesh, idx) = g.into_model(h_payload, center);
        pieces.push(Piece { part: p, sep_jd: t, mesh_center: center, mesh, idx, out: Vec3::ZERO });
    }
    // bloki boczne: każdy osobno (rozchodzą się na boki)
    for &(p, t) in &sides {
        let id = m.parts[p].id.as_str();
        let (n, lf, rf) = match id {
            "boosters" => (4, 0.85, 0.75),
            "gem" => (6, 0.45, 0.3),
            "side" => (2, 1.0, 1.0),
            _ => (2, 0.9, 0.45),
        };
        let len = stage_h0.max(h_total * 0.3) * lf;
        let rb = r * rf;
        for b in 0..n {
            let a = b as f32 / n as f32 * std::f32::consts::TAU + 0.3;
            let (s, c) = a.sin_cos();
            let (cx, cy) = ((r + rb * 1.02) * c, (r + rb * 1.02) * s);
            let mut g = Geo::default();
            g.frustum((cx, cy), rb, 0.0, rb, len * 0.9, white);
            g.frustum((cx, cy), rb, len * 0.9, rb * 0.15, len, white);
            g.frustum((cx, cy), rb * 0.5, -h_total * 0.025, rb * 0.8, 0.0, [0.18, 0.18, 0.2]);
            let center = to_model(h_payload, cx, cy, len / 2.0);
            let (mesh, idx) = g.into_model(h_payload, center);
            pieces.push(Piece { part: p, sep_jd: t, mesh_center: center, mesh, idx, out: Vec3::new(c, 0.0, s) });
        }
    }
    // ładunek
    let crewed = m.crewed();
    let pr = top_r * 0.8;
    let ph = h_total * payload_f;
    let mut g = Geo::default();
    let pcol = if steel { white } else if crewed { [0.80, 0.80, 0.82] } else { [0.85, 0.68, 0.30] };
    if crewed {
        g.frustum((0.0, 0.0), pr, h_payload0, pr * 0.95, h_payload0 + ph * 0.45, [0.72, 0.72, 0.74]);
        g.frustum((0.0, 0.0), pr * 0.95, h_payload0 + ph * 0.45, pr * 0.25, h_payload0 + ph, pcol);
    } else {
        g.frustum((0.0, 0.0), pr, h_payload0, pr, h_payload0 + ph * 0.8, pcol);
        g.frustum((0.0, 0.0), pr, h_payload0 + ph * 0.8, pr * 0.3, h_payload0 + ph, pcol);
    }
    let payload = g.into_model(h_payload, Vec3::ZERO);
    // osłony i wieże na szczycie
    for &(p, t) in &noses {
        let id = m.parts[p].id.as_str();
        let mut g = Geo::default();
        let (h0, h1);
        if matches!(id, "les" | "las" | "tower") {
            h0 = h_payload0 + ph;
            h1 = h0 + h_total * 0.08;
            g.frustum((0.0, 0.0), pr * 0.3, h0, pr * 0.12, h0 + (h1 - h0) * 0.7, [0.85, 0.85, 0.86]);
            g.frustum((0.0, 0.0), pr * 0.12, h0 + (h1 - h0) * 0.7, 0.05, h1, [0.75, 0.2, 0.15]);
        } else {
            h0 = h_payload0;
            h1 = h_payload0 + ph + h_total * nose_f;
            let fr = top_r * 1.03;
            let mid = h_payload0 + ph * 1.1;
            g.frustum((0.0, 0.0), fr, h0, fr, mid, white);
            g.frustum((0.0, 0.0), fr, mid, fr * 0.12, h1, white);
        }
        let center = to_model(h_payload, 0.0, 0.0, (h0 + h1) / 2.0);
        let (mesh, idx) = g.into_model(h_payload, center);
        pieces.push(Piece { part: p, sep_jd: t, mesh_center: center, mesh, idx, out: Vec3::new(0.0, 0.0, 1.0) });
    }
    let end_jd = seq.iter().map(|x| x.1).fold(m.launch_jd, f64::max);
    Some(Plan { mission: mi, length_m: h_total, end_jd, pieces, payload })
}

fn plans() -> &'static Vec<Option<Plan>> {
    static P: OnceLock<Vec<Option<Plan>>> = OnceLock::new();
    P.get_or_init(|| missions::db().missions.iter().enumerate().map(|(i, m)| build(i, m)).collect())
}

/// Plan rakiety misji (jeśli scenariusz opisuje człony).
pub fn plan(mi: usize) -> Option<&'static Plan> {
    plans().get(mi)?.as_ref()
}

/// Czy w chwili `jd` rakieta jeszcze leci (albo jej człony jeszcze widać).
pub fn active(mi: usize, jd: f64) -> bool {
    plan(mi).is_some_and(|p| jd >= p.mission_launch() && jd <= p.end_jd + FALL_S / 86_400.0)
}

impl Plan {
    fn mission_launch(&self) -> f64 {
        missions::db().missions[self.mission].launch_jd
    }

    fn mesh_of(verts: &[([f32; 3], [f32; 3])], idx: &[u32]) -> Mesh {
        Mesh { verts: verts.iter().map(|(p, c)| [p[0], p[1], p[2], c[0], c[1], c[2]]).collect(), idx: idx.to_vec() }
    }

    /// Elementy do narysowania w chwili `jd`: zestaw (to, co jeszcze połączone) i odpadające człony.
    pub fn draws(&self, jd: f64) -> Vec<Draw> {
        let id = &missions::db().missions[self.mission].id;
        let mut out = Vec::new();
        if jd < self.mission_launch() {
            return out;
        }
        // zestaw: ładunek + elementy jeszcze nieodłączone (klucz zależy od tego, co odpadło)
        let mut mask: u128 = 0;
        for (k, p) in self.pieces.iter().enumerate() {
            if jd >= p.sep_jd && k < 128 {
                mask |= 1 << k;
            }
        }
        if jd <= self.end_jd {
            let key = format!("rocket:{id}:{mask:x}");
            if !models::ready(&key) {
                let mut verts: Vec<([f32; 3], [f32; 3])> = self.payload.0.clone();
                let mut idx: Vec<u32> = self.payload.1.clone();
                for (k, p) in self.pieces.iter().enumerate() {
                    if mask & (1 << k) != 0 {
                        continue;
                    }
                    let base = verts.len() as u32;
                    let c = p.mesh_center;
                    verts.extend(p.mesh.iter().map(|(v, col)| ([v[0] + c.x, v[1] + c.y, v[2] + c.z], *col)));
                    idx.extend(p.idx.iter().map(|i| i + base));
                }
                models::put(&key, Self::mesh_of(&verts, &idx));
            }
            out.push(Draw { key, offset: Vec3::ZERO, back_m: 0.0, tumble: 0.0, drift: Vec3::ZERO });
        }
        // odpadające elementy
        for (k, p) in self.pieces.iter().enumerate() {
            let dt = (jd - p.sep_jd) * 86_400.0;
            if !(0.0..=FALL_S).contains(&dt) {
                continue;
            }
            let key = format!("rocket:{id}:p{k}");
            if !models::ready(&key) {
                models::put(&key, Self::mesh_of(&p.mesh, &p.idx));
            }
            let dt = dt as f32;
            out.push(Draw {
                key,
                offset: p.mesh_center,
                back_m: 3.0 * dt + 0.5 * 16.0 * dt * dt,
                tumble: 0.025 * dt * if k % 2 == 0 { 1.0 } else { -1.0 },
                drift: p.out * (2.5 * dt),
            });
        }
        out
    }

    /// Nazwa części, która właśnie odpada (do etykiety).
    pub fn falling_parts(&self, jd: f64) -> Vec<usize> {
        let mut v: Vec<usize> = self.pieces.iter().filter(|p| (0.0..=FALL_S).contains(&((jd - p.sep_jd) * 86_400.0))).map(|p| p.part).collect();
        v.dedup();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apollo_11_has_a_saturn_v_stack() {
        let i = missions::index_of("apollo-11").expect("apollo-11");
        let p = plan(i).expect("plan");
        assert!(p.length_m > 100.0);
        let m = &missions::db().missions[i];
        // tuż po starcie: cały zestaw; po odrzuceniu S-IC widać spadający człon
        assert_eq!(p.draws(m.launch_jd + 10.0 / 86_400.0).len(), 1);
        let s_ic = m.events.iter().find(|e| e.sep.iter().any(|s| s == "s-ic")).expect("S-IC").jd;
        let d = p.draws(s_ic + 20.0 / 86_400.0);
        assert!(d.len() >= 2);
        assert!(d.iter().any(|x| x.back_m > 0.0));
    }

    #[test]
    fn plans_build_for_all_missions() {
        let n = plans().iter().filter(|p| p.is_some()).count();
        assert!(n > 50, "{n}");
        for p in plans().iter().flatten() {
            assert!(p.pieces.iter().all(|x| !x.idx.is_empty() && x.idx.iter().all(|&i| (i as usize) < x.mesh.len())));
        }
    }
}
