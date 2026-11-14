//! Wybór daty i godziny symulacji: rok, miesiąc, kalendarz dni, godzina, wpisanie wprost,
//! szybkie skoki i ważne daty. Od daty zależą satelity, starty i położenia ciał.

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use eframe::egui::{self, Color32, RichText};

use super::theme;
use crate::app::{App, JD_MAX, JD_MIN};
use crate::astro::time;
use crate::i18n::tr;

const MONTHS_SHORT: [&str; 12] = ["sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru"];
const WEEKDAYS: [&str; 7] = ["Pn", "Wt", "Śr", "Cz", "Pt", "So", "Nd"];

/// Ważne chwile w historii lotów kosmicznych (data, opis).
const MOMENTS: [(&str, &str); 8] = [
    ("1957-10-04T19:28:34", "Sputnik 1 – początek ery kosmicznej"),
    ("1961-04-12T06:07", "Gagarin – pierwszy człowiek w kosmosie"),
    ("1969-07-20T20:17:40", "Apollo 11 – lądowanie na Księżycu"),
    ("1977-09-05T12:56", "Start Voyagera 1"),
    ("1990-04-25T19:38", "Teleskop Hubble’a na orbicie"),
    ("1998-11-20T06:40", "Pierwszy moduł ISS"),
    ("2012-08-06T05:17:57", "Curiosity na Marsie"),
    ("2021-12-25T12:20", "Start Teleskopu Webba"),
];

fn set(app: &mut App, dt: NaiveDateTime) {
    app.clock.jd = time::from_datetime(&dt).clamp(JD_MIN, JD_MAX);
}

