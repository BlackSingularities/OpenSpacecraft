//! Lewy panel: katalog misji, satelitów, startów i ciał niebieskich.

use eframe::egui::{self, Color32, RichText};

use super::{section, theme, toggle_chip, CATALOG_W, TIMELINE_H, TOP_BAR_H};
use crate::app::{App, Selection, Tab};
use crate::astro::bodies::{self, Kind as BKind};
use crate::astro::time;
use crate::i18n::tr;
use crate::missions::{self, Kind, Status};
use crate::sats;

fn c32(c: [f32; 3]) -> Color32 {
    Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

pub fn catalog(app: &mut App, ctx: &egui::Context) {
    if !app.prefs.catalog_open {
        return;
    }
    let screen = ctx.content_rect();
    let height = (screen.height() - TOP_BAR_H - TIMELINE_H).max(200.0);
    app.glass.push((egui::Rect::from_min_size(egui::pos2(screen.left(), screen.top() + TOP_BAR_H), egui::vec2(CATALOG_W, height)), 1.0));
    egui::Area::new(egui::Id::new("katalog")).fixed_pos([screen.left(), screen.top() + TOP_BAR_H]).order(egui::Order::Middle).show(ctx, |ui| {
        let gutter = theme::SCROLL_GUTTER;
        egui::Frame::new().fill(theme::glass_fill()).inner_margin(egui::Margin { left: 14, right: (14.0 - gutter) as i8, top: 12, bottom: 12 }).show(ui, |ui| {
            ui.set_width(CATALOG_W - 28.0 + gutter);
            ui.set_height(height - 24.0);
            // zakładki
            ui.horizontal(|ui| {
                let w = (CATALOG_W - 28.0 - 3.0 * 6.0) / 4.0;
                ui.spacing_mut().item_spacing.x = 6.0;
                for (tab, glyph, label) in [
                    (Tab::Missions, theme::icon::ROCKET_LAUNCH, "Misje"),
                    (Tab::Satellites, theme::icon::GLOBE_HEMISPHERE_WEST, "Satelity"),
                    (Tab::Launches, theme::icon::ROCKET, "Starty"),
                    (Tab::Bodies, theme::icon::PLANET, "Ciała"),
                ] {
                    if theme::tile(ui, glyph, label, app.prefs.tab == tab, w).clicked() {
                        app.prefs.tab = tab;
                    }
                }
            });
            ui.add_space(4.0);
            match app.prefs.tab {
                Tab::Missions => missions_tab(app, ui),
                Tab::Satellites => sats_tab(app, ui),
                Tab::Bodies => bodies_tab(app, ui),
                Tab::Launches => launches_tab(app, ui),
            }
        });
    });
}

fn search_box(ui: &mut egui::Ui, text: &mut String, hint: &str) {
    ui.add(egui::TextEdit::singleline(text).hint_text(format!("{}  {}", theme::icon::MAGNIFYING_GLASS, tr(hint))).desired_width(f32::INFINITY).margin(egui::Margin::symmetric(10, 7)));
}

fn matches(hay: &str, needle: &str) -> bool {
    needle.is_empty() || hay.to_lowercase().contains(&needle.to_lowercase())
}

/* ── misje ───────────────────────────────────────────────────────────── */

fn missions_tab(app: &mut App, ui: &mut egui::Ui) {
    let db = missions::db();
    search_box(ui, &mut app.search, "Nazwa, agencja, kraj, rok…");
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
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
        if toggle_chip(ui, tr("trwające teraz"), Some(theme::icon::CLOCK), app.prefs.only_active, theme::OK).clicked() {
            app.prefs.only_active = !app.prefs.only_active;
        }
    });
    ui.add_space(4.0);
    let jd = app.clock.jd;
    let list: Vec<usize> = db
        .missions
        .iter()
        .enumerate()
        .filter(|(_, m)| app.kind_visible(m.kind))
        .filter(|(_, m)| !app.prefs.only_active || m.live_at(jd))
        .filter(|(_, m)| {
            let s = &app.search;
            matches(&m.name, s) || matches(&m.agency, s) || matches(&m.country_name(), s) || matches(&m.launch[..4], s) || m.crew.iter().any(|c| matches(c, s))
        })
        .map(|(i, _)| i)
        .collect();
    ui.label(RichText::new(tf!("{} misji · {} trwa w tej chwili", list.len(), list.iter().filter(|&&i| db.missions[i].live_at(jd)).count())).small().color(theme::paper_3()));
    ui.separator();
    let scroll = std::mem::take(&mut app.scroll_to_selected);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_max_width(CATALOG_W - 28.0);
        let mut last_decade = 0;
        for i in list {
            let m = &db.missions[i];
            let year = time::year(m.launch_jd);
            let decade = year / 10 * 10;
            if decade != last_decade {
                last_decade = decade;
                ui.add_space(4.0);
                ui.label(RichText::new(tf!("Lata {}.", decade)).small().strong().color(theme::paper_3()));
            }
            let sel = app.selection == Selection::Mission(i);
            let resp = mission_row(ui, m, sel, jd);
            if sel && scroll {
                resp.scroll_to_me(Some(egui::Align::Center));
            }
            if resp.clicked() {
                app.select_mission(i, false);
            }
            if resp.double_clicked() {
                app.select_mission(i, true);
            }
            resp.on_hover_text(tr("Kliknij – pokaż misję · dwuklik – odtwórz od startu"));
        }
    });
}

