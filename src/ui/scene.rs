//! Widok 3D: budowa klatki (ciała, orbity, trajektorie, satelity), etykiety nad sceną,
//! wskazywanie obiektów kursorem i sterowanie kamerą.

use eframe::egui::{self, Color32, Pos2};
use eframe::egui_wgpu;
use glam::{DVec3, Mat3, Vec3};

use crate::app::{App, Focus, Selection, JD_MAX, JD_MIN};
use crate::astro::bodies::{self, BodyId, Kind as BKind, EARTH, MOON, SATURN, SUN};
use crate::astro::time;
use crate::missions;
use crate::render::{BodyDraw, Frame, LineInst, PointInst, Renderer};
use crate::sats;
use crate::ui::theme;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PickKind {
    Body(BodyId),
    Mission(usize),
    Sat(u32),
    Event(usize, usize),
    /// Część misji lecąca osobno: (misja, część).
    Part(usize, usize),
}

#[derive(Clone, Copy, Debug)]
pub struct Pick {
    pub kind: PickKind,
    pub at: Pos2,
    pub r: f32,
}

fn c32(c: [f32; 3], a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, (a * 255.0) as u8)
}

/// Orbita zamknięta z wektora stanu: punkty równomiernie w anomalii mimośrodowej.
pub fn closed_orbit(r: DVec3, v: DVec3, mu: f64, n: usize) -> Vec<DVec3> {
    let el = crate::astro::kepler::Elements::from_state(r, v, mu, 0.0);
    if !(el.e < 1.0) || !el.a.is_finite() {
        return Vec::new();
    }
    let rot = el.rotation();
    let b = el.a * (1.0 - el.e * el.e).sqrt();
    (0..=n)
        .map(|k| {
            let ea = std::f64::consts::TAU * k as f64 / n as f64;
            rot * DVec3::new(el.a * (ea.cos() - el.e), b * ea.sin(), 0.0)
        })
        .collect()
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn v3(d: DVec3) -> [f32; 3] {
    [d.x as f32, d.y as f32, d.z as f32]
}

/// Przesłonięcie: czy odcinek oko → punkt przecina kulę bliższego ciała.
fn occluded(eye: DVec3, p: DVec3, spheres: &[(DVec3, f64)]) -> bool {
    let d = p - eye;
    let len = d.length();
    if len <= 0.0 {
        return false;
    }
    let dir = d / len;
    spheres.iter().any(|&(c, r)| {
        let oc = c - eye;
        let t = oc.dot(dir);
        if t <= 0.0 || t >= len - r * 0.5 {
            return false;
        }
        (oc - dir * t).length_squared() < r * r * 0.995
    })
}

impl App {
    pub fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let (space, faster, slower, reverse, now, focus, esc, palette, labels, orbits, sats, help, left, right, up, down) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::Period),
                i.key_pressed(egui::Key::Comma),
                i.key_pressed(egui::Key::R),
                i.key_pressed(egui::Key::N),
                i.key_pressed(egui::Key::F),
                i.key_pressed(egui::Key::Escape),
                (i.modifiers.command && i.key_pressed(egui::Key::K)) || i.key_pressed(egui::Key::Slash),
                i.key_pressed(egui::Key::L),
                i.key_pressed(egui::Key::O),
                i.key_pressed(egui::Key::S),
                i.key_pressed(egui::Key::F1) || (i.modifiers.shift && i.key_pressed(egui::Key::Slash)),
                i.key_down(egui::Key::ArrowLeft),
                i.key_down(egui::Key::ArrowRight),
                i.key_down(egui::Key::ArrowUp),
                i.key_down(egui::Key::ArrowDown),
            )
        });
        if space {
            self.clock.playing = !self.clock.playing;
        }
        if faster {
            self.step_rate(1);
        }
        if slower {
            self.step_rate(-1);
        }
        if reverse {
            self.clock.rate = -self.clock.rate;
        }
        if now {
            self.clock.jd = time::now();
            self.clock.rate = 1.0;
        }
        if focus {
            match self.selection {
                Selection::Mission(i) => {
                    let d = self.mission_view_dist(i);
                    self.set_focus(Focus::Mission(i), Some(d));
                }
                Selection::Sat(n) => self.select_sat(n),
                Selection::Body(b) => self.select_body(b),
                Selection::Craft(_) | Selection::Launch(_) | Selection::None => {}
            }
        }
        let (missions_key, date_key) = ctx.input(|i| (i.key_pressed(egui::Key::M), i.key_pressed(egui::Key::D)));
        if missions_key {
            self.missions_open = !self.missions_open;
        }
        if date_key {
            self.menu = match self.menu {
                Some((crate::app::Menu::Date, _)) => None,
                _ => Some((crate::app::Menu::Date, egui::Rect::NOTHING)),
            };
        }
        if esc {
            if self.palette.is_some() {
                self.palette = None;
            } else if self.menu.is_some() {
                self.menu = None;
            } else if self.missions_open {
                self.missions_open = false;
            } else {
                self.selection = Selection::None;
            }
        }
        if palette {
            self.palette = Some(Default::default());
        }
        if labels {
            self.prefs.layers.labels = !self.prefs.layers.labels;
        }
        if orbits {
            self.prefs.layers.orbits = !self.prefs.layers.orbits;
        }
        if sats {
            self.prefs.layers.sats = !self.prefs.layers.sats;
        }
        if help {
            self.help_open = !self.help_open;
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.1) as f64;
        let k = 1.6 * dt;
        if left || right || up || down {
            self.cam.rotate(if left { -k } else if right { k } else { 0.0 }, if up { k } else if down { -k } else { 0.0 });
            ctx.request_repaint();
        }
    }

    pub const RATES: [f64; 13] = [1.0, 10.0, 60.0, 600.0, 3_600.0, 21_600.0, 86_400.0, 604_800.0, 2_592_000.0, 7_776_000.0, 31_557_600.0, 157_788_000.0, 315_576_000.0];

    pub fn step_rate(&mut self, dir: i32) {
        let sign = self.clock.rate.signum();
        let cur = self.clock.rate.abs();
        let i = Self::RATES.iter().position(|&r| r >= cur - 1e-9).unwrap_or(0) as i32;
        let n = (i + dir).clamp(0, Self::RATES.len() as i32 - 1) as usize;
        self.clock.rate = Self::RATES[n] * sign;
    }

    fn advance_clock(&mut self, ctx: &egui::Context) {
        let now = std::time::Instant::now();
        let dt = (now - self.last_frame).as_secs_f64().min(0.25);
        self.last_frame = now;
        if self.clock.playing {
            self.clock.jd = (self.clock.jd + self.clock.rate * dt / 86_400.0).clamp(JD_MIN, JD_MAX);
            ctx.request_repaint();
        }
        self.cam.tick(dt);
        if self.cam.animating() {
            ctx.request_repaint();
        }
    }

    /// Orbity ciał: punkty względem rodzica, odświeżane gdy czas przesunie się o ułamek okresu.
    fn body_orbit(&mut self, id: BodyId, jd: f64) -> Option<Vec<DVec3>> {
        let b = bodies::get(id);
        let parent = b.parent?;
        let period = bodies::orbit_period_days(id);
        let fresh = self.orbit_cache.entries.get(&id).is_some_and(|(t, _)| (jd - t).abs() < period * 0.02);
        if !fresh {
            let n = if b.kind == BKind::Small { 180 } else { 256 };
            let pts: Vec<DVec3> = (0..=n)
                .map(|k| {
                    let t = jd - period / 2.0 + period * k as f64 / n as f64;
                    bodies::position(id, t) - bodies::position(parent, t)
                })
                .collect();
            self.orbit_cache.entries.insert(id, (jd, pts));
        }
        self.orbit_cache.entries.get(&id).map(|(_, p)| p.clone())
    }

    /// Bliska powierzchnia: odległość oka od najbliższej powierzchni (do płaszczyzny bliskiej).
    fn nearest_surface(eye: DVec3, jd: f64) -> f64 {
        let mut d = f64::INFINITY;
        for (id, b) in bodies::all().iter().enumerate() {
            if b.radius <= 0.0 {
                continue;
            }
            d = d.min((eye - bodies::position(id, jd)).length() - b.radius);
        }
        d.max(1.0)
    }

    pub fn view_ui(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.renderer.is_none() {
            if let Some(rs) = frame.wgpu_render_state() {
                self.renderer = Some(Renderer::new(rs, ctx.clone()));
            }
        }
        self.advance_clock(&ctx);
        self.director(&ctx);
        let rect = ui.max_rect();
        self.view_rect = rect;
        let resp = ui.interact(rect, egui::Id::new("widok-3d"), egui::Sense::click_and_drag());
        let jd = self.clock.jd;

        // trajektoria zaznaczonej misji: kamera dołącza, gdy jest gotowa
        if let Selection::Mission(i) = self.selection {
            self.trajs.request(i, &ctx);
        }
        if let Some(i) = self.pending_focus {
            if self.trajs.get(i).is_some() {
                self.pending_focus = None;
                if self.selection == Selection::Mission(i) {
                    let d = self.scene_dist.take().unwrap_or_else(|| self.mission_view_dist(i));
                    self.set_focus(Focus::Mission(i), Some(d));
                }
            } else {
                self.trajs.request(i, &ctx);
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        // sterowanie kamerą (w trybie zrzutu mysz nie działa)
        let live_input = self.capture.is_none();
        if live_input && (resp.dragged_by(egui::PointerButton::Primary) || resp.dragged_by(egui::PointerButton::Secondary)) {
            self.director_hold = 6.0;
            let d = resp.drag_delta();
            self.cam.rotate(-d.x as f64 * 0.006, d.y as f64 * 0.006);
        }
        if resp.hovered() && live_input {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.zoom_delta().ln() * 300.0);
            if scroll.abs() > 0.01 {
                self.director_hold = 8.0;
                let min = self.min_dist();
                self.cam.zoom((-scroll as f64 * 0.0028).exp(), min, 3.0e13);
            }
        }
        let target = self.focus_pos(self.focus, jd).or_else(|| self.focus_pos(Focus::Body(EARTH), jd)).unwrap_or(DVec3::ZERO);
        let min = self.min_dist();
        if self.cam.dist < min && !self.cam.animating() {
            self.cam.dist = min;
        }
        let eye_guess = target + self.cam.dir() * self.cam.dist;
        let near = (Self::nearest_surface(eye_guess, jd) * 0.2).min(self.cam.dist * 0.05).clamp(0.05, 1.0e8);
        let mut view = self.cam.view(target, [rect.width(), rect.height()], near);
        // oko nigdy pod powierzchnią ciała: przesunięcie nad powierzchnię i nowy kierunek kamery
        for (id, b) in bodies::all().iter().enumerate() {
            if b.radius <= 0.0 {
                continue;
            }
            let c = bodies::position(id, jd);
            let rel = view.eye - c;
            if rel.length() < b.radius * 1.01 {
                let eye = c + rel.normalize_or(DVec3::Z) * b.radius * 1.02;
                let d = eye - view.target;
                if d.length() > 1.0 {
                    let dn = d.normalize();
                    self.cam.pitch = dn.z.asin();
                    self.cam.yaw = dn.y.atan2(dn.x);
                    self.cam.dist = d.length();
                    view = self.cam.view(target, [rect.width(), rect.height()], near.min(self.cam.dist * 0.05).max(0.05));
                }
                break;
            }
        }
        let eye = view.eye;
        let rel = |p: DVec3| v3(p - eye);

        // ── ciała ──
        let mut draws: Vec<BodyDraw> = Vec::new();
        let mut spheres: Vec<(DVec3, f64)> = Vec::new();
        let positions: Vec<DVec3> = (0..bodies::all().len()).map(|i| bodies::position(i, jd)).collect();
        let sun_rel = positions[SUN] - eye;
        for (id, b) in bodies::all().iter().enumerate() {
            if b.radius <= 0.0 || b.kind == BKind::Point {
                continue;
            }
            if b.kind == BKind::Small && !self.prefs.layers.small_bodies && !self.related_body(id) {
                continue;
            }
            let p = positions[id];
            let d = (p - eye).length();
            if view.px_radius(b.radius, d) < 0.25 {
                continue;
            }
            spheres.push((p, b.radius));
            let o = bodies::orientation(id, jd);
            let rot = Mat3::from_cols(Vec3::from(v3(o.x_axis)), Vec3::from(v3(o.y_axis)), Vec3::from(v3(o.z_axis)));
            draws.push(BodyDraw {
                id,
                center: Vec3::from(rel(p)),
                radius: b.radius as f32,
                rot,
                color: b.color,
                atmo: b.atmosphere.map(|c| [c[0], c[1], c[2], if id == EARTH { 1.0 } else { 0.55 }]),
                sun: id == SUN,
                cloud_u: (((jd - time::J2000) * 86_400.0 / 2.0e6).rem_euclid(1.0)) as f32,
                rings: id == SATURN,
            });
        }

        // ── linie ──
        let mut lines: Vec<LineInst> = Vec::new();
        let push_line = |lines: &mut Vec<LineInst>, a: DVec3, b: DVec3, color: [f32; 4], width: f32| {
            lines.push(LineInst { p0: v3(a - eye), p1: v3(b - eye), color, width });
        };
        let selected_body = match self.selection {
            Selection::Body(b) => Some(b),
            _ => None,
        };
        if self.prefs.layers.orbits {
            for id in 1..bodies::all().len() {
                let b = bodies::get(id);
                if b.kind == BKind::Point || (b.kind == BKind::Small && !self.prefs.layers.small_bodies && !self.related_body(id)) {
                    continue;
                }
                let Some(parent) = b.parent else { continue };
                // orbity księżyców tylko w pobliżu planety
                if b.kind == BKind::Moon {
                    let r = (positions[id] - positions[parent]).length();
                    let d = (positions[parent] - eye).length();
                    if d > r * 120.0 {
                        continue;
                    }
                }
                // duże orbity znikają, gdy kamera jest blisko ciała (przecinałyby widok)
                let span = (positions[id] - positions[parent]).length();
                let fade = smoothstep(0.01 * span, 0.06 * span, self.cam.dist) as f32;
                if fade <= 0.01 {
                    continue;
                }
                let Some(pts) = self.body_orbit(id, jd) else { continue };
                let base = positions[parent];
                let sel = selected_body == Some(id);
                let alpha = fade * if sel { 0.85 } else if b.kind == BKind::Small { 0.22 } else { 0.38 };
                let col = [b.color[0] * 0.8 + 0.2, b.color[1] * 0.8 + 0.2, b.color[2] * 0.8 + 0.2, alpha];
                for w in pts.windows(2) {
                    push_line(&mut lines, base + w[0], base + w[1], col, if sel { 2.0 } else { 1.2 });
                }
            }
        }

        // ── misje ──
        let all_trajs = self.trajs.snapshot();
        let db = missions::db();
        let sel_mission = match self.selection {
            Selection::Mission(i) => Some(i),
            _ => None,
        };
        let mut mission_marks: Vec<(usize, DVec3)> = Vec::new();
        for &i in all_trajs.keys() {
            let m = &db.missions[i];
            let show = Some(i) == sel_mission || (self.prefs.layers.missions && m.live_at(jd) && self.kind_visible(m.kind));
            if !show {
                continue;
            }
            if let Some((p, _)) = self.mission_position(i, jd) {
                mission_marks.push((i, p));
            }
        }
        // misja prowadzona przez obiekt z katalogu (po wejściu na orbitę)
        let handed = sel_mission.filter(|&i| self.mission_handed_off(i, jd));
        let handoff_t = handed.map(|i| db.missions[i].launch_jd + crate::app::HANDOFF_D);
        if let (Some(i), true) = (sel_mission, self.prefs.layers.trajectory) {
            if let Some(t) = all_trajs.get(&i) {
                let m = &db.missions[i];
                let kc = m.kind.color();
                let base = bodies::position(t.frame, jd);
                for s in &t.strips {
                    for k in 1..s.t.len() {
                        if handoff_t.is_some_and(|h| s.t[k - 1] > h) {
                            break; // dalej – rzeczywista orbita z katalogu
                        }
                        let past = s.t[k] <= jd;
                        let col = if past { [kc[0], kc[1], kc[2], 0.95] } else { [kc[0], kc[1], kc[2], 0.32] };
                        push_line(&mut lines, base + s.p[k - 1], base + s.p[k], col, if past { 2.2 } else { 1.4 });
                    }
                }
                // bieżąca orbita wokół ciała
                if let (None, Some(b), Some(p)) = (handed, t.bound_body(jd), t.position(jd)) {
                    let h = 5.0 / 86_400.0;
                    if let (Some(p1), Some(p0)) = (t.position(jd + h), t.position(jd - h)) {
                        let bp = positions[b];
                        let vb = bodies::velocity(b, jd);
                        let r = p - bp;
                        let v = (p1 - p0) / 10.0 - vb;
                        let mu = bodies::get(b).gm;
                        let e = v.length_squared() / 2.0 - mu / r.length();
                        if e < 0.0 {
                            let pts = closed_orbit(r, v, mu, 200);
                            for w in pts.windows(2) {
                                push_line(&mut lines, bp + w[0], bp + w[1], [kc[0], kc[1], kc[2], 0.75], 1.6);
                            }
                        }
                    }
                }
            }
        }

        // części zaznaczonej misji lecące osobno (moduł na orbicie, lądownik na powierzchni)
        let mut part_marks: Vec<(usize, usize, DVec3)> = Vec::new();
        if let Some(i) = sel_mission {
            if let Some(t) = all_trajs.get(&i) {
                let kc = db.missions[i].kind.color();
                for c in &t.companions {
                    let Some(p) = c.position(jd) else { continue };
                    part_marks.push((i, c.part, p));
                    if let (true, crate::missions::traj::Seg::Conic { center, .. }) = (self.prefs.layers.trajectory, &c.seg) {
                        let h = 5.0 / 86_400.0;
                        let bp = positions[*center];
                        let r = p - bp;
                        let v = (c.seg.pos(jd + h) - c.seg.pos(jd - h)) / 10.0 - bodies::velocity(*center, jd);
                        let mu = bodies::get(*center).gm;
                        if v.length_squared() / 2.0 - mu / r.length() < 0.0 {
                            let pts = closed_orbit(r, v, mu, 160);
                            for w in pts.windows(2) {
                                push_line(&mut lines, bp + w[0], bp + w[1], [kc[0] * 0.6 + 0.4, kc[1] * 0.6 + 0.4, kc[2] * 0.6 + 0.4, 0.45], 1.2);
                            }
                        }
                    }
                }
            }
        }

        // ── satelity ──
        let earth = positions[EARTH];
        let earth_d = (earth - eye).length();
        let mut points: Vec<PointInst> = Vec::new();
        let cat = self.sats.current();
        let sat_layer = self.prefs.layers.sats && earth_d < 2.5e9;
        if sat_layer {
            let recompute = match &self.sat_visible {
                Some((t, _, c)) => (t - jd).abs() > 1e-9 || !std::sync::Arc::ptr_eq(c, &cat),
                None => true,
            };
            if recompute {
                let v = sats::compute(&cat, &self.prefs.sat_filter, jd);
                self.sat_visible = Some((jd, v, cat.clone()));
            }
            if let Some((_, v, _)) = &self.sat_visible {
                let fade = (1.0 - ((earth_d.log10() - 8.3) / 1.0).clamp(0.0, 1.0)) as f32;
                let size = (2.6 - ((earth_d.log10() - 7.0) * 0.5).clamp(0.0, 1.2)) as f32;
                let erel = earth - eye;
                points.reserve(v.pos.len());
                for (k, p) in v.pos.iter().enumerate() {
                    let o = &cat.objs[v.idx[k] as usize];
                    let c = o.kind.color(o.active || o.gp.is_some());
                    points.push(PointInst {
                        p: [(erel.x + p[0] as f64) as f32, (erel.y + p[1] as f64) as f32, (erel.z + p[2] as f64) as f32],
                        size,
                        color: [c[0], c[1], c[2], 0.9 * fade],
                    });
                }
            }
        }
        // misja = obiekt z katalogu: jego orbita oraz to, co zostało z tego startu (człony, odłamki)
        if let Some(i) = sel_mission {
            if let Some(n) = self.mission_norad(i) {
                let kc = db.missions[i].kind.color();
                if let (Some(_), Some(o)) = (handed, cat.find(n)) {
                    if o.alive_at(jd) {
                        let period = if o.period_min > 0.0 { o.period_min } else { 95.0 } / 1440.0;
                        let pts: Vec<DVec3> = (0..=180).filter_map(|k| o.position(jd + period * k as f64 / 180.0, &cat.gp).map(|(p, _)| p)).collect();
                        for w in pts.windows(2) {
                            push_line(&mut lines, earth + w[0], earth + w[1], [kc[0], kc[1], kc[2], 0.8], 1.8);
                        }
                    }
                }
                if let Some(o) = cat.find(n) {
                    let desig = &o.intl[..o.intl.len().min(8)];
                    for r in cat.objs.iter().filter(|r| r.norad != n && r.intl.starts_with(desig) && r.alive_at(jd)) {
                        if let Some((p, _)) = r.position(jd, &cat.gp) {
                            let c = r.kind.color(r.active);
                            points.push(PointInst { p: v3(earth + p - eye), size: 5.5, color: [c[0], c[1], c[2], 1.0] });
                        }
                    }
                }
            }
        }
        // orbita zaznaczonego satelity
        if let Selection::Sat(n) = self.selection {
            if let Some(o) = cat.find(n) {
                let period = if o.period_min > 0.0 { o.period_min } else { 95.0 } / 1440.0;
                let steps = 180;
                let pts: Vec<DVec3> = (0..=steps).filter_map(|k| o.position(jd + period * k as f64 / steps as f64, &cat.gp).map(|(p, _)| p)).collect();
                for w in pts.windows(2) {
                    push_line(&mut lines, earth + w[0], earth + w[1], [0.55, 0.95, 0.8, 0.85], 1.8);
                }
                if let Some((p, _)) = o.position(jd, &cat.gp) {
                    points.push(PointInst { p: v3(earth + p - eye), size: 9.0, color: [0.55, 1.0, 0.85, 1.0] });
                }
            }
        }
        // model 3D zaznaczonego statku (rakieta przy starcie albo model NASA 3D Resources), gdy kamera jest blisko
        let mut crafts: Vec<crate::render::CraftDraw> = Vec::new();
        let mut model_close: Option<usize> = None;
        if let Some(i) = sel_mission {
            if let Some(&(_, p)) = mission_marks.iter().find(|(k, _)| *k == i) {
                let d = (p - eye).length();
                let rocket = crate::rocket::plan(i).filter(|_| crate::rocket::active(i, jd));
                let model = if rocket.is_none() { crate::models::for_mission(&db.missions[i].id, jd) } else { None };
                let size = rocket.map(|r| r.length_m).or(model.map(|m| m.size_m)).unwrap_or(0.0) as f64;
                if size > 0.0 && d < size * 400.0 {
                    let rot = self.craft_rotation(i, jd, p);
                    let fwd = -rot.y_axis;
                    if let Some(r) = rocket {
                        for dr in r.draws(jd) {
                            let off = rot * (dr.offset + dr.drift) - fwd * dr.back_m;
                            crafts.push(crate::render::CraftDraw { key: dr.key, center: Vec3::from(v3(p - eye)) + off, rot: rot * Mat3::from_rotation_x(dr.tumble), scale: 1.0 });
                        }
                        if d < size * 60.0 {
                            model_close = Some(i);
                        }
                    } else if let Some(mr) = model {
                        crafts.push(crate::render::CraftDraw { key: mr.url(), center: Vec3::from(v3(p - eye)), rot, scale: mr.size_m });
                        if d < size * 60.0 && crate::models::get(&mr.url(), &ctx).is_some() {
                            model_close = Some(i);
                        }
                    }
                }
            }
        }
        // znaczniki misji (punkty w scenie – przesłaniane przez planety)
        for &(i, p) in &mission_marks {
            if model_close == Some(i) {
                continue;
            }
            let c = db.missions[i].kind.color();
            let size = if Some(i) == sel_mission { 9.0 } else { 6.0 };
            points.push(PointInst { p: v3(p - eye), size, color: [c[0], c[1], c[2], 1.0] });
        }
        for &(_, _, p) in &part_marks {
            points.push(PointInst { p: v3(p - eye), size: 6.5, color: [0.92, 0.94, 1.0, 1.0] });
        }
        // poświata Słońca
        let sun_d = sun_rel.length();
        let sun_px = view.px_radius(bodies::get(SUN).radius, sun_d);
        let glows = [PointInst { p: v3(sun_rel), size: (sun_px * 9.0).clamp(46.0, 2400.0), color: [1.0, 0.86, 0.6, 0.85] }];

        // ── render ──
        let ppp = ctx.pixels_per_point();
        let glass: Vec<([f32; 4], f32)> = self
            .glass
            .iter()
            .filter_map(|(r, a)| {
                let i = r.intersect(rect);
                (i.width() > 0.0 && i.height() > 0.0).then(|| {
                    ([(i.left() - rect.left()) / rect.width(), (i.top() - rect.top()) / rect.height(), (i.right() - rect.left()) / rect.width(), (i.bottom() - rect.top()) / rect.height()], *a)
                })
            })
            .collect();
        let vp = view.view_proj();
        let sky = view.proj * view.rot;
        let to_f = |m: glam::DMat4| glam::Mat4::from_cols_array(&m.to_cols_array().map(|x| x as f32));
        let star_b = if self.prefs.layers.stars { self.settings.stars } else { 0.0 };
        if let Some(r) = &mut self.renderer {
            let f = Frame {
                view_proj: to_f(vp),
                sky_proj: to_f(sky),
                sun_rel: Vec3::from(v3(sun_rel)),
                star_brightness: star_b,
                exposure: 1.0,
                near: near as f32,
                bodies: &draws,
                lines: &lines,
                points: &points,
                glows: &glows,
                crafts: &crafts,
            };
            let blit = r.render(&f, [rect.width(), rect.height()], ppp, self.settings.render_scale.clamp(0.5, 2.0), &glass);
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(rect, blit));
        }

        // ── etykiety i wskazywanie ──
        self.picks.clear();
        let painter = ui.painter_at(rect);
        let mut taken: Vec<egui::Rect> = Vec::new();
        let font = egui::FontId::proportional(12.5);
        let to_screen = |p: DVec3| view.project(p).map(|(s, d)| (rect.min + egui::vec2(s[0], s[1]), d));
        let label = |painter: &egui::Painter, taken: &mut Vec<egui::Rect>, at: Pos2, text: &str, color: Color32, force: bool| {
            let g = painter.layout_no_wrap(text.to_string(), font.clone(), color);
            let r = egui::Rect::from_min_size(at + egui::vec2(9.0, -g.size().y / 2.0), g.size()).expand(2.0);
            if !force && taken.iter().any(|t| t.intersects(r)) {
                return false;
            }
            if !rect.contains_rect(r) && !force {
                return false;
            }
            taken.push(r);
            painter.rect_filled(r.expand(1.0), 4.0, Color32::from_black_alpha(110));
            painter.galley(r.min + egui::vec2(2.0, 2.0), g, color);
            true
        };
        // zaznaczona misja najpierw (pierwszeństwo etykiety)
        let mut order: Vec<(usize, DVec3)> = mission_marks.clone();
        order.sort_by_key(|(i, _)| if Some(*i) == sel_mission { 0 } else { 1 });
        for (i, p) in order {
            let Some((s, _)) = to_screen(p) else { continue };
            if occluded(eye, p, &spheres) {
                continue;
            }
            let m = &db.missions[i];
            let sel = Some(i) == sel_mission;
            self.picks.push(Pick { kind: PickKind::Mission(i), at: s, r: 9.0 });
            if self.prefs.layers.labels || sel {
                label(&painter, &mut taken, s, &m.name, if sel { theme::PAPER } else { c32(m.kind.color(), 0.95) }, sel);
            }
            if sel {
                painter.circle_stroke(s, 10.0, egui::Stroke::new(1.5_f32, c32(m.kind.color(), 1.0)));
            }
        }
        // części misji lecące osobno
        for &(i, k, p) in &part_marks {
            let Some((s, _)) = to_screen(p) else { continue };
            if occluded(eye, p, &spheres) {
                continue;
            }
            self.picks.push(Pick { kind: PickKind::Part(i, k), at: s, r: 8.0 });
            label(&painter, &mut taken, s, db.missions[i].parts[k].name(), theme::paper_2(), true);
        }
        // wydarzenia zaznaczonej misji na trasie
        if let Some(i) = sel_mission {
            if let Some(t) = all_trajs.get(&i) {
                let m = &db.missions[i];
                let base_now = bodies::position(t.frame, jd);
                for (k, e) in m.events.iter().enumerate() {
                    let Some(p) = t.position(e.jd) else { continue };
                    let p = p - bodies::position(t.frame, e.jd) + base_now;
                    let Some((s, _)) = to_screen(p) else { continue };
                    if occluded(eye, p, &spheres) {
                        continue;
                    }
                    let past = e.jd <= jd;
                    let col = if past { c32(m.kind.color(), 1.0) } else { c32(m.kind.color(), 0.5) };
                    let d = 4.0;
                    painter.add(egui::Shape::convex_polygon(vec![s + egui::vec2(0.0, -d), s + egui::vec2(d, 0.0), s + egui::vec2(0.0, d), s + egui::vec2(-d, 0.0)], col, egui::Stroke::new(1.0_f32, Color32::from_black_alpha(160))));
                    self.picks.push(Pick { kind: PickKind::Event(i, k), at: s, r: 7.0 });
                }
            }
        }
        // ciała
        let mut body_order: Vec<BodyId> = (0..bodies::all().len()).collect();
        body_order.sort_by_key(|&id| match bodies::get(id).kind {
            BKind::Star => 0,
            BKind::Planet => 1,
            BKind::Dwarf => 2,
            BKind::Moon => if id == MOON { 1 } else { 3 },
            BKind::Small => 4,
            BKind::Point => 5,
        });
        for id in body_order {
            let b = bodies::get(id);
            if b.kind == BKind::Small && !self.prefs.layers.small_bodies && !self.related_body(id) {
                continue;
            }
            if b.kind == BKind::Point && !matches!(self.selection, Selection::Body(x) if x == id) && !self.related_body(id) {
                continue;
            }
            let p = positions[id];
            let Some((s, d)) = to_screen(p) else { continue };
            if !rect.expand(40.0).contains(s) {
                continue;
            }
            let pr = view.px_radius(b.radius, d);
            // księżyc schowany przy tarczy planety (z daleka)
            if let (BKind::Moon, Some(par)) = (b.kind, b.parent) {
                if let Some((ps, _)) = to_screen(positions[par]) {
                    if ps.distance(s) < 14.0 {
                        continue;
                    }
                }
            }
            if occluded(eye, p, &spheres.iter().copied().filter(|(c, _)| (*c - p).length() > 1.0).collect::<Vec<_>>()) {
                continue;
            }
            let col = c32(b.color, 1.0);
            if pr < 3.0 {
                painter.circle_filled(s, if b.kind == BKind::Point { 2.5 } else { 3.0 }, col);
            }
            let sel = matches!(self.selection, Selection::Body(x) if x == id);
            if sel {
                painter.circle_stroke(s, pr.max(6.0) + 4.0, egui::Stroke::new(1.5_f32, theme::BRASS));
            }
            self.picks.push(Pick { kind: PickKind::Body(id), at: s, r: pr.max(8.0) });
            if self.prefs.layers.labels || sel {
                let at = s + egui::vec2(pr.min(rect.width()) * 0.7, 0.0);
                label(&painter, &mut taken, at, b.name(), if b.kind == BKind::Small || b.kind == BKind::Point { theme::paper_3() } else { theme::paper_2() }, sel);
            }
        }
        // kosmodromy: zaznaczonej misji oraz z roku przeglądanego w zakładce startów
        if earth_d < 4.0e8 {
            let mut sites: Vec<(String, f64, f64, usize)> = Vec::new();
            if let Some(s) = sel_mission.and_then(|i| db.missions[i].site()) {
                sites.push((s.name.clone(), s.lat, s.lon, 0));
            }
            if let Some(u) = self.upcoming_sel.and_then(|k| self.upcoming.get(&ctx).and_then(|v| v.get(k).cloned())) {
                sites.push((format!("{} · {}", u.pad, crate::upcoming::countdown(u.jd, time::now())), u.lat, u.lon, 0));
            }
            if self.prefs.tab == crate::app::Tab::Launches && self.prefs.catalog_open {
                let y = self.launch_year.unwrap_or_else(|| time::year(jd));
                let (a, b) = (time::year_start(y), time::year_start(y + 1));
                for l in cat.launches.iter().filter(|l| l.jd >= a && l.jd < b) {
                    if let Some((name, lat, lon)) = sats::site_info(&l.site) {
                        match sites.iter_mut().find(|s| s.0 == name) {
                            Some(s) => s.3 += 1,
                            None => sites.push((name.to_string(), lat, lon, 1)),
                        }
                    }
                }
            }
            let rot = bodies::orientation(EARTH, jd);
            let r = bodies::get(EARTH).radius;
            for (name, lat, lon, n) in sites {
                let (la, lo) = (lat.to_radians(), lon.to_radians());
                let p = earth + rot * DVec3::new(la.cos() * lo.cos(), la.cos() * lo.sin(), la.sin()) * (r + 2_000.0);
                // tylko po widocznej stronie globu
                if (p - earth).dot(eye - earth) <= 0.0 {
                    continue;
                }
                let Some((s, _)) = to_screen(p) else { continue };
                let rad = if n > 0 { (3.0 + (n as f32).sqrt() * 1.6).min(14.0) } else { 5.0 };
                painter.circle_filled(s, rad, theme::WARN.gamma_multiply(0.35));
                painter.circle_stroke(s, rad, egui::Stroke::new(1.5_f32, theme::WARN));
                let text = if n > 0 { format!("{name} · {n}") } else { name };
                label(&painter, &mut taken, s + egui::vec2(rad - 6.0, 0.0), &text, theme::WARN, n == 0);
            }
        }
        // zaznaczony satelita
        if let Selection::Sat(n) = self.selection {
            if let Some(o) = cat.find(n) {
                if let Some((p, _)) = o.position(jd, &cat.gp) {
                    if let Some((s, _)) = to_screen(earth + p) {
                        if !occluded(eye, earth + p, &spheres) {
                            label(&painter, &mut taken, s, &o.name, theme::PAPER, true);
                        }
                    }
                }
            }
        }
        // satelity do wskazania (tylko w pobliżu kursora – tanio)
        let pointer = resp.hover_pos();
        if let (true, Some(ptr), Some((_, v, _))) = (sat_layer, pointer, &self.sat_visible) {
            let mut best: Option<(f32, u32, Pos2)> = None;
            for (k, p) in v.pos.iter().enumerate() {
                let w = earth + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
                let Some((s, _)) = to_screen(w) else { continue };
                let dd = s.distance(ptr);
                if dd < 7.0 && best.is_none_or(|b| dd < b.0) && !occluded(eye, w, &spheres) {
                    best = Some((dd, cat.objs[v.idx[k] as usize].norad, s));
                }
            }
            if let Some((_, norad, s)) = best {
                self.picks.push(Pick { kind: PickKind::Sat(norad), at: s, r: 7.0 });
            }
        }
        // wskazanie pod kursorem (w trybie zrzutu mysz jest pomijana)
        let pointer = if self.capture.is_some() { None } else { pointer };
        self.hover = pointer.and_then(|ptr| {
            self.picks
                .iter()
                .filter(|p| p.at.distance(ptr) <= p.r.max(6.0))
                .min_by(|a, b| a.at.distance(ptr).total_cmp(&b.at.distance(ptr)))
                .copied()
        });
        if let Some(h) = self.hover {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            let text = self.pick_label(h.kind);
            if !text.is_empty() {
                egui::Tooltip::always_open(ctx.clone(), ui.layer_id(), egui::Id::new("wskazanie"), egui::PopupAnchor::Pointer).show(|ui| {
                    ui.add(egui::Label::new(text).extend());
                });
            }
        }
        if resp.clicked() && self.capture.is_none() {
            match self.hover.map(|h| h.kind) {
                Some(PickKind::Body(b)) => {
                    self.selection = Selection::Body(b);
                    self.scroll_to_selected = true;
                }
                Some(PickKind::Mission(i)) => {
                    self.selection = Selection::Mission(i);
                    self.scroll_to_selected = true;
                }
                Some(PickKind::Sat(n)) => match self.mission_of_norad(n) {
                    Some(i) => {
                        self.selection = Selection::Mission(i);
                        self.scroll_to_selected = true;
                    }
                    None => self.selection = Selection::Sat(n),
                },
                Some(PickKind::Event(i, k)) => {
                    self.selection = Selection::Mission(i);
                    self.clock.jd = db.missions[i].events[k].jd;
                    self.clock.playing = false;
                }
                Some(PickKind::Part(i, _)) => {
                    self.selection = Selection::Mission(i);
                }
                None => {}
            }
        }
        if resp.double_clicked() {
            match self.hover.map(|h| h.kind) {
                Some(PickKind::Body(b)) => self.select_body(b),
                Some(PickKind::Mission(i)) => {
                    self.selection = Selection::Mission(i);
                    let d = self.mission_view_dist(i);
                    self.set_focus(Focus::Mission(i), Some(d));
                }
                Some(PickKind::Sat(n)) => self.select_sat(n),
                _ => {}
            }
        }
        // skala odległości w lewym dolnym rogu widoku
        let km = self.cam.dist / 1000.0;
        let txt = if km > 0.05 * crate::astro::AU / 1000.0 { tf!("{:.2} AU od celu", self.cam.dist / crate::astro::AU) } else { tf!("{} od celu", crate::ui::fmt_km(km)) };
        let left = if self.prefs.catalog_open { crate::ui::CATALOG_W } else { 0.0 };
        painter.text(
            egui::pos2(rect.left() + left + 16.0, rect.bottom() - crate::ui::TIMELINE_H - 14.0),
            egui::Align2::LEFT_BOTTOM,
            txt,
            egui::FontId::proportional(12.0),
            theme::paper_3(),
        );
    }

    /// Kamera reżyserska: tempo zegara zależy od odległości do najbliższego wydarzenia (zwalnia przy
    /// odłączeniach, lądowaniach, manewrach, przyspiesza w rejsie), a kamera zbliża się do statku.
    fn director(&mut self, ctx: &egui::Context) {
        if !self.director {
            return;
        }
        let Selection::Mission(i) = self.selection else {
            self.director = false;
            return;
        };
        let m = &missions::db().missions[i];
        let jd = self.clock.jd;
        if m.end_jd.is_some_and(|e| jd >= e) || jd >= JD_MAX - 1.0 {
            self.director = false;
            self.clock.rate = crate::app::default_rate(m);
            return;
        }
        let dt = ctx.input(|i| i.stable_dt).clamp(0.001, 0.1) as f64;
        self.director_hold = (self.director_hold - dt).max(0.0);
        self.clock.playing = true;
        if self.focus != Focus::Mission(i) && self.trajs.get(i).is_some() && self.mission_position(i, jd).is_some() {
            let d = self.mission_view_dist(i);
            self.set_focus(Focus::Mission(i), Some(d));
        }
        let start = m.launch_jd.min(jd);
        let next = m.events.iter().find(|e| e.jd > jd).map(|e| (e.jd - jd) * 86_400.0).unwrap_or_else(|| m.end_jd.map(|e| (e - jd) * 86_400.0).unwrap_or(3.0e8));
        let prev = m.events.iter().rev().find(|e| e.jd <= jd).map(|e| (jd - e.jd) * 86_400.0).unwrap_or((jd - start) * 86_400.0 + 1.0);
        // do następnego wydarzenia w ok. 6 s, po wydarzeniu chwilę wolniej
        let target = (next / 6.0).min(prev.max(6.0) * 0.6).clamp(3.0, 4.0e6);
        let cur = self.clock.rate.abs().max(1.0);
        let k = 1.0 - (-dt * 2.5).exp();
        self.clock.rate = (cur.ln() + (target.ln() - cur.ln()) * k).exp();
        // odległość kamery: blisko przy wydarzeniach (rakieta, model statku), dalej w rejsie
        if self.director_hold <= 0.0 && !self.cam.animating() && self.focus == Focus::Mission(i) {
            let size = crate::rocket::plan(i).filter(|_| crate::rocket::active(i, jd)).map(|r| r.length_m as f64).or_else(|| crate::models::for_mission(&m.id, jd).map(|x| x.size_m as f64));
            let close = next.min(prev) < 120.0;
            let want = match size {
                Some(s) if close || crate::rocket::active(i, jd) => s * 4.0,
                _ => self.mission_view_dist(i) * if close { 0.5 } else { 1.0 },
            }
            .max(self.min_dist());
            let k = 1.0 - (-dt * 1.2).exp();
            self.cam.dist = (self.cam.dist.ln() + (want.ln() - self.cam.dist.ln()) * k).exp();
            self.cam.yaw += dt * 0.04;
        }
        ctx.request_repaint();
    }

    /// Orientacja modelu statku: przód (−Y modelu) wzdłuż prędkości względem najbliższego ciała,
    /// a tuż po starcie (i przy małej prędkości) – pionowo w górę od powierzchni.
    pub fn craft_rotation(&self, i: usize, jd: f64, p: DVec3) -> Mat3 {
        let near = crate::missions::traj::nearest_body(p, jd);
        let bp = bodies::position(near, jd);
        let h = 2.0 / 86_400.0;
        let vel = match (self.mission_position(i, jd + h), self.mission_position(i, jd - h)) {
            (Some((a, _)), Some((b, _))) => (a - b) / (2.0 * h * 86_400.0) - bodies::velocity(near, jd),
            _ => DVec3::ZERO,
        };
        let up = (p - bp).normalize_or(DVec3::Z);
        let launch = missions::db().missions[i].launch_jd;
        let t = (jd - launch) * 86_400.0;
        // pierwsze sekundy: pionowo; potem płynnie w kierunek lotu
        let w = smoothstep(5.0, 40.0, t) * smoothstep(2.0, 50.0, vel.length());
        let dir = if vel.length() > 0.5 { vel.normalize() } else { up };
        let fwd = (up * (1.0 - w) + dir * w).normalize_or(up);
        let fwd = Vec3::from(v3(fwd));
        let side = fwd.cross(Vec3::Z).try_normalize().unwrap_or(Vec3::X);
        let top = side.cross(fwd).normalize_or(Vec3::Z);
        Mat3::from_cols(side, -fwd, top)
    }

    /// Ciało związane z zaznaczoną misją (cel lotu) – pokazywane nawet przy wyłączonej warstwie.
    pub fn related_body(&self, id: BodyId) -> bool {
        let Selection::Mission(i) = self.selection else { return false };
        let m = &missions::db().missions[i];
        let bid = &bodies::get(id).id;
        match &m.traj {
            missions::TrajSpec::Helio { legs, fin } => {
                legs.iter().any(|l| l.body.as_deref() == Some(bid.as_str()))
                    || fin.as_ref().is_some_and(|f| f.orbit.as_ref().is_some_and(|o| &o.body == bid) || f.surface.as_ref().is_some_and(|s| &s.body == bid))
            }
            missions::TrajSpec::Lagrange { point, .. } => point == bid,
            _ => false,
        }
    }

    pub fn kind_visible(&self, k: missions::Kind) -> bool {
        self.prefs.kinds.is_empty() || self.prefs.kinds.contains(&k)
    }

    pub fn pick_label(&self, k: PickKind) -> String {
        let db = missions::db();
        match k {
            PickKind::Body(b) => bodies::get(b).name().to_string(),
            PickKind::Mission(i) => {
                let m = &db.missions[i];
                format!("{} · {}", m.name, m.kind.label())
            }
            PickKind::Sat(n) => {
                let cat = self.sats.current();
                cat.find(n).map(|o| format!("{} · NORAD {}", o.name, o.norad)).unwrap_or_default()
            }
            PickKind::Event(i, e) => {
                let ev = &db.missions[i].events[e];
                format!("{} · {}", time::fmt_datetime(ev.jd), ev.text())
            }
            PickKind::Part(i, k) => {
                let m = &db.missions[i];
                let p = &m.parts[k];
                format!("{} · {}", p.name(), m.name)
            }
        }
    }
}
