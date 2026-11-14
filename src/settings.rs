//! Ustawienia programu: `%APPDATA%\OpenSpacecraft\settings.json`.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub auto_update: bool,
    /// Synchronizacja klatek z monitorem (zmiana działa po ponownym uruchomieniu).
    pub vsync: bool,
    /// Język interfejsu (kod z `i18n::LANGS`); pusty – według systemu.
    pub lang: String,
    /// Skala rozdzielczości renderu 3D.
    pub render_scale: f32,
    /// Jasność gwiazd 0–2.
    pub stars: f32,
    pub high_contrast: bool,
    /// Skala interfejsu 0,75–2.
    pub ui_scale: f32,
    pub tour_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { auto_update: true, vsync: true, lang: String::new(), render_scale: 1.0, stars: 1.0, high_contrast: false, ui_scale: 1.0, tour_done: false }
    }
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("OpenSpacecraft")
}

impl Settings {
    pub fn load() -> Self {
        fs::read(config_dir().join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dir = config_dir();
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("settings.json"), serde_json::to_vec_pretty(self).unwrap_or_default())
    }
}
