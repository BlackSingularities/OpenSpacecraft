//! Widoki okna „Misje” poza listą: oś czasu (misje jako odcinki na jednej linii lat),
//! statystyki przefiltrowanych misji i porównanie dwóch misji obok siebie.

use std::collections::HashMap;
use std::sync::Mutex;

use eframe::egui::{self, Color32, RichText, Stroke};

use super::theme;
use crate::app::{App, Selection};
use crate::astro::bodies::{self, EARTH};
use crate::astro::time;
use crate::i18n::tr;
use crate::missions::{self, Kind, Mission, Status};

fn c32(c: [f32; 3]) -> Color32 {
    Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

/// Koniec odcinka misji na osi czasu (trwające – dziś).
fn end_of(m: &Mission) -> f64 {
    match m.end_jd {
        Some(e) => e,
        None if m.status == Status::Active => time::now(),
        None => m.launch_jd + 1.0,
    }
}

const NAME_W: f32 = 170.0;
const ROW: f32 = 17.0;

/// Oś czasu: każda misja to odcinek od startu do końca; pionowa linia – chwila symulacji.
pub fn timeline(app: &mut App, ui: &mut egui::Ui, list: &[usize], height: f32) {
    let db = missions::db();
    if list.is_empty() {
        return;
    }
    let mut rows: Vec<usize> = list.to_vec();
    rows.sort_by(|a, b| db.missions[*a].launch_jd.total_cmp(&db.missions[*b].launch_jd));
    let y0 = time::year(rows.iter().map(|&i| db.missions[i].launch_jd).fold(f64::MAX, f64::min)).div_euclid(5) * 5;
    let y1 = time::year(rows.iter().map(|&i| end_of(&db.missions[i])).fold(time::now(), f64::max)) + 1;
    let (j0, j1) = (time::year_start(y0), time::year_start(y1));
    let jd = app.clock.jd;
    let width = ui.available_width();
    let chart_w = width - NAME_W - 8.0;
    let x_of = |left: f32, t: f64| left + NAME_W + ((t - j0) / (j1 - j0)) as f32 * chart_w;
    // podziałka lat
    let (axis, _) = ui.allocate_exact_size(egui::vec2(width, 20.0), egui::Sense::hover());
    let p = ui.painter();
    let step = if y1 - y0 > 40 { 10 } else { 5 };
    let mut y = y0;
    while y <= y1 {
        let x = x_of(axis.left(), time::year_start(y));
        p.text(egui::pos2(x, axis.center().y), egui::Align2::CENTER_CENTER, y.to_string(), egui::FontId::monospace(10.5), theme::paper_3());
        y += step;
    }
    ui.label(RichText::new(tr("Odcinek od startu do końca misji. Kliknij – pokaż, dwuklik – odtwórz. Złota linia to wybrana data.")).small().color(theme::paper_3()));
    let total = rows.len();
    egui::ScrollArea::vertical().id_salt("os-czasu-misji").auto_shrink([false, false]).max_height(height - 60.0).show_rows(ui, ROW, total, |ui, range| {
        ui.set_max_width(width);
        for r in range {
            let i = rows[r];
            let m = &db.missions[i];
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, ROW), egui::Sense::click());
            let p = ui.painter();
            let sel = app.selection == Selection::Mission(i);
            if sel {
                p.rect_filled(rect, 4.0, theme::BRASS_DIM);
            } else if resp.hovered() {
                p.rect_filled(rect, 4.0, Color32::from_white_alpha(14));
            } else if r % 2 == 0 {
                p.rect_filled(rect, 0.0, Color32::from_white_alpha(4));
            }
            // siatka lat
            let mut y = y0;
            while y <= y1 {
                let x = x_of(rect.left(), time::year_start(y));
                p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], Stroke::new(0.5_f32, Color32::from_white_alpha(14)));
                y += step;
            }
            let name_clip = ui.painter_at(egui::Rect::from_min_size(rect.min, egui::vec2(NAME_W, ROW)));
            name_clip.text(rect.left_center() + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, &m.name, egui::FontId::proportional(12.0), if sel { theme::PAPER } else { theme::paper_2() });
            let kc = c32(m.kind.color());
            let live = m.live_at(jd);
            let (a, b) = (x_of(rect.left(), m.launch_jd), x_of(rect.left(), end_of(m)));
            let bar = egui::Rect::from_min_max(egui::pos2(a, rect.top() + 4.0), egui::pos2(b.max(a + 3.0), rect.bottom() - 4.0));
            p.rect_filled(bar, 3.0, if live { kc } else { kc.gamma_multiply(0.62) });
            if m.status == Status::Failed {
                p.rect_stroke(bar, 3.0, Stroke::new(1.0_f32, theme::SIGNAL), egui::StrokeKind::Outside);
            }
            if m.crewed() {
                p.circle_filled(egui::pos2(a - 5.0, rect.center().y), 2.5, theme::PAPER);
            }
            let resp = resp.on_hover_ui(|ui| {
                ui.label(RichText::new(&m.name).strong());
                ui.label(format!("{} – {}", time::fmt_date(m.launch_jd), m.end_jd.map(time::fmt_date).unwrap_or_else(|| m.status.label().to_string())));
                ui.label(time::fmt_duration(end_of(m) - m.launch_jd));
            });
            if resp.double_clicked() {
                app.select_mission(i, true);
                app.missions_open = false;
            } else if resp.clicked() {
                app.select_mission(i, false);
            }
        }
    });
    // chwila symulacji
    let x = x_of(ui.min_rect().left(), jd);
    let top = axis.bottom();
    let bottom = ui.min_rect().bottom();
    if jd >= j0 && jd <= j1 {
        ui.painter().line_segment([egui::pos2(x, top), egui::pos2(x, bottom)], Stroke::new(1.5_f32, theme::BRASS));
    }
}

