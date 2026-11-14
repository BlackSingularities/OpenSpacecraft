//! Kamera orbitalna wokół wybranego celu, z płynnymi przejściami między celami i logarytmicznym
//! przybliżaniem (od kilkudziesięciu metrów do skali całego Układu Słonecznego).

use glam::{DMat4, DVec3, DVec4};

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// Kąt w płaszczyźnie ekliptyki i wysokość nad nią [rad].
    pub yaw: f64,
    pub pitch: f64,
    /// Odległość od celu [m].
    pub dist: f64,
    pub fov_y: f64,
    /// Przejście do nowego celu: punkt początkowy (względem bieżącego celu) i postęp 0–1.
    blend_from: Option<DVec3>,
    blend_t: f64,
    blend_dist: Option<(f64, f64)>,
}

impl Default for Camera {
    fn default() -> Self {
        Self { yaw: -1.2, pitch: 0.45, dist: 3.0e7, fov_y: 50f64.to_radians(), blend_from: None, blend_t: 1.0, blend_dist: None }
    }
}

/// Widok na klatkę (położenia w metrach, układ ekliptyczny względem Słońca).
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub eye: DVec3,
    pub target: DVec3,
    /// Rzutowanie z odwróconą głębią, kamera w początku układu.
    pub proj: DMat4,
    pub rot: DMat4,
    pub size: [f32; 2],
}

impl View {
    pub fn view_proj(&self) -> DMat4 {
        self.proj * self.rot
    }

    /// Punkt świata → piksel widoku (logiczny) i odległość; `None` za kamerą.
    pub fn project(&self, world: DVec3) -> Option<([f32; 2], f64)> {
        let rel = world - self.eye;
        let c = self.view_proj() * DVec4::new(rel.x, rel.y, rel.z, 1.0);
        if c.w <= 1e-9 {
            return None;
        }
        let x = (c.x / c.w * 0.5 + 0.5) * self.size[0] as f64;
        let y = (0.5 - c.y / c.w * 0.5) * self.size[1] as f64;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        Some(([x as f32, y as f32], rel.length()))
    }

    /// Promień kuli (m) w odległości `d` → piksele.
    pub fn px_radius(&self, radius: f64, d: f64) -> f32 {
        let f = 1.0 / (self.proj.y_axis.y);
        ((radius / d.max(1e-9)) / f * self.size[1] as f64 * 0.5) as f32
    }

    /// Kierunek promienia przez piksel (świat).
    pub fn ray(&self, px: [f32; 2]) -> DVec3 {
        let nx = px[0] as f64 / self.size[0] as f64 * 2.0 - 1.0;
        let ny = 1.0 - px[1] as f64 / self.size[1] as f64 * 2.0;
        let inv = self.view_proj().inverse();
        let p = inv * DVec4::new(nx, ny, 0.5, 1.0);
        (p.truncate() / p.w).normalize()
    }
}

impl Camera {
    pub fn dir(&self) -> DVec3 {
        DVec3::new(self.pitch.cos() * self.yaw.cos(), self.pitch.cos() * self.yaw.sin(), self.pitch.sin())
    }

    /// Przejście do nowego celu: kamera płynnie przesuwa się z poprzedniego położenia.
    pub fn retarget(&mut self, old_target: DVec3, new_target: DVec3, new_dist: Option<f64>) {
        self.blend_from = Some(old_target - new_target);
        self.blend_t = 0.0;
        self.blend_dist = new_dist.map(|d| (self.dist, d));
    }

    pub fn animating(&self) -> bool {
        self.blend_t < 1.0
    }

    pub fn tick(&mut self, dt: f64) {
        if self.blend_t < 1.0 {
            self.blend_t = (self.blend_t + dt / 1.3).min(1.0);
            if let Some((a, b)) = self.blend_dist {
                let s = ease(self.blend_t);
                self.dist = (a.ln() + (b.ln() - a.ln()) * s).exp();
            }
            if self.blend_t >= 1.0 {
                self.blend_from = None;
                self.blend_dist = None;
            }
        }
    }

    pub fn zoom(&mut self, factor: f64, min: f64, max: f64) {
        self.dist = (self.dist * factor).clamp(min, max);
        self.blend_dist = None;
    }

    pub fn rotate(&mut self, dyaw: f64, dpitch: f64) {
        self.yaw = (self.yaw + dyaw).rem_euclid(std::f64::consts::TAU);
        self.pitch = (self.pitch + dpitch).clamp(-1.55, 1.55);
    }

    pub fn view(&self, target: DVec3, size: [f32; 2], near: f64) -> View {
        let offset = match self.blend_from {
            Some(from) => from * (1.0 - ease(self.blend_t)),
            None => DVec3::ZERO,
        };
        let center = target + offset;
        let eye = center + self.dir() * self.dist;
        let rot = DMat4::look_at_rh(DVec3::ZERO, center - eye, DVec3::Z);
        let aspect = (size[0] / size[1].max(1.0)) as f64;
        let proj = DMat4::perspective_infinite_reverse_rh(self.fov_y, aspect, near);
        View { eye, target: center, proj, rot, size }
    }
}

fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
