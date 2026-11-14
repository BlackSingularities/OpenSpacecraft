//! Główna aplikacja: widok 3D na całe okno i unoszący się nad nim interfejs (jak w GRANI):
//! górny pasek, katalog po lewej, szczegóły po prawej i oś czasu na dole.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::astro::bodies::{self, BodyId, EARTH};
use crate::astro::time;
use crate::missions::traj::{self, Trajectory};
use crate::missions::{self, Kind};
use crate::render::camera::Camera;
use crate::render::Renderer;
use crate::sats;
use crate::settings::Settings;
use crate::ui::theme;
use crate::updater;

/// Na co patrzy kamera.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Focus {
    Body(BodyId),
    Mission(usize),
    Sat(u32),
}

/// Co jest zaznaczone (prawy panel).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Selection {
    None,
    Mission(usize),
    Sat(u32),
    Body(BodyId),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Tab {
    Missions,
    Satellites,
    Bodies,
    Launches,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Layers {
    pub orbits: bool,
    pub labels: bool,
    /// Wszystkie misje trwające w danej chwili (znaczniki).
    pub missions: bool,
    pub trajectory: bool,
    pub sats: bool,
    pub stars: bool,
    pub small_bodies: bool,
}

impl Default for Layers {
    fn default() -> Self {
        Self { orbits: true, labels: true, missions: true, trajectory: true, sats: true, stars: true, small_bodies: true }
    }
}

/// Stan widoku zapisywany między uruchomieniami.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub layers: Layers,
    pub tab: Tab,
    pub catalog_open: bool,
    pub sat_filter: sats::Filter,
    pub kinds: Vec<Kind>,
    pub only_active: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { layers: Layers::default(), tab: Tab::Missions, catalog_open: true, sat_filter: sats::Filter::default(), kinds: Vec::new(), only_active: false }
    }
}

impl Serialize for Kind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(Kind::ALL.iter().position(|k| k == self).unwrap_or(0) as u8)
    }
}

impl<'de> Deserialize<'de> for Kind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let i = u8::deserialize(d)? as usize;
        Ok(Kind::ALL[i.min(Kind::ALL.len() - 1)])
    }
}

/// Zegar symulacji.
pub struct Clock {
    pub jd: f64,
    /// Sekundy symulacji na sekundę rzeczywistą (ujemne – wstecz).
    pub rate: f64,
    pub playing: bool,
}

pub const JD_MIN: f64 = 2_435_839.5; // 1957-01-01
pub const JD_MAX: f64 = 2_488_069.5; // 2100-01-01

/// Stan aktualizacji programu.
pub enum Update {
    Idle,
    Checking(Arc<Mutex<Option<Result<updater::Release, String>>>>, bool),
    Available(updater::Release),
    Downloading(updater::Release, Arc<AtomicU64>, Arc<Mutex<Option<Result<PathBuf, String>>>>),
}

/// Trajektorie liczone w tle.
#[derive(Clone, Default)]
pub struct Trajs {
    pub done: Arc<Mutex<HashMap<usize, Arc<Trajectory>>>>,
    pending: Arc<Mutex<HashSet<usize>>>,
}

impl Trajs {
    pub fn get(&self, i: usize) -> Option<Arc<Trajectory>> {
        self.done.lock().ok()?.get(&i).cloned()
    }

    /// Zleca policzenie trajektorii (jeśli jeszcze nie ma).
    pub fn request(&self, i: usize, ctx: &egui::Context) {
        if self.done.lock().is_ok_and(|d| d.contains_key(&i)) {
            return;
        }
        if !self.pending.lock().is_ok_and(|mut p| p.insert(i)) {
            return;
        }
        let (done, pending, ctx) = (self.done.clone(), self.pending.clone(), ctx.clone());
        std::thread::spawn(move || {
            let m = &missions::db().missions[i];
            let t = Arc::new(traj::build(m));
            if let Ok(mut d) = done.lock() {
                d.insert(i, t);
            }
            if let Ok(mut p) = pending.lock() {
                p.remove(&i);
            }
            ctx.request_repaint();
        });
    }

