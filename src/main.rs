//! OpenSpacecraft — atlas misji kosmicznych 3D (Windows, natywnie: Rust + wgpu + egui).
#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

#[macro_use]
mod i18n;

mod app;
mod astro;
mod catalog;
mod feedback;
mod missions;
mod models;
mod net;
mod photos;
mod render;
mod sats;
mod settings;
mod ui;
mod updater;

/// Dziennik programu: `%LOCALAPPDATA%\OpenSpacecraft\openspacecraft.log` (poprzedni przebieg w `.1.log`).
pub fn log_path() -> std::path::PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("OpenSpacecraft")
        .join("openspacecraft.log")
}

fn init_log() {
    let mut b = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_hal=warn,wgpu_core=warn,naga=warn"));
    let path = log_path();
    let file = path.parent().and_then(|d| std::fs::create_dir_all(d).ok()).and_then(|_| {
        let _ = std::fs::rename(&path, path.with_file_name("openspacecraft.1.log"));
        std::fs::File::create(&path).ok()
    });
    if let Some(f) = file {
        b.target(env_logger::Target::Pipe(Box::new(f)));
        crate::feedback::set_log_file(path);
    }
    b.init();
    log::info!("OpenSpacecraft {} · {} {} · język {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH, i18n::current());
}

fn main() -> eframe::Result<()> {
    feedback::install("openspacecraft", "OpenSpacecraft", env!("CARGO_PKG_VERSION"), "OpenSpacecraft");
    let s = settings::Settings::load();
    i18n::set(if s.lang.is_empty() { i18n::system() } else { &s.lang });
    init_log();
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).unwrap_or_default();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("OpenSpacecraft")
            .with_app_id("openspacecraft")
            .with_inner_size([1480.0, 920.0])
            .with_min_inner_size([1100.0, 680.0])
            .with_icon(icon),
        renderer: eframe::Renderer::Wgpu,
        vsync: s.vsync,
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native("OpenSpacecraft", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
