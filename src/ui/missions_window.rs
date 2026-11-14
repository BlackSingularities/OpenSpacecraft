//! Okno „Misje”: przeglądarka misji niezależna od daty symulacji – wyszukiwanie, filtry
//! (załogowe/bezzałogowe, rodzaj, stan, kraj, dekada), sortowanie i archiwum statków.
//! Dopiero wybór misji pokazuje ją w 3D; „Odtwórz” przenosi zegar na jej start.

use eframe::egui::{self, Color32, RichText};

use super::{theme, toggle_chip, DETAILS_W, TIMELINE_H, TOP_BAR_H};
use crate::app::{App, Selection};
use crate::astro::time;
use crate::i18n::tr;
use crate::missions::{self, Kind, Status};

const ROW_H: f32 = 52.0;
const FILTERS_W: f32 = 270.0;

fn c32(c: [f32; 3]) -> Color32 {
    Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

#[derive(Clone, Copy)]
enum Entry {
    Mission(usize),
    Craft(usize),
}

impl Entry {
    fn pack(self) -> usize {
        match self {
            Entry::Mission(i) => i << 1,
            Entry::Craft(i) => (i << 1) | 1,
        }
    }
    fn unpack(p: usize) -> Entry {
        if p & 1 == 0 { Entry::Mission(p >> 1) } else { Entry::Craft(p >> 1) }
    }
}

fn status_code(s: Status) -> u8 {
    match s {
        Status::Active => 0,
        Status::Completed => 1,
        Status::Failed => 2,
    }
}

fn decade(jd: f64) -> i32 {
    time::year(jd).div_euclid(10) * 10
}

/// Kraje misji z bazy, od najczęstszego.
fn countries() -> Vec<(String, usize)> {
    let mut v: Vec<(String, usize)> = Vec::new();
    for m in &missions::db().missions {
        match v.iter_mut().find(|x| x.0 == m.country) {
            Some(x) => x.1 += 1,
            None => v.push((m.country.clone(), 1)),
        }
    }
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

fn build_list(app: &App) -> Vec<Entry> {
    let f = &app.prefs.mf;
    let db = missions::db();
    let s = app.search.trim().to_lowercase();
    let terms: Vec<&str> = s.split_whitespace().collect();
    let hit = |hay: &[&str]| terms.iter().all(|t| hay.iter().any(|h| h.to_lowercase().contains(t)));
    let mut v: Vec<(f64, String, Entry)> = Vec::new();
    for (i, m) in db.missions.iter().enumerate() {
        if !app.kind_visible(m.kind)
            || (f.crew == 1 && !m.crewed())
            || (f.crew == 2 && m.crewed())
            || (!f.status.is_empty() && !f.status.contains(&status_code(m.status)))
            || (!f.countries.is_empty() && !f.countries.contains(&m.country))
            || (!f.decades.is_empty() && !f.decades.contains(&decade(m.launch_jd)))
        {
            continue;
        }
        let crew = m.crew.join(" ");
        let country = m.country_name();
        let veh = m.vehicle().map(|v| v.name.as_str()).unwrap_or("");
        if !hit(&[m.name.as_str(), m.agency.as_str(), country.as_str(), &m.launch[..4], crew.as_str(), veh, m.summary()]) {
            continue;
        }
        v.push((m.launch_jd, m.name.to_lowercase(), Entry::Mission(i)));
    }
    // archiwum (bez trasy 3D) – tylko gdy filtry da się do niego zastosować
    if app.prefs.archive && app.prefs.kinds.is_empty() && f.status.is_empty() {
        let arch = crate::catalog::get();
        for (i, c) in arch.crafts.iter().enumerate() {
            if c.parent.is_some() || arch.mission_of_cospar(&c.id).is_some() {
                continue;
            }
            let crewed = c.types.iter().any(|t| t == "human spaceflight" || t.contains("crewed"));
            if (f.crew == 1 && !crewed) || (f.crew == 2 && crewed) {
                continue;
            }
            if !f.countries.is_empty() && !c.cc.iter().any(|x| f.countries.contains(x)) {
                continue;
            }
            if !f.decades.is_empty() && !c.jd.is_some_and(|jd| f.decades.contains(&decade(jd))) {
                continue;
            }
            if terms.iter().all(|t| c.search.contains(t) || c.t0.as_deref().is_some_and(|d| d.starts_with(t))) {
                v.push((c.jd.unwrap_or(f64::MAX), arch.craft_name(c).to_lowercase(), Entry::Craft(i)));
            }
        }
    }
    match f.sort {
        1 => v.sort_by(|a, b| b.0.total_cmp(&a.0)),
        2 => v.sort_by(|a, b| a.1.cmp(&b.1)),
        _ => v.sort_by(|a, b| a.0.total_cmp(&b.0)),
    }
    v.into_iter().map(|x| x.2).collect()
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.missions_open {
        return;
    }
    let screen = ctx.content_rect();
    let right = if super::details::open(app) { DETAILS_W } else { 0.0 };
    let x0 = screen.left() + 12.0;
    let x1 = screen.right() - 12.0 - right;
    let w = (x1 - x0).clamp(560.0, 1180.0);
    let h = (screen.height() - TOP_BAR_H - TIMELINE_H - 22.0).max(320.0);
    let rect = egui::Rect::from_min_size(egui::pos2(x0 + ((x1 - x0) - w).max(0.0) / 2.0, screen.top() + TOP_BAR_H + 10.0), egui::vec2(w, h));
    app.glass.push((rect, 1.0));
    egui::Area::new(egui::Id::new("okno-misje")).fixed_pos(rect.min).order(egui::Order::Foreground).show(ctx, |ui| {
        egui::Frame::new()
            .fill(theme::POPUP_FILL)
            .corner_radius(16)
            .stroke(egui::Stroke::new(1.0_f32, theme::glass_edge()))
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(w - 32.0);
                ui.set_height(h - 32.0);
                header(app, ui);
                ui.add_space(4.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(FILTERS_W);
                        egui::ScrollArea::vertical().id_salt("filtry-misji").auto_shrink([false, false]).max_height(h - 120.0).show(ui, |ui| {
                            ui.set_max_width(FILTERS_W - 10.0);
                            filters(app, ui);
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        list(app, ui, h - 120.0);
                    });
                });
            });
    });
}

