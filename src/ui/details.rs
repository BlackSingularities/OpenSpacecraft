//! Prawy panel: szczegóły zaznaczonej misji, satelity albo ciała niebieskiego, z odczytami
//! na żywo w chwili symulacji.

use eframe::egui::{self, Color32, RichText};

use super::{chip, fact, fact_mono, fmt_km, fmt_mass, fmt_speed, section, theme, DETAILS_W, TIMELINE_H, TOP_BAR_H};
use crate::app::{App, Focus, PosSource, Selection};
use crate::astro::bodies::{self, Kind as BKind, EARTH, SUN};
use crate::astro::{time, C};
use crate::i18n::tr;
use crate::missions::{self, traj, Status};

pub fn open(app: &App) -> bool {
    app.selection != Selection::None
}

fn c32(c: [f32; 3]) -> Color32 {
    Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

pub fn details(app: &mut App, ctx: &egui::Context) {
    if !open(app) {
        return;
    }
    let screen = ctx.content_rect();
    let height = (screen.height() - TOP_BAR_H - TIMELINE_H).max(200.0);
    app.glass.push((egui::Rect::from_min_size(egui::pos2(screen.right() - DETAILS_W, screen.top() + TOP_BAR_H), egui::vec2(DETAILS_W, height)), 1.0));
    egui::Area::new(egui::Id::new("szczegoly")).fixed_pos([screen.right() - DETAILS_W, screen.top() + TOP_BAR_H]).order(egui::Order::Middle).show(ctx, |ui| {
        let gutter = theme::SCROLL_GUTTER;
        egui::Frame::new().fill(theme::glass_fill()).inner_margin(egui::Margin { left: 18, right: (18.0 - gutter) as i8, top: 14, bottom: 14 }).show(ui, |ui| {
            ui.set_width(DETAILS_W - 36.0 + gutter);
            ui.set_height(height - 28.0);
            let st = ui.style_mut();
            st.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(14.5));
            st.spacing.item_spacing.y = 6.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(theme::icon::INFO).size(18.0).color(theme::BRASS));
                ui.label(RichText::new(tr("Szczegóły")).size(16.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(gutter);
                    if ui.small_button(theme::icon::X).on_hover_text(tr("Zamknij (Esc)")).clicked() {
                        app.selection = Selection::None;
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.set_max_width(DETAILS_W - 36.0);
                match app.selection {
                    Selection::Mission(i) => {
                        mission(app, ui, i);
                        super::archive::mission_extra(app, ui, &missions::db().missions[i].id);
                    }
                    Selection::Sat(n) => satellite(app, ui, n),
                    Selection::Body(b) => body(app, ui, b),
                    Selection::Craft(c) => super::archive::craft(app, ui, c),
                    Selection::Launch(l) => super::archive::launch(app, ui, l),
                    Selection::None => {}
                }
            });
        });
    });
}

