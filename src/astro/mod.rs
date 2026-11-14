//! Astronomia i mechanika lotu: czas, efemerydy, ciała, propagacja orbit.

pub mod bodies;
pub mod kepler;
pub mod moon;
pub mod time;
pub mod vsop;

/// Jednostka astronomiczna [m].
pub const AU: f64 = 149_597_870_700.0;
/// Stała grawitacji (Orbiter).
pub const G: f64 = 6.674_30e-11;
/// Parametr grawitacyjny Słońca [m³/s²].
pub const GM_SUN: f64 = 1.327_124_400_18e20;
/// Nachylenie ekliptyki J2000 [rad].
pub const OBLIQUITY: f64 = 0.409_092_802_3;
/// Prędkość światła [m/s].
pub const C: f64 = 299_792_458.0;

/// Kierunek z rektascensji i deklinacji (stopnie) w ekliptyce J2000.
pub fn radec_to_ecliptic(ra_deg: f64, dec_deg: f64) -> glam::DVec3 {
    let (a, d) = (ra_deg.to_radians(), dec_deg.to_radians());
    let eq = glam::DVec3::new(d.cos() * a.cos(), d.cos() * a.sin(), d.sin());
    glam::DMat3::from_rotation_x(-OBLIQUITY) * eq
}

/// Kierunek z długości i szerokości ekliptycznej (stopnie).
pub fn ecliptic_dir(lon_deg: f64, lat_deg: f64) -> glam::DVec3 {
    let (l, b) = (lon_deg.to_radians(), lat_deg.to_radians());
    glam::DVec3::new(b.cos() * l.cos(), b.cos() * l.sin(), b.sin())
}

/// Układ równikowy Ziemi (TEME/J2000 – różnica pomijalna przy tej skali) → ekliptyka.
pub fn equatorial_to_ecliptic(v: glam::DVec3) -> glam::DVec3 {
    glam::DMat3::from_rotation_x(-OBLIQUITY) * v
}
