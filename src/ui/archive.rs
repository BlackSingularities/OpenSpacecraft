//! Archiwum: katalog statków kosmicznych i startów (Wikidata, NASA NSSDCA, Launch Library 2)
//! – zakładka w lewym panelu i karty szczegółów w prawym. Zdjęcia dociągane z sieci.

use eframe::egui::{self, Color32, RichText};

use super::{chip, fact, fact_mono, fmt_mass, section, theme};
use crate::app::{App, Selection};
use crate::astro::time;
use crate::catalog::{self, Craft, Text};
use crate::i18n::tr;
use crate::missions;

const ROW_H: f32 = 46.0;

/* ── wiersz listy misji ──────────────────────────────────────────────── */

pub fn craft_row(ui: &mut egui::Ui, c: &Craft, sel: bool) -> egui::Response {
    let cat = catalog::get();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_H), egui::Sense::click());
    theme::focus_ring(ui, &resp, rect, 10.0);
    let p = ui.painter();
    let bg = if sel { theme::BRASS_DIM } else if resp.hovered() { Color32::from_white_alpha(16) } else { Color32::TRANSPARENT };
    p.rect_filled(rect, 10.0, bg);
    // archiwum: neutralne kółko (misje z trasą 3D mają kolor rodzaju)
    let gc = if c.has_desc() { theme::paper_2() } else { theme::paper_3() };
    p.circle_filled(rect.left_center() + egui::vec2(18.0, 0.0), 14.0, Color32::from_white_alpha(10));
    let glyph = if c.has_desc() { theme::icon::BOOK_OPEN_TEXT } else { theme::icon::CUBE };
    p.text(rect.left_center() + egui::vec2(18.0, 0.0), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(14.0), gc);
    let clip = egui::Rect::from_min_max(rect.min + egui::vec2(40.0, 0.0), rect.max - egui::vec2(8.0, 0.0));
    let pc = ui.painter_at(clip);
    pc.text(rect.left_top() + egui::vec2(40.0, 6.0), egui::Align2::LEFT_TOP, cat.craft_name(c), egui::FontId::proportional(14.0), theme::paper_2());
    let mut sub = format!("{} · {}", c.t0.as_deref().map(|t| &t[..t.len().min(10)]).unwrap_or("—"), c.id);
    let country = c.country();
    if !country.is_empty() {
        sub += &format!(" · {country}");
    }
    pc.text(rect.left_bottom() + egui::vec2(40.0, -7.0), egui::Align2::LEFT_BOTTOM, sub, egui::FontId::proportional(11.5), theme::paper_3());
    resp
}

/* ── wspólne ──────────────────────────────────────────────────────────── */

/// Opis z oznaczeniem języka oryginału i podziałem na akapity (śródtytuły „## ”).
fn long_text(ui: &mut egui::Ui, t: &Text) {
    if !t.translated {
        ui.label(RichText::new(format!("{} {}", theme::icon::TRANSLATE, tr("Opis w oryginale (angielski) – tłumaczenie w przygotowaniu."))).small().color(theme::WARN));
    }
    for para in t.text.split("\n\n") {
        if let Some(h) = para.strip_prefix("## ") {
            ui.add_space(4.0);
            ui.label(RichText::new(h).strong().color(theme::BRASS));
        } else {
            ui.add(egui::Label::new(RichText::new(para).color(theme::paper_2())).wrap());
        }
    }
}

fn sources(ui: &mut egui::Ui, links: &[(&str, String)]) {
    section(ui, "Źródła");
    ui.horizontal_wrapped(|ui| {
        for (label, url) in links {
            ui.hyperlink_to(format!("{} {}", theme::icon::ARROW_SQUARE_OUT, label), url);
        }
    });
}

