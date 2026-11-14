//! VSOP87B – heliocentryczne współrzędne sferyczne planet w ekliptyce J2000 (Bretagnon i Francou).
//! Szeregi z danych symulatora Orbiter (licencja MIT), przycięte do wyrazów ≥ 2·10⁻⁸
//! (dokładność rzędu sekund łuku w XX–XXI wieku).

use std::sync::OnceLock;

use glam::DVec3;

use super::AU;

/// Szereg jednej planety: [współrzędna L/B/R][potęga t] → wyrazy (A, B, C).
struct Series {
    coords: [Vec<Vec<[f64; 3]>>; 3],
}

const FILES: [&[u8]; 8] = [
    include_bytes!("../../assets/vsop/mercury.bin"),
    include_bytes!("../../assets/vsop/venus.bin"),
    include_bytes!("../../assets/vsop/earth.bin"),
    include_bytes!("../../assets/vsop/mars.bin"),
    include_bytes!("../../assets/vsop/jupiter.bin"),
    include_bytes!("../../assets/vsop/saturn.bin"),
    include_bytes!("../../assets/vsop/uranus.bin"),
    include_bytes!("../../assets/vsop/neptune.bin"),
];

fn parse(b: &[u8]) -> Series {
    let mut p = 0usize;
    let u32_at = |p: &mut usize| {
        let v = u32::from_le_bytes([b[*p], b[*p + 1], b[*p + 2], b[*p + 3]]);
        *p += 4;
        v as usize
    };
    let f64_at = |p: &mut usize| {
        let mut a = [0u8; 8];
        a.copy_from_slice(&b[*p..*p + 8]);
        *p += 8;
        f64::from_le_bytes(a)
    };
    let nal = u32_at(&mut p);
    let mut coords: [Vec<Vec<[f64; 3]>>; 3] = Default::default();
    for c in coords.iter_mut() {
        for _ in 0..=nal {
            let n = u32_at(&mut p);
            let mut terms = Vec::with_capacity(n);
            for _ in 0..n {
                terms.push([f64_at(&mut p), f64_at(&mut p), f64_at(&mut p)]);
            }
            c.push(terms);
        }
    }
    Series { coords }
}

fn series() -> &'static [Series] {
    static S: OnceLock<Vec<Series>> = OnceLock::new();
    S.get_or_init(|| FILES.iter().map(|b| parse(b)).collect())
}

/// Położenie planety `idx` (0 Merkury … 7 Neptun) względem Słońca [m], ekliptyka J2000.
pub fn position(idx: usize, jd: f64) -> DVec3 {
    let s = &series()[idx];
    let t = (jd - super::time::J2000) / 365_250.0;
    let mut v = [0.0f64; 3];
    for (k, c) in s.coords.iter().enumerate() {
        let mut tp = 1.0;
        let mut sum = 0.0;
        for terms in c {
            let mut part = 0.0;
            for [a, b, cc] in terms {
                part += a * (b + cc * t).cos();
            }
            sum += part * tp;
            tp *= t;
        }
        v[k] = sum;
    }
    let (l, b, r) = (v[0], v[1], v[2]);
    DVec3::new(r * b.cos() * l.cos(), r * b.cos() * l.sin(), r * b.sin()) * AU
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_distance() {
        // peryhelium ~0.9833 AU na początku stycznia, aphelium ~1.0167 AU na początku lipca
        let p = position(2, crate::astro::time::parse("2024-01-03").unwrap()).length() / AU;
        assert!((p - 0.9833).abs() < 0.001, "{p}");
        let a = position(2, crate::astro::time::parse("2024-07-05").unwrap()).length() / AU;
        assert!((a - 1.0167).abs() < 0.001, "{a}");
    }
}
