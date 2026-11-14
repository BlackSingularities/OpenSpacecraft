//! Wykresy w panelach: populacja obiektów na orbicie i starty w kolejnych latach.

use eframe::egui::{self, Color32, Stroke};

use super::theme;
use crate::sats::{Catalog, ObjType};

fn c32(c: [f32; 3]) -> Color32 {
    Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

/// Słupki skumulowane: obiekty na orbicie na koniec każdego roku według rodzaju.
pub fn population(ui: &mut egui::Ui, cat: &Catalog, year: i32) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 120.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 8.0, Color32::from_white_alpha(6));
    let data = &cat.per_year;
    if data.is_empty() {
        p.text(rect.center(), egui::Align2::CENTER_CENTER, crate::i18n::tr("brak danych"), egui::FontId::proportional(12.0), theme::paper_3());
        return;
    }
    let max = data.iter().map(|(_, c)| c.iter().sum::<u32>()).max().unwrap_or(1).max(1) as f32;
    let n = data.len() as f32;
    let inner = rect.shrink2(egui::vec2(6.0, 8.0));
    let bw = inner.width() / n;
    let order = [ObjType::Payload, ObjType::RocketBody, ObjType::Debris, ObjType::Unknown];
    for (k, (y, c)) in data.iter().enumerate() {
        let x0 = inner.left() + k as f32 * bw;
        let mut base = inner.bottom();
        for t in order {
            let h = c[t as usize] as f32 / max * inner.height();
            if h > 0.0 {
                let alpha = if *y == year { 1.0 } else { 0.75 };
                p.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, base - h), egui::pos2(x0 + bw.max(1.0) - 0.4, base)), 0.0, c32(t.color(true)).gamma_multiply(alpha));
                base -= h;
            }
        }
        if *y == year {
            p.line_segment([egui::pos2(x0 + bw / 2.0, inner.top()), egui::pos2(x0 + bw / 2.0, inner.bottom())], Stroke::new(1.0_f32, theme::PAPER));
        }
    }
    p.text(inner.left_top(), egui::Align2::LEFT_TOP, super::group(max as i64), egui::FontId::monospace(10.0), theme::paper_3());
    p.text(inner.left_bottom() + egui::vec2(0.0, 6.0), egui::Align2::LEFT_BOTTOM, data[0].0.to_string(), egui::FontId::monospace(10.0), theme::paper_3());
    p.text(inner.right_bottom() + egui::vec2(0.0, 6.0), egui::Align2::RIGHT_BOTTOM, data[data.len() - 1].0.to_string(), egui::FontId::monospace(10.0), theme::paper_3());
    if let Some(pos) = resp.hover_pos() {
        let k = (((pos.x - inner.left()) / bw) as usize).min(data.len() - 1);
        let (y, c) = data[k];
        resp.on_hover_text(tf!("{}: satelity {}, człony rakiet {}, odłamki {}, inne {}", y, c[0], c[1], c[2], c[3]));
    }
}

/// Liczba nowych satelitów w kolejnych latach.
pub fn launches(ui: &mut egui::Ui, cat: &Catalog, year: i32) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 80.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 8.0, Color32::from_white_alpha(6));
    let data = &cat.launches_per_year;
    if data.is_empty() {
        return;
    }
    let max = data.iter().map(|d| d.1).max().unwrap_or(1).max(1) as f32;
    let inner = rect.shrink2(egui::vec2(6.0, 8.0));
    let bw = inner.width() / data.len() as f32;
    for (k, (y, c)) in data.iter().enumerate() {
        let h = (*c as f32 / max).sqrt() * inner.height();
        let x0 = inner.left() + k as f32 * bw;
        let col = if *y == year { theme::PAPER } else { theme::BRASS.gamma_multiply(0.8) };
        p.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, inner.bottom() - h), egui::pos2(x0 + bw.max(1.0) - 0.4, inner.bottom())), 0.0, col);
    }
    p.text(inner.left_top(), egui::Align2::LEFT_TOP, tf!("maks. {} / rok (skala pierwiastkowa)", max as i64), egui::FontId::monospace(10.0), theme::paper_3());
    if let Some(pos) = resp.hover_pos() {
        let k = (((pos.x - inner.left()) / bw) as usize).min(data.len() - 1);
        resp.on_hover_text(tf!("{}: {} nowych satelitów", data[k].0, data[k].1));
    }
}

/// Liczba startów orbitalnych w latach; kliknięcie wybiera rok.
pub fn launch_years(ui: &mut egui::Ui, data: &[(i32, u32)], year: i32) -> Option<i32> {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 96.0), egui::Sense::click());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 8.0, Color32::from_white_alpha(6));
    if data.is_empty() {
        return None;
    }
    let max = data.iter().map(|d| d.1).max().unwrap_or(1).max(1) as f32;
    let inner = rect.shrink2(egui::vec2(6.0, 8.0));
    let bw = inner.width() / data.len() as f32;
    for (k, (y, c)) in data.iter().enumerate() {
        let h = *c as f32 / max * inner.height();
        let x0 = inner.left() + k as f32 * bw;
        let col = if *y == year { theme::PAPER } else { theme::BRASS.gamma_multiply(0.8) };
        p.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, inner.bottom() - h), egui::pos2(x0 + bw.max(1.0) - 0.4, inner.bottom())), 0.0, col);
    }
    p.text(inner.left_top(), egui::Align2::LEFT_TOP, tf!("maks. {} startów / rok", max as i64), egui::FontId::monospace(10.0), theme::paper_3());
    p.text(inner.left_bottom() + egui::vec2(0.0, 6.0), egui::Align2::LEFT_BOTTOM, data[0].0.to_string(), egui::FontId::monospace(10.0), theme::paper_3());
    p.text(inner.right_bottom() + egui::vec2(0.0, 6.0), egui::Align2::RIGHT_BOTTOM, data[data.len() - 1].0.to_string(), egui::FontId::monospace(10.0), theme::paper_3());
    let pos = resp.hover_pos()?;
    let k = (((pos.x - inner.left()) / bw) as usize).min(data.len() - 1);
    let clicked = resp.clicked();
    resp.on_hover_text(tf!("{}: {} startów – kliknij, by wybrać rok", data[k].0, data[k].1));
    clicked.then_some(data[k].0)
}