fn mission(app: &mut App, ui: &mut egui::Ui, i: usize) {
    let m = &missions::db().missions[i];
    let jd = app.clock.jd;
    let kc = c32(m.kind.color());
    ui.horizontal(|ui| {
        ui.label(RichText::new(m.kind.icon()).size(26.0).color(kc));
        ui.vertical(|ui| {
            ui.label(RichText::new(&m.name).size(22.0).strong());
            ui.label(RichText::new(format!("{} · {}", m.agency, m.country_name())).color(theme::paper_2()));
        });
    });
    ui.horizontal_wrapped(|ui| {
        chip(ui, m.kind.label(), kc.gamma_multiply(0.3), theme::PAPER);
        let (bg, fg) = match m.status {
            Status::Active => (theme::OK.gamma_multiply(0.3), theme::PAPER),
            Status::Failed => (theme::SIGNAL.gamma_multiply(0.3), theme::PAPER),
            Status::Completed => (Color32::from_white_alpha(18), theme::PAPER),
        };
        chip(ui, m.status.label(), bg, fg);
        if m.live_at(jd) {
            chip(ui, tr("w locie w tej chwili"), theme::BRASS_DIM, theme::PAPER);
        }
    });
    ui.add_space(4.0);
    // przyciski
    ui.horizontal(|ui| {
        if ui.add(theme::primary(&theme::ic(theme::icon::PLAY, "Odtwórz od startu"))).clicked() {
            app.select_mission(i, true);
        }
        if ui.button(theme::ic(theme::icon::CROSSHAIR, "Śledź")).on_hover_text(tr("Kamera podąża za statkiem (F)")).clicked() {
            let d = app.mission_view_dist(i);
            app.set_focus(Focus::Mission(i), Some(d));
        }
        if !m.live_at(jd) && m.end_jd.is_some() && ui.button(theme::ic(theme::icon::FLAG_CHECKERED, "Koniec")).clicked() {
            app.clock.jd = m.end_jd.unwrap_or(jd) - 1.0 / 24.0;
        }
    });
    super::archive::mission_photo(ui, &m.id);
    theme::para(ui, m.summary());

    // odczyty na żywo
    let t = app.trajs.get(i);
    section(ui, &tf!("Stan w chwili {}", time::fmt_datetime(jd)));
    match &t {
        None => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(14.0));
                ui.label(RichText::new(tr("Liczenie trajektorii…")).color(theme::paper_3()));
            });
        }
        Some(_) => {
            if jd < m.launch_jd {
                fact(ui, "Do startu", time::fmt_duration(m.launch_jd - jd));
            } else {
                fact_mono(ui, "Czas misji (MET)", time::fmt_duration(jd - m.launch_jd));
            }
            if let Some((p, source)) = app.mission_position(i, jd) {
                let h = 5.0 / 86_400.0;
                let v = match (app.mission_position(i, jd + h).map(|x| x.0), app.mission_position(i, jd - h).map(|x| x.0)) {
                    (Some(a), Some(b)) => (a - b) / 10.0,
                    _ => glam::DVec3::ZERO,
                };
                let earth = bodies::position(EARTH, jd);
                let de = (p - earth).length();
                fact_mono(ui, "Odległość od Ziemi", fmt_km(de / 1000.0));
                fact_mono(ui, "Odległość od Słońca", fmt_km(p.length() / 1000.0));
                fact_mono(ui, "Sygnał z Ziemi", time::fmt_duration(de / C / 86_400.0));
                let near = traj::nearest_body(p, jd);
                if near != SUN {
                    let b = bodies::get(near);
                    let bp = bodies::position(near, jd);
                    let alt = (p - bp).length() - b.radius;
                    let vr = (v - bodies::velocity(near, jd)).length();
                    fact_mono(ui, &tf!("Wysokość ({})", b.name()), fmt_km(alt.max(0.0) / 1000.0));
                    fact_mono(ui, &tf!("Prędkość ({})", b.name()), fmt_speed(vr));
                } else {
                    fact_mono(ui, "Prędkość względem Słońca", fmt_speed(v.length()));
                }
                let src = match source {
                    PosSource::Horizons => tr("JPL Horizons – rzeczywista trajektoria"),
                    PosSource::Model => tr("model fizyczny (łuki Keplera, przybliżenie)"),
                    PosSource::Sgp4 => tr("CelesTrak – elementy orbity (SGP4)"),
                    PosSource::Satcat => tr("CelesTrak SATCAT – orbita z katalogu (położenie na orbicie umowne)"),
                };
                fact(ui, "Źródło położenia", src);
            } else if jd >= m.launch_jd {
                ui.label(RichText::new(tr("Statek nie istnieje już w tej chwili (powrót, deorbitacja albo zderzenie).")).small().color(theme::paper_3()));
            }
            // ten sam obiekt w katalogu: czy jeszcze krąży
            if let Some(n) = app.mission_norad(i) {
                let cat = app.sats.current();
                if let Some(o) = cat.find(n) {
                    let st = match o.decay_jd {
                        Some(d) if d <= jd => tf!("spłonął w atmosferze {}", time::fmt_date(d)),
                        Some(d) if d <= time::now() => tf!("na orbicie (spłonie {})", time::fmt_date(d)),
                        _ if m.end_jd.is_some_and(|e| jd > e) => tr("na orbicie – nieczynny, krąży do dziś").to_string(),
                        _ => tr("na orbicie").to_string(),
                    };
                    fact(ui, "Obiekt", st);
                    fact_mono(ui, "Orbita", format!("{:.0} × {:.0} km · {:.1}° · {:.1} min", o.peri_km, o.apo_km, o.inc_deg, o.period_min));
                }
            }
        }
    }

    section(ui, "Dane misji");
    fact_mono(ui, "Start", time::fmt_datetime(m.launch_jd));
    if let Some(e) = m.end_jd {
        fact_mono(ui, "Koniec", time::fmt_datetime(e));
        fact(ui, "Czas trwania", time::fmt_duration(e - m.launch_jd));
    }
    if let Some(v) = m.vehicle() {
        fact(ui, "Rakieta nośna", &v.name);
        let mut spec = tf!("{:.0} m · {} t", v.height_m, super::group(v.mass_t.round() as i64));
        if let Some(l) = v.leo_t {
            spec += &tf!(" · LEO {} t", l);
        }
        fact_mono(ui, "", spec).on_hover_text(v.summary());
    }
    if let Some(s) = m.site() {
        fact(ui, "Miejsce startu", &s.name);
    }
    if let Some(kg) = m.mass_kg {
        fact_mono(ui, "Masa", fmt_mass(kg));
    }
    if !m.crew.is_empty() {
        fact(ui, "Załoga", m.crew.join(", "));
    }
    if let Some(n) = m.norad {
        ui.horizontal(|ui| {
            ui.add_sized([150.0, 18.0], egui::Label::new(RichText::new(tr("Katalog NORAD")).color(theme::paper_3())));
            if ui.link(n.to_string()).on_hover_text(tr("Pokaż obiekt w katalogu satelitów")).clicked() {
                app.select_sat(n);
            }
        });
    }
    if !m.achievements().is_empty() {
        section(ui, "Osiągnięcia");
        for a in m.achievements() {
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(theme::icon::STAR_FOUR).color(theme::WARN));
                ui.add(egui::Label::new(a).wrap());
            });
        }
    }
    if !m.events.is_empty() {
        section(ui, "Przebieg – kliknij, by przejść do chwili");
        for (k, e) in m.events.iter().enumerate() {
            let past = e.jd <= jd;
            let next = !past && m.events[..k].iter().all(|x| x.jd <= jd);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::click());
            let p = ui.painter();
            if resp.hovered() {
                p.rect_filled(rect, 8.0, Color32::from_white_alpha(14));
            }
            if next {
                p.rect_stroke(rect, 8.0, egui::Stroke::new(1.0_f32, theme::BRASS), egui::StrokeKind::Inside);
            }
            let dot = rect.left_center() + egui::vec2(12.0, 0.0);
            if past {
                p.circle_filled(dot, 5.0, kc);
            } else {
                p.circle_stroke(dot, 5.0, egui::Stroke::new(1.5_f32, kc));
            }
            let clip = rect.shrink2(egui::vec2(4.0, 0.0));
            ui.painter_at(clip).text(rect.left_top() + egui::vec2(28.0, 5.0), egui::Align2::LEFT_TOP, e.text(), egui::FontId::proportional(13.5), if past { theme::PAPER } else { theme::paper_2() });
            ui.painter_at(clip).text(rect.left_bottom() + egui::vec2(28.0, -5.0), egui::Align2::LEFT_BOTTOM, time::fmt_datetime(e.jd), egui::FontId::monospace(11.0), theme::paper_3());
            if resp.clicked() {
                app.clock.jd = e.jd;
                app.clock.playing = false;
                if app.focus != Focus::Mission(i) {
                    let d = app.mission_view_dist(i);
                    app.set_focus(Focus::Mission(i), Some(d));
                }
            }
        }
    }
}

