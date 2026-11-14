//! Dolny pasek: zegar symulacji, tempo, data i oś czasu (cała era kosmiczna albo przebieg misji).

use eframe::egui::{self, Color32, RichText, Stroke};

use super::{fmt_rate, glyph_button, theme, TIMELINE_H};
use crate::app::{App, Selection, JD_MAX, JD_MIN};
use crate::astro::time;
use crate::i18n::tr;
use crate::missions;

pub fn timeline(app: &mut App, ctx: &egui::Context) {
    let screen = ctx.content_rect();
    let bar = egui::Rect::from_min_size(egui::pos2(screen.left(), screen.bottom() - TIMELINE_H), egui::vec2(screen.width(), TIMELINE_H));
    app.glass.push((bar, 1.0));
    egui::Area::new(egui::Id::new("os-czasu")).fixed_pos(bar.min).order(egui::Order::Middle).show(ctx, |ui| {
        let p = ui.painter();
        p.rect_filled(bar, 0.0, theme::glass_fill());
        p.line_segment([bar.left_top(), bar.right_top()], Stroke::new(1.0_f32, theme::glass_edge()));
        ui.allocate_rect(bar, egui::Sense::hover());
        let inner = bar.shrink2(egui::vec2(14.0, 8.0));
        // lewa część: przyciski i data
        let left = egui::Rect::from_min_size(inner.min, egui::vec2(560.0f32.min(inner.width() * 0.55), inner.height()));
        ui.scope_builder(egui::UiBuilder::new().max_rect(left).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if glyph_button(ui, theme::icon::REWIND, app.clock.rate < 0.0, "Wstecz (R)").clicked() {
                app.clock.rate = -app.clock.rate.abs();
                app.clock.playing = true;
            }
            let (g, h) = if app.clock.playing { (theme::icon::PAUSE, "Pauza (spacja)") } else { (theme::icon::PLAY, "Odtwarzaj (spacja)") };
            if glyph_button(ui, g, false, h).clicked() {
                if !app.clock.playing && app.clock.rate < 0.0 {
                    app.clock.rate = app.clock.rate.abs();
                }
                app.clock.playing = !app.clock.playing;
            }
            if glyph_button(ui, theme::icon::CARET_DOUBLE_LEFT, false, "Wolniej (,)").clicked() {
                app.step_rate(-1);
            }
            ui.add_sized([96.0, 30.0], egui::Label::new(RichText::new(fmt_rate(app.clock.rate)).monospace().color(theme::BRASS)));
            if glyph_button(ui, theme::icon::CARET_DOUBLE_RIGHT, false, "Szybciej (.)").clicked() {
                app.step_rate(1);
            }
            ui.add_space(8.0);
            // data: kliknięcie – edycja
            match &mut app.date_edit {
                Some(text) => {
                    let r = ui.add(egui::TextEdit::singleline(text).desired_width(170.0).font(egui::TextStyle::Monospace));
                    r.request_focus();
                    if r.lost_focus() {
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            match time::parse(text) {
                                Some(jd) => app.clock.jd = jd.clamp(JD_MIN, JD_MAX),
                                None => app.toast = Some((tr("Nie rozpoznano daty – użyj formatu RRRR-MM-DD GG:MM").to_string(), std::time::Instant::now())),
                            }
                        }
                        app.date_edit = None;
                    }
                }
                None => {
                    let r = ui.add(egui::Button::new(RichText::new(time::fmt_datetime_s(app.clock.jd)).monospace().size(15.0)).frame(false)).on_hover_text(tr("Kliknij, aby wpisać datę (UTC)"));
                    if r.clicked() {
                        app.date_edit = Some(time::fmt_datetime_s(app.clock.jd));
                    }
                    ui.label(RichText::new("UTC").small().color(theme::paper_3()));
                }
            }
            if glyph_button(ui, theme::icon::CLOCK_COUNTER_CLOCKWISE, false, "Teraz, czas rzeczywisty (N)").clicked() {
                app.clock.jd = time::now();
                app.clock.rate = 1.0;
                app.clock.playing = true;
            }
        });
        // prawa część: oś czasu
        let track = egui::Rect::from_min_max(egui::pos2(left.right() + 16.0, inner.top() + 6.0), egui::pos2(inner.right(), inner.bottom() - 6.0));
        track_ui(app, ui, track);
    });
}