    pub fn snapshot(&self) -> HashMap<usize, Arc<Trajectory>> {
        self.done.lock().map(|d| d.clone()).unwrap_or_default()
    }
}

/// Pamięć linii orbit ciał (punkty względem rodzica) – liczona rzadko, bo VSOP jest kosztowny.
#[derive(Default)]
pub struct OrbitCache {
    pub entries: HashMap<BodyId, (f64, Vec<glam::DVec3>)>,
}

pub struct App {
    pub settings: Settings,
    pub prefs: Prefs,
    pub renderer: Option<Renderer>,
    pub cam: Camera,
    pub focus: Focus,
    pub selection: Selection,
    pub clock: Clock,
    pub trajs: Trajs,
    pub sats: sats::Loader,
    pub sat_visible: Option<(f64, sats::Visible, Arc<sats::Catalog>)>,
    pub orbit_cache: OrbitCache,
    pub search: String,
    pub sat_search: String,
    pub body_search: String,
    /// Rok przeglądany w zakładce startów (None – rok chwili symulacji).
    pub launch_year: Option<i32>,
    /// Zaznaczony start z katalogu (prefiks COSPAR).
    pub launch_sel: Option<String>,
    pub palette: Option<crate::ui::palette::Palette>,
    pub glass: Vec<(egui::Rect, f32)>,
    pub last_frame: Instant,
    pub toast: Option<(String, Instant)>,
    pub settings_open: bool,
    pub help_open: bool,
    pub update: Update,
    pub feedback: crate::feedback::FeedbackUi,
    pub date_edit: Option<String>,
    pub hover: Option<crate::ui::scene::Pick>,
    pub picks: Vec<crate::ui::scene::Pick>,
    pub view_rect: egui::Rect,
    pub tour: Option<usize>,
    /// Zrzut okna do pliku (OPENSPACECRAFT_CAPTURE=plik.png) – do dokumentacji.
    pub capture: Option<(PathBuf, Instant, f32, bool)>,
    pub scroll_to_selected: bool,
    /// Misja, na którą kamera ma się skierować, gdy jej trajektoria będzie gotowa.
    pub pending_focus: Option<usize>,
    /// Odległość kamery narzucona przez scenę startową (zrzuty).
    pub scene_dist: Option<f64>,
}

