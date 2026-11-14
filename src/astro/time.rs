//! Czas: dzień juliański (JD, UTC), zamiana na datę kalendarzową i z powrotem, formatowanie.
//! Różnica TT–UTC (ok. 69 s) jest pomijana – przy skali wizualizacji jest niewidoczna.

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};

pub const J2000: f64 = 2_451_545.0;
const UNIX_EPOCH_JD: f64 = 2_440_587.5;

/// Bieżąca chwila jako JD.
pub fn now() -> f64 {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    UNIX_EPOCH_JD + d / 86_400.0
}

pub fn from_datetime(dt: &NaiveDateTime) -> f64 {
    let secs = dt.and_utc().timestamp() as f64 + dt.and_utc().timestamp_subsec_nanos() as f64 * 1e-9;
    UNIX_EPOCH_JD + secs / 86_400.0
}

pub fn to_datetime(jd: f64) -> NaiveDateTime {
    // zaokrąglenie do milisekundy: JD w f64 ma błąd rzędu dziesiątek µs, a odczyt ma pokazać 17:54:00, nie 17:53:59
    let secs = ((jd - UNIX_EPOCH_JD) * 86_400_000.0).round() / 1000.0;
    let whole = secs.floor();
    let nanos = ((secs - whole) * 1e9).round().clamp(0.0, 999_999_999.0) as u32;
    chrono::DateTime::from_timestamp(whole as i64, nanos)
        .map(|d| d.naive_utc())
        .unwrap_or_default()
}

/// ISO 8601: `2024-05-03`, `2024-05-03T09:27`, `2024-05-03T09:27:15Z`, `2024-05-03 09:27`.
pub fn parse(s: &str) -> Option<f64> {
    let s = s.trim().trim_end_matches('Z').trim_end_matches(" UTC").replace(' ', "T");
    for fmt in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(&s, fmt) {
            return Some(from_datetime(&dt));
        }
    }
    NaiveDate::parse_from_str(&s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| from_datetime(&dt))
}

/// `1969-07-20 20:17 UTC`
pub fn fmt_datetime(jd: f64) -> String {
    let d = to_datetime(jd);
    format!("{:04}-{:02}-{:02} {:02}:{:02} UTC", d.year(), d.month(), d.day(), d.hour(), d.minute())
}

/// `1969-07-20 20:17:40`
pub fn fmt_datetime_s(jd: f64) -> String {
    let d = to_datetime(jd);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", d.year(), d.month(), d.day(), d.hour(), d.minute(), d.second())
}

pub fn fmt_date(jd: f64) -> String {
    let d = to_datetime(jd);
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}

pub fn year(jd: f64) -> i32 {
    to_datetime(jd).year()
}

/// JD początku roku.
pub fn year_start(y: i32) -> f64 {
    NaiveDate::from_ymd_opt(y, 1, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| from_datetime(&dt))
        .unwrap_or(J2000)
}

/// Czas trwania: „3 lata 41 d”, „5 d 4 h”, „2 h 13 min”, „45 s”.
pub fn fmt_duration(days: f64) -> String {
    let neg = days < 0.0;
    let d = days.abs();
    let s = if d >= 365.25 * 2.0 {
        let y = (d / 365.25).floor();
        let rest = (d - y * 365.25).floor();
        tf!("{} lat {} d", y as i64, rest as i64)
    } else if d >= 2.0 {
        let whole = d.floor();
        tf!("{} d {} h", whole as i64, ((d - whole) * 24.0).floor() as i64)
    } else if d * 24.0 >= 1.0 {
        let h = (d * 24.0).floor();
        tf!("{} h {} min", h as i64, ((d * 24.0 - h) * 60.0).floor() as i64)
    } else if d * 1440.0 >= 1.0 {
        let m = (d * 1440.0).floor();
        tf!("{} min {} s", m as i64, ((d * 1440.0 - m) * 60.0).floor() as i64)
    } else {
        tf!("{} s", (d * 86_400.0).round() as i64)
    };
    if neg { format!("−{s}") } else { s }
}

/// Średni czas gwiazdowy Greenwich [rad] (IAU 1982).
pub fn gmst(jd: f64) -> f64 {
    let t = (jd - J2000) / 36_525.0;
    let deg = 280.460_618_37 + 360.985_647_366_29 * (jd - J2000) + 0.000_387_933 * t * t - t * t * t / 38_710_000.0;
    deg.rem_euclid(360.0).to_radians()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let jd = parse("1969-07-20T20:17:40Z").unwrap();
        assert!((jd - 2_440_423.345_6).abs() < 1e-3, "{jd}");
        assert_eq!(fmt_datetime_s(jd), "1969-07-20 20:17:40");
        assert!((parse("2000-01-01T12:00").unwrap() - J2000).abs() < 1e-9);
        assert!(parse("1957-10-04").is_some());
    }
}
