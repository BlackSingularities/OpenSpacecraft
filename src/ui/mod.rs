//! Interfejs nad widokiem 3D: górny pasek, katalog (lewy panel), szczegóły (prawy panel),
//! oś czasu, sterowanie kamerą, okna i paleta wyszukiwania.

pub mod archive;
pub mod catalog;
pub mod charts;
pub mod datepick;
pub mod details;
pub mod missions_window;
pub mod palette;
pub mod scene;
pub mod theme;
pub mod timeline;
pub mod windows;

pub use catalog::catalog;
pub use details::details;
pub use timeline::timeline;
pub use windows::windows;

use eframe::egui::{self, Color32, RichText};

use crate::app::{App, Focus};
use crate::astro::bodies;
use crate::i18n::tr;

pub const TOP_BAR_H: f32 = 56.0;
pub const TIMELINE_H: f32 = 74.0;
pub const CATALOG_W: f32 = 372.0;
pub const DETAILS_W: f32 = 400.0;

/// Szkło pod obszarem z poprzedniej klatki: obraz pod nim jest rozmyty.
pub fn glass_area(app: &mut App, ctx: &egui::Context, id: egui::Id, alpha: f32) {
    if let Some(r) = ctx.memory(|m| m.area_rect(id)) {
        app.glass.push((r, alpha));
    }
}

/// Odległość: m / km / mln km / AU.
pub fn fmt_km(km: f64) -> String {
    let au = 149_597_870.7;
    if km.abs() >= 0.2 * au {
        tf!("{:.3} AU", km / au)
    } else if km.abs() >= 1.0e6 {
        tf!("{:.2} mln km", km / 1.0e6)
    } else if km.abs() >= 10_000.0 {
        tf!("{} km", group(km.round() as i64))
    } else if km.abs() >= 1.0 {
        tf!("{:.1} km", km)
    } else {
        tf!("{:.0} m", km * 1000.0)
    }
}

/// Liczba z odstępami tysięcy (1 234 567).
pub fn group(n: i64) -> String {
    let s = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('\u{202f}');
        }
        out.push(c);
    }
    if n < 0 { format!("−{out}") } else { out }
}

pub fn fmt_speed(ms: f64) -> String {
    if ms >= 1000.0 { tf!("{:.2} km/s", ms / 1000.0) } else { tf!("{:.0} m/s", ms) }
}

pub fn fmt_mass(kg: f64) -> String {
    if kg >= 10_000.0 { tf!("{:.1} t", kg / 1000.0) } else { tf!("{} kg", group(kg.round() as i64)) }
}

/// Tempo zegara: „×1”, „1 h/s”, „30 d/s”, „1 rok/s”.
pub fn fmt_rate(r: f64) -> String {
    let a = r.abs();
    let s = if a < 1.5 {
        tr("na żywo").to_string()
    } else if a < 60.0 {
        tf!("{} s/s", a.round())
    } else if a < 3_600.0 {
        tf!("{} min/s", (a / 60.0).round())
    } else if a < 86_400.0 {
        tf!("{} h/s", (a / 3_600.0).round())
    } else if a < 31_557_600.0 * 0.9 {
        tf!("{} d/s", (a / 86_400.0).round())
    } else {
        tf!("{} lat/s", (a / 31_557_600.0).round())
    };
    if r < 0.0 { format!("◀ {s}") } else { s }
}

/// Mały przycisk z ikoną na szkle (pasek, nawigacja).
pub fn glyph_button(ui: &mut egui::Ui, glyph: &str, on: bool, hint: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::click());
    theme::focus_ring(ui, &resp, rect, 9.0);
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 9.0, theme::BRASS_DIM);
    } else if resp.hovered() {
        p.rect_filled(rect, 9.0, Color32::from_white_alpha(18));
    }
    let col = if on { theme::BRASS } else if resp.hovered() { theme::PAPER } else { theme::paper_2() };
    p.text(rect.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(18.0), col);
    resp.on_hover_text(tr(hint))
}