/// Słupek poziomy z podpisem i liczbą.
fn hbar(ui: &mut egui::Ui, label: &str, parts: &[(f32, Color32)], max: f32, value: &str) {
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 16.0], egui::Label::new(RichText::new(label).small()).truncate());
        let w = (ui.available_width() - 60.0).max(40.0);
        let (r, _) = ui.allocate_exact_size(egui::vec2(w, 11.0), egui::Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, 3.0, Color32::from_white_alpha(8));
        let mut x = r.left();
        for (v, c) in parts {
            let dw = w * v / max.max(1e-6);
            if dw > 0.0 {
                p.rect_filled(egui::Rect::from_min_size(egui::pos2(x, r.top()), egui::vec2(dw, r.height())), 2.0, *c);
                x += dw;
            }
        }
        ui.label(RichText::new(value).monospace().small());
    });
}

fn tile(ui: &mut egui::Ui, w: f32, k: &str, v: String) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 54.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 10.0, Color32::from_white_alpha(9));
    p.text(rect.left_top() + egui::vec2(10.0, 8.0), egui::Align2::LEFT_TOP, tr(k), egui::FontId::proportional(11.0), theme::paper_3());
    p.text(rect.left_bottom() + egui::vec2(10.0, -8.0), egui::Align2::LEFT_BOTTOM, v, egui::FontId::proportional(19.0), theme::PAPER);
}

