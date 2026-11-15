//! Satelita nad Ziemią: mapa ze śladem naziemnym i przeloty nad miejscem obserwatora.
//! Plus kafelki z odczytami na żywo dla zaznaczonej misji.

use std::sync::OnceLock;

use eframe::egui::{self, Color32, RichText, Stroke};
use glam::DVec3;

use super::{fmt_km, fmt_speed, section, theme};
use crate::app::App;
use crate::astro::bodies::{self, EARTH, MOON, SUN};
use crate::astro::{time, C};
use crate::i18n::tr;
use crate::missions::traj;
use crate::passes;

/// Szerokość i długość geograficzna punktu pod satelitą.
fn subpoint(p: DVec3, jd: f64) -> (f64, f64) {
    let local = bodies::orientation(EARTH, jd).transpose() * p.normalize();
    (local.z.clamp(-1.0, 1.0).asin().to_degrees(), local.y.atan2(local.x).to_degrees())
}

fn earth_texture(ctx: &egui::Context) -> &'static egui::TextureHandle {
    static T: OnceLock<egui::TextureHandle> = OnceLock::new();
    T.get_or_init(|| {
        let img = image::load_from_memory(include_bytes!("../../assets/textures/earth.jpg")).map(|i| i.thumbnail(1024, 512).to_rgba8()).unwrap_or_else(|_| image::RgbaImage::new(2, 1));
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        ctx.load_texture("mapa-ziemi", ci, egui::TextureOptions::LINEAR)
    })
}

/// Czas lokalny obserwatora (strefa systemu) – do przelotów.
pub fn local_time(jd: f64) -> String {
    use chrono::TimeZone;
    let utc = time::to_datetime(jd);
    chrono::Local.from_utc_datetime(&utc).format("%Y-%m-%d %H:%M").to_string()
}

/// Mapa świata ze śladem naziemnym obiektu (jedna orbita wstecz, półtorej naprzód).
/// Kliknięcie ustawia miejsce obserwatora.
pub fn ground_track(app: &mut App, ui: &mut egui::Ui, pos: &dyn Fn(f64) -> Option<DVec3>, period_d: f64) {
    let jd = app.clock.jd;
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, w / 2.0), egui::Sense::click());
    let p = ui.painter_at(rect);
    p.image(earth_texture(ui.ctx()).id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::from_gray(200));
    p.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, Color32::from_white_alpha(40)), egui::StrokeKind::Inside);
    let to = |lat: f64, lon: f64| egui::pos2(rect.left() + ((lon + 180.0) / 360.0) as f32 * rect.width(), rect.top() + ((90.0 - lat) / 180.0) as f32 * rect.height());
    // siatka co 30°
    for k in 1..12 {
        let x = rect.left() + rect.width() * k as f32 / 12.0;
        p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], Stroke::new(0.5_f32, Color32::from_white_alpha(22)));
    }
    for k in 1..6 {
        let y = rect.top() + rect.height() * k as f32 / 6.0;
        p.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], Stroke::new(0.5_f32, Color32::from_white_alpha(22)));
    }
    // ślad
    let n = 360;
    let (t0, t1) = (jd - period_d, jd + period_d * 1.5);
    let mut prev: Option<(egui::Pos2, f64, bool)> = None;
    for k in 0..=n {
        let t = t0 + (t1 - t0) * k as f64 / n as f64;
        let Some(r) = pos(t) else {
            prev = None;
            continue;
        };
        let (lat, lon) = subpoint(r, t);
        let s = to(lat, lon);
        let future = t > jd;
        if let Some((ps, plon, _)) = prev {
            if (lon - plon).abs() < 180.0 {
                let col = if future { Color32::from_rgba_unmultiplied(140, 245, 215, 150) } else { Color32::from_rgb(140, 245, 215) };
                p.line_segment([ps, s], Stroke::new(if future { 1.2_f32 } else { 2.0 }, col));
            }
        }
        prev = Some((s, lon, future));
    }
    // obserwator
    let home = to(app.settings.home_lat, app.settings.home_lon);
    p.circle_filled(home, 4.0, theme::WARN);
    p.circle_stroke(home, 7.0, Stroke::new(1.2_f32, theme::WARN));
    // teraz
    if let Some(r) = pos(jd) {
        let (lat, lon) = subpoint(r, jd);
        let s = to(lat, lon);
        // zasięg widoczności (horyzont 10°) – przybliżony okrąg
        let re = bodies::get(EARTH).radius;
        let h = r.length() - re;
        let el = passes::MIN_EL.to_radians();
        let lam = (re / (re + h) * el.cos()).acos() - el;
        let rad_deg = lam.to_degrees();
        let pts: Vec<egui::Pos2> = (0..=48)
            .map(|k| {
                let a = k as f64 / 48.0 * std::f64::consts::TAU;
                let la = (lat + rad_deg * a.sin()).clamp(-89.0, 89.0);
                let lo = lon + rad_deg * a.cos() / la.to_radians().cos().max(0.2);
                to(la, (lo + 180.0).rem_euclid(360.0) - 180.0)
            })
            .collect();
        for wnd in pts.windows(2) {
            if (wnd[0].x - wnd[1].x).abs() < rect.width() / 2.0 {
                p.line_segment([wnd[0], wnd[1]], Stroke::new(1.0_f32, Color32::from_white_alpha(90)));
            }
        }
        p.circle_filled(s, 5.0, Color32::from_rgb(140, 245, 215));
        p.circle_stroke(s, 5.0, Stroke::new(1.0_f32, Color32::BLACK));
    }
    let resp = resp.on_hover_text(tr("Ślad naziemny: jasny – za nami, przygaszony – przed nami. Okrąg – skąd satelitę widać nad horyzontem. Kliknij mapę, by ustawić swoje miejsce."));
    if resp.clicked() {
        if let Some(ptr) = resp.interact_pointer_pos() {
            let lon = ((ptr.x - rect.left()) / rect.width()) as f64 * 360.0 - 180.0;
            let lat = 90.0 - ((ptr.y - rect.top()) / rect.height()) as f64 * 180.0;
            app.settings.home_lat = (lat * 100.0).round() / 100.0;
            app.settings.home_lon = (lon * 100.0).round() / 100.0;
            app.settings.home_name = format!("{:.2}°, {:.2}°", app.settings.home_lat, app.settings.home_lon);
            let _ = app.settings.save();
            app.passes = None;
        }
    }
}