/// Przycisk z ikoną i napisem na górnym pasku (np. „Misje ▾”).
pub fn menu_button(ui: &mut egui::Ui, glyph: &str, text: &str, on: bool, hint: &str) -> egui::Response {
    let label = format!("{glyph}  {}  {}", tr(text), theme::icon::CARET_DOWN);
    let col = if on { theme::BRASS } else { theme::PAPER };
    let g = ui.painter().layout_no_wrap(label, egui::FontId::proportional(14.5), col);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(g.size().x + 22.0, 36.0), egui::Sense::click());
    theme::focus_ring(ui, &resp, rect, 9.0);
    let p = ui.painter();
    let bg = if on { theme::BRASS_DIM } else if resp.hovered() { Color32::from_white_alpha(18) } else { Color32::from_white_alpha(8) };
    p.rect_filled(rect, 9.0, bg);
    p.galley(egui::pos2(rect.left() + 11.0, rect.center().y - g.size().y / 2.0), g, col);
    resp.on_hover_text(tr(hint))
}

const MONTHS: [&str; 12] = ["stycznia", "lutego", "marca", "kwietnia", "maja", "czerwca", "lipca", "sierpnia", "września", "października", "listopada", "grudnia"];

/// „16 czerwca 1963” – data słownie.
pub fn fmt_long_date(jd: f64) -> String {
    use chrono::Datelike;
    let d = crate::astro::time::to_datetime(jd);
    tf!("{} {} {}", d.day(), tr(MONTHS[d.month0() as usize]), d.year())
}

/// Górny pasek na całą szerokość okna: panel, misje, ciała – data na środku – wyszukiwarka, warstwy, ustawienia.
pub fn top_bar(app: &mut App, ctx: &egui::Context) {
    use chrono::Timelike;
    use crate::app::Menu;
    let screen = ctx.content_rect();
    let bar = egui::Rect::from_min_size(screen.min, egui::vec2(screen.width(), TOP_BAR_H));
    app.glass.push((bar, 1.0));
    egui::Area::new(egui::Id::new("gorny-pasek")).fixed_pos(screen.min).order(egui::Order::Middle).show(ctx, |ui| {
        let p = ui.painter();
        p.rect_filled(bar, 0.0, theme::glass_fill());
        p.line_segment([bar.left_bottom(), bar.right_bottom()], egui::Stroke::new(1.0_f32, theme::glass_edge()));
        ui.allocate_rect(bar, egui::Sense::hover());
        let inner = bar.shrink2(egui::vec2(14.0, 9.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let logo = theme::logo(ui.ctx());
            ui.add(egui::Image::new(&logo).fit_to_exact_size(egui::vec2(30.0, 30.0)).corner_radius(7));
            if screen.width() > 1400.0 {
                ui.label(RichText::new("OpenSpacecraft").size(17.0).strong());
            }
            ui.add_space(8.0);
            if glyph_button(ui, theme::icon::SIDEBAR_SIMPLE, app.prefs.catalog_open, "Satelity i starty w wybranej chwili (Tab)").clicked() {
                app.prefs.catalog_open = !app.prefs.catalog_open;
            }
            if menu_button(ui, theme::icon::ROCKET_LAUNCH, "Misje", app.missions_open, "Przeglądaj i filtruj misje (M)").clicked() {
                app.missions_open = !app.missions_open;
                app.menu = None;
            }
            let bodies_on = matches!(app.menu, Some((Menu::Bodies, _)));
            let r = menu_button(ui, theme::icon::PLANET, "Ciała", bodies_on, "Planety, księżyce, planetoidy i komety");
            if r.clicked() {
                app.menu = if bodies_on { None } else { Some((Menu::Bodies, r.rect)) };
            }
        });
        // data – najważniejszy element, na środku paska
        let r = egui::Rect::from_center_size(inner.center(), egui::vec2(330.0, TOP_BAR_H - 10.0));
        let date_on = matches!(app.menu, Some((Menu::Date, _)));
        ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
            let (rect, resp) = ui.allocate_exact_size(r.size(), egui::Sense::click());
            theme::focus_ring(ui, &resp, rect, 12.0);
            let jd = app.clock.jd;
            let d = crate::astro::time::to_datetime(jd);
            let live = app.clock.playing && (app.clock.rate - 1.0).abs() < 1e-6 && (jd - crate::astro::time::now()).abs() < 1.0 / 1440.0;
            let state = if live {
                tr("na żywo").to_string()
            } else if app.clock.playing {
                fmt_rate(app.clock.rate)
            } else {
                tr("zatrzymany").to_string()
            };
            let p = ui.painter();
            let bg = if date_on { theme::BRASS_DIM } else if resp.hovered() { Color32::from_white_alpha(22) } else { Color32::from_white_alpha(10) };
            p.rect_filled(rect, 12.0, bg);
            p.text(rect.left_center() + egui::vec2(16.0, 0.0), egui::Align2::LEFT_CENTER, theme::icon::CALENDAR_BLANK, egui::FontId::proportional(20.0), theme::BRASS);
            p.text(egui::pos2(rect.center().x + 8.0, rect.top() + 5.0), egui::Align2::CENTER_TOP, fmt_long_date(jd), egui::FontId::proportional(17.0), theme::PAPER);
            let sub = format!("{:02}:{:02}:{:02} UTC  ·  {}", d.hour(), d.minute(), d.second(), state);
            p.text(egui::pos2(rect.center().x + 8.0, rect.bottom() - 5.0), egui::Align2::CENTER_BOTTOM, sub, egui::FontId::monospace(11.5), if live { theme::OK } else { theme::paper_2() });
            p.text(rect.right_center() - egui::vec2(14.0, 0.0), egui::Align2::RIGHT_CENTER, theme::icon::CARET_DOWN, egui::FontId::proportional(14.0), theme::paper_2());
            if resp.on_hover_text(tr("Wybierz datę i godzinę (D)")).clicked() {
                app.menu = if date_on { None } else { Some((Menu::Date, rect)) };
            }
        });
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if glyph_button(ui, theme::icon::GEAR_SIX, app.settings_open, "Ustawienia").clicked() {
                app.settings_open = !app.settings_open;
            }
            if glyph_button(ui, theme::icon::QUESTION, app.help_open, "Pomoc i skróty (F1)").clicked() {
                app.help_open = !app.help_open;
            }
            if glyph_button(ui, theme::icon::CHAT_CIRCLE_TEXT, false, "Zgłoś błąd lub sugestię").clicked() {
                app.feedback.open();
            }
            if glyph_button(ui, theme::icon::MAGNIFYING_GLASS, app.palette.is_some(), "Szukaj misji, satelitów, ciał niebieskich (Ctrl+K)").clicked() {
                app.palette = Some(Default::default());
            }
            ui.separator();
            let l = &mut app.prefs.layers;
            for (on, glyph, hint) in [
                (&mut l.stars, theme::icon::STAR, "Gwiazdy"),
                (&mut l.small_bodies, theme::icon::METEOR, "Planetoidy i komety"),
                (&mut l.sats, theme::icon::GLOBE_HEMISPHERE_WEST, "Satelity na orbicie (S)"),
                (&mut l.missions, theme::icon::ROCKET, "Misje trwające w tej chwili"),
                (&mut l.trajectory, theme::icon::PATH, "Trasa zaznaczonej misji"),
                (&mut l.orbits, theme::icon::CIRCLE_DASHED, "Orbity (O)"),
                (&mut l.labels, theme::icon::TAG, "Etykiety (L)"),
            ] {
                if glyph_button(ui, glyph, *on, hint).clicked() {
                    *on = !*on;
                }
            }
        });
    });
}

