//! Mechanika dwóch ciał: elementy orbitalne, propagacja w zmiennych uniwersalnych (Vallado,
//! alg. 8) i zagadnienie Lamberta (Vallado, alg. 58 – zmienne uniwersalne z bisekcją).

use glam::DVec3;

/// Elementy keplerowskie względem ciała centralnego (płaszczyzna odniesienia – ekliptyka J2000
/// albo dowolna, zależnie od użycia). Kąty w radianach, `a` w metrach.
#[derive(Clone, Copy, Debug)]
pub struct Elements {
    pub a: f64,
    pub e: f64,
    pub i: f64,
    pub node: f64,
    pub peri: f64,
    /// Anomalia średnia w epoce.
    pub m0: f64,
    pub epoch: f64,
    pub mu: f64,
}

impl Elements {
    pub fn period_s(&self) -> f64 {
        std::f64::consts::TAU * (self.a.powi(3) / self.mu).sqrt()
    }

    /// Położenie i prędkość w chwili `jd` (orbita eliptyczna).
    pub fn state(&self, jd: f64) -> (DVec3, DVec3) {
        let n = (self.mu / self.a.powi(3)).sqrt();
        let m = self.m0 + n * (jd - self.epoch) * 86_400.0;
        let ea = solve_kepler(m, self.e);
        let (s, c) = ea.sin_cos();
        let b = self.a * (1.0 - self.e * self.e).sqrt();
        let x = self.a * (c - self.e);
        let y = b * s;
        let r = self.a * (1.0 - self.e * c);
        let vx = -self.a * n * s * self.a / r;
        let vy = b * n * c * self.a / r;
        let rot = self.rotation();
        (rot * DVec3::new(x, y, 0.0), rot * DVec3::new(vx, vy, 0.0))
    }

    /// Macierz z płaszczyzny orbity (x – perycentrum) do układu odniesienia.
    pub fn rotation(&self) -> glam::DMat3 {
        glam::DMat3::from_rotation_z(self.node) * glam::DMat3::from_rotation_x(self.i) * glam::DMat3::from_rotation_z(self.peri)
    }

    /// Elementy z wektora stanu (orbita eliptyczna albo hiperboliczna – dla hiperboli `a` < 0).
    pub fn from_state(r: DVec3, v: DVec3, mu: f64, epoch: f64) -> Self {
        let h = r.cross(v);
        let n = DVec3::Z.cross(h);
        let rl = r.length();
        let ev = ((v.length_squared() - mu / rl) * r - r.dot(v) * v) / mu;
        let e = ev.length();
        let energy = v.length_squared() / 2.0 - mu / rl;
        let a = -mu / (2.0 * energy);
        let i = (h.z / h.length()).clamp(-1.0, 1.0).acos();
        let node = if n.length() > 1e-9 { n.y.atan2(n.x) } else { 0.0 };
        let peri = if n.length() > 1e-9 && e > 1e-9 {
            let mut w = (n.dot(ev) / (n.length() * e)).clamp(-1.0, 1.0).acos();
            if ev.z < 0.0 {
                w = std::f64::consts::TAU - w;
            }
            w
        } else if e > 1e-9 {
            ev.y.atan2(ev.x)
        } else {
            0.0
        };
        let nu = if e > 1e-9 {
            let mut nu = (ev.dot(r) / (e * rl)).clamp(-1.0, 1.0).acos();
            if r.dot(v) < 0.0 {
                nu = std::f64::consts::TAU - nu;
            }
            nu
        } else {
            0.0
        };
        let m0 = if e < 1.0 {
            let ea = 2.0 * ((1.0 - e).sqrt() * (nu / 2.0).sin()).atan2((1.0 + e).sqrt() * (nu / 2.0).cos());
            ea - e * ea.sin()
        } else {
            0.0
        };
        Self { a, e, i, node, peri, m0, epoch, mu }
    }
}

