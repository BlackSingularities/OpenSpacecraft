//! Lewy panel: stan w wybranej chwili – satelity na orbicie i starty. Lista ciał niebieskich
//! (menu „Ciała” na górnym pasku) też jest tutaj. Misje mają osobne okno (`missions_window`).

use eframe::egui::{self, Color32, RichText};

use super::{section, theme, toggle_chip, CATALOG_W, TIMELINE_H, TOP_BAR_H};
use crate::app::{App, Selection, Tab};
use crate::astro::bodies::{self, Kind as BKind};
use crate::astro::time;
use crate::i18n::tr;
use crate::missions;
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
            // nagłówek: wszystko w tym panelu dotyczy wybranej chwili
            ui.horizontal(|ui| {
                ui.label(RichText::new(theme::icon::CALENDAR_BLANK).color(theme::BRASS));
                ui.label(RichText::new(tf!("Stan na {}", super::fmt_long_date(app.clock.jd))).strong());
            });
            ui.add_space(2.0);
            // zakładki (dawne „Misje” i „Ciała” są teraz na górnym pasku)
            if !matches!(app.prefs.tab, Tab::Satellites | Tab::Launches) {
                app.prefs.tab = Tab::Satellites;
            }
            ui.horizontal(|ui| {
                let w = (CATALOG_W - 28.0 - 6.0) / 2.0;
                ui.spacing_mut().item_spacing.x = 6.0;
                for (tab, glyph, label) in [(Tab::Satellites, theme::icon::GLOBE_HEMISPHERE_WEST, "Satelity"), (Tab::Launches, theme::icon::ROCKET, "Starty")] {
                    if theme::tile(ui, glyph, label, app.prefs.tab == tab, w).clicked() {
                        app.prefs.tab = tab;
                    }
                }
            });
            ui.add_space(4.0);
            match app.prefs.tab {
                Tab::Launches => launches_tab(app, ui),
                _ => sats_tab(app, ui),
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

/// Lista ciał niebieskich (menu „Ciała”); zwraca `true`, gdy wybrano ciało.
pub fn bodies_list(app: &mut App, ui: &mut egui::Ui) -> bool {
    let mut picked = false;
    let width = ui.available_width();
    search_box(ui, &mut app.body_search, "Planeta, księżyc, planetoida, kometa…");
    egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
        ui.set_max_width(width);
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
                        picked = true;
                    }
                });
            }
        }
        ui.add_space(8.0);
        theme::para(ui, "Położenia planet: teoria VSOP87 (Bretagnon i Francou), Księżyca: ELP-2000/82 w ujęciu Meeusa, planetoid i komet: elementy JPL SBDB. Parametry fizyczne, obrót i tekstury: symulator Orbiter (licencja MIT).");
    });
    picked
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

/// Kamera nad kosmodromem (Ziemia w środku, kosmodrom zwrócony do kamery).
pub fn look_at_site(app: &mut App, lat: f64, lon: f64) {
    let jd = app.clock.jd;
    let (la, lo) = (lat.to_radians(), lon.to_radians());
    let dir = crate::astro::bodies::orientation(crate::astro::bodies::EARTH, jd) * glam::DVec3::new(la.cos() * lo.cos(), la.cos() * lo.sin(), la.sin());
    app.selection = Selection::None;
    app.set_focus(crate::app::Focus::Body(crate::astro::bodies::EARTH), Some(crate::astro::bodies::get(crate::astro::bodies::EARTH).radius * 3.2));
    app.cam.yaw = dir.y.atan2(dir.x);
    app.cam.pitch = dir.z.clamp(-1.0, 1.0).asin().clamp(-1.4, 1.4);
}