/// Statystyki misji spełniających filtry.
pub fn stats(ui: &mut egui::Ui, list: &[usize], height: f32) {
    let db = missions::db();
    let ms: Vec<&Mission> = list.iter().map(|&i| &db.missions[i]).collect();
    if ms.is_empty() {
        return;
    }
    egui::ScrollArea::vertical().id_salt("statystyki-misji").auto_shrink([false, false]).max_height(height - 30.0).show(ui, |ui| {
        let crewed = ms.iter().filter(|m| m.crewed()).count();
        let failed = ms.iter().filter(|m| m.status == Status::Failed).count();
        let active = ms.iter().filter(|m| m.status == Status::Active).count();
        let mut people: Vec<&str> = ms.iter().flat_map(|m| m.crew.iter().map(|s| s.as_str())).collect();
        people.sort_unstable();
        people.dedup();
        let mass: f64 = ms.iter().filter_map(|m| m.mass_kg).sum();
        let longest = ms.iter().max_by(|a, b| (end_of(a) - a.launch_jd).total_cmp(&(end_of(b) - b.launch_jd)));
        let gap = 8.0;
        let w = (ui.available_width() - gap * 3.0) / 4.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            tile(ui, w, "Misje", ms.len().to_string());
            tile(ui, w, "Załogowe", format!("{crewed}  ·  {}", tf!("{} osób", people.len())));
            tile(ui, w, "Trwające", active.to_string());
            tile(ui, w, "Udane", format!("{:.0}%", (ms.len() - failed) as f64 / ms.len() as f64 * 100.0));
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            tile(ui, w, "Wyniesiona masa statków", super::fmt_mass(mass));
            if let Some(l) = longest {
                tile(ui, w * 2.0 + gap, "Najdłuższa misja", format!("{} · {}", l.name, time::fmt_duration(end_of(l) - l.launch_jd)));
            }
            let first = ms.iter().min_by(|a, b| a.launch_jd.total_cmp(&b.launch_jd)).map(|m| time::year(m.launch_jd)).unwrap_or(0);
            let last = ms.iter().max_by(|a, b| a.launch_jd.total_cmp(&b.launch_jd)).map(|m| time::year(m.launch_jd)).unwrap_or(0);
            tile(ui, w, "Lata startów", format!("{first}–{last}"));
        });
        // dekady: słupki skumulowane według rodzaju
        super::section(ui, "Starty w dekadach (według rodzaju)");
        let mut dec: Vec<(i32, Vec<usize>)> = Vec::new();
        for m in &ms {
            let d = time::year(m.launch_jd).div_euclid(10) * 10;
            let k = Kind::ALL.iter().position(|x| *x == m.kind).unwrap_or(0);
            match dec.iter_mut().find(|x| x.0 == d) {
                Some(x) => x.1[k] += 1,
                None => {
                    let mut v = vec![0; Kind::ALL.len()];
                    v[k] = 1;
                    dec.push((d, v));
                }
            }
        }
        dec.sort_by_key(|x| x.0);
        let max = dec.iter().map(|x| x.1.iter().sum::<usize>()).max().unwrap_or(1) as f32;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 150.0), egui::Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 8.0, Color32::from_white_alpha(5));
        let inner = rect.shrink2(egui::vec2(10.0, 22.0));
        let bw = inner.width() / dec.len().max(1) as f32;
        for (k, (d, counts)) in dec.iter().enumerate() {
            let x0 = inner.left() + k as f32 * bw + bw * 0.18;
            let x1 = inner.left() + (k + 1) as f32 * bw - bw * 0.18;
            let mut base = inner.bottom();
            for (ki, c) in counts.iter().enumerate() {
                let h = *c as f32 / max * inner.height();
                if h > 0.0 {
                    p.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, base - h), egui::pos2(x1, base)), 2.0, c32(Kind::ALL[ki].color()).gamma_multiply(0.85));
                    base -= h;
                }
            }
            let n: usize = counts.iter().sum();
            p.text(egui::pos2((x0 + x1) / 2.0, base - 3.0), egui::Align2::CENTER_BOTTOM, n.to_string(), egui::FontId::monospace(10.5), theme::paper_2());
            p.text(egui::pos2((x0 + x1) / 2.0, rect.bottom() - 4.0), egui::Align2::CENTER_BOTTOM, format!("{d}s"), egui::FontId::monospace(10.5), theme::paper_3());
        }
        ui.horizontal_wrapped(|ui| {
            for k in Kind::ALL {
                if ms.iter().any(|m| m.kind == k) {
                    ui.label(RichText::new("●").color(c32(k.color())));
                    ui.label(RichText::new(k.label()).small().color(theme::paper_2()));
                }
            }
        });
        // kraje: udane / nieudane / trwające
        super::section(ui, "Kraje i agencje");
        let mut cc: Vec<(String, [usize; 3])> = Vec::new();
        for m in &ms {
            let s = match m.status {
                Status::Completed => 0,
                Status::Active => 1,
                Status::Failed => 2,
            };
            match cc.iter_mut().find(|x| x.0 == m.country) {
                Some(x) => x.1[s] += 1,
                None => {
                    let mut v = [0; 3];
                    v[s] = 1;
                    cc.push((m.country.clone(), v));
                }
            }
        }
        cc.sort_by(|a, b| b.1.iter().sum::<usize>().cmp(&a.1.iter().sum::<usize>()));
        let max = cc.first().map(|x| x.1.iter().sum::<usize>()).unwrap_or(1) as f32;
        for (c, v) in &cc {
            let total: usize = v.iter().sum();
            hbar(ui, &missions::country_name(c), &[(v[0] as f32, theme::BRASS.gamma_multiply(0.85)), (v[1] as f32, theme::OK), (v[2] as f32, theme::SIGNAL)], max, &total.to_string());
        }
        ui.horizontal(|ui| {
            for (col, t) in [(theme::BRASS, "zakończone"), (theme::OK, "trwające"), (theme::SIGNAL, "niepowodzenie")] {
                ui.label(RichText::new("●").color(col));
                ui.label(RichText::new(tr(t)).small().color(theme::paper_2()));
            }
        });
        // rakiety
        super::section(ui, "Najczęstsze rakiety");
        let mut veh: Vec<(String, usize)> = Vec::new();
        for m in &ms {
            if let Some(v) = m.vehicle() {
                match veh.iter_mut().find(|x| x.0 == v.name) {
                    Some(x) => x.1 += 1,
                    None => veh.push((v.name.clone(), 1)),
                }
            }
        }
        veh.sort_by(|a, b| b.1.cmp(&a.1));
        let max = veh.first().map(|x| x.1).unwrap_or(1) as f32;
        for (n, c) in veh.iter().take(10) {
            hbar(ui, n, &[(*c as f32, theme::paper_2().gamma_multiply(0.7))], max, &c.to_string());
        }
    });
}