fn mission_row(ui: &mut egui::Ui, m: &missions::Mission, sel: bool, jd: f64) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 46.0), egui::Sense::click());
    theme::focus_ring(ui, &resp, rect, 10.0);
    let p = ui.painter();
    let bg = if sel { theme::BRASS_DIM } else if resp.hovered() { Color32::from_white_alpha(16) } else { Color32::TRANSPARENT };
    p.rect_filled(rect, 10.0, bg);
    let kc = c32(m.kind.color());
    p.circle_filled(rect.left_center() + egui::vec2(18.0, 0.0), 14.0, kc.gamma_multiply(0.22));
    p.text(rect.left_center() + egui::vec2(18.0, 0.0), egui::Align2::CENTER_CENTER, m.kind.icon(), egui::FontId::proportional(15.0), kc);
    let title = p.layout_no_wrap(m.name.clone(), egui::FontId::proportional(14.0), theme::PAPER);
    let clip = egui::Rect::from_min_max(rect.min + egui::vec2(40.0, 0.0), rect.max - egui::vec2(8.0, 0.0));
    ui.painter_at(clip).galley(rect.left_top() + egui::vec2(40.0, 6.0), title, theme::PAPER);
    let sub = format!("{} · {} · {}", &m.launch[..10], m.agency, m.country_name());
    ui.painter_at(clip).text(rect.left_bottom() + egui::vec2(40.0, -7.0), egui::Align2::LEFT_BOTTOM, sub, egui::FontId::proportional(11.5), theme::paper_3());
    if m.live_at(jd) {
        p.circle_filled(rect.right_center() - egui::vec2(12.0, 0.0), 4.0, theme::OK);
    } else if m.status == Status::Failed {
        p.circle_filled(rect.right_center() - egui::vec2(12.0, 0.0), 4.0, theme::SIGNAL);
    }
    resp
}

/* ── satelity ────────────────────────────────────────────────────────── */