fn satellite(app: &mut App, ui: &mut egui::Ui, norad: u32) {
    let cat = app.sats.current();
    let jd = app.clock.jd;
    let Some(o) = cat.find(norad) else {
        ui.label(tr("Obiekt niedostępny w katalogu."));
        return;
    };
    ui.label(RichText::new(&o.name).size(22.0).strong());
    ui.horizontal_wrapped(|ui| {
        chip(ui, o.kind.label(), c32(o.kind.color(true)).gamma_multiply(0.3), theme::PAPER);
        chip(ui, o.class.label(), Color32::from_white_alpha(18), theme::PAPER);
        if o.active {
            chip(ui, tr("działa"), theme::OK.gamma_multiply(0.3), theme::PAPER);
        }
    });
    ui.horizontal(|ui| {
        if ui.button(theme::ic(theme::icon::CROSSHAIR, "Śledź")).clicked() {
            app.select_sat(norad);
        }
        if !o.alive_at(jd) && ui.button(theme::ic(theme::icon::CLOCK, "Przejdź do startu")).clicked() {
            app.clock.jd = o.launch_jd + 0.5;
        }
    });
    super::archive::sat_extra(app, ui, norad);
    section(ui, &tf!("Stan w chwili {}", time::fmt_datetime(jd)));
    match o.position(jd, &cat.gp).filter(|_| o.alive_at(jd) || o.launch_jd == 0.0) {
        Some((p, exact)) => {
            let alt = p.length() / 1000.0 - 6_371.0;
            fact_mono(ui, "Wysokość", fmt_km(alt));
            // punkt pod satelitą
            let local = bodies::orientation(EARTH, jd).transpose() * p.normalize();
            let (lat, lon) = (local.z.asin().to_degrees(), local.y.atan2(local.x).to_degrees());
            fact_mono(ui, "Nad punktem", format!("{:.2}° {}, {:.2}° {}", lat.abs(), if lat >= 0.0 { "N" } else { "S" }, lon.abs(), if lon >= 0.0 { "E" } else { "W" }));
            let h = 1.0 / 86_400.0;
            if let (Some((a, _)), Some((b, _))) = (o.position(jd + h, &cat.gp), o.position(jd - h, &cat.gp)) {
                fact_mono(ui, "Prędkość", fmt_speed((a - b).length() / 2.0));
            }
            fact(ui, "Model", if exact { tr("SGP4 – aktualne elementy orbitalne") } else { tr("orbita odtworzona z katalogu (położenie umowne)") });
        }
        None => {
            ui.label(RichText::new(tr("Obiektu nie było w tej chwili na orbicie.")).color(theme::paper_3()));
        }
    }
    section(ui, "Katalog");
    fact_mono(ui, "NORAD", o.norad.to_string());
    if !o.intl.is_empty() {
        fact_mono(ui, "COSPAR", &o.intl);
    }
    if !o.owner.is_empty() {
        fact(ui, "Właściciel", o.owner_name());
    }
    if o.launch_jd > 0.0 {
        fact_mono(ui, "Start", time::fmt_date(o.launch_jd));
    }
    if let Some(d) = o.decay_jd {
        fact_mono(ui, "Wejście w atmosferę", time::fmt_date(d));
    }
    if o.period_min > 0.0 {
        fact_mono(ui, "Okres obiegu", tf!("{:.1} min", o.period_min));
        fact_mono(ui, "Nachylenie", tf!("{:.2}°", o.inc_deg));
        fact_mono(ui, "Perygeum / apogeum", tf!("{} / {}", fmt_km(o.peri_km), fmt_km(o.apo_km)));
    }
    // dane z archiwum (ten sam obiekt): fakty, start, pozostałości startu, opis, źródła
    super::archive::sat_archive(app, ui, norad);
}