/// Mapa i przeloty dla obiektu z katalogu satelitów.
pub fn sat_tools(app: &mut App, ui: &mut egui::Ui, norad: u32) {
    let cat = app.sats.current();
    let Some(o) = cat.find(norad) else { return };
    let jd = app.clock.jd;
    if !o.alive_at(jd) || o.period_min <= 0.0 {
        return;
    }
    section(ui, "Ślad nad Ziemią");
    let period = o.period_min / 1440.0;
    {
        let cat2 = cat.clone();
        let pos = move |t: f64| o.position(t, &cat2.gp).map(|x| x.0);
        ground_track(app, ui, &pos, period.min(0.6));
    }
    // przeloty
    section(ui, &tf!("Przeloty nad: {}", app.settings.home_name));
    let key = (norad, (jd * 24.0).floor() as i64, app.settings.home_lat.to_bits(), app.settings.home_lon.to_bits());
    let fresh = app.passes.as_ref().is_some_and(|(k, _)| *k == key);
    if !fresh {
        let list = passes::predict(o, &cat.gp, app.settings.home_lat, app.settings.home_lon, (jd * 24.0).floor() / 24.0, 3.0, 10);
        app.passes = Some((key, list));
    }
    let list = app.passes.as_ref().map(|x| x.1.clone()).unwrap_or_default();
    if o.gp.is_none() {
        ui.label(RichText::new(tr("Brak aktualnych elementów orbity – przeloty są tylko orientacyjne.")).small().color(theme::WARN));
    }
    if list.is_empty() {
        ui.label(RichText::new(tr("W ciągu 3 dni od wybranej chwili obiekt nie wzniesie się wyżej niż 10° nad horyzont.")).color(theme::paper_3()));
    }
    for ps in &list {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::click());
        let p = ui.painter();
        let now = jd >= ps.start && jd <= ps.end;
        p.rect_filled(rect, 8.0, if resp.hovered() { Color32::from_white_alpha(16) } else { Color32::from_white_alpha(6) });
        if now {
            p.rect_stroke(rect, 8.0, Stroke::new(1.0_f32, theme::BRASS), egui::StrokeKind::Inside);
        }
        let icon = if ps.visible { theme::icon::EYE } else { theme::icon::EYE_SLASH };
        let ic_col = if ps.visible { theme::WARN } else { theme::paper_3() };
        p.text(rect.left_center() + egui::vec2(14.0, 0.0), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(16.0), ic_col);
        let line1 = format!("{}  ·  {}", local_time(ps.start), tf!("{:.0}° nad horyzontem", ps.max_el));
        let dur = (ps.end - ps.start) * 1440.0;
        let line2 = format!(
            "{} → {}  ·  {}  ·  {}",
            passes::compass(ps.az_start),
            passes::compass(ps.az_end),
            tf!("{:.0} min", dur.max(1.0)),
            if ps.visible { tr("widoczny gołym okiem") } else { tr("niewidoczny (w cieniu albo za dnia)") }
        );
        let clip = ui.painter_at(rect.shrink(2.0));
        clip.text(rect.left_top() + egui::vec2(32.0, 6.0), egui::Align2::LEFT_TOP, line1, egui::FontId::proportional(13.5), theme::PAPER);
        clip.text(rect.left_bottom() + egui::vec2(32.0, -6.0), egui::Align2::LEFT_BOTTOM, line2, egui::FontId::proportional(11.5), theme::paper_3());
        if resp.on_hover_text(tr("Czas lokalny. Kliknij, by przenieść zegar na początek przelotu.")).clicked() {
            app.clock.jd = ps.start - 1.0 / 1440.0;
            app.clock.rate = 10.0;
            app.clock.playing = true;
        }
    }
    ui.label(RichText::new(tr("Miejsce zmienisz w ustawieniach albo kliknięciem w mapę.")).small().color(theme::paper_3()));
}