/// Rozwijane menu górnego paska: wybór daty albo lista ciał niebieskich.
pub fn menus(app: &mut App, ctx: &egui::Context) {
    use crate::app::Menu;
    let Some((menu, anchor)) = app.menu else { return };
    let screen = ctx.content_rect();
    let (pos, width) = match menu {
        Menu::Date => (egui::pos2(screen.center().x - 210.0, screen.top() + TOP_BAR_H + 6.0), 420.0),
        Menu::Bodies => (egui::pos2(anchor.left().max(screen.left() + 8.0), screen.top() + TOP_BAR_H + 6.0), 340.0),
    };
    let max_h = (screen.height() - TOP_BAR_H - TIMELINE_H - 24.0).max(240.0);
    let mut close = false;
    let resp = egui::Area::new(egui::Id::new("menu-gorne")).fixed_pos(pos).order(egui::Order::Foreground).show(ctx, |ui| {
        egui::Frame::new()
            .fill(theme::POPUP_FILL)
            .corner_radius(14)
            .stroke(egui::Stroke::new(1.0_f32, theme::glass_edge()))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(width - 28.0);
                match menu {
                    Menu::Date => datepick::show(app, ui),
                    Menu::Bodies => {
                        ui.set_max_height(max_h - 28.0);
                        close = catalog::bodies_list(app, ui);
                    }
                }
            });
    });
    // kliknięcie poza menu (i poza przyciskiem, który je otworzył) zamyka je
    let rect = resp.response.rect;
    let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p) && !anchor.contains(p)));
    if close || (outside && app.palette.is_none()) {
        app.menu = None;
    }
}

