//! Nadchodzące starty z Launch Library 2 (The Space Devs): pobierane w tle, z pamięcią
//! podręczną na dysku (odświeżanie co 3 godziny – darmowy dostęp ma limit zapytań).

use std::io::Read;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::astro::time;

pub const API: &str = "https://ll.thespacedevs.com/2.3.0/launches/upcoming/?limit=40&mode=normal";
pub const SOURCE: &str = "https://thespacedevs.com/llapi";
const MAX_AGE_H: f64 = 3.0;

#[derive(Clone, Debug)]
pub struct Upcoming {
    pub name: String,
    pub jd: f64,
    /// Dokładność terminu: „Minute”, „Hour”, „Day”, „Month”…
    pub precision: String,
    pub status: String,
    pub provider: String,
    pub rocket: String,
    pub mission: String,
    pub orbit: String,
    pub pad: String,
    pub location: String,
    pub lat: f64,
    pub lon: f64,
}

impl Upcoming {
    /// Termin podany co do minuty albo godziny (odliczanie ma sens).
    pub fn precise(&self) -> bool {
        matches!(self.precision.as_str(), "" | "Second" | "Minute" | "Hour")
    }

    /// Start potwierdzony („Go”).
    pub fn go(&self) -> bool {
        self.status == "Go"
    }
}

#[derive(Default)]
pub enum State {
    #[default]
    Idle,
    Loading,
    Ready(Arc<Vec<Upcoming>>),
    Failed(String),
}

#[derive(Clone, Default)]
pub struct Loader {
    pub state: Arc<Mutex<State>>,
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn s(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or("").trim().to_string()
}

pub fn parse(text: &str) -> Result<Vec<Upcoming>, String> {
    let j: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let res = j.get("results").and_then(Value::as_array).ok_or("brak wyników")?;
    let mut out = Vec::new();
    for r in res {
        let Some(jd) = r.get("net").and_then(Value::as_str).and_then(time::parse) else { continue };
        let pad = r.get("pad");
        let (Some(lat), Some(lon)) = (num(pad.and_then(|p| p.get("latitude"))), num(pad.and_then(|p| p.get("longitude")))) else { continue };
        let mission = r.get("mission").filter(|m| !m.is_null());
        out.push(Upcoming {
            name: s(r.get("name")),
            jd,
            precision: s(r.get("net_precision").and_then(|p| p.get("name"))),
            status: s(r.get("status").and_then(|p| p.get("abbrev"))),
            provider: s(r.get("launch_service_provider").and_then(|p| p.get("name"))),
            rocket: s(r.get("rocket").and_then(|p| p.get("configuration")).and_then(|c| c.get("full_name").or(c.get("name")))),
            mission: s(mission.and_then(|m| m.get("description"))),
            orbit: s(mission.and_then(|m| m.get("orbit")).and_then(|o| o.get("name"))),
            pad: s(pad.and_then(|p| p.get("name"))),
            location: s(pad.and_then(|p| p.get("location")).and_then(|l| l.get("name"))),
            lat,
            lon,
        });
    }
    out.sort_by(|a, b| a.jd.total_cmp(&b.jd));
    Ok(out)
}

fn cache_path() -> std::path::PathBuf {
    crate::sats::cache_dir().join("upcoming.json")
}

fn fetch(force: bool) -> Result<Vec<Upcoming>, String> {
    let p = cache_path();
    let age_h = std::fs::metadata(&p).ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).map(|d| d.as_secs_f64() / 3600.0);
    if let (false, Some(a)) = (force, age_h) {
        if a < MAX_AGE_H {
            if let Ok(t) = std::fs::read_to_string(&p) {
                if let Ok(v) = parse(&t) {
                    return Ok(v);
                }
            }
        }
    }
    let net = crate::net::agent().get(API).call().map_err(|e| e.to_string()).and_then(|r| {
        let mut buf = String::new();
        r.into_reader().take(8 * 1024 * 1024).read_to_string(&mut buf).map_err(|e| e.to_string())?;
        Ok(buf)
    });
    match net {
        Ok(t) => {
            let v = parse(&t)?;
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&p, &t);
            Ok(v)
        }
        // bez sieci (albo po przekroczeniu limitu) – ostatnia zapisana lista
        Err(e) => std::fs::read_to_string(&p).ok().and_then(|t| parse(&t).ok()).ok_or(e),
    }
}

impl Loader {
    /// Lista (jeśli gotowa); pierwsze wywołanie zaczyna pobieranie.
    pub fn get(&self, ctx: &eframe::egui::Context) -> Option<Arc<Vec<Upcoming>>> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match &*st {
            State::Ready(v) => Some(v.clone()),
            State::Idle => {
                *st = State::Loading;
                drop(st);
                self.load(ctx, false);
                None
            }
            _ => None,
        }
    }

    pub fn load(&self, ctx: &eframe::egui::Context, force: bool) {
        let (state, ctx) = (self.state.clone(), ctx.clone());
        if let Ok(mut s) = state.lock() {
            *s = State::Loading;
        }
        std::thread::spawn(move || {
            let r = fetch(force);
            if let Ok(mut s) = state.lock() {
                *s = match r {
                    Ok(v) => State::Ready(Arc::new(v)),
                    Err(e) => State::Failed(e),
                };
            }
            ctx.request_repaint();
        });
    }

    pub fn error(&self) -> Option<String> {
        match &*self.state.lock().ok()? {
            State::Failed(e) => Some(e.clone()),
            _ => None,
        }
    }

    pub fn loading(&self) -> bool {
        self.state.lock().is_ok_and(|s| matches!(*s, State::Loading))
    }
}

/// Odliczanie do startu: „T− 3 d 04:12:09” (albo „T+ …” po starcie).
pub fn countdown(jd: f64, now: f64) -> String {
    let dt = (jd - now) * 86_400.0;
    let sign = if dt >= 0.0 { "T−" } else { "T+" };
    let s = dt.abs() as i64;
    let (d, h, m, sec) = (s / 86_400, s / 3600 % 24, s / 60 % 60, s % 60);
    if d > 0 {
        format!("{sign} {d} d {h:02}:{m:02}:{sec:02}")
    } else {
        format!("{sign} {h:02}:{m:02}:{sec:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ll2_shape() {
        let t = r#"{"results":[{"name":"Falcon 9 | Test","net":"2030-01-02T03:04:05Z","net_precision":{"name":"Minute"},
            "status":{"abbrev":"Go"},"launch_service_provider":{"name":"SpaceX"},"rocket":{"configuration":{"full_name":"Falcon 9 Block 5"}},
            "mission":{"description":"Opis","orbit":{"name":"Low Earth Orbit"}},
            "pad":{"name":"SLC-40","latitude":"28.56","longitude":-80.57,"location":{"name":"Cape Canaveral"}}},
            {"name":"bez daty","pad":{}}]}"#;
        let v = parse(t).expect("lista");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rocket, "Falcon 9 Block 5");
        assert!((v[0].lat - 28.56).abs() < 1e-9 && v[0].go() && v[0].precise());
        assert_eq!(countdown(v[0].jd, v[0].jd - 1.5), "T− 1 d 12:00:00");
    }
}