/// Największa odległość od Ziemi na trasie misji [m] (z policzonej trajektorii; pamiętana).
fn max_distance(app: &App, i: usize) -> Option<f64> {
    static CACHE: Mutex<Option<HashMap<usize, f64>>> = Mutex::new(None);
    if let Some(v) = CACHE.lock().ok()?.as_ref().and_then(|c| c.get(&i).copied()) {
        return Some(v);
    }
    let t = app.trajs.get(i)?;
    let mut best = 0.0f64;
    for s in &t.strips {
        let stride = (s.t.len() / 400).max(1);
        for k in (0..s.t.len()).step_by(stride) {
            let d = if t.frame == EARTH { s.p[k].length() } else { (s.p[k] + bodies::position(t.frame, s.t[k]) - bodies::position(EARTH, s.t[k])).length() };
            best = best.max(d);
        }
    }
    let mut g = CACHE.lock().ok()?;
    g.get_or_insert_with(HashMap::new).insert(i, best);
    Some(best)
}

fn pick(ui: &mut egui::Ui, id: &str, cur: &mut String, list: &[usize]) {
    let db = missions::db();
    let name = missions::index_of(cur).map(|i| db.missions[i].name.clone()).unwrap_or_else(|| tr("wybierz misję").to_string());
    egui::ComboBox::from_id_salt(id).selected_text(name).width(ui.available_width() - 8.0).height(420.0).show_ui(ui, |ui| {
        for &i in list {
            let m = &db.missions[i];
            if ui.selectable_label(*cur == m.id, format!("{}  ·  {}", m.name, &m.launch[..4])).clicked() {
                *cur = m.id.clone();
            }
        }
    });
}