/// Równanie Keplera M = E − e sin E (Newton).
pub fn solve_kepler(m: f64, e: f64) -> f64 {
    let m = m.rem_euclid(std::f64::consts::TAU);
    let mut ea = if e < 0.8 { m } else { std::f64::consts::PI };
    for _ in 0..30 {
        let f = ea - e * ea.sin() - m;
        let d = f / (1.0 - e * ea.cos());
        ea -= d;
        if d.abs() < 1e-12 {
            break;
        }
    }
    ea
}

/// Funkcje Stumpffa c2, c3.
fn stumpff(psi: f64) -> (f64, f64) {
    if psi > 1e-6 {
        let s = psi.sqrt();
        ((1.0 - s.cos()) / psi, (s - s.sin()) / (s * psi))
    } else if psi < -1e-6 {
        let s = (-psi).sqrt();
        ((1.0 - s.cosh()) / psi, (s.sinh() - s) / (-psi * s))
    } else {
        (0.5 - psi / 24.0, 1.0 / 6.0 - psi / 120.0)
    }
}

/// Propagacja stanu o `dt` sekund (dowolna orbita stożkowa).
pub fn propagate(r0: DVec3, v0: DVec3, dt: f64, mu: f64) -> (DVec3, DVec3) {
    if dt.abs() < 1e-9 {
        return (r0, v0);
    }
    let r0l = r0.length();
    let v0l2 = v0.length_squared();
    let sqmu = mu.sqrt();
    let alpha = -v0l2 / mu + 2.0 / r0l;
    let rdv = r0.dot(v0);
    let mut chi = if alpha > 1e-12 {
        sqmu * dt * alpha
    } else if alpha < -1e-12 {
        let a = 1.0 / alpha;
        dt.signum() * (-a).sqrt() * ((-2.0 * mu * alpha * dt) / (rdv + dt.signum() * (-mu * a).sqrt() * (1.0 - r0l * alpha))).abs().ln()
    } else {
        let h = r0.cross(v0);
        let p = h.length_squared() / mu;
        let s = 0.5 * (1.0 / (3.0 * (mu / p.powi(3)).sqrt() * dt)).atan();
        let w = (s.tan().cbrt()).atan();
        p.sqrt() * 2.0 / (2.0 * w).tan()
    };
    if !chi.is_finite() {
        chi = sqmu * dt / r0l;
    }
    let (mut c2, mut c3);
    let mut r = r0l;
    let mut psi;
    for _ in 0..60 {
        psi = chi * chi * alpha;
        (c2, c3) = stumpff(psi);
        r = chi * chi * c2 + rdv / sqmu * chi * (1.0 - psi * c3) + r0l * (1.0 - psi * c2);
        let next = chi + (sqmu * dt - chi.powi(3) * c3 - rdv / sqmu * chi * chi * c2 - r0l * chi * (1.0 - psi * c3)) / r;
        let done = (next - chi).abs() < 1e-7;
        chi = next;
        if done {
            break;
        }
    }
    psi = chi * chi * alpha;
    (c2, c3) = stumpff(psi);
    let f = 1.0 - chi * chi / r0l * c2;
    let g = dt - chi.powi(3) / sqmu * c3;
    let gdot = 1.0 - chi * chi / r * c2;
    let fdot = sqmu / (r * r0l) * chi * (psi * c3 - 1.0);
    (f * r0 + g * v0, fdot * r0 + gdot * v0)
}

