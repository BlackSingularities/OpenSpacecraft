//! Okna: ustawienia (z „O programie”), pomoc ze skrótami i przewodnik pierwszego uruchomienia.

use eframe::egui::{self, RichText};

use super::theme;
use crate::app::App;
use crate::i18n::{self, tr};
use crate::updater;

pub fn windows(app: &mut App, ctx: &egui::Context) {
    settings(app, ctx);
    help(app, ctx);
    tour(app, ctx);
}

fn settings(app: &mut App, ctx: &egui::Context) {
    if !app.settings_open {
        return;
    }
    let mut open = true;
    let mut changed = false;
    let mut check = false;
    let mut report = false;
    egui::Window::new(tr("Ustawienia")).open(&mut open).collapsible(false).resizable(false).default_width(420.0).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.label(theme::kicker("Język"));
        let cur = if app.settings.lang.is_empty() { i18n::system().to_string() } else { app.settings.lang.clone() };
        ui.horizontal(|ui| {
            for l in i18n::LANGS {
                if ui.selectable_label(cur == l.code, l.name).clicked() {
                    app.settings.lang = l.code.to_string();
                    i18n::set(l.code);
                    changed = true;
                }
            }
        });
        ui.add_space(6.0);
        ui.label(theme::kicker("Wygląd"));
        if ui.add(egui::Slider::new(&mut app.settings.stars, 0.0..=2.0).text(tr("jasność gwiazd"))).changed() {
            changed = true;
        }
        if ui.add(egui::Slider::new(&mut app.settings.render_scale, 0.5..=2.0).text(tr("rozdzielczość renderu 3D"))).changed() {
            changed = true;
        }
        let mut z = app.settings.ui_scale;
        if ui.add(egui::Slider::new(&mut z, 0.75..=2.0).text(tr("skala interfejsu"))).drag_stopped() {
            app.settings.ui_scale = z;
            ctx.set_zoom_factor(z);
            changed = true;
        }
        if theme::switch(ui, &mut app.settings.high_contrast, "Wysoki kontrast").changed() {
            theme::set_high_contrast(ctx, app.settings.high_contrast);
            changed = true;
        }
        if theme::switch(ui, &mut app.settings.vsync, "Synchronizacja z monitorem (po ponownym uruchomieniu)").changed() {
            changed = true;
        }
        ui.add_space(6.0);
        ui.label(theme::kicker("Aktualizacje"));
        if theme::switch(ui, &mut app.settings.auto_update, "Sprawdzaj nowe wersje przy starcie").changed() {
            changed = true;
        }
        ui.horizontal(|ui| {
            if ui.button(theme::ic(theme::icon::ARROWS_CLOCKWISE, "Sprawdź teraz")).clicked() {
                check = true;
            }
            ui.label(RichText::new(tf!("wersja {}", updater::current())).monospace().color(theme::paper_3()));
        });
        ui.add_space(6.0);
        ui.label(theme::kicker("O programie"));
        ui.label(RichText::new(tf!("OpenSpacecraft {}", env!("CARGO_PKG_VERSION"))).size(18.0).strong());
        theme::para(ui, "Atlas misji kosmicznych 3D: Układ Słoneczny w dowolnej chwili od 1957 r., odtwarzanie przebiegu misji, satelity na orbicie i wszystkie starty orbitalne.");
        ui.horizontal(|ui| {
            if ui.button(theme::ic(theme::icon::BUG, "Zgłoś błąd lub sugestię…")).clicked() {
                report = true;
            }
            if ui.button(theme::ic(theme::icon::FILE_TEXT, "Dziennik programu")).clicked() {
                let _ = open::that(crate::log_path().parent().unwrap_or(std::path::Path::new(".")));
            }
        });
        ui.add_space(6.0);
        ui.label(theme::kicker("Autor"));
        if ui.link("redzikowski.com").clicked() {
            let _ = open::that("https://redzikowski.com/portfolio/openspacecraft");
        }
        ui.label(RichText::new("kontakt@redzikowski.com").small().color(theme::paper_3()));
        if ui.add(theme::primary(&theme::ic(theme::icon::COFFEE, "Postaw kawę"))).clicked() {
            let _ = open::that("https://buycoffee.to/adamredzikowski");
        }
        ui.add_space(6.0);
        ui.label(theme::kicker("Źródła danych"));
        super::archive::sources_and_licenses(ui);
    });
    if !open {
        app.settings_open = false;
    }
    if changed {
        let _ = app.settings.save();
    }
    if check {
        app.check_updates(true);
    }
    if report {
        app.feedback.open();
    }
}

