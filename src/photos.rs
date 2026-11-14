//! Zdjęcia dociągane z sieci w tle (Wikimedia Commons, Launch Library 2) z pamięcią podręczną
//! na dysku w `%LOCALAPPDATA%\OpenSpacecraft\cache\img`.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Mutex, OnceLock};

use eframe::egui;
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub enum Photo {
    Loading,
    Ready(egui::TextureHandle),
    Failed,
}

fn store() -> &'static Mutex<HashMap<String, Photo>> {
    static S: OnceLock<Mutex<HashMap<String, Photo>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

const MAX_BYTES: u64 = 12 * 1024 * 1024;
const MAX_SIDE: u32 = 1000;

/// Zwraca stan zdjęcia; przy pierwszym wywołaniu zaczyna pobieranie w tle.
pub fn get(ctx: &egui::Context, url: &str) -> Photo {
    let mut s = store().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = s.get(url) {
        return p.clone();
    }
    s.insert(url.to_string(), Photo::Loading);
    let (ctx, url) = (ctx.clone(), url.to_string());
    std::thread::spawn(move || {
        let photo = match load(&url).and_then(|bytes| decode(&bytes)) {
            Ok(img) => Photo::Ready(ctx.load_texture(format!("zdjecie:{url}"), img, egui::TextureOptions::LINEAR)),
            Err(e) => {
                log::warn!("zdjęcie {url}: {e}");
                Photo::Failed
            }
        };
        store().lock().unwrap_or_else(|e| e.into_inner()).insert(url, photo);
        ctx.request_repaint();
    });
    Photo::Loading
}

fn cache_path(url: &str) -> std::path::PathBuf {
    let h = Sha256::digest(url.as_bytes());
    let name: String = h.iter().take(16).map(|b| format!("{b:02x}")).collect();
    crate::sats::cache_dir().join("img").join(name)
}

fn load(url: &str) -> Result<Vec<u8>, String> {
    let path = cache_path(url);
    if let Ok(b) = std::fs::read(&path) {
        return Ok(b);
    }
    let resp = crate::net::agent().get(url).call().map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    resp.into_reader().take(MAX_BYTES).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, &buf);
    Ok(buf)
}

fn decode(bytes: &[u8]) -> Result<egui::ColorImage, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let img = if img.width() > MAX_SIDE || img.height() > MAX_SIDE { img.thumbnail(MAX_SIDE, MAX_SIDE) } else { img };
    let rgba = img.to_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

/// Podpis zdjęcia: autor, źródło i licencja (z odnośnikami).
pub struct Credit<'a> {
    pub author: Option<String>,
    /// Co przedstawia zdjęcie.
    pub caption: Option<&'a str>,
    pub source: &'a str,
    pub page: Option<String>,
    pub license: Option<&'a str>,
    pub license_url: Option<&'a str>,
}

/// Autor zdjęcia do podpisu: bez adresów, uciętych nawiasów, sklejonych słów i znaków spoza
/// czcionki programu (np. chińskich), z „NASA” zamiast pełnej nazwy.
pub fn clean_author(a: &str) -> Option<String> {
    let mut s = a.trim().to_string();
    if let Some(i) = s.find("http") {
        s.truncate(i);
    }
    s = s.replace("National Aeronautics and Space Administration", "NASA");
    // tylko alfabet łaciński, grecki i cyrylica oraz interpunkcja
    s = s.chars().filter(|c| (*c as u32) < 0x0530 || (0x2010..=0x2026).contains(&(*c as u32))).collect();
    // sklejone fragmenty: „STS-91Retouched” → „STS-91, Retouched”, „NASAScan” → „NASA, Scan”
    let ch: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for k in 0..ch.len() {
        if k >= 1 && k + 1 < ch.len() && ch[k].is_uppercase() && ch[k + 1].is_lowercase() {
            let digit = ch[k - 1].is_ascii_digit();
            let caps = k >= 3 && ch[k - 3..k].iter().all(|c| c.is_uppercase());
            if digit || caps {
                out.push_str(", ");
            }
        }
        out.push(ch[k]);
    }
    s = out;
    // ucięty nawias na końcu: „NASA (NAS…”
    if let Some(i) = s.rfind('(') {
        if !s[i..].contains(')') {
            s.truncate(i);
        }
    }
    let cyr = |c: char| (0x0400..=0x04FF).contains(&(c as u32));
    // dwie wersje nazwiska („Alexander Mokletsov / Александр Моклецов”) – zostaje pierwsza
    if let Some((a, b)) = s.split_once(" / ") {
        if b.chars().any(cyr) && !a.chars().any(cyr) {
            s = a.to_string();
        }
    }
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s = s.trim_matches(|c: char| c == '…' || c == '/' || c == '·' || c == ',' || c == ':' || c == ';' || c.is_whitespace()).to_string();
    (!s.is_empty()).then_some(s)
}