fn body(app: &mut App, ui: &mut egui::Ui, id: usize) {
    let b = bodies::get(id);
    let jd = app.clock.jd;
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), 12.0, c32(b.color));
        ui.label(RichText::new(b.name()).size(22.0).strong());
    });
    let kind = match b.kind {
        BKind::Star => tr("gwiazda"),
        BKind::Planet => tr("planeta"),
        BKind::Dwarf => tr("planeta karłowata"),
        BKind::Moon => tr("księżyc"),
        BKind::Small => tr("planetoida / kometa"),
        BKind::Point => tr("punkt libracyjny"),
    };
    ui.horizontal(|ui| {
        chip(ui, kind, Color32::from_white_alpha(18), theme::PAPER);
        if let Some(p) = b.parent.filter(|&p| p != SUN) {
            chip(ui, &tf!("krąży wokół: {}", bodies::get(p).name()), Color32::from_white_alpha(18), theme::PAPER);
        }
    });
    if ui.button(theme::ic(theme::icon::CROSSHAIR, "Pokaż z bliska")).clicked() {
        app.select_body(id);
    }
    section(ui, &tf!("Stan w chwili {}", time::fmt_datetime(jd)));
    let p = bodies::position(id, jd);
    if id != SUN {
        fact_mono(ui, "Odległość od Słońca", fmt_km(p.length() / 1000.0));
    }
    if id != EARTH {
        let de = (p - bodies::position(EARTH, jd)).length();
        fact_mono(ui, "Odległość od Ziemi", fmt_km(de / 1000.0));
        fact_mono(ui, "Światło z Ziemi", time::fmt_duration(de / C / 86_400.0));
    }
    if let Some(par) = b.parent {
        let v = (bodies::velocity(id, jd) - bodies::velocity(par, jd)).length();
        fact_mono(ui, "Prędkość orbitalna", fmt_speed(v));
    }
    section(ui, "Parametry");
    if b.radius > 0.0 {
        fact_mono(ui, "Promień", fmt_km(b.radius / 1000.0));
    }
    if b.gm > 0.0 {
        let mass = b.gm / crate::astro::G;
        let exp = mass.log10().floor();
        fact_mono(ui, "Masa", tf!("{:.3} × 10^{} kg", mass / 10f64.powf(exp), exp as i32));
        if b.radius > 1000.0 && b.kind != BKind::Small {
            fact_mono(ui, "Grawitacja (pow.)", tf!("{:.2} m/s²", b.gm / (b.radius * b.radius)));
            fact_mono(ui, "Prędkość ucieczki", fmt_speed((2.0 * b.gm / b.radius).sqrt()));
        }
    }
    if b.parent.is_some() && b.kind != BKind::Point {
        let d = bodies::orbit_period_days(id);
        fact_mono(ui, "Okres obiegu", if d > 730.0 { tf!("{:.2} lat", d / 365.25) } else { tf!("{:.2} d", d) });
    }
    if let Some(r) = b.rot {
        if b.kind != BKind::Small {
            let h = r.period_s.abs() / 3600.0;
            let txt = if h > 48.0 { tf!("{:.2} d", h / 24.0) } else { tf!("{:.2} h", h) };
            fact_mono(ui, "Obrót gwiazdowy", if r.period_s < 0.0 { tf!("{} (wsteczny)", txt) } else { txt });
        }
    }
    // misje związane z ciałem
    let bid = b.id.clone();
    let related: Vec<usize> = missions::db()
        .missions
        .iter()
        .enumerate()
        .filter(|(_, m)| match &m.traj {
            missions::TrajSpec::Helio { legs, fin } => legs.iter().skip(1).any(|l| l.body.as_deref() == Some(bid.as_str())) || fin.as_ref().is_some_and(|f| f.orbit.as_ref().is_some_and(|o| o.body == bid) || f.surface.as_ref().is_some_and(|s| s.body == bid)),
            missions::TrajSpec::Lunar { .. } => bid == "moon",
            missions::TrajSpec::Orbit { body, .. } => *body == bid,
            missions::TrajSpec::Lagrange { point, .. } => *point == bid,
            missions::TrajSpec::Trailing { .. } => false,
        })
        .map(|(i, _)| i)
        .collect();
    if !related.is_empty() {
        section(ui, &tf!("Misje ({})", related.len()));
        for mi in related {
            let m = &missions::db().missions[mi];
            if theme::row_button(ui, m.kind.icon(), &format!("{} · {}", m.name, &m.launch[..4]), false).clicked() {
                app.select_mission(mi, false);
            }
        }
    }
}