fn sats_tab(app: &mut App, ui: &mut egui::Ui) {
    let cat = app.sats.current();
    let jd = app.clock.jd;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_max_width(CATALOG_W - 28.0);
        // stan danych
        ui.horizontal(|ui| {
            let busy = app.sats.busy();
            let (txt, col) = if busy {
                (tr("Pobieranie katalogu CelesTrak…").to_string(), theme::WARN)
            } else if cat.objs.is_empty() {
                (tr("Brak danych – sprawdź połączenie z internetem").to_string(), theme::SIGNAL)
            } else {
                (tf!("{} obiektów · {} z aktualnymi elementami", super::group(cat.objs.len() as i64), super::group(cat.gp.len() as i64)), theme::paper_2())
            };
            if busy {
                ui.add(egui::Spinner::new().size(14.0));
            }
            ui.label(RichText::new(txt).small().color(col));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(!busy, egui::Button::new(theme::icon::ARROWS_CLOCKWISE).small()).on_hover_text(tr("Pobierz najnowsze dane z CelesTrak")).clicked() {
                    app.sats.load(ui.ctx().clone(), true);
                }
            });
        });
        if let Some(e) = app.sats.error.lock().ok().and_then(|e| e.clone()) {
            ui.label(RichText::new(tf!("Nie udało się odświeżyć danych: {}", e)).small().color(theme::WARN));
        }
        if let Some(h) = cat.gp_age_h {
            ui.label(RichText::new(tf!("Elementy orbitalne sprzed {:.0} h · katalog sprzed {:.0} h", h, cat.satcat_age_h.unwrap_or(0.0))).small().color(theme::paper_3()));
        }
        ui.add_space(4.0);
        theme::switch(ui, &mut app.prefs.layers.sats, "Pokaż obiekty na orbicie");
        // liczba obiektów na orbicie w bieżącej chwili
        let mut counts = [0usize; 4];
        let mut total = 0usize;
        for o in cat.objs.iter().filter(|o| o.alive_at(jd)) {
            counts[o.kind as usize] += 1;
            total += 1;
        }
        section(ui, &tf!("Na orbicie {}", time::fmt_date(jd)));
        ui.label(RichText::new(super::group(total as i64)).size(26.0).strong());
        let f = &mut app.prefs.sat_filter;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            for t in sats::ObjType::ALL {
                let on = f.types[t as usize];
                let label = format!("{} · {}", t.label(), super::group(counts[t as usize] as i64));
                if toggle_chip(ui, &label, Some("●"), on, c32(t.color(true))).clicked() {
                    f.types[t as usize] = !on;
                }
            }
        });
        section(ui, "Orbita");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            for c in sats::OrbitClass::ALL {
                let on = f.classes[c as usize];
                if toggle_chip(ui, c.label(), None, on, theme::BRASS).clicked() {
                    f.classes[c as usize] = !on;
                }
            }
        });
        section(ui, "Grupa");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            for g in sats::Group::ALL {
                if toggle_chip(ui, g.label(), None, f.group == g, theme::TEAL).clicked() {
                    f.group = g;
                }
            }
        });
        theme::switch(ui, &mut f.only_active, "Tylko działające satelity");
        if let Some((_, v, _)) = &app.sat_visible {
            ui.label(RichText::new(tf!("Na scenie: {} (dokładnie wg SGP4: {})", super::group(v.pos.len() as i64), super::group(v.precise as i64))).small().color(theme::paper_3()));
        }
        theme::para(ui, "Obiekty z aktualnymi elementami liczone są modelem SGP4. Pozostałe – także historyczne – krążą po orbitach odtworzonych z katalogu: wysokość, nachylenie i okres są prawdziwe, położenie na orbicie jest umowne.");
        section(ui, "Obiekty na orbicie w kolejnych latach");
        super::charts::population(ui, &cat, time::year(jd));
        section(ui, "Satelity wyniesione w danym roku");
        super::charts::launches(ui, &cat, time::year(jd));
        section(ui, "Szukaj obiektu");
        search_box(ui, &mut app.sat_search, "Nazwa albo numer NORAD (np. ISS, 25544)");
        if app.sat_search.trim().len() >= 2 {
            let q = app.sat_search.trim().to_lowercase();
            let num: Option<u32> = q.parse().ok();
            let mut shown = 0;
            for o in cat.objs.iter().filter(|o| Some(o.norad) == num || o.name.to_lowercase().contains(&q) || o.intl.to_lowercase().contains(&q)) {
                if shown >= 120 {
                    ui.label(RichText::new(tr("…i więcej – zawęź wyszukiwanie")).small().color(theme::paper_3()));
                    break;
                }
                shown += 1;
                let sel = app.selection == Selection::Sat(o.norad);
                let alive = o.alive_at(jd);
                let text = format!("{}  ·  {}  ·  {}", o.name, o.norad, if alive { o.class.label() } else { tr("nie na orbicie") });
                let r = theme::row_button(ui, if o.kind == sats::ObjType::Payload { theme::icon::BROADCAST } else { theme::icon::CUBE }, &text, sel);
                if r.clicked() {
                    if !alive {
                        app.clock.jd = o.launch_jd + 1.0;
                    }
                    app.select_sat(o.norad);
                }
            }
            if shown == 0 {
                ui.label(RichText::new(tr("Nic nie znaleziono.")).small().color(theme::paper_3()));
            }
        }
        ui.add_space(8.0);
        ui.label(RichText::new(tr("Dane: CelesTrak (SATCAT, elementy GP), model SGP4.")).small().color(theme::paper_3()));
    });
}

/* ── ciała ───────────────────────────────────────────────────────────── */