fn help(app: &mut App, ctx: &egui::Context) {
    if !app.help_open {
        return;
    }
    let mut open = true;
    egui::Window::new(tr("Pomoc i skróty")).open(&mut open).collapsible(false).resizable(false).default_width(460.0).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        ui.set_width(460.0);
        theme::para(ui, "Przeciągnij myszą, aby obracać kamerę wokół celu, kółkiem przybliżaj i oddalaj. Kliknięcie zaznacza obiekt, dwuklik kieruje na niego kamerę. Oś czasu na dole przesuwa chwilę symulacji – od startu Sputnika do 2100 r.");
        ui.add_space(6.0);
        egui::Grid::new("skroty").num_columns(2).spacing([18.0, 6.0]).show(ui, |ui| {
            for (k, d) in [
                ("Spacja", "odtwarzanie / pauza"),
                (", .", "wolniej / szybciej"),
                ("R", "odwróć kierunek czasu"),
                ("N", "teraz, czas rzeczywisty"),
                ("F", "kamera na zaznaczony obiekt"),
                ("Ctrl+K  /", "wyszukiwarka"),
                ("D", "wybór daty"),
                ("M", "okno misji"),
                ("Esc", "odznacz / zamknij"),
                ("L  O  S", "etykiety / orbity / satelity"),
                ("← → ↑ ↓", "obrót kamery"),
                ("F1", "ta pomoc"),
            ] {
                ui.label(RichText::new(k).monospace().color(theme::BRASS));
                ui.label(tr(d));
                ui.end_row();
            }
        });
        ui.add_space(6.0);
        theme::para(ui, "Trasy sond z oznaczeniem „JPL Horizons” to rzeczywiste położenia. Pozostałe (np. Apollo, Wostok, Łuna) są odtworzone modelem fizycznym: wznoszenie, orbita parkingowa, łuki Keplera wyznaczone z zagadnienia Lamberta między rzeczywistymi położeniami ciał, przeloty i orbity wokół celu.");
    });
    if !open {
        app.help_open = false;
    }
}

const TOUR: [(&str, &str); 4] = [
    ("Witaj w OpenSpacecraft", "To trójwymiarowy atlas misji kosmicznych. Widzisz Ziemię w tej chwili – razem z tysiącami satelitów krążących po orbicie."),
    ("Data", "Najważniejsza jest data na środku górnego paska – kliknij ją (albo naciśnij D), by wybrać dzień i godzinę. Satelity, starty i położenia planet w lewym panelu pokazują stan z tej chwili. Zegar na dole przyspiesza, cofa i przesuwa czas."),
    ("Misje", "Przycisk Misje (M) otwiera osobne okno: przeglądaj, filtruj (załogowe, bezzałogowe, kraj, dekada, rodzaj) i sortuj – bez zmiany daty. Wybraną misję odtworzysz od startu: zobaczysz każdy etap, odłączane człony i moduły."),
    ("Szczegóły", "Kliknij planetę, statek albo satelitę, by zobaczyć jego dane i odczyty na żywo: odległość, prędkość, wysokość i opóźnienie sygnału."),
];

fn tour(app: &mut App, ctx: &egui::Context) {
    let Some(step) = app.tour else { return };
    let (title, text) = TOUR[step.min(TOUR.len() - 1)];
    let mut next = None;
    egui::Area::new(egui::Id::new("przewodnik")).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).order(egui::Order::Foreground).show(ctx, |ui| {
        theme::glass().inner_margin(egui::Margin::same(18)).show(ui, |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new(tf!("{} / {}", step + 1, TOUR.len())).small().color(theme::paper_3()));
            ui.label(RichText::new(tr(title)).size(20.0).strong());
            ui.add_space(4.0);
            theme::para(ui, text);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(theme::primary(if step + 1 < TOUR.len() { "Dalej" } else { "Zaczynamy" })).clicked() {
                    next = Some(if step + 1 < TOUR.len() { Some(step + 1) } else { None });
                }
                if step + 1 < TOUR.len() && ui.button(tr("Pomiń")).clicked() {
                    next = Some(None);
                }
            });
        });
    });
    if let Some(n) = next {
        app.tour = n;
        if n.is_none() {
            app.settings.tour_done = true;
            let _ = app.settings.save();
        }
    }
}