/// Kafelki z odczytami na żywo (wysokość, prędkość, odległości, opóźnienie sygnału).
pub fn telemetry(app: &App, ui: &mut egui::Ui, i: usize) {
    let jd = app.clock.jd;
    let Some((p, _)) = app.mission_position(i, jd) else { return };
    let h = 2.0 / 86_400.0;
    let v = match (app.mission_position(i, jd + h), app.mission_position(i, jd - h)) {
        (Some((a, _)), Some((b, _))) => (a - b) / 4.0,
        _ => DVec3::ZERO,
    };
    let near = traj::nearest_body(p, jd);
    let earth = bodies::position(EARTH, jd);
    let moon = bodies::position(MOON, jd);
    let de = (p - earth).length();
    let mut tiles: Vec<(String, String)> = Vec::new();
    if near != SUN {
        let b = bodies::get(near);
        let alt = (p - bodies::position(near, jd)).length() - b.radius;
        tiles.push((tf!("Wysokość · {}", b.name()), fmt_km(alt.max(0.0) / 1000.0)));
        tiles.push((tf!("Prędkość · {}", b.name()), fmt_speed((v - bodies::velocity(near, jd)).length())));
    } else {
        tiles.push((tr("Prędkość · Słońce").into(), fmt_speed(v.length())));
        tiles.push((tr("Od Słońca").into(), tf!("{:.3} AU", p.length() / crate::astro::AU)));
    }
    tiles.push((tr("Od Ziemi").into(), if de > 0.05 * crate::astro::AU { tf!("{:.3} AU", de / crate::astro::AU) } else { fmt_km(de / 1000.0) }));
    if de < 3.0e9 {
        tiles.push((tr("Od Księżyca").into(), fmt_km((p - moon).length() / 1000.0)));
    }
    let lt = de / C;
    tiles.push((tr("Sygnał z Ziemi").into(), if lt < 60.0 { tf!("{:.2} s", lt) } else { time::fmt_duration(lt / 86_400.0) }));
    let cols = 3;
    let gap = 6.0;
    let w = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32;
    for row in tiles.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (k, v) in row {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 46.0), egui::Sense::hover());
                let p = ui.painter();
                p.rect_filled(rect, 8.0, Color32::from_white_alpha(9));
                p.text(rect.left_top() + egui::vec2(8.0, 6.0), egui::Align2::LEFT_TOP, k, egui::FontId::proportional(10.5), theme::paper_3());
                p.text(rect.left_bottom() + egui::vec2(8.0, -6.0), egui::Align2::LEFT_BOTTOM, v, egui::FontId::monospace(13.5), theme::PAPER);
            }
        });
    }
}