/// Porównanie dwóch misji obok siebie.
pub fn compare(app: &mut App, ui: &mut egui::Ui, list: &[usize], height: f32) {
    let db = missions::db();
    let all: Vec<usize> = if list.len() >= 2 { list.to_vec() } else { (0..db.missions.len()).collect() };
    while app.prefs.mf.cmp.len() < 2 {
        let fallback = ["apollo-11", "artemis-1"][app.prefs.mf.cmp.len()];
        app.prefs.mf.cmp.push(fallback.to_string());
    }
    for k in 0..2 {
        let i = all.get(k).copied().unwrap_or(0);
        if missions::index_of(&app.prefs.mf.cmp[k]).is_none() {
            app.prefs.mf.cmp[k] = db.missions[i].id.clone();
        }
    }
    let colw = (ui.available_width() - 160.0) / 2.0;
    ui.horizontal(|ui| {
        ui.add_space(152.0);
        for k in 0..2 {
            ui.allocate_ui(egui::vec2(colw, 30.0), |ui| {
                let mut cur = app.prefs.mf.cmp[k].clone();
                pick(ui, &format!("porownanie-{k}"), &mut cur, &all);
                app.prefs.mf.cmp[k] = cur;
            });
        }
    });
    let ids: Vec<usize> = app.prefs.mf.cmp.iter().take(2).filter_map(|s| missions::index_of(s)).collect();
    if ids.len() < 2 {
        return;
    }
    for &i in &ids {
        app.trajs.request(i, ui.ctx());
    }
    let (a, b) = (&db.missions[ids[0]], &db.missions[ids[1]]);
    let dist = [max_distance(app, ids[0]), max_distance(app, ids[1])];
    egui::ScrollArea::vertical().id_salt("porownanie-misji").auto_shrink([false, false]).max_height(height - 50.0).show(ui, |ui| {
        let text_row = |ui: &mut egui::Ui, k: &str, va: String, vb: String| {
            ui.horizontal_top(|ui| {
                ui.add_sized([150.0, 18.0], egui::Label::new(RichText::new(tr(k)).color(theme::paper_3())));
                for v in [va, vb] {
                    ui.allocate_ui(egui::vec2(colw, 18.0), |ui| {
                        ui.set_width(colw);
                        ui.add(egui::Label::new(v).wrap());
                    });
                }
            });
        };
        // wiersz z liczbami: słupki względem większej wartości
        let num_row = |ui: &mut egui::Ui, k: &str, va: Option<f64>, vb: Option<f64>, fmt: &dyn Fn(f64) -> String| {
            let max = va.unwrap_or(0.0).max(vb.unwrap_or(0.0)).max(1e-9);
            ui.horizontal(|ui| {
                ui.add_sized([150.0, 30.0], egui::Label::new(RichText::new(tr(k)).color(theme::paper_3())));
                for (v, m) in [(va, a), (vb, b)] {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(colw, 30.0), egui::Sense::hover());
                    let p = ui.painter();
                    match v {
                        Some(x) => {
                            let w = (r.width() - 6.0) * (x / max) as f32;
                            p.rect_filled(egui::Rect::from_min_size(egui::pos2(r.left(), r.bottom() - 8.0), egui::vec2(w.max(2.0), 6.0)), 3.0, c32(m.kind.color()).gamma_multiply(0.8));
                            p.text(r.left_top() + egui::vec2(0.0, 2.0), egui::Align2::LEFT_TOP, fmt(x), egui::FontId::monospace(13.0), theme::PAPER);
                        }
                        None => {
                            p.text(r.left_top() + egui::vec2(0.0, 2.0), egui::Align2::LEFT_TOP, "—", egui::FontId::monospace(13.0), theme::paper_3());
                        }
                    }
                }
            });
        };
        ui.horizontal(|ui| {
            ui.add_space(152.0);
            for m in [a, b] {
                ui.allocate_ui(egui::vec2(colw, 46.0), |ui| {
                    ui.set_width(colw);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(m.kind.icon()).size(22.0).color(c32(m.kind.color())));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&m.name).size(17.0).strong());
                            ui.label(RichText::new(format!("{} · {}", m.kind.label(), m.status.label())).small().color(theme::paper_3()));
                        });
                    });
                });
            }
        });
        ui.separator();
        text_row(ui, "Agencja", a.agency.clone(), b.agency.clone());
        text_row(ui, "Kraj", a.country_name(), b.country_name());
        text_row(ui, "Start", time::fmt_datetime(a.launch_jd), time::fmt_datetime(b.launch_jd));
        let end = |m: &Mission| m.end_jd.map(time::fmt_datetime).unwrap_or_else(|| m.status.label().to_string());
        text_row(ui, "Koniec", end(a), end(b));
        let veh = |m: &Mission| m.vehicle().map(|v| v.name.clone()).unwrap_or_else(|| "—".into());
        text_row(ui, "Rakieta nośna", veh(a), veh(b));
        let site = |m: &Mission| m.site().map(|s| s.name.clone()).unwrap_or_else(|| "—".into());
        text_row(ui, "Miejsce startu", site(a), site(b));
        let crew = |m: &Mission| if m.crew.is_empty() { tr("bez załogi").to_string() } else { m.crew.join(", ") };
        text_row(ui, "Załoga", crew(a), crew(b));
        ui.add_space(4.0);
        num_row(ui, "Czas trwania", Some(end_of(a) - a.launch_jd), Some(end_of(b) - b.launch_jd), &|x| time::fmt_duration(x));
        num_row(ui, "Masa statku", a.mass_kg, b.mass_kg, &|x| super::fmt_mass(x));
        num_row(ui, "Wysokość rakiety", a.vehicle().map(|v| v.height_m), b.vehicle().map(|v| v.height_m), &|x| format!("{x:.0} m"));
        num_row(ui, "Masa rakiety", a.vehicle().map(|v| v.mass_t), b.vehicle().map(|v| v.mass_t), &|x| format!("{} t", super::group(x.round() as i64)));
        num_row(ui, "Najdalej od Ziemi", dist[0], dist[1], &|x| if x > 0.05 * crate::astro::AU { format!("{:.2} AU", x / crate::astro::AU) } else { super::fmt_km(x / 1000.0) });
        num_row(ui, "Etapy w przebiegu", Some(a.events.len() as f64), Some(b.events.len() as f64), &|x| format!("{x:.0}"));
        num_row(ui, "Części zestawu", Some(a.parts.len() as f64), Some(b.parts.len() as f64), &|x| format!("{x:.0}"));
        num_row(ui, "Załoga (osoby)", Some(a.crew.len() as f64), Some(b.crew.len() as f64), &|x| format!("{x:.0}"));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(152.0);
            for &i in &ids {
                ui.allocate_ui(egui::vec2(colw, 30.0), |ui| {
                    if ui.add(theme::primary(&theme::ic(theme::icon::PLAY, "Odtwórz"))).clicked() {
                        app.select_mission(i, true);
                        app.missions_open = false;
                    }
                });
            }
        });
        if dist.iter().any(|d| d.is_none()) {
            ui.label(RichText::new(tr("Odległości liczą się z trajektorii – chwilę to potrwa.")).small().color(theme::paper_3()));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        }
    });
}