/// Źródła danych programu z licencjami: (nazwa, adres, co daje, licencja, adres licencji).
const DATA_SOURCES: &[(&str, &str, &str, &str, &str)] = &[
    ("NASA/JPL Horizons", "https://ssd.jpl.nasa.gov/horizons/", "trajektorie sond", "domena publiczna", "https://www.nasa.gov/nasa-brand-center/images-and-media/"),
    ("JPL Small-Body Database", "https://ssd.jpl.nasa.gov/tools/sbdb_lookup.html", "planetoidy i komety", "domena publiczna", "https://www.nasa.gov/nasa-brand-center/images-and-media/"),
    ("VSOP87 · ELP-2000/82", "https://cdsarc.cds.unistra.fr/viz-bin/cat/VI/81", "ruch planet i Księżyca (Bretagnon, Francou; Meeus)", "dane naukowe", ""),
    ("Orbiter (Martin Schweiger)", "https://github.com/orbitersim/orbiter", "parametry ciał, obrót, tekstury", "MIT", "https://github.com/orbitersim/orbiter/blob/main/LICENSE"),
    ("Yale Bright Star Catalogue", "http://tdc-www.harvard.edu/catalogs/bsc5.html", "gwiazdy", "dane naukowe", ""),
    ("CelesTrak", "https://celestrak.org/", "SATCAT, elementy orbit GP (SGP4)", "publiczne", "https://celestrak.org/webmaster.php"),
    ("Wikidata", "https://www.wikidata.org/", "archiwum: nazwy, daty, operatorzy, masa, zdjęcia", "CC0 1.0", "https://creativecommons.org/publicdomain/zero/1.0/"),
    ("NASA NSSDCA Master Catalog", "https://nssdc.gsfc.nasa.gov/nmc/", "archiwum: pełne opisy statków", "domena publiczna", "https://www.nasa.gov/nasa-brand-center/images-and-media/"),
    ("Launch Library 2 (The Space Devs)", "https://thespacedevs.com/llapi", "historia startów, opisy i zdjęcia startów", "z podaniem źródła", "https://thespacedevs.com/"),
    ("Wikimedia Commons", "https://commons.wikimedia.org/", "zdjęcia statków (autor i licencja przy każdym)", "licencje wolne (różne)", "https://commons.wikimedia.org/wiki/Commons:Licensing"),
    ("Wikipedia", "https://www.wikipedia.org/", "odnośniki do artykułów", "CC BY-SA 4.0", "https://creativecommons.org/licenses/by-sa/4.0/"),
];

/// Lista źródeł danych z licencjami (Ustawienia, Archiwum).
pub fn sources_and_licenses(ui: &mut egui::Ui) {
    for (name, url, what, lic, lic_url) in DATA_SOURCES {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.hyperlink_to(RichText::new(*name).small().strong(), *url);
            ui.label(RichText::new(format!("– {} ·", tr(what))).small().color(theme::paper_3()));
            if lic_url.is_empty() {
                ui.label(RichText::new(tr(lic)).small().color(theme::paper_2()));
            } else {
                ui.hyperlink_to(RichText::new(tr(lic)).small(), *lic_url);
            }
        });
    }
    theme::para(ui, "Tłumaczenia opisów na polski: OpenSpacecraft. Zdjęcia z sieci są podpisane autorem i licencją; bez znanej licencji nie są pokazywane.");
}

fn terms(list: &[String]) -> String {
    let cat = catalog::get();
    list.iter().map(|t| cat.term(t)).collect::<Vec<_>>().join(", ")
}

/// Dodatki do karty misji opracowanej ręcznie: zdjęcie, pełny opis NSSDCA, źródła.
pub fn mission_photo(ui: &mut egui::Ui, mission_id: &str) {
    let cat = catalog::get();
    if let Some(c) = cat.craft_of_mission(mission_id).map(|i| &cat.crafts[i]) {
        if let Some(url) = c.photo_url() {
            crate::photos::show(ui, &url, Some(&c.photo_credit()));
        }
    }
}