/// Nadchodzące starty (Launch Library 2) z odliczaniem do rzeczywistej chwili.
fn upcoming_list(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let now = time::now();
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}  {}", theme::icon::TIMER, tr("Nadchodzące starty"))).size(15.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.upcoming.loading() {
                ui.add(egui::Spinner::new().size(13.0));
            } else if ui.small_button(theme::icon::ARROWS_CLOCKWISE).on_hover_text(tr("Odśwież listę")).clicked() {
                app.upcoming.load(&ctx, true);
            }
        });
    });
    let Some(list) = app.upcoming.get(&ctx) else {
        match app.upcoming.error() {
            Some(e) => {
                ui.label(RichText::new(tr("Nie udało się pobrać nadchodzących startów.")).small().color(theme::WARN)).on_hover_text(e);
            }
            None => {
                ui.label(RichText::new(tr("Pobieranie z Launch Library 2…")).small().color(theme::paper_3()));
            }
        }
        return;
    };
    ctx.request_repaint_after(std::time::Duration::from_secs(1));
    let shown: Vec<(usize, &crate::upcoming::Upcoming)> = list.iter().enumerate().filter(|(_, u)| u.jd > now - 2.0 / 24.0).take(12).collect();
    if shown.is_empty() {
        ui.label(RichText::new(tr("Brak zapowiedzianych startów.")).small().color(theme::paper_3()));
    }
    for (k, u) in shown {
        let sel = app.upcoming_sel == Some(k);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 42.0), egui::Sense::click());
        let p = ui.painter();
        p.rect_filled(rect, 8.0, if sel { theme::BRASS_DIM } else if resp.hovered() { Color32::from_white_alpha(16) } else { Color32::from_white_alpha(5) });
        let col = if u.go() { theme::OK } else { theme::paper_3() };
        p.circle_filled(rect.left_center() + egui::vec2(12.0, 0.0), 4.0, col);
        let clip = ui.painter_at(rect.shrink2(egui::vec2(4.0, 0.0)));
        let cd = if u.precise() { crate::upcoming::countdown(u.jd, now) } else { tf!("ok. {}", time::fmt_date(u.jd)) };
        let cd_g = ui.painter().layout_no_wrap(cd, egui::FontId::monospace(12.0), if u.jd > now { theme::BRASS } else { theme::OK });
        let cd_w = cd_g.size().x;
        clip.galley(egui::pos2(rect.right() - 8.0 - cd_w, rect.top() + 5.0), cd_g, theme::BRASS);
        let title_clip = ui.painter_at(egui::Rect::from_min_max(rect.min, egui::pos2(rect.right() - cd_w - 14.0, rect.bottom())));
        title_clip.text(rect.left_top() + egui::vec2(26.0, 5.0), egui::Align2::LEFT_TOP, &u.name, egui::FontId::proportional(13.0), theme::PAPER);
        clip.text(rect.left_bottom() + egui::vec2(26.0, -5.0), egui::Align2::LEFT_BOTTOM, format!("{}  ·  {}", crate::ui::sky::local_time(u.jd), u.location), egui::FontId::proportional(10.5), theme::paper_3());
        let resp = resp.on_hover_text(tr("Kliknij, by zobaczyć kosmodrom na globie"));
        if resp.clicked() {
            if sel {
                app.upcoming_sel = None;
            } else {
                app.upcoming_sel = Some(k);
                app.clock.jd = now;
                app.clock.rate = 1.0;
                look_at_site(app, u.lat, u.lon);
            }
        }
        if sel {
            egui::Frame::new().fill(Color32::from_white_alpha(8)).corner_radius(8.0).inner_margin(8.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                super::fact(ui, "Rakieta", &u.rocket);
                super::fact(ui, "Wykonawca", &u.provider);
                super::fact(ui, "Stanowisko", &u.pad);
                if !u.orbit.is_empty() {
                    super::fact(ui, "Orbita", &u.orbit);
                }
                super::fact_mono(ui, "Termin (UTC)", time::fmt_datetime(u.jd));
                let st = match u.status.as_str() {
                    "Go" => tr("potwierdzony (Go)").to_string(),
                    "TBD" => tr("termin niepewny (TBD)").to_string(),
                    "TBC" => tr("termin do potwierdzenia (TBC)").to_string(),
                    "Hold" => tr("wstrzymany").to_string(),
                    "Success" => tr("udany").to_string(),
                    "Failure" => tr("nieudany").to_string(),
                    s => s.to_string(),
                };
                super::fact(ui, "Stan", st);
                if !u.mission.is_empty() {
                    ui.add(egui::Label::new(RichText::new(&u.mission).small().color(theme::paper_2())).wrap());
                }
            });
        }
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Dane:")).small().color(theme::paper_3()));
        ui.hyperlink_to(RichText::new("Launch Library 2 (The Space Devs)").small(), crate::upcoming::SOURCE);
    });
}