fn credit_line(ui: &mut egui::Ui, c: &Credit) {
    use crate::ui::theme;
    if let Some(cap) = c.caption {
        ui.add(egui::Label::new(egui::RichText::new(cap).small().italics().color(theme::paper_2())).wrap());
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let small = |t: &str| egui::RichText::new(t.to_string()).small().color(theme::paper_3());
        ui.label(small(&format!("{} {}", theme::icon::CAMERA, crate::i18n::tr("Zdjęcie:"))));
        if let Some(a) = c.author.as_deref().filter(|a| !a.is_empty()) {
            ui.label(small(a));
            ui.label(small("·"));
        }
        match &c.page {
            Some(p) => {
                ui.hyperlink_to(egui::RichText::new(c.source).small(), p);
            }
            None => {
                ui.label(small(c.source));
            }
        }
        if let Some(l) = c.license {
            ui.label(small("·"));
            match c.license_url {
                Some(u) => {
                    ui.hyperlink_to(egui::RichText::new(l).small(), u);
                }
                None => {
                    ui.label(small(l));
                }
            }
        }
    });
}

/// Rysuje zdjęcie na szerokość panelu (z miejscem na wskaźnik ładowania; po błędzie nic).
pub fn show(ui: &mut egui::Ui, url: &str, caption: Option<&Credit>) {
    let w = ui.available_width();
    match get(ui.ctx(), url) {
        Photo::Ready(t) => {
            let [iw, ih] = t.size();
            let h = (w * ih as f32 / iw.max(1) as f32).min(w * 0.85);
            let ww = h * iw as f32 / ih.max(1) as f32;
            ui.vertical_centered(|ui| {
                ui.add(egui::Image::new((t.id(), egui::vec2(ww, h))).corner_radius(10.0));
            });
            if let Some(c) = caption {
                credit_line(ui, c);
            }
        }
        Photo::Loading => {
            let (r, _) = ui.allocate_exact_size(egui::vec2(w, w * 0.5), egui::Sense::hover());
            ui.painter().rect_filled(r, 10.0, egui::Color32::from_white_alpha(10));
            ui.put(egui::Rect::from_center_size(r.center(), egui::vec2(20.0, 20.0)), egui::Spinner::new());
        }
        Photo::Failed => {}
    }
}

#[cfg(test)]
mod tests {
    use super::clean_author;

    #[test]
    fn authors_are_cleaned() {
        assert_eq!(clean_author("National Aeronautics and Space Administration (NAS").as_deref(), Some("NASA"));
        assert_eq!(clean_author("NASAScan by Kipp Teague").as_deref(), Some("NASA, Scan by Kipp Teague"));
        assert_eq!(clean_author("Alexander Mokletsov / Александр Моклецов").as_deref(), Some("Alexander Mokletsov"));
        assert_eq!(clean_author("https://images.nasa.gov/details/sts088"), None);
        assert_eq!(clean_author("中国新闻社"), None);
        assert_eq!(clean_author("McDonald").as_deref(), Some("McDonald"));
    }
}