pub fn mission_extra(app: &mut App, ui: &mut egui::Ui, mission_id: &str) {
    let cat = catalog::get();
    let Some(i) = cat.craft_of_mission(mission_id) else { return };
    let c = &cat.crafts[i];
    if let Some(d) = cat.craft_desc(c) {
        ui.add_space(6.0);
        egui::CollapsingHeader::new(RichText::new(tr("Pełny opis (NASA NSSDCA)")).strong()).id_salt("nssdca").show(ui, |ui| long_text(ui, &d));
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("COSPAR")).color(theme::paper_3()));
        if ui.link(&c.id).on_hover_text(tr("Pokaż w archiwum")).clicked() {
            app.selection = Selection::Craft(i);
            app.prefs.tab = crate::app::Tab::Missions;
            app.prefs.archive = true;
            app.search.clear();
            app.scroll_to_selected = true;
        }
    });
    sources(ui, &c.sources());
}

/* ── karta statku ─────────────────────────────────────────────────────── */

pub fn craft(app: &mut App, ui: &mut egui::Ui, i: usize) {
    let cat = catalog::get();
    let Some(c) = cat.crafts.get(i) else { return };
    ui.label(RichText::new(cat.craft_name(c)).size(22.0).strong());
    if cat.craft_name(c) != c.name {
        ui.label(RichText::new(&c.name).color(theme::paper_3()));
    }
    ui.horizontal_wrapped(|ui| {
        for t in c.types.iter().take(2) {
            chip(ui, cat.term(t), Color32::from_white_alpha(18), theme::PAPER);
        }
        if c.m.is_some() {
            chip(ui, tr("misja w atlasie 3D"), theme::BRASS_DIM, theme::PAPER);
        }
    });
    if let Some(mid) = &c.m {
        if let Some(mi) = missions::index_of(mid) {
            if ui.add(theme::primary(&theme::ic(theme::icon::PLAY, "Pokaż misję w 3D"))).clicked() {
                app.select_mission(mi, true);
            }
        }
    }
    if let Some(n) = c.norad {
        if app.sats.current().find(n).is_some() && ui.button(theme::ic(theme::icon::GLOBE_HEMISPHERE_WEST, "Pokaż na orbicie")).clicked() {
            app.select_sat(n);
        }
    }
    if let Some(url) = c.photo_url() {
        crate::photos::show(ui, &url, Some(&c.photo_credit()));
    }
    if let Some(s) = cat.craft_short(c) {
        theme::para(ui, s.text);
    }

    section(ui, "Dane");
    fact_mono(ui, "COSPAR", &c.id);
    if let Some(n) = c.norad {
        fact_mono(ui, "Katalog NORAD", n.to_string());
    }
    if let Some(j) = c.jd {
        fact_mono(ui, "Start", time::fmt_datetime(j));
    }
    if let Some(e) = c.end_jd {
        fact_mono(ui, "Koniec", time::fmt_datetime(e));
    }
    let country = c.country();
    if !country.is_empty() {
        fact(ui, "Kraj", country);
    }
    if !c.op.is_empty() {
        fact(ui, "Operator", c.op.join(", "));
    }
    if !c.maker.is_empty() {
        fact(ui, "Producent", c.maker.join(", "));
    }
    if let Some(v) = &c.veh {
        fact(ui, "Rakieta nośna", v);
    }
    if let Some(s) = &c.site {
        fact(ui, "Miejsce startu", s);
    }
    if let Some(m) = c.mass {
        fact_mono(ui, "Masa", fmt_mass(m));
    }
    if let Some(p) = &c.power {
        fact_mono(ui, "Moc", p);
    }
    if !c.disc.is_empty() {
        fact(ui, "Dziedziny", terms(&c.disc));
    }
    if !c.alt.is_empty() {
        fact(ui, "Inne nazwy", c.alt.join(", "));
    }
    if let Some(li) = c.launch.as_deref().and_then(|l| cat.launch(l)) {
        ui.horizontal(|ui| {
            ui.add_sized([150.0, 18.0], egui::Label::new(RichText::new(tr("Start rakiety")).color(theme::paper_3())));
            if ui.link(&cat.launches[li].name).clicked() {
                app.selection = Selection::Launch(li);
            }
        });
    }
    if let Some(d) = cat.craft_desc(c) {
        section(ui, "Opis");
        long_text(ui, &d);
    }
    sources(ui, &c.sources());
}