fn with_date(cur: NaiveDateTime, y: i32, m: u32, d: u32) -> Option<NaiveDateTime> {
    // dzień przycięty do długości miesiąca (31 marca → 30 kwietnia)
    let last = (28..=31).rev().find(|&x| NaiveDate::from_ymd_opt(y, m, x).is_some())?;
    NaiveDate::from_ymd_opt(y, m, d.min(last)).map(|nd| nd.and_time(cur.time()))
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let cur = time::to_datetime(app.clock.jd);
    let (y, m, d) = (cur.year(), cur.month(), cur.day());
    ui.horizontal(|ui| {
        ui.label(RichText::new(theme::icon::CALENDAR_BLANK).size(18.0).color(theme::BRASS));
        ui.label(RichText::new(tr("Data symulacji")).size(16.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new("UTC").small().color(theme::paper_3()));
        });
    });
    theme::para(ui, "Od daty zależy wszystko w widoku: położenia planet, satelity na orbicie i starty w lewym panelu. Misje przeglądasz osobno.");
    ui.add_space(4.0);

    // rok
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (txt, dy) in [("−10", -10), ("−1", -1)] {
            if ui.button(txt).on_hover_text(tr("Rok wcześniej")).clicked() {
                if let Some(nd) = with_date(cur, y + dy, m, d) {
                    set(app, nd);
                }
            }
        }
        let mut yy = y;
        let r = ui.add(egui::DragValue::new(&mut yy).range(1957..=2099).speed(0.2).custom_formatter(|v, _| format!("{v:.0}")));
        if r.changed() && yy != y {
            if let Some(nd) = with_date(cur, yy, m, d) {
                set(app, nd);
            }
        }
        for (txt, dy) in [("+1", 1), ("+10", 10)] {
            if ui.button(txt).on_hover_text(tr("Rok później")).clicked() {
                if let Some(nd) = with_date(cur, y + dy, m, d) {
                    set(app, nd);
                }
            }
        }
    });
    ui.add_space(4.0);

    // miesiące
    egui::Grid::new("miesiace").num_columns(6).spacing([4.0, 4.0]).show(ui, |ui| {
        for (k, name) in MONTHS_SHORT.iter().enumerate() {
            let on = k as u32 + 1 == m;
            if ui.add_sized([60.0, 26.0], egui::Button::new(RichText::new(tr(name)).color(if on { theme::PAPER } else { theme::paper_2() })).fill(if on { theme::BRASS_DIM } else { Color32::from_white_alpha(8) })).clicked() {
                if let Some(nd) = with_date(cur, y, k as u32 + 1, d) {
                    set(app, nd);
                }
            }
            if k % 6 == 5 {
                ui.end_row();
            }
        }
    });
    ui.add_space(6.0);

    // kalendarz dni
    if let Some(first) = NaiveDate::from_ymd_opt(y, m, 1) {
        let offset = first.weekday().num_days_from_monday() as usize;
        let days = (28..=31).rev().find(|&x| NaiveDate::from_ymd_opt(y, m, x).is_some()).unwrap_or(28) as usize;
        egui::Grid::new("dni").num_columns(7).spacing([4.0, 3.0]).show(ui, |ui| {
            for w in WEEKDAYS {
                ui.add_sized([48.0, 16.0], egui::Label::new(RichText::new(tr(w)).small().color(theme::paper_3())));
            }
            ui.end_row();
            let mut col = 0;
            for _ in 0..offset {
                ui.label("");
                col += 1;
            }
            for day in 1..=days {
                let on = day as u32 == d;
                let today = {
                    let n = time::to_datetime(time::now());
                    n.year() == y && n.month() == m && n.day() == day as u32
                };
                let mut txt = RichText::new(day.to_string()).monospace();
                if today {
                    txt = txt.color(theme::OK);
                }
                if ui.add_sized([48.0, 26.0], egui::Button::new(txt).fill(if on { theme::BRASS_DIM } else { Color32::from_white_alpha(6) })).clicked() {
                    if let Some(nd) = with_date(cur, y, m, day as u32) {
                        set(app, nd);
                    }
                }
                col += 1;
                if col % 7 == 0 {
                    ui.end_row();
                }
            }
        });
    }
    ui.add_space(6.0);

    // godzina
    ui.horizontal(|ui| {
        ui.label(RichText::new(theme::icon::CLOCK).color(theme::paper_2()));
        let (mut h, mut mi, mut s) = (cur.hour(), cur.minute(), cur.second());
        let a = ui.add(egui::DragValue::new(&mut h).range(0..=23).speed(0.1).custom_formatter(|v, _| format!("{v:02.0}")));
        ui.label(":");
        let b = ui.add(egui::DragValue::new(&mut mi).range(0..=59).speed(0.2).custom_formatter(|v, _| format!("{v:02.0}")));
        ui.label(":");
        let c = ui.add(egui::DragValue::new(&mut s).range(0..=59).speed(0.2).custom_formatter(|v, _| format!("{v:02.0}")));
        if a.changed() || b.changed() || c.changed() {
            if let Some(t) = chrono::NaiveTime::from_hms_opt(h, mi, s) {
                set(app, cur.date().and_time(t));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (g, hint) = if app.clock.playing { (theme::icon::PAUSE, "Zatrzymaj czas") } else { (theme::icon::PLAY, "Puść czas") };
            if ui.button(theme::ic(g, hint)).clicked() {
                app.clock.playing = !app.clock.playing;
            }
        });
    });
    ui.add_space(4.0);

    // skoki
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for (txt, days) in [("−1 mies.", -30.4375), ("−1 d", -1.0), ("−1 h", -1.0 / 24.0), ("+1 h", 1.0 / 24.0), ("+1 d", 1.0), ("+1 mies.", 30.4375)] {
            if ui.small_button(tr(txt)).clicked() {
                app.clock.jd = (app.clock.jd + days).clamp(JD_MIN, JD_MAX);
            }
        }
        if ui.add(egui::Button::new(theme::ic(theme::icon::CLOCK_COUNTER_CLOCKWISE, "Teraz")).small().fill(theme::BRASS_DIM)).on_hover_text(tr("Teraz, czas rzeczywisty (N)")).clicked() {
            app.clock.jd = time::now();
            app.clock.rate = 1.0;
            app.clock.playing = true;
        }
    });
    ui.add_space(4.0);

    // wpisanie wprost
    ui.horizontal(|ui| {
        let text = app.date_text.get_or_insert_with(|| time::fmt_datetime_s(app.clock.jd));
        let r = ui.add(egui::TextEdit::singleline(text).desired_width(210.0).font(egui::TextStyle::Monospace).hint_text("RRRR-MM-DD GG:MM"));
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let go = ui.button(tr("Przejdź")).clicked();
        if enter || go {
            match time::parse(text) {
                Some(jd) => {
                    app.clock.jd = jd.clamp(JD_MIN, JD_MAX);
                    app.date_text = None;
                }
                None => app.notify(tr("Nie rozpoznano daty – użyj formatu RRRR-MM-DD GG:MM")),
            }
        } else if !r.has_focus() {
            // pole pokazuje bieżącą chwilę, dopóki nikt w nim nie pisze
            app.date_text = None;
        }
    });

    ui.add_space(6.0);
    super::section(ui, "Ważne chwile");
    for (t, what) in MOMENTS {
        let Some(jd) = time::parse(t) else { continue };
        if theme::row_button(ui, theme::icon::STAR_FOUR, &format!("{}  ·  {}", time::fmt_date(jd), tr(what)), false).clicked() {
            app.clock.jd = jd;
            app.clock.playing = false;
        }
    }
}