fn track_ui(app: &mut App, ui: &mut egui::Ui, rect: egui::Rect) {
    if rect.width() < 60.0 {
        return;
    }
    let db = missions::db();
    let mission = match app.selection {
        Selection::Mission(i) => Some(i),
        _ => None,
    };
    // zakres: przebieg misji albo cała era kosmiczna
    let (a, b) = match mission {
        Some(i) => {
            let m = &db.missions[i];
            let end = m.end_jd.unwrap_or_else(|| time::now().max(m.events.last().map(|e| e.jd).unwrap_or(m.launch_jd)));
            let pad = ((end - m.launch_jd) * 0.04).max(0.05);
            (m.launch_jd - pad, end + pad)
        }
        None => (time::year_start(1957), time::year_start(time::year(time::now()) + 11)),
    };
    let resp = ui.interact(rect, egui::Id::new("os-czasu-suwak"), egui::Sense::click_and_drag());
    let p = ui.painter_at(rect.expand(2.0));
    let x_of = |jd: f64| rect.left() + ((jd - a) / (b - a)).clamp(0.0, 1.0) as f32 * rect.width();
    let mid = rect.center().y + 4.0;
    p.rect_filled(egui::Rect::from_min_max(egui::pos2(rect.left(), mid - 2.0), egui::pos2(rect.right(), mid + 2.0)), 2.0, Color32::from_white_alpha(28));
    // podziałka
    let span_y = (b - a) / 365.25;
    let label_font = egui::FontId::monospace(10.0);
    if span_y > 3.0 {
        let step = if span_y > 60.0 { 10 } else if span_y > 25.0 { 5 } else if span_y > 8.0 { 2 } else { 1 };
        let y0 = time::year(a);
        let y1 = time::year(b);
        for y in y0..=y1 + 1 {
            if y % step != 0 {
                continue;
            }
            let x = x_of(time::year_start(y));
            if x <= rect.left() || x >= rect.right() {
                continue;
            }
            p.line_segment([egui::pos2(x, mid - 6.0), egui::pos2(x, mid + 6.0)], Stroke::new(1.0_f32, Color32::from_white_alpha(50)));
            p.text(egui::pos2(x, rect.bottom()), egui::Align2::CENTER_BOTTOM, y.to_string(), label_font.clone(), theme::paper_3());
        }
    } else {
        // dni / miesiące
        let days = b - a;
        let step = if days > 400.0 { 60.0 } else if days > 120.0 { 30.0 } else if days > 40.0 { 7.0 } else if days > 6.0 { 1.0 } else { 0.25 };
        let mut t = (a / step).ceil() * step;
        while t < b {
            let x = x_of(t);
            p.line_segment([egui::pos2(x, mid - 5.0), egui::pos2(x, mid + 5.0)], Stroke::new(1.0_f32, Color32::from_white_alpha(44)));
            let txt = if step < 1.0 { time::fmt_datetime(t)[11..16].to_string() } else { time::fmt_date(t)[5..].to_string() };
            p.text(egui::pos2(x, rect.bottom()), egui::Align2::CENTER_BOTTOM, txt, label_font.clone(), theme::paper_3());
            t += step;
        }
    }
    // starty misji (cała era) albo wydarzenia (misja)
    match mission {
        None => {
            for m in &db.missions {
                if !app.kind_visible(m.kind) {
                    continue;
                }
                let x = x_of(m.launch_jd);
                let c = m.kind.color();
                p.circle_filled(egui::pos2(x, mid - 10.0), 2.2, Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8));
            }
        }
        Some(i) => {
            let m = &db.missions[i];
            let c = m.kind.color();
            let col = Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8);
            let x0 = x_of(m.launch_jd);
            let x1 = x_of(m.end_jd.unwrap_or(b));
            p.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, mid - 2.0), egui::pos2(x1, mid + 2.0)), 2.0, col.gamma_multiply(0.6));
            for e in &m.events {
                let x = x_of(e.jd);
                let s = egui::pos2(x, mid - 11.0);
                let d = 4.5;
                p.add(egui::Shape::convex_polygon(vec![s + egui::vec2(0.0, -d), s + egui::vec2(d, 0.0), s + egui::vec2(0.0, d), s + egui::vec2(-d, 0.0)], if e.jd <= app.clock.jd { col } else { col.gamma_multiply(0.5) }, Stroke::NONE));
            }
            if let Some(pos) = resp.hover_pos() {
                if let Some(e) = m.events.iter().find(|e| (x_of(e.jd) - pos.x).abs() < 6.0 && pos.y < mid) {
                    resp.clone().on_hover_text(format!("{} · {}", time::fmt_datetime(e.jd), e.text()));
                }
            }
        }
    }
    // „teraz”
    let xn = x_of(time::now());
    if xn > rect.left() && xn < rect.right() {
        p.line_segment([egui::pos2(xn, rect.top()), egui::pos2(xn, mid + 8.0)], Stroke::new(1.0_f32, theme::OK.gamma_multiply(0.7)));
    }
    // wskaźnik bieżącej chwili
    let x = x_of(app.clock.jd);
    p.circle_filled(egui::pos2(x, mid), 7.0, theme::BRASS);
    p.circle_stroke(egui::pos2(x, mid), 7.0, Stroke::new(2.0_f32, theme::INK_0));
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let f = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
            app.clock.jd = (a + (b - a) * f).clamp(JD_MIN, JD_MAX);
        }
    }
    if resp.hovered() && !resp.dragged() {
        if let Some(pos) = resp.hover_pos() {
            let f = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
            p.line_segment([egui::pos2(pos.x, mid - 8.0), egui::pos2(pos.x, mid + 8.0)], Stroke::new(1.0_f32, theme::paper_2()));
            let _ = f;
        }
    }
}