fn header(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(theme::icon::ROCKET_LAUNCH).size(22.0).color(theme::BRASS));
        ui.vertical(|ui| {
            ui.label(RichText::new(tr("Misje")).size(20.0).strong());
            ui.label(RichText::new(tr("Przeglądanie i filtry nie zmieniają daty. Kliknij misję, by ją pokazać; „Odtwórz” przenosi zegar na jej start.")).small().color(theme::paper_3()));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(egui::Button::new(RichText::new(theme::icon::X).size(16.0)).frame(false)).on_hover_text(tr("Zamknij (Esc)")).clicked() {
                app.missions_open = false;
            }
        });
    });
    ui.add_space(4.0);
    ui.add(egui::TextEdit::singleline(&mut app.search).hint_text(format!("{}  {}", theme::icon::MAGNIFYING_GLASS, tr("Nazwa, agencja, kraj, rok, rakieta, załoga…"))).desired_width(f32::INFINITY).margin(egui::Margin::symmetric(10, 8)));
}

fn filters(app: &mut App, ui: &mut egui::Ui) {
    let chips = |ui: &mut egui::Ui, body: &mut dyn FnMut(&mut egui::Ui)| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            body(ui);
        });
    };
    let db = missions::db();
    // załoga
    super::section(ui, "Załoga");
    let crewed = db.missions.iter().filter(|m| m.crewed()).count();
    chips(ui, &mut |ui| {
        for (code, label, n) in [(0u8, tr("wszystkie").to_string(), db.missions.len()), (1, tr("załogowe").to_string(), crewed), (2, tr("bezzałogowe").to_string(), db.missions.len() - crewed)] {
            let glyph = match code {
                1 => theme::icon::USERS,
                2 => theme::icon::BROADCAST,
                _ => theme::icon::ASTERISK,
            };
            if toggle_chip(ui, &format!("{label} · {n}"), Some(glyph), app.prefs.mf.crew == code, theme::BRASS).clicked() {
                app.prefs.mf.crew = code;
            }
        }
    });
    // rodzaj
    super::section(ui, "Rodzaj");
    chips(ui, &mut |ui| {
        for k in Kind::ALL {
            let on = app.prefs.kinds.contains(&k);
            if toggle_chip(ui, k.label(), Some(k.icon()), on, c32(k.color())).clicked() {
                if on {
                    app.prefs.kinds.retain(|x| *x != k);
                } else {
                    app.prefs.kinds.push(k);
                }
            }
        }
    });
    // stan
    super::section(ui, "Stan");
    chips(ui, &mut |ui| {
        for (st, color) in [(Status::Active, theme::OK), (Status::Completed, theme::paper_2()), (Status::Failed, theme::SIGNAL)] {
            let code = status_code(st);
            let on = app.prefs.mf.status.contains(&code);
            if toggle_chip(ui, st.label(), None, on, color).clicked() {
                if on {
                    app.prefs.mf.status.retain(|x| *x != code);
                } else {
                    app.prefs.mf.status.push(code);
                }
            }
        }
    });
    // kraj
    super::section(ui, "Kraj");
    chips(ui, &mut |ui| {
        for (c, n) in countries() {
            let on = app.prefs.mf.countries.contains(&c);
            if toggle_chip(ui, &format!("{} · {}", missions::country_name(&c), n), None, on, theme::TEAL).clicked() {
                if on {
                    app.prefs.mf.countries.retain(|x| *x != c);
                } else {
                    app.prefs.mf.countries.push(c);
                }
            }
        }
    });
    // dekada
    super::section(ui, "Dekada startu");
    chips(ui, &mut |ui| {
        let last = decade(time::now());
        let mut d = 1950;
        while d <= last {
            let on = app.prefs.mf.decades.contains(&d);
            if toggle_chip(ui, &tf!("{}–{}", d, d + 9), None, on, theme::WARN).clicked() {
                if on {
                    app.prefs.mf.decades.retain(|x| *x != d);
                } else {
                    app.prefs.mf.decades.push(d);
                }
            }
            d += 10;
        }
    });
    // źródło
    super::section(ui, "Zakres");
    theme::switch(ui, &mut app.prefs.archive, "Także statki z archiwum (bez trasy 3D)");
    ui.add_space(8.0);
    let any = app.prefs.mf != Default::default() || !app.prefs.kinds.is_empty() || !app.search.is_empty();
    if ui.add_enabled(any, egui::Button::new(theme::ic(theme::icon::FUNNEL_X, "Wyczyść filtry"))).clicked() {
        let sort = app.prefs.mf.sort;
        app.prefs.mf = Default::default();
        app.prefs.mf.sort = sort;
        app.prefs.kinds.clear();
        app.search.clear();
    }
    ui.add_space(8.0);
    egui::CollapsingHeader::new(RichText::new(tr("Źródła danych i licencje")).small()).id_salt("arch_src_okno").show(ui, super::archive::sources_and_licenses);
}