/// Zagadnienie Lamberta: prędkości na początku i końcu łuku z `r1` do `r2` w czasie `dt` [s]
/// (bez pełnych obiegów). `prograde` – ruch zgodny z osią +Z (jak planety).
pub fn lambert(r1: DVec3, r2: DVec3, dt: f64, mu: f64, prograde: bool) -> Option<(DVec3, DVec3)> {
    let r1l = r1.length();
    let r2l = r2.length();
    let cos_dnu = (r1.dot(r2) / (r1l * r2l)).clamp(-1.0, 1.0);
    let cross_z = r1.cross(r2).z;
    let long_way = if prograde { cross_z < 0.0 } else { cross_z >= 0.0 };
    let tm = if long_way { -1.0 } else { 1.0 };
    let a = tm * (r1l * r2l * (1.0 + cos_dnu)).sqrt();
    if a.abs() < 1e-9 {
        return None;
    }
    let sqmu = mu.sqrt();
    let y_of = |psi: f64| {
        let (c2, c3) = stumpff(psi);
        (r1l + r2l + a * (psi * c3 - 1.0) / c2.sqrt(), c2, c3)
    };
    let t_of = |psi: f64| -> Option<f64> {
        let (y, c2, c3) = y_of(psi);
        if y < 0.0 {
            return None;
        }
        let chi = (y / c2).sqrt();
        Some((chi.powi(3) * c3 + a * y.sqrt()) / sqmu)
    };
    let mut lo = -4.0 * std::f64::consts::PI * std::f64::consts::PI;
    let mut hi = 4.0 * std::f64::consts::PI * std::f64::consts::PI;
    // dolna granica: y(ψ) ≥ 0 (dla a > 0 przesuwamy ψ w górę)
    while t_of(lo).is_none() && lo < hi {
        lo += 0.1;
    }
    let mut psi = 0.0f64.clamp(lo, hi);
    for _ in 0..200 {
        let t = match t_of(psi) {
            Some(t) => t,
            None => {
                lo = psi;
                psi = (lo + hi) / 2.0;
                continue;
            }
        };
        if (t - dt).abs() < 1e-6 * dt.max(1.0) {
            break;
        }
        if t <= dt {
            lo = psi;
        } else {
            hi = psi;
        }
        psi = (lo + hi) / 2.0;
    }
    let (y, _, _) = y_of(psi);
    if !(y.is_finite()) || y < 0.0 {
        return None;
    }
    let f = 1.0 - y / r1l;
    let g = a * (y / mu).sqrt();
    let gdot = 1.0 - y / r2l;
    if g.abs() < 1e-12 {
        return None;
    }
    let v1 = (r2 - f * r1) / g;
    let v2 = (gdot * r2 - r1) / g;
    (v1.is_finite() && v2.is_finite()).then_some((v1, v2))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MU: f64 = 3.986_004_418e14;

    #[test]
    fn propagate_circular() {
        let r = 7_000_000.0;
        let v = (MU / r).sqrt();
        let period = std::f64::consts::TAU * (r.powi(3) / MU).sqrt();
        let (p, _) = propagate(DVec3::new(r, 0.0, 0.0), DVec3::new(0.0, v, 0.0), period / 4.0, MU);
        assert!((p - DVec3::new(0.0, r, 0.0)).length() < 10.0, "{p}");
    }

    #[test]
    fn lambert_matches_propagation() {
        let r0 = DVec3::new(7_000_000.0, 0.0, 0.0);
        let v0 = DVec3::new(0.0, 8_500.0, 1_000.0);
        for dt in [1_800.0, 5_000.0, 8_000.0] {
            let (r1, v1) = propagate(r0, v0, dt, MU);
            let (va, vb) = lambert(r0, r1, dt, MU, true).expect("lambert");
            assert!((va - v0).length() < 1.0, "dt {dt}: {va} vs {v0}");
            assert!((vb - v1).length() < 1.0, "dt {dt}: {vb} vs {v1}");
        }
    }

    #[test]
    fn hyperbolic_propagation() {
        let r0 = DVec3::new(7_000_000.0, 0.0, 0.0);
        let v0 = DVec3::new(0.0, 12_000.0, 0.0);
        let (r1, v1) = propagate(r0, v0, 3_600.0, MU);
        let (r2, _) = propagate(r1, v1, -3_600.0, MU);
        assert!((r2 - r0).length() < 50.0, "{r2}");
    }

    #[test]
    fn elements_roundtrip() {
        let el = Elements { a: 2.0e7, e: 0.3, i: 0.5, node: 1.0, peri: 2.0, m0: 0.7, epoch: 0.0, mu: MU };
        let (r, v) = el.state(0.0);
        let back = Elements::from_state(r, v, MU, 0.0);
        assert!((back.a - el.a).abs() < 1.0 && (back.e - el.e).abs() < 1e-9 && (back.m0 - el.m0).abs() < 1e-7);
    }
}
