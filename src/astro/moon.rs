//! Księżyc: skrócona teoria ELP-2000/82 według J. Meeusa („Astronomical Algorithms”, rozdz. 47),
//! największe wyrazy – dokładność ok. 0,05° (kilkaset km), wystarczająca do wizualizacji.

use glam::DVec3;

/// (D, M, M', F, Σl [1e-6°], Σr [m])
const LR: [(i8, i8, i8, i8, f64, f64); 24] = [
    (0, 0, 1, 0, 6_288_774.0, -20_905_355.0),
    (2, 0, -1, 0, 1_274_027.0, -3_699_111.0),
    (2, 0, 0, 0, 658_314.0, -2_955_968.0),
    (0, 0, 2, 0, 213_618.0, -569_925.0),
    (0, 1, 0, 0, -185_116.0, 48_888.0),
    (0, 0, 0, 2, -114_332.0, -3_149.0),
    (2, 0, -2, 0, 58_793.0, 246_158.0),
    (2, -1, -1, 0, 57_066.0, -152_138.0),
    (2, 0, 1, 0, 53_322.0, -170_733.0),
    (2, -1, 0, 0, 45_758.0, -204_586.0),
    (0, 1, -1, 0, -40_923.0, -129_620.0),
    (1, 0, 0, 0, -34_720.0, 108_743.0),
    (0, 1, 1, 0, -30_383.0, 104_755.0),
    (2, 0, 0, -2, 15_327.0, 10_321.0),
    (0, 0, 1, 2, -12_528.0, 0.0),
    (0, 0, 1, -2, 10_980.0, 79_661.0),
    (4, 0, -1, 0, 10_675.0, -34_782.0),
    (0, 0, 3, 0, 10_034.0, -23_210.0),
    (4, 0, -2, 0, 8_548.0, -21_636.0),
    (2, 1, -1, 0, -7_888.0, 24_208.0),
    (2, 1, 0, 0, -6_766.0, 30_824.0),
    (1, 0, -1, 0, -5_163.0, -8_379.0),
    (1, 1, 0, 0, 4_987.0, -16_675.0),
    (2, -1, 1, 0, 4_036.0, -12_831.0),
];

/// (D, M, M', F, Σb [1e-6°])
const B: [(i8, i8, i8, i8, f64); 20] = [
    (0, 0, 0, 1, 5_128_122.0),
    (0, 0, 1, 1, 280_602.0),
    (0, 0, 1, -1, 277_693.0),
    (2, 0, 0, -1, 173_237.0),
    (2, 0, -1, 1, 55_413.0),
    (2, 0, -1, -1, 46_271.0),
    (2, 0, 0, 1, 32_573.0),
    (0, 0, 2, 1, 17_198.0),
    (2, 0, 1, -1, 9_266.0),
    (0, 0, 2, -1, 8_822.0),
    (2, -1, 0, -1, 8_216.0),
    (2, 0, -2, -1, 4_324.0),
    (2, 0, 1, 1, 4_200.0),
    (2, 1, 0, -1, -3_359.0),
    (2, -1, -1, 1, 2_463.0),
    (2, -1, 0, 1, 2_211.0),
    (2, -1, -1, -1, 2_065.0),
    (0, 1, -1, -1, -1_870.0),
    (4, 0, -1, -1, 1_828.0),
    (0, 1, 0, 1, -1_794.0),
];

/// Położenie Księżyca względem środka Ziemi [m], ekliptyka J2000.
pub fn geocentric(jd: f64) -> DVec3 {
    let t = (jd - super::time::J2000) / 36_525.0;
    let lp = 218.316_447_7 + 481_267.881_234_21 * t;
    let d = (297.850_192_1 + 445_267.111_403_4 * t).to_radians();
    let m = (357.529_109_2 + 35_999.050_290_9 * t).to_radians();
    let mp = (134.963_396_4 + 477_198.867_505_5 * t).to_radians();
    let f = (93.272_095_0 + 483_202.017_523_3 * t).to_radians();
    let e = 1.0 - 0.002_516 * t;
    let a1 = (119.75 + 131.849 * t).to_radians();
    let a2 = (53.09 + 479_264.290 * t).to_radians();
    let a3 = (313.45 + 481_266.484 * t).to_radians();
    let lpr = lp.to_radians();
    let (mut sl, mut sr, mut sb) = (0.0, 0.0, 0.0);
    for &(cd, cm, cmp, cf, l, r) in &LR {
        let arg = cd as f64 * d + cm as f64 * m + cmp as f64 * mp + cf as f64 * f;
        let k = e.powi(cm.unsigned_abs() as i32);
        sl += l * k * arg.sin();
        sr += r * k * arg.cos();
    }
    for &(cd, cm, cmp, cf, b) in &B {
        let arg = cd as f64 * d + cm as f64 * m + cmp as f64 * mp + cf as f64 * f;
        sb += b * e.powi(cm.unsigned_abs() as i32) * arg.sin();
    }
    sl += 3958.0 * a1.sin() + 1962.0 * (lpr - f).sin() + 318.0 * a2.sin();
    sb += -2235.0 * lpr.sin() + 382.0 * a3.sin() + 175.0 * (a1 - f).sin() + 175.0 * (a1 + f).sin()
        + 127.0 * (lpr - mp).sin()
        - 115.0 * (lpr + mp).sin();
    // długość względem równonocy daty → J2000 (precesja ogólna w długości)
    let lon = (lp + sl / 1e6 - 1.396_971_3 * t).to_radians();
    let lat = (sb / 1e6).to_radians();
    let dist = 385_000_560.0 + sr;
    DVec3::new(dist * lat.cos() * lon.cos(), dist * lat.cos() * lon.sin(), dist * lat.sin())
}

#[cfg(test)]
mod tests {
    #[test]
    fn meeus_example() {
        // Meeus, przykład 47.a: 1992-04-12 0h TD → λ = 133.162655°, β = −3.229126°, Δ = 368 409,7 km
        let jd = 2_448_724.5;
        let p = super::geocentric(jd);
        let t = (jd - crate::astro::time::J2000) / 36_525.0;
        let lon = p.y.atan2(p.x).to_degrees().rem_euclid(360.0) + 1.396_971_3 * t;
        let lat = (p.z / p.length()).asin().to_degrees();
        assert!((lon - 133.162_655).abs() < 0.05, "{lon}");
        assert!((lat + 3.229_126).abs() < 0.05, "{lat}");
        assert!((p.length() / 1000.0 - 368_409.7).abs() < 300.0, "{}", p.length());
    }
}