fn list(app: &mut App, ui: &mut egui::Ui, height: f32) {
    let key = format!("{:?}|{}|{:?}|{}", app.prefs.mf, app.search.to_lowercase(), app.prefs.kinds, app.prefs.archive);
    if app.archive_list.as_ref().is_none_or(|(k, _)| *k != key) {
        let v = build_list(app);
        app.archive_list = Some((key, v.into_iter().map(|e| e.pack()).collect()));
    }
    let list: Vec<Entry> = app.archive_list.as_ref().map(|x| x.1.iter().map(|&p| Entry::unpack(p)).collect()).unwrap_or_default();
    let n3d = list.iter().filter(|e| matches!(e, Entry::Mission(_))).count();
    ui.horizontal(|ui| {
        ui.label(RichText::new(tf!("{} misji z przebiegiem 3D", n3d)).strong());
        if list.len() > n3d {
            ui.label(RichText::new(tf!("· {} w archiwum", super::group((list.len() - n3d) as i64))).color(theme::paper_3()));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (code, label) in [(2u8, "nazwa"), (1, "najnowsze"), (0, "najstarsze")] {
                if ui.selectable_label(app.prefs.mf.sort == code, tr(label)).clicked() {
                    app.prefs.mf.sort = code;
                }
            }
            ui.label(RichText::new(theme::icon::SORT_ASCENDING).color(theme::paper_3()));
        });
    });
    ui.separator();
    let scroll = std::mem::take(&mut app.scroll_to_selected);
    let sel_row = list.iter().position(|e| match (e, app.selection) {
        (Entry::Mission(i), Selection::Mission(j)) => *i == j,
        (Entry::Craft(i), Selection::Craft(j)) => *i == j,
        _ => false,
    });
    let mut area = egui::ScrollArea::vertical().id_salt("lista-misji").auto_shrink([false, false]).max_height(height - 30.0);
    if let (true, Some(r)) = (scroll, sel_row) {
        area = area.vertical_scroll_offset((r as f32 * (ROW_H + ui.spacing().item_spacing.y) - 160.0).max(0.0));
    }
    if list.is_empty() {
        ui.add_space(20.0);
        ui.label(RichText::new(tr("Żadna misja nie pasuje do filtrów.")).color(theme::paper_3()));
        return;
    }
    let width = ui.available_width();
    area.show_rows(ui, ROW_H, list.len(), |ui, range| {
        ui.set_max_width(width);
        for r in range {
            match list[r] {
                Entry::Mission(i) => mission_row(app, ui, i),
                Entry::Craft(i) => {
                    let c = &crate::catalog::get().crafts[i];
                    let sel = app.selection == Selection::Craft(i) || matches!(app.selection, Selection::Sat(n) if Some(n) == c.norad);
                    let resp = ui.allocate_ui(egui::vec2(width, ROW_H), |ui| super::archive::craft_row(ui, c, sel)).inner;
                    if resp.clicked() {
                        app.open_craft(i);
                    }
                }
            }
        }
    });
}