/* ── karta startu ─────────────────────────────────────────────────────── */

pub fn launch(app: &mut App, ui: &mut egui::Ui, i: usize) {
    let cat = catalog::get();
    let Some(l) = cat.launches.get(i) else { return };
    ui.label(RichText::new(l.mission.as_deref().unwrap_or(&l.name)).size(22.0).strong());
    ui.label(RichText::new(&l.name).color(theme::paper_3()));
    ui.horizontal_wrapped(|ui| {
        match l.success() {
            Some(true) => chip(ui, tr("udany"), theme::OK.gamma_multiply(0.3), theme::PAPER),
            Some(false) => chip(ui, tr("nieudany"), theme::SIGNAL.gamma_multiply(0.3), theme::PAPER),
            None => {}
        }
        if let Some(k) = &l.kind {
            chip(ui, cat.term(k), Color32::from_white_alpha(18), theme::PAPER);
        }
    });
    if ui.button(theme::ic(theme::icon::CLOCK_COUNTER_CLOCKWISE, "Przejdź do chwili startu")).clicked() {
        app.clock.jd = l.jd;
        app.clock.playing = false;
    }
    if let Some(url) = &l.img {
        crate::photos::show(ui, url, Some(&l.photo_credit()));
    }
    if let Some(d) = cat.launch_desc(l) {
        long_text(ui, &d);
    }
    section(ui, "Dane startu");
    fact_mono(ui, "Data", time::fmt_datetime(l.jd));
    if let Some(r) = &l.rocket {
        fact(ui, "Rakieta nośna", r);
    }
    if let Some(p) = &l.provider {
        fact(ui, "Operator startu", p);
    }
    if let Some(c) = &l.cc {
        fact(ui, "Kraj", missions::country_name(c));
    }
    if let Some(s) = &l.site {
        fact(ui, "Kosmodrom", s);
    }
    if let Some(p) = &l.pad {
        fact(ui, "Stanowisko", p);
    }
    if let (Some(a), Some(b)) = (l.lat, l.lon) {
        fact_mono(ui, "Współrzędne", format!("{a:.3}°, {b:.3}°"));
    }
    if let Some(o) = &l.orbit {
        fact(ui, "Orbita docelowa", cat.term(o));
    }
    if let Some(f) = &l.fail {
        fact(ui, "Przyczyna awarii", f);
    }
    if let Some(d) = &l.desig {
        let pays = cat.payloads(d);
        if !pays.is_empty() {
            section(ui, "Wyniesione obiekty");
            for k in pays.into_iter().take(40) {
                let c = &cat.crafts[k];
                if theme::row_button(ui, theme::icon::CUBE, &format!("{}  ·  {}", c.id, cat.craft_name(c)), false).clicked() {
                    app.selection = Selection::Craft(k);
                }
            }
        }
    }
    sources(ui, &[("Launch Library 2 (The Space Devs)", l.source_url())]);
}

/// Starty z katalogu w danym roku (lista w zakładce „Starty”).
pub fn launches_of_year(app: &mut App, ui: &mut egui::Ui, a: f64, b: f64) {
    let cat = catalog::get();
    let from = cat.launches.partition_point(|l| l.jd < a);
    let to = cat.launches.partition_point(|l| l.jd < b);
    if from >= to {
        return;
    }
    section(ui, &tf!("Wszystkie starty roku ({})", to - from));
    for i in from..to {
        let l = &cat.launches[i];
        let glyph = match l.success() {
            Some(false) => theme::icon::X_CIRCLE,
            _ => theme::icon::ROCKET,
        };
        let text = format!("{}  ·  {}", &l.t[5..10], l.name);
        if theme::row_button(ui, glyph, &text, app.selection == Selection::Launch(i)).clicked() {
            app.selection = Selection::Launch(i);
        }
    }
}