fn bodies_tab(app: &mut App, ui: &mut egui::Ui) {
    search_box(ui, &mut app.body_search, "Planeta, księżyc, planetoida, kometa…");
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_max_width(CATALOG_W - 28.0);
        let groups: [(&str, &[BKind]); 5] = [
            ("Gwiazda", &[BKind::Star]),
            ("Planety i ich księżyce", &[BKind::Planet, BKind::Moon]),
            ("Planety karłowate", &[BKind::Dwarf]),
            ("Planetoidy i komety – cele misji", &[BKind::Small]),
            ("Punkty libracyjne", &[BKind::Point]),
        ];
        for (title, kinds) in groups {
            let items: Vec<usize> = bodies::all()
                .iter()
                .enumerate()
                .filter(|(_, b)| kinds.contains(&b.kind) && matches(b.name(), &app.body_search))
                .map(|(i, _)| i)
                .collect();
            if items.is_empty() {
                continue;
            }
            section(ui, title);
            // planety z księżycami pod spodem
            let mut ordered: Vec<usize> = Vec::new();
            for &i in &items {
                let b = bodies::get(i);
                if b.kind == BKind::Moon {
                    continue;
                }
                ordered.push(i);
                ordered.extend(items.iter().copied().filter(|&m| bodies::get(m).kind == BKind::Moon && bodies::get(m).parent == Some(i)));
            }
            for i in ordered {
                let b = bodies::get(i);
                let sel = app.selection == Selection::Body(i);
                let indent = if b.kind == BKind::Moon { 20.0 } else { 0.0 };
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::click());
                    let p = ui.painter();
                    p.rect_filled(rect, 9.0, if sel { theme::BRASS_DIM } else if resp.hovered() { Color32::from_white_alpha(16) } else { Color32::TRANSPARENT });
                    p.circle_filled(rect.left_center() + egui::vec2(16.0, 0.0), if b.kind == BKind::Moon || b.kind == BKind::Small { 5.0 } else { 8.0 }, c32(b.color));
                    p.text(rect.left_center() + egui::vec2(34.0, 0.0), egui::Align2::LEFT_CENTER, b.name(), egui::FontId::proportional(14.0), theme::PAPER);
                    if b.radius > 0.0 {
                        p.text(rect.right_center() - egui::vec2(10.0, 0.0), egui::Align2::RIGHT_CENTER, super::fmt_km(b.radius / 1000.0), egui::FontId::monospace(11.0), theme::paper_3());
                    }
                    if resp.clicked() {
                        app.select_body(i);
                    }
                });
            }
        }
        ui.add_space(8.0);
        theme::para(ui, "Położenia planet: teoria VSOP87 (Bretagnon i Francou), Księżyca: ELP-2000/82 w ujęciu Meeusa, planetoid i komet: elementy JPL SBDB. Parametry fizyczne, obrót i tekstury: symulator Orbiter (licencja MIT).");
    });
}

/* ── starty ──────────────────────────────────────────────────────────── */

/// Odtworzenie startu z katalogu: chwila startu, kamera nad kosmodromem, zaznaczony pierwszy ładunek.
fn play_launch(app: &mut App, l: &sats::Launch, cat: &sats::Catalog) {
    app.clock.jd = l.jd;
    app.clock.rate = 600.0;
    app.clock.playing = true;
    app.launch_sel = Some(l.cospar.clone());
    if let Some(&first) = l.objects.first() {
        let n = cat.objs[first as usize].norad;
        app.select_sat(n);
    }
}