fn mission_row(app: &mut App, ui: &mut egui::Ui, i: usize) {
    let m = &missions::db().missions[i];
    let sel = app.selection == Selection::Mission(i);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_H), egui::Sense::click());
    theme::focus_ring(ui, &resp, rect, 10.0);
    // przycisk „Odtwórz” po prawej
    let play_rect = egui::Rect::from_center_size(rect.right_center() - egui::vec2(60.0, 0.0), egui::vec2(104.0, 30.0));
    let play = ui.interact(play_rect, resp.id.with("odtworz"), egui::Sense::click());
    let p = ui.painter();
    let bg = if sel { theme::BRASS_DIM } else if resp.hovered() { Color32::from_white_alpha(16) } else { Color32::TRANSPARENT };
    p.rect_filled(rect, 10.0, bg);
    let kc = c32(m.kind.color());
    p.circle_filled(rect.left_center() + egui::vec2(20.0, 0.0), 15.0, kc.gamma_multiply(0.22));
    p.text(rect.left_center() + egui::vec2(20.0, 0.0), egui::Align2::CENTER_CENTER, m.kind.icon(), egui::FontId::proportional(16.0), kc);
    let clip = egui::Rect::from_min_max(rect.min + egui::vec2(44.0, 0.0), egui::pos2(play_rect.left() - 8.0, rect.bottom()));
    let pc = ui.painter_at(clip);
    let mut title = m.name.clone();
    if m.crewed() {
        title = format!("{title}  {}", theme::icon::USERS);
    }
    pc.text(rect.left_top() + egui::vec2(44.0, 6.0), egui::Align2::LEFT_TOP, title, egui::FontId::proportional(15.0), theme::PAPER);
    let mut sub = format!("{} · {} · {} · {}", &m.launch[..10], m.agency, m.country_name(), m.kind.label());
    if !m.crew.is_empty() {
        sub += &format!(" · {}", m.crew.join(", "));
    }
    pc.text(rect.left_top() + egui::vec2(44.0, 26.0), egui::Align2::LEFT_TOP, sub, egui::FontId::proportional(11.5), theme::paper_3());
    let meta = tf!("{} etapów · {} części", m.events.len(), m.parts.len());
    pc.text(rect.left_bottom() + egui::vec2(44.0, -4.0), egui::Align2::LEFT_BOTTOM, meta, egui::FontId::proportional(10.5), theme::paper_3());
    // stan
    let dot = match m.status {
        Status::Active => Some(theme::OK),
        Status::Failed => Some(theme::SIGNAL),
        Status::Completed => None,
    };
    if let Some(c) = dot {
        p.circle_filled(egui::pos2(play_rect.left() - 4.0, rect.top() + 12.0), 4.0, c);
    }
    let p = ui.painter();
    p.rect_filled(play_rect, 8.0, if play.hovered() { theme::BRASS } else { theme::BRASS_DIM });
    p.text(play_rect.center(), egui::Align2::CENTER_CENTER, format!("{}  {}", theme::icon::PLAY, tr("Odtwórz")), egui::FontId::proportional(13.0), if play.hovered() { theme::INK_0 } else { theme::PAPER });
    if play.on_hover_text(tr("Przenieś zegar na start i odtwórz misję etap po etapie")).clicked() {
        app.select_mission(i, true);
        app.missions_open = false;
    } else if resp.double_clicked() {
        app.select_mission(i, true);
        app.missions_open = false;
    } else if resp.clicked() {
        app.select_mission(i, false);
    }
}