/// Sterowanie kamerą w prawym dolnym rogu widoku.
pub fn nav(app: &mut App, ctx: &egui::Context) {
    let right = if details::open(app) { DETAILS_W } else { 0.0 };
    let id = egui::Id::new("kamera");
    egui::Area::new(id).anchor(egui::Align2::RIGHT_BOTTOM, [-14.0 - right, -14.0 - TIMELINE_H]).order(egui::Order::Middle).show(ctx, |ui| {
        theme::glass_float().inner_margin(egui::Margin::same(3)).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.vertical(|ui| {
                let min = app.min_dist();
                if glyph_button(ui, theme::icon::PLUS, false, "Przybliż (kółko myszy)").clicked() {
                    app.cam.zoom(0.5, min, 3.0e13);
                }
                if glyph_button(ui, theme::icon::MINUS, false, "Oddal").clicked() {
                    app.cam.zoom(2.0, min, 3.0e13);
                }
                if glyph_button(ui, theme::icon::SUN, matches!(app.focus, Focus::Body(0)), "Cały Układ Słoneczny").clicked() {
                    app.selection = crate::app::Selection::None;
                    app.set_focus(Focus::Body(bodies::SUN), Some(1.2e12));
                }
                if glyph_button(ui, theme::icon::GLOBE_HEMISPHERE_EAST, matches!(app.focus, Focus::Body(3)), "Wróć do Ziemi").clicked() {
                    app.select_body(bodies::EARTH);
                    app.cam.dist = 3.6e7;
                }
            });
        });
    });
    if let Some(r) = ctx.memory(|m| m.area_rect(id)) {
        app.glass.push((r.shrink(3.0), 1.0));
    }
}

/// Nagłówek sekcji w panelach.
pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(theme::kicker(text));
}

/// Wiersz „etykieta – wartość” w panelu szczegółów.
pub fn fact(ui: &mut egui::Ui, k: &str, v: impl Into<String>) {
    ui.horizontal_top(|ui| {
        ui.add_sized([150.0, 18.0], egui::Label::new(RichText::new(tr(k)).color(theme::paper_3())).truncate());
        ui.add(egui::Label::new(RichText::new(v.into()).color(theme::PAPER)).wrap());
    });
}

/// Ten sam wiersz z wartością w czcionce danych technicznych.
pub fn fact_mono(ui: &mut egui::Ui, k: &str, v: impl Into<String>) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 18.0], egui::Label::new(RichText::new(tr(k)).color(theme::paper_3())).truncate());
        ui.add(egui::Label::new(RichText::new(v.into()).monospace().color(theme::PAPER)).truncate())
    })
    .inner
}

/// Etykieta w kształcie pigułki.
pub fn chip(ui: &mut egui::Ui, text: &str, fill: Color32, fg: Color32) {
    let g = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(11.5), fg);
    let (rect, _) = ui.allocate_exact_size(g.size() + egui::vec2(16.0, 6.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 10.0, fill);
    ui.painter().galley(rect.min + egui::vec2(8.0, 3.0), g, fg);
}

/// Pigułka do przełączania filtra (zaznaczona – obwódka dookoła).
pub fn toggle_chip(ui: &mut egui::Ui, text: &str, glyph: Option<&str>, on: bool, color: Color32) -> egui::Response {
    let label = match glyph {
        Some(g) => format!("{g}  {text}"),
        None => text.to_string(),
    };
    let g = ui.painter().layout_no_wrap(label, egui::FontId::proportional(12.5), if on { theme::PAPER } else { theme::paper_2() });
    let (rect, resp) = ui.allocate_exact_size(g.size() + egui::vec2(20.0, 10.0), egui::Sense::click());
    theme::focus_ring(ui, &resp, rect, 12.0);
    let p = ui.painter();
    let bg = if on { color.gamma_multiply(0.28) } else if resp.hovered() { Color32::from_white_alpha(18) } else { Color32::from_white_alpha(8) };
    p.rect_filled(rect, 13.0, bg);
    if on {
        p.rect_stroke(rect, 13.0, egui::Stroke::new(1.2_f32, color), egui::StrokeKind::Inside);
    }
    p.galley(rect.min + egui::vec2(10.0, 5.0), g, theme::PAPER);
    resp
}
