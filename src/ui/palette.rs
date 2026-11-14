//! Paleta wyszukiwania (Ctrl+K, `/`): misje, ciała, rakiety i satelity w jednym miejscu.

use eframe::egui::{self, RichText};

use super::theme;
use crate::app::App;
use crate::astro::bodies;
use crate::i18n::tr;
use crate::missions;

#[derive(Default)]
pub struct Palette {
    pub query: String,
    pub cursor: usize,
}

enum Hit {
    Mission(usize),
    Body(usize),
    Sat(u32, String),
    Craft(usize),
}

fn score(hay: &str, q: &str) -> Option<i32> {
    let h = hay.to_lowercase();
    if h == q {
        Some(0)
    } else if h.starts_with(q) {
        Some(1)
    } else if h.split([' ', '-', '/', '(']).any(|w| w.starts_with(q)) {
        Some(2)
    } else if h.contains(q) {
        Some(3)
    } else {
        None
    }
}

fn search(app: &App, q: &str) -> Vec<(i32, Hit)> {
    let q = q.trim().to_lowercase();
    let mut out: Vec<(i32, Hit)> = Vec::new();
    if q.is_empty() {
        return out;
    }
    let db = missions::db();
    for (i, m) in db.missions.iter().enumerate() {
        let s = [score(&m.name, &q), score(&m.agency, &q).map(|s| s + 3), m.crew.iter().filter_map(|c| score(c, &q)).min().map(|s| s + 2)].into_iter().flatten().min();
        if let Some(s) = s {
            out.push((s, Hit::Mission(i)));
        }
    }
    for (i, b) in bodies::all().iter().enumerate() {
        if let Some(s) = score(&b.name_pl, &q).or_else(|| score(&b.name_en, &q)) {
            out.push((s, Hit::Body(i)));
        }
    }
    if q.len() >= 2 {
        // archiwum (bez statków powiązanych z misjami – te są wyżej jako misje)
        let arch = crate::catalog::get();
        let mut n = 0;
        for (i, c) in arch.crafts.iter().enumerate() {
            if c.m.is_some() {
                continue;
            }
            let s = if c.id.to_lowercase() == q { Some(0) } else { score(&c.name, &q).or_else(|| c.name_pl.as_deref().and_then(|p| score(p, &q))).map(|s| s + 3) };
            if let Some(s) = s {
                out.push((s, Hit::Craft(i)));
                n += 1;
                if n > 40 {
                    break;
                }
            }
        }
        let cat = app.sats.current();
        let num: Option<u32> = q.parse().ok();
        let mut n = 0;
        for o in &cat.objs {
            let s = if Some(o.norad) == num { Some(0) } else { score(&o.name, &q).map(|s| s + 4) };
            if let Some(s) = s {
                out.push((s, Hit::Sat(o.norad, o.name.clone())));
                n += 1;
                if n > 60 {
                    break;
                }
            }
        }
    }
    out.sort_by_key(|(s, _)| *s);
    out.truncate(40);
    out
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut pal) = app.palette.take() else { return };
    let hits = search(app, &pal.query);
    let mut close = false;
    let mut chosen: Option<usize> = None;
    let (down, up, enter) = ctx.input(|i| (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::Enter)));
    if down {
        pal.cursor = (pal.cursor + 1).min(hits.len().saturating_sub(1));
    }
    if up {
        pal.cursor = pal.cursor.saturating_sub(1);
    }
    let screen = ctx.content_rect();
    let w = (screen.width() * 0.4).clamp(380.0, 620.0);
    egui::Area::new(egui::Id::new("paleta")).fixed_pos([screen.center().x - w / 2.0, 70.0]).order(egui::Order::Foreground).show(ctx, |ui| {
        theme::glass().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(w);
            let r = ui.add(egui::TextEdit::singleline(&mut pal.query).hint_text(tr("Szukaj: Apollo, Voyager, Mars, ISS, Starlink, 25544…")).desired_width(f32::INFINITY).font(egui::FontId::proportional(17.0)).margin(egui::Margin::symmetric(10, 8)));
            r.request_focus();
            if r.changed() {
                pal.cursor = 0;
            }
            if enter && !hits.is_empty() {
                chosen = Some(pal.cursor);
            }
            ui.add_space(6.0);
            if pal.query.trim().is_empty() {
                theme::para(ui, "Wpisz nazwę misji, statku, planety, księżyca albo satelity (także numer NORAD). Strzałki – wybór, Enter – przejście, Esc – zamknięcie.");
            } else if hits.is_empty() {
                ui.label(RichText::new(tr("Nic nie znaleziono.")).color(theme::paper_3()));
            }
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                let db = missions::db();
                for (k, (_, h)) in hits.iter().enumerate() {
                    let (glyph, text) = match h {
                        Hit::Mission(i) => {
                            let m = &db.missions[*i];
                            (m.kind.icon(), format!("{}  ·  {}  ·  {}", m.name, &m.launch[..4], m.agency))
                        }
                        Hit::Body(i) => (theme::icon::PLANET, bodies::get(*i).name().to_string()),
                        Hit::Sat(n, name) => (theme::icon::BROADCAST, format!("{name}  ·  NORAD {n}")),
                        Hit::Craft(i) => {
                            let c = &crate::catalog::get().crafts[*i];
                            (theme::icon::ARCHIVE, format!("{}  ·  {}  ·  {}", c.name, c.id, tr("archiwum")))
                        }
                    };
                    let resp = theme::row_button(ui, glyph, &text, k == pal.cursor);
                    if k == pal.cursor && (down || up) {
                        resp.scroll_to_me(None);
                    }
                    if resp.clicked() {
                        chosen = Some(k);
                    }
                }
            });
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if let Some(k) = chosen {
        if let Some((_, h)) = hits.get(k) {
            match h {
                Hit::Mission(i) => app.select_mission(*i, false),
                Hit::Body(i) => app.select_body(*i),
                Hit::Craft(i) => {
                    app.open_craft(*i);
                    app.prefs.tab = crate::app::Tab::Missions;
                    app.prefs.archive = true;
                    app.search.clear();
                    app.prefs.catalog_open = true;
                    app.scroll_to_selected = true;
                }
                Hit::Sat(n, _) => {
                    let cat = app.sats.current();
                    if let Some(o) = cat.find(*n) {
                        if !o.alive_at(app.clock.jd) && o.launch_jd > 0.0 {
                            app.clock.jd = o.launch_jd + 1.0;
                        }
                    }
                    app.select_sat(*n);
                }
            }
        }
        close = true;
    }
    if !close {
        app.palette = Some(pal);
    }
}