const PREFS_KEY: &str = "openspacecraft-1";

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let settings = Settings::load();
        theme::set_high_contrast(&cc.egui_ctx, settings.high_contrast);
        cc.egui_ctx.set_zoom_factor(settings.ui_scale.clamp(0.75, 2.0));
        let prefs = cc.storage.and_then(|s| eframe::get_value::<Prefs>(s, PREFS_KEY)).unwrap_or_default();
        updater::cleanup();
        let trajs = Trajs::default();
        // wszystkie trajektorie w tle (znaczniki misji trwających w danej chwili)
        {
            let ctx = cc.egui_ctx.clone();
            let t = trajs.clone();
            std::thread::spawn(move || {
                let n = missions::db().missions.len();
                // najpierw misje okołoziemskie i księżycowe (szybkie), potem planetarne
                for i in 0..n {
                    t.request(i, &ctx);
                    std::thread::sleep(std::time::Duration::from_millis(15));
                }
            });
        }
        let capture = std::env::var_os("OPENSPACECRAFT_CAPTURE").map(|p| {
            (PathBuf::from(p), Instant::now(), std::env::var("OPENSPACECRAFT_CAPTURE_AFTER").ok().and_then(|s| s.parse().ok()).unwrap_or(8.0), false)
        });
        let mut app = Self {
            sats: sats::Loader::new(cc.egui_ctx.clone()),
            settings,
            prefs,
            renderer: None,
            cam: Camera::default(),
            focus: Focus::Body(EARTH),
            selection: Selection::None,
            clock: Clock { jd: time::now(), rate: 1.0, playing: true },
            trajs,
            sat_visible: None,
            orbit_cache: OrbitCache::default(),
            search: String::new(),
            sat_search: String::new(),
            body_search: String::new(),
            launch_year: None,
            launch_sel: None,
            palette: None,
            glass: Vec::new(),
            last_frame: Instant::now(),
            toast: None,
            settings_open: false,
            help_open: false,
            update: Update::Idle,
            feedback: Default::default(),
            date_edit: None,
            hover: None,
            picks: Vec::new(),
            view_rect: egui::Rect::NOTHING,
            tour: None,
            capture,
            scroll_to_selected: false,
            pending_focus: None,
            scene_dist: None,
        };
        app.tour = if app.settings.tour_done || app.capture.is_some() { None } else { Some(0) };
        // scena startowa (także do zrzutów): OPENSPACECRAFT_SCENE=misja@data
        if let Ok(scene) = std::env::var("OPENSPACECRAFT_SCENE") {
            app.apply_scene(&scene);
        }
        if app.settings.auto_update && app.capture.is_none() {
            app.check_updates(false);
        }
        app
    }

    /// `apollo-11@1969-07-20T21:00` albo `earth` – wybór i chwila (zrzuty ekranu, testy).
    fn apply_scene(&mut self, s: &str) {
        let (what, when) = s.split_once('@').unwrap_or((s, ""));
        if let Some(jd) = time::parse(when) {
            self.clock.jd = jd;
        }
        if let Some(i) = missions::index_of(what) {
            self.select_mission(i, false);
            self.clock.playing = std::env::var("OPENSPACECRAFT_PLAY").is_ok();
        } else if let Some(b) = bodies::find(what) {
            self.select_body(b);
        }
        if let Some(d) = std::env::var("OPENSPACECRAFT_DIST").ok().and_then(|d| d.parse::<f64>().ok()) {
            self.cam.dist = d;
            self.scene_dist = Some(d);
        }
        if let Some(y) = std::env::var("OPENSPACECRAFT_YAW").ok().and_then(|d| d.parse::<f64>().ok()) {
            self.cam.yaw = y.to_radians();
        }
        if let Some(p) = std::env::var("OPENSPACECRAFT_PITCH").ok().and_then(|d| d.parse::<f64>().ok()) {
            self.cam.pitch = p.to_radians();
        }
        if let Ok(t) = std::env::var("OPENSPACECRAFT_TAB") {
            self.prefs.tab = match t.as_str() {
                "sats" => Tab::Satellites,
                "bodies" => Tab::Bodies,
                "launches" => Tab::Launches,
                _ => Tab::Missions,
            };
        }
    }

    pub fn notify(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), Instant::now()));
    }

    /* ── wybór i kamera ─────────────────────────────────────────────── */

    /// Położenie celu kamery (względem Słońca).
    pub fn focus_pos(&self, f: Focus, jd: f64) -> Option<glam::DVec3> {
        match f {
            Focus::Body(b) => Some(bodies::position(b, jd)),
            Focus::Mission(i) => self.trajs.get(i).and_then(|t| t.position(jd)),
            Focus::Sat(n) => {
                let cat = self.sats.current();
                let o = cat.find(n)?;
                o.position(jd, &cat.gp).map(|(p, _)| p + bodies::position(EARTH, jd))
            }
        }
    }

    /// Najmniejsza odległość kamery od celu.
    pub fn min_dist(&self) -> f64 {
        match self.focus {
            Focus::Body(b) => {
                let r = bodies::get(b).radius;
                if r > 0.0 { r * 1.06 } else { 1.0e6 }
            }
            Focus::Mission(_) | Focus::Sat(_) => 20.0,
        }
    }

    pub fn set_focus(&mut self, f: Focus, dist: Option<f64>) {
        let jd = self.clock.jd;
        let old = self.focus_pos(self.focus, jd);
        let new = self.focus_pos(f, jd);
        self.focus = f;
        match (old, new) {
            (Some(o), Some(n)) => self.cam.retarget(o, n, dist),
            _ => {
                if let Some(d) = dist {
                    self.cam.dist = d;
                }
            }
        }
    }

    pub fn select_body(&mut self, b: BodyId) {
        self.selection = Selection::Body(b);
        let r = bodies::get(b).radius;
        let d = if b == bodies::SUN { r * 12.0 } else if r > 0.0 { r * 4.2 } else { 3.0e9 };
        self.set_focus(Focus::Body(b), Some(d));
    }

    pub fn select_sat(&mut self, norad: u32) {
        self.selection = Selection::Sat(norad);
        let cat = self.sats.current();
        let d = cat.find(norad).map(|o| if o.class == sats::OrbitClass::Leo { 2.5e6 } else { 2.5e7 }).unwrap_or(2.5e6);
        self.set_focus(Focus::Sat(norad), Some(d));
    }

    /// Wybór misji; `from_start` – zegar na start i odtwarzanie.
    pub fn select_mission(&mut self, i: usize, from_start: bool) {
        let ctx_needed = self.trajs.get(i).is_none();
        self.selection = Selection::Mission(i);
        let m = &missions::db().missions[i];
        if from_start {
            self.clock.jd = m.launch_jd - 2.0 / 1440.0;
            self.clock.playing = true;
            self.clock.rate = default_rate(m);
        }
        if ctx_needed {
            // trajektoria jeszcze się liczy – kamera dołączy, gdy będzie gotowa
            self.pending_focus = Some(i);
            return;
        }
        self.pending_focus = None;
        let d = self.mission_view_dist(i);
        self.set_focus(Focus::Mission(i), Some(d));
    }

    /// Dobra odległość kamery dla misji w bieżącej chwili.
    pub fn mission_view_dist(&self, i: usize) -> f64 {
        let jd = self.clock.jd;
        let Some(t) = self.trajs.get(i) else { return 1.0e7 };
        let Some(p) = t.position(jd) else {
            return if t.frame == EARTH { 8.0e8 } else { 4.0e11 };
        };
        let near = traj::nearest_body(p, jd);
        if near == bodies::SUN {
            return (p.length() * 0.9).max(5.0e10);
        }
        let r = bodies::get(near).radius;
        let alt = (p - bodies::position(near, jd)).length() - r;
        if alt < r * 0.05 {
            // tuż przy powierzchni (lądownik, wznoszenie): widok z bliska
            (r * 0.4).max(3.0e5)
        } else {
            (r * 3.5 + alt * 2.0).min(bodies::soi(near, jd).max(r * 4.0))
        }
    }

    /* ── aktualizacje ───────────────────────────────────────────────── */

    pub fn check_updates(&mut self, manual: bool) {
        let slot = Arc::new(Mutex::new(None));
        let s = slot.clone();
        std::thread::spawn(move || {
            let r = updater::latest();
            if let Ok(mut g) = s.lock() {
                *g = Some(r);
            }
        });
        self.update = Update::Checking(slot, manual);
    }

    fn poll_updates(&mut self, ctx: &egui::Context) {
        match &self.update {
            Update::Checking(slot, manual) => {
                let manual = *manual;
                let done = slot.lock().ok().and_then(|mut g| g.take());
                match done {
                    Some(Ok(rel)) if updater::cmp_version(&rel.version, updater::current()) > 0 => self.update = Update::Available(rel),
                    Some(Ok(_)) => {
                        self.update = Update::Idle;
                        if manual {
                            self.notify(tf!("Masz najnowszą wersję ({}).", updater::current()));
                        }
                    }
                    Some(Err(e)) => {
                        self.update = Update::Idle;
                        if manual {
                            self.notify(tf!("Nie udało się sprawdzić aktualizacji: {}", e));
                        }
                    }
                    None => ctx.request_repaint_after(std::time::Duration::from_millis(300)),
                }
            }
            Update::Downloading(_, _, slot) => {
                let done = slot.lock().ok().and_then(|mut g| g.take());
                match done {
                    Some(Ok(file)) => match updater::run_installer(&file) {
                        Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                        Err(e) => {
                            self.update = Update::Idle;
                            self.notify(e);
                        }
                    },
                    Some(Err(e)) => {
                        self.update = Update::Idle;
                        self.notify(e);
                    }
                    None => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
                }
            }
            _ => {}
        }
    }

    fn update_banner(&mut self, ctx: &egui::Context) {
        let (rel, progress) = match &self.update {
            Update::Available(r) => (r.clone(), None),
            Update::Downloading(r, p, _) => (r.clone(), Some(p.load(Ordering::Relaxed))),
            _ => return,
        };
        let mut start = false;
        let mut later = false;
        egui::Area::new(egui::Id::new("aktualizacja"))
            .anchor(egui::Align2::CENTER_TOP, [0.0, crate::ui::TOP_BAR_H + 12.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                theme::glass().inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(theme::icon::ARROW_CIRCLE_UP).size(20.0).color(theme::BRASS));
                        ui.label(egui::RichText::new(tf!("Dostępna wersja {}", rel.version)).strong());
                        match progress {
                            Some(p) => {
                                let frac = if rel.size > 0 { p as f32 / rel.size as f32 } else { 0.0 };
                                ui.add(egui::ProgressBar::new(frac).desired_width(160.0).show_percentage());
                            }
                            None => {
                                if ui.add(theme::primary("Zainstaluj")).clicked() {
                                    start = true;
                                }
                                if ui.button(crate::i18n::tr("Później")).clicked() {
                                    later = true;
                                }
                            }
                        }
                    });
                });
            });
        if later {
            self.update = Update::Idle;
        }
        if start {
            let p = Arc::new(AtomicU64::new(0));
            let slot = Arc::new(Mutex::new(None));
            let (r2, p2, s2) = (rel.clone(), p.clone(), slot.clone());
            std::thread::spawn(move || {
                let res = updater::download(&r2, p2);
                if let Ok(mut g) = s2.lock() {
                    *g = Some(res);
                }
            });
            self.update = Update::Downloading(rel, p, slot);
        }
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some((msg, at)) = &self.toast else { return };
        let age = at.elapsed().as_secs_f32();
        if age > 5.0 {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        let msg = msg.clone();
        egui::Area::new(egui::Id::new("powiadomienie"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -crate::ui::TIMELINE_H - 24.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.multiply_opacity((5.0 - age).min(1.0));
                theme::glass().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
                    ui.label(msg);
                });
            });
    }

    fn take_screenshot(&mut self, ctx: &egui::Context) {
        let Some((path, start, after, requested)) = self.capture.clone() else { return };
        if !requested && start.elapsed().as_secs_f32() >= after {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            if let Some(c) = &mut self.capture {
                c.3 = true;
            }
        }
        if !requested {
            ctx.request_repaint();
        }
        let img = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = img {
            let [w, h] = img.size;
            let raw: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(b) = image::RgbaImage::from_raw(w as u32, h as u32, raw) {
                let _ = b.save(&path);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// Domyślne tempo odtwarzania misji: tak, by przebieg trwał rozsądnie długo.
pub fn default_rate(m: &missions::Mission) -> f64 {
    let dur = m.end_jd.unwrap_or_else(|| time::now().max(m.launch_jd + 365.0)) - m.launch_jd;
    match m.traj {
        missions::TrajSpec::Orbit { .. } => 60.0,
        missions::TrajSpec::Lunar { .. } => 3_600.0,
        _ if dur > 3_650.0 => 86_400.0 * 30.0,
        _ if dur > 365.0 => 86_400.0 * 7.0,
        _ => 86_400.0,
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, PREFS_KEY, &self.prefs);
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.poll_updates(ctx);
        self.take_screenshot(ctx);
        self.glass.clear();
        self.keyboard(ctx);
        crate::ui::top_bar(self, ctx);
        crate::ui::catalog(self, ctx);
        crate::ui::details(self, ctx);
        crate::ui::timeline(self, ctx);
        crate::ui::nav(self, ctx);
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            self.view_ui(ui, frame);
        });
        crate::ui::palette::show(self, ctx);
        crate::ui::windows(self, ctx);
        self.update_banner(ctx);
        self.toast_ui(ctx);
        self.feedback.show(ctx);
    }
}