fn launches_tab(app: &mut App, ui: &mut egui::Ui) {
    let cat = app.sats.current();
    let db = missions::db();
    let sim_year = time::year(app.clock.jd);
    let year = app.launch_year.unwrap_or(sim_year).clamp(1957, time::year(time::now()));
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_max_width(CATALOG_W - 28.0);
        upcoming_list(app, ui);
        ui.add_space(6.0);
        ui.separator();
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
            section(ui, "Misje tego roku – odtwórz start");
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
        let arch = crate::catalog::get();
        if list.is_empty() {
            crate::ui::archive::launches_of_year(app, ui, a, b);
        }
        for l in &list {
            let ll = arch.launch_by_desig(&l.cospar);
            let first = l.objects.first().map(|&i| &cat.objs[i as usize]);
            let name = first.map(|o| o.name.as_str()).unwrap_or("?");
            let more = if l.payloads > 1 { tf!(" +{}", l.payloads - 1) } else { String::new() };
            let site = sats::site_info(&l.site).map(|s| s.0).unwrap_or(l.site.as_str());
            let sel = app.launch_sel.as_deref() == Some(l.cospar.as_str());
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::click());
            let info = ll.map(|k| {
                let r = egui::Rect::from_center_size(rect.right_center() - egui::vec2(18.0, 0.0), egui::vec2(28.0, 28.0));
                (k, r, ui.interact(r, resp.id.with("info"), egui::Sense::click()))
            });
            let p = ui.painter();
            p.rect_filled(rect, 8.0, if sel { theme::BRASS_DIM } else if resp.hovered() { Color32::from_white_alpha(16) } else { Color32::TRANSPARENT });
            let past = l.jd <= jd;
            p.circle_filled(rect.left_center() + egui::vec2(12.0, 0.0), 4.0, if past { theme::BRASS } else { theme::INK_4 });
            let clip = rect.shrink2(egui::vec2(4.0, 0.0));
            let clip = if info.is_some() { clip.with_max_x(rect.right() - 36.0) } else { clip };
            let title = match ll {
                Some(k) => format!("{}{more}", arch.launches[k].name),
                None => format!("{name}{more}"),
            };
            ui.painter_at(clip).text(rect.left_top() + egui::vec2(26.0, 4.0), egui::Align2::LEFT_TOP, title, egui::FontId::proportional(13.5), theme::PAPER);
            ui.painter_at(clip).text(rect.left_bottom() + egui::vec2(26.0, -4.0), egui::Align2::LEFT_BOTTOM, format!("{}  ·  {}  ·  {}  ·  {}", time::fmt_date(l.jd), l.cospar, site, sats::owner_name(&l.owner)), egui::FontId::monospace(10.5), theme::paper_3());
            if let Some((k, r, ir)) = &info {
                let on = app.selection == Selection::Launch(*k);
                if ir.hovered() || on {
                    ui.painter().rect_filled(*r, 6.0, Color32::from_white_alpha(24));
                }
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, theme::icon::INFO, egui::FontId::proportional(16.0), if on { theme::BRASS } else { theme::paper_2() });
                if ir.clone().on_hover_text(tr("Szczegóły startu")).clicked() {
                    app.selection = Selection::Launch(*k);
                }
            }
            if resp.clicked() && !info.as_ref().is_some_and(|i| i.2.clicked()) {
                play_launch(app, l, &cat);
                // dokładna godzina startu z Launch Library 2 zamiast północy z SATCAT
                if let Some(k) = ll {
                    app.clock.jd = arch.launches[k].jd - 60.0 / 86_400.0;
                }
            }
        }
        ui.add_space(8.0);
        theme::para(ui, "Starty pochodzą z katalogu SATCAT (CelesTrak): obiekty o wspólnym numerze COSPAR to jeden start. Godziny, rakiety i opisy – z Launch Library 2 (The Space Devs), wbudowane w program. Ikona ⓘ otwiera kartę startu.");
    });
}