fn launches_tab(app: &mut App, ui: &mut egui::Ui) {
    let cat = app.sats.current();
    let db = missions::db();
    let sim_year = time::year(app.clock.jd);
    let year = app.launch_year.unwrap_or(sim_year).clamp(1957, time::year(time::now()));
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_max_width(CATALOG_W - 28.0);
        ui.horizontal(|ui| {
            if ui.button(theme::icon::CARET_LEFT).clicked() {
                app.launch_year = Some((year - 1).max(1957));
            }
            ui.label(RichText::new(year.to_string()).size(26.0).strong());
            if ui.button(theme::icon::CARET_RIGHT).clicked() {
                app.launch_year = Some((year + 1).min(time::year(time::now())));
            }
            if app.launch_year.is_some() && ui.small_button(tr("rok symulacji")).clicked() {
                app.launch_year = None;
            }
        });
        let (a, b) = (time::year_start(year), time::year_start(year + 1));
        let list: Vec<&sats::Launch> = cat.launches.iter().filter(|l| l.jd >= a && l.jd < b).collect();
        let payloads: u32 = list.iter().map(|l| l.payloads).sum();
        if cat.launches.is_empty() {
            ui.label(RichText::new(if app.sats.busy() { tr("Pobieranie katalogu startów…") } else { tr("Brak katalogu startów – sprawdź połączenie z internetem") }).small().color(theme::WARN));
        } else {
            ui.label(RichText::new(tf!("{} startów orbitalnych · {} satelitów", list.len(), super::group(payloads as i64))).color(theme::paper_2()));
        }
        // kosmodromy w tym roku
        let mut sites: Vec<(String, usize)> = Vec::new();
        for l in &list {
            let name = sats::site_info(&l.site).map(|s| s.0.to_string()).unwrap_or_else(|| l.site.clone());
            match sites.iter_mut().find(|s| s.0 == name) {
                Some(s) => s.1 += 1,
                None => sites.push((name, 1)),
            }
        }
        sites.sort_by(|x, y| y.1.cmp(&x.1));
        if !sites.is_empty() {
            section(ui, "Kosmodromy");
            let max = sites[0].1.max(1) as f32;
            for (name, n) in sites.iter().take(8) {
                ui.horizontal(|ui| {
                    ui.add_sized([150.0, 16.0], egui::Label::new(RichText::new(name).small()).truncate());
                    let w = ui.available_width() - 40.0;
                    let (r, _) = ui.allocate_exact_size(egui::vec2(w, 10.0), egui::Sense::hover());
                    ui.painter().rect_filled(egui::Rect::from_min_size(r.min, egui::vec2(w * *n as f32 / max, 10.0)), 3.0, theme::BRASS.gamma_multiply(0.8));
                    ui.label(RichText::new(n.to_string()).monospace().small());
                });
            }
        }
        section(ui, "Starty w kolejnych latach");
        if let Some(y) = super::charts::launch_years(ui, &cat.launch_count, year) {
            app.launch_year = Some(y);
        }
        // misje z bazy, które wystartowały w tym roku
        let ms: Vec<usize> = db.missions.iter().enumerate().filter(|(_, m)| m.launch_jd >= a && m.launch_jd < b).map(|(i, _)| i).collect();
        if !ms.is_empty() {
            section(ui, "Misje z bazy – odtwórz start");
            for i in ms {
                let m = &db.missions[i];
                let veh = m.vehicle().map(|v| v.name.as_str()).unwrap_or("");
                let r = theme::row_button(ui, m.kind.icon(), &format!("{}  ·  {}  ·  {}", &m.launch[5..10], m.name, veh), app.selection == Selection::Mission(i));
                if r.clicked() {
                    app.select_mission(i, true);
                    app.clock.rate = 60.0;
                }
            }
        }
        section(ui, "Wszystkie starty – kliknij, by odtworzyć");
        let jd = app.clock.jd;
        for l in &list {
            let first = l.objects.first().map(|&i| &cat.objs[i as usize]);
            let name = first.map(|o| o.name.as_str()).unwrap_or("?");
            let more = if l.payloads > 1 { tf!(" +{}", l.payloads - 1) } else { String::new() };
            let site = sats::site_info(&l.site).map(|s| s.0).unwrap_or(l.site.as_str());
            let sel = app.launch_sel.as_deref() == Some(l.cospar.as_str());
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::click());
            let p = ui.painter();
            p.rect_filled(rect, 8.0, if sel { theme::BRASS_DIM } else if resp.hovered() { Color32::from_white_alpha(16) } else { Color32::TRANSPARENT });
            let past = l.jd <= jd;
            p.circle_filled(rect.left_center() + egui::vec2(12.0, 0.0), 4.0, if past { theme::BRASS } else { theme::INK_4 });
            let clip = rect.shrink2(egui::vec2(4.0, 0.0));
            ui.painter_at(clip).text(rect.left_top() + egui::vec2(26.0, 4.0), egui::Align2::LEFT_TOP, format!("{name}{more}"), egui::FontId::proportional(13.5), theme::PAPER);
            ui.painter_at(clip).text(rect.left_bottom() + egui::vec2(26.0, -4.0), egui::Align2::LEFT_BOTTOM, format!("{}  ·  {}  ·  {}  ·  {}", time::fmt_date(l.jd), l.cospar, site, sats::owner_name(&l.owner)), egui::FontId::monospace(10.5), theme::paper_3());
            if resp.clicked() {
                play_launch(app, l, &cat);
            }
        }
        ui.add_space(8.0);
        theme::para(ui, "Starty pochodzą z katalogu SATCAT (CelesTrak): obiekty o wspólnym numerze COSPAR to jeden start. Katalog podaje tylko datę startu, więc odtworzenie zaczyna się o północy UTC tego dnia.");
    });
}
