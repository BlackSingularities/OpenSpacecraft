//! Modele 3D statków z NASA 3D Resources (github.com/nasa/NASA-3D-Resources, pliki GLB).
//! Pobierane w tle przy pierwszym zbliżeniu, z pamięcią podręczną na dysku
//! (`%LOCALAPPDATA%\OpenSpacecraft\cache\models`). Czytnik GLB jest własny i minimalny:
//! węzły z przekształceniami, trójkąty, barwa bazowa materiału.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex, OnceLock};

use glam::{Mat4, Quat, Vec3, Vec4};
use serde_json::Value;

/// Siatka znormalizowana: środek w zerze, najdłuższy wymiar = 1, oś Z w górę.
pub struct Mesh {
    /// Położenie (3) i barwa liniowa (3).
    pub verts: Vec<[f32; 6]>,
    pub idx: Vec<u32>,
}

/// Model przypisany do misji: ścieżka w repozytorium, rozmiar [m] (najdłuższy wymiar)
/// i opcjonalnie okres, w którym pokazujemy właśnie ten statek (np. lądownik Apollo).
pub struct ModelRef {
    pub mission: &'static str,
    pub path: &'static str,
    pub size_m: f32,
    pub from: Option<&'static str>,
    pub to: Option<&'static str>,
}

const fn m(mission: &'static str, path: &'static str, size_m: f32) -> ModelRef {
    ModelRef { mission, path, size_m, from: None, to: None }
}

const fn w(mission: &'static str, path: &'static str, size_m: f32, from: &'static str, to: &'static str) -> ModelRef {
    ModelRef { mission, path, size_m, from: Some(from), to: Some(to) }
}

const LM: &str = "Apollo Lunar Module/Apollo Lunar Module.glb";

pub const MODELS: &[ModelRef] = &[
    w("apollo-11", LM, 9.4, "1969-07-20T17:44:00Z", "1969-07-21T23:41:31Z"),
    w("apollo-12", LM, 9.4, "1969-11-19T04:16Z", "1969-11-20T20:21:31Z"),
    w("apollo-15", LM, 9.4, "1971-07-30T18:13Z", "1971-08-03T01:04Z"),
    w("apollo-17", LM, 9.4, "1972-12-11T17:20:56Z", "1972-12-15T04:51Z"),
    m("skylab", "Skylab/Skylab.glb", 36.0),
    m("pioneer-10", "Pioneer 10/Pioneer 10.glb", 5.0),
    m("pioneer-11", "Pioneer 10/Pioneer 10.glb", 5.0),
    m("viking-1", "Viking Lander/Viking Lander.glb", 3.0),
    m("voyager-1", "Voyager Probe (A)/Voyager Probe (A).glb", 13.0),
    m("voyager-2", "Voyager Probe (A)/Voyager Probe (A).glb", 13.0),
    m("sts-1", "Space Shuttle (A)/Space Shuttle (A).glb", 37.0),
    m("mir", "Mir/Mir.glb", 31.0),
    m("galileo", "Galileo/Galileo.glb", 11.0),
    m("hubble", "Hubble Space Telescope (A)/Hubble Space Telescope (A).glb", 13.2),
    m("ulysses", "Ulysses/Ulysses.glb", 7.5),
    m("soho", "Solar and Heliospheric Observatory/Solar and Heliospheric Observatory.glb", 9.5),
    m("near-shoemaker", "NEAR Shoemaker/NEAR Shoemaker.glb", 4.0),
    m("cassini", "Cassini-Huygens (A)/Cassini-Huygens (A).glb", 11.0),
    m("stardust", "Stardust/Stardust.glb", 5.0),
    m("iss", "International Space Station (ISS) (B)/International Space Station (ISS) (B).glb", 109.0),
    m("chandra", "Chandra X-ray Observatory/Chandra X-ray Observatory.glb", 19.5),
    m("spirit", "Mars Exploration Rover - Opportunity (MER-B)/Mars Exploration Rover - Opportunity (MER-B).glb", 2.3),
    m("opportunity", "Mars Exploration Rover - Opportunity (MER-B)/Mars Exploration Rover - Opportunity (MER-B).glb", 2.3),
    m("spitzer", "Spitzer Space Telescope/Spitzer Space Telescope.glb", 4.5),
    m("rosetta", "Rosetta/Rosetta.glb", 32.0),
    m("messenger", "MErcury Surface, Space ENvironment, GEochemistry, and Ranging (MESSENGER)/MErcury Surface, Space ENvironment, GEochemistry, and Ranging (MESSENGER).glb", 6.0),
    m("deep-impact", "Deep Impact (EPOXI)/Deep Impact.glb", 3.3),
    m("mro", "Mars Reconnaissance Orbiter (MRO) (C)/Mars Reconnaissance Orbiter (MRO) (C).glb", 13.6),
    m("dawn", "Dawn/Dawn.glb", 19.7),
    m("kepler", "Kepler (A)/Kepler (A).glb", 4.7),
    m("lro", "Lunar Reconnaissance Orbiter (A)/Lunar Reconnaissance Orbiter (A).glb", 4.0),
    m("juno", "Juno (B)/Juno (B).glb", 20.0),
    m("osiris-rex", "Origins, Spectral Interpretation, Resource Identification, and Security - Regolith Explorer (OSIRIS-REx)/OSIRIS-REx.glb", 6.2),
    m("insight", "InSight Cruise Lander/InSight Cruise Lander (panels deployed).glb", 6.0),
    m("parker", "Parker Solar Probe/Parker Solar Probe.glb", 3.0),
    m("tess", "Transiting Exoplanet Survey Satellite (TESS) (A)/Transiting Exoplanet Survey Satellite (TESS) (A).glb", 3.7),
    m("perseverance", "Mars 2020 Perseverance Rover/Mars 2020 Perseverance Rover.glb", 3.0),
    m("jwst", "James Webb Space Telescope (B)/James Webb Space Telescope (B).glb", 21.2),
];

pub const SOURCE_URL: &str = "https://github.com/nasa/NASA-3D-Resources";

impl ModelRef {
    pub fn url(&self) -> String {
        let mut o = String::from("https://raw.githubusercontent.com/nasa/NASA-3D-Resources/master/3D%20Models/");
        for b in self.path.bytes() {
            if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) {
                o.push(b as char);
            } else {
                o.push_str(&format!("%{b:02X}"));
            }
        }
        o
    }

    pub fn page(&self) -> String {
        let dir = self.path.split('/').next().unwrap_or("");
        format!("{SOURCE_URL}/tree/master/3D%20Models/{}", dir.replace(' ', "%20"))
    }
}

/// Model misji pokazywany w chwili `jd` (jeśli jest).
pub fn for_mission(id: &str, jd: f64) -> Option<&'static ModelRef> {
    MODELS.iter().find(|r| {
        r.mission == id
            && r.from.and_then(crate::astro::time::parse).is_none_or(|f| jd >= f)
            && r.to.and_then(crate::astro::time::parse).is_none_or(|t| jd <= t)
    })
}

/// Czy misja ma jakikolwiek model.
pub fn has_model(id: &str) -> bool {
    MODELS.iter().any(|r| r.mission == id)
}

#[derive(Clone)]
enum State {
    Loading,
    Ready(Arc<Mesh>),
    Failed,
}

fn store() -> &'static Mutex<HashMap<String, State>> {
    static S: OnceLock<Mutex<HashMap<String, State>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Siatka modelu albo `None` (pobieranie trwa albo się nie udało); pierwsze wywołanie zaczyna pobieranie.
pub fn get(url: &str, ctx: &eframe::egui::Context) -> Option<Arc<Mesh>> {
    let mut s = store().lock().unwrap_or_else(|e| e.into_inner());
    match s.get(url) {
        Some(State::Ready(m)) => return Some(m.clone()),
        Some(_) => return None,
        None => {}
    }
    s.insert(url.to_string(), State::Loading);
    let (url, ctx) = (url.to_string(), ctx.clone());
    std::thread::spawn(move || {
        let st = match load(&url).and_then(|b| parse_glb(&b)) {
            Ok(m) => State::Ready(Arc::new(m)),
            Err(e) => {
                log::warn!("model {url}: {e}");
                State::Failed
            }
        };
        store().lock().unwrap_or_else(|e| e.into_inner()).insert(url, st);
        ctx.request_repaint();
    });
    None
}

/// Czy model jest w trakcie pobierania.
pub fn loading(url: &str) -> bool {
    matches!(store().lock().ok().and_then(|s| s.get(url).cloned()), Some(State::Loading))
}

const MAX_BYTES: u64 = 30 * 1024 * 1024;

fn load(url: &str) -> Result<Vec<u8>, String> {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(url.as_bytes());
    let name: String = h.iter().take(16).map(|b| format!("{b:02x}")).collect();
    let path = crate::sats::cache_dir().join("models").join(format!("{name}.glb"));
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

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

fn f32_at(b: &[u8], o: usize) -> Option<f32> {
    u32_at(b, o).map(f32::from_bits)
}

fn num(v: &Value, k: &str) -> usize {
    v.get(k).and_then(Value::as_u64).unwrap_or(0) as usize
}

/// Dane akcesora: (bajty widoku, przesunięcie, krok, liczba, typ składowej).
fn accessor<'a>(j: &Value, bin: &'a [u8], a: usize) -> Option<(&'a [u8], usize, usize, usize, u64)> {
    let acc = j.get("accessors")?.get(a)?;
    let view = j.get("bufferViews")?.get(acc.get("bufferView")?.as_u64()? as usize)?;
    let start = num(view, "byteOffset");
    let len = num(view, "byteLength");
    let data = bin.get(start..start + len)?;
    let comp = acc.get("componentType")?.as_u64()?;
    let count = num(acc, "count");
    let ncomp = match acc.get("type")?.as_str()? {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        _ => return None,
    };
    let csize = match comp {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        _ => 4,
    };
    let stride = match num(view, "byteStride") {
        0 => ncomp * csize,
        s => s,
    };
    Some((data, num(acc, "byteOffset"), stride, count, comp))
}

fn node_matrix(n: &Value) -> Mat4 {
    if let Some(m) = n.get("matrix").and_then(Value::as_array) {
        let v: Vec<f32> = m.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
        if v.len() == 16 {
            return Mat4::from_cols_slice(&v);
        }
    }
    let arr = |k: &str, d: &[f32]| -> Vec<f32> { n.get(k).and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_else(|| d.to_vec()) };
    let t = arr("translation", &[0.0, 0.0, 0.0]);
    let r = arr("rotation", &[0.0, 0.0, 0.0, 1.0]);
    let s = arr("scale", &[1.0, 1.0, 1.0]);
    if t.len() < 3 || r.len() < 4 || s.len() < 3 {
        return Mat4::IDENTITY;
    }
    Mat4::from_scale_rotation_translation(Vec3::new(s[0], s[1], s[2]), Quat::from_xyzw(r[0], r[1], r[2], r[3]).normalize(), Vec3::new(t[0], t[1], t[2]))
}

/// Barwa materiału (liniowa). Tekstur nie czytamy: przy białej barwie z teksturą – jasna szarość.
fn material_color(j: &Value, mat: Option<u64>) -> [f32; 3] {
    let Some(m) = mat.and_then(|i| j.get("materials")?.get(i as usize)) else { return [0.72, 0.72, 0.74] };
    let pbr = m.get("pbrMetallicRoughness");
    let f: Vec<f32> = pbr.and_then(|p| p.get("baseColorFactor")).and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(1.0) as f32).collect()).unwrap_or_else(|| vec![1.0, 1.0, 1.0, 1.0]);
    let textured = pbr.is_some_and(|p| p.get("baseColorTexture").is_some());
    let c = [f.first().copied().unwrap_or(1.0), f.get(1).copied().unwrap_or(1.0), f.get(2).copied().unwrap_or(1.0)];
    if textured && c.iter().all(|x| *x > 0.95) {
        [0.66, 0.66, 0.68]
    } else {
        [c[0].clamp(0.03, 1.0), c[1].clamp(0.03, 1.0), c[2].clamp(0.03, 1.0)]
    }
}

pub fn parse_glb(b: &[u8]) -> Result<Mesh, String> {
    if b.get(0..4) != Some(b"glTF") {
        return Err("to nie jest plik GLB".into());
    }
    let jlen = u32_at(b, 12).ok_or("brak fragmentu JSON")? as usize;
    let j: Value = serde_json::from_slice(b.get(20..20 + jlen).ok_or("ucięty JSON")?).map_err(|e| e.to_string())?;
    let bstart = 20 + jlen;
    let bin = match u32_at(b, bstart) {
        Some(blen) => b.get(bstart + 8..bstart + 8 + blen as usize).unwrap_or(&[]),
        None => &[][..],
    };
    let nodes = j.get("nodes").and_then(Value::as_array).cloned().unwrap_or_default();
    let scene = j.get("scene").and_then(Value::as_u64).unwrap_or(0) as usize;
    let roots: Vec<usize> = j
        .get("scenes")
        .and_then(|s| s.get(scene))
        .and_then(|s| s.get("nodes"))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_u64().map(|v| v as usize)).collect())
        .unwrap_or_else(|| (0..nodes.len()).collect());
    let mut verts: Vec<[f32; 6]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    let mut stack: Vec<(usize, Mat4, u32)> = roots.into_iter().map(|r| (r, Mat4::IDENTITY, 0)).collect();
    while let Some((ni, parent, depth)) = stack.pop() {
        let Some(n) = nodes.get(ni) else { continue };
        if depth > 64 {
            continue;
        }
        let mtx = parent * node_matrix(n);
        if let Some(children) = n.get("children").and_then(Value::as_array) {
            for c in children.iter().filter_map(Value::as_u64) {
                stack.push((c as usize, mtx, depth + 1));
            }
        }
        let Some(mesh) = n.get("mesh").and_then(Value::as_u64).and_then(|mi| j.get("meshes")?.get(mi as usize)) else { continue };
        for prim in mesh.get("primitives").and_then(Value::as_array).into_iter().flatten() {
            if prim.get("mode").and_then(Value::as_u64).unwrap_or(4) != 4 {
                continue;
            }
            let Some(pa) = prim.get("attributes").and_then(|a| a.get("POSITION")).and_then(Value::as_u64) else { continue };
            let Some((data, off, stride, count, comp)) = accessor(&j, bin, pa as usize) else { continue };
            if comp != 5126 {
                continue;
            }
            let color = material_color(&j, prim.get("material").and_then(Value::as_u64));
            let base = verts.len() as u32;
            for k in 0..count {
                let o = off + k * stride;
                let (Some(x), Some(y), Some(z)) = (f32_at(data, o), f32_at(data, o + 4), f32_at(data, o + 8)) else { break };
                let p = mtx * Vec4::new(x, y, z, 1.0);
                verts.push([p.x, p.y, p.z, color[0], color[1], color[2]]);
            }
            let n_new = verts.len() as u32 - base;
            match prim.get("indices").and_then(Value::as_u64).and_then(|ia| accessor(&j, bin, ia as usize)) {
                Some((data, off, stride, count, comp)) => {
                    for k in 0..count {
                        let o = off + k * stride;
                        let i = match comp {
                            5121 => data.get(o).map(|v| *v as u32),
                            5123 => data.get(o..o + 2).map(|v| u16::from_le_bytes([v[0], v[1]]) as u32),
                            _ => u32_at(data, o),
                        };
                        match i {
                            Some(i) if i < n_new => idx.push(base + i),
                            _ => idx.push(base),
                        }
                    }
                }
                None => idx.extend(base..base + n_new),
            }
            idx.truncate(idx.len() / 3 * 3);
        }
        if verts.len() > 3_000_000 {
            break;
        }
    }
    if idx.is_empty() {
        return Err("model bez trójkątów".into());
    }
    // normalizacja: środek prostopadłościanu w zerze, najdłuższy wymiar 1, glTF (Y w górę) → Z w górę
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in &verts {
        let p = Vec3::new(v[0], v[1], v[2]);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let c = (lo + hi) / 2.0;
    let s = 1.0 / (hi - lo).max_element().max(1e-6);
    for v in &mut verts {
        let p = (Vec3::new(v[0], v[1], v[2]) - c) * s;
        v[0] = p.x;
        v[1] = -p.z;
        v[2] = p.y;
    }
    Ok(Mesh { verts, idx })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Najmniejszy GLB: jeden trójkąt z indeksami u16.
    fn tiny_glb() -> Vec<u8> {
        let mut bin: Vec<u8> = Vec::new();
        for f in [0.0f32, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend(f.to_le_bytes());
        }
        for i in [0u16, 1, 2, 0] {
            bin.extend(i.to_le_bytes());
        }
        let json = r#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0,"translation":[1,0,0]}],
"meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]}],
"materials":[{"pbrMetallicRoughness":{"baseColorFactor":[1,0.5,0.25,1]}}],
"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],
"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":6}],"buffers":[{"byteLength":44}]}"#;
        let mut j = json.as_bytes().to_vec();
        while j.len() % 4 != 0 {
            j.push(b' ');
        }
        let total = 12 + 8 + j.len() + 8 + bin.len();
        let mut out = Vec::new();
        out.extend(b"glTF");
        out.extend(2u32.to_le_bytes());
        out.extend((total as u32).to_le_bytes());
        out.extend((j.len() as u32).to_le_bytes());
        out.extend(b"JSON");
        out.extend(&j);
        out.extend((bin.len() as u32).to_le_bytes());
        out.extend(b"BIN\0");
        out.extend(&bin);
        out
    }

    #[test]
    fn parses_minimal_glb() {
        let m = parse_glb(&tiny_glb()).expect("GLB");
        assert_eq!(m.verts.len(), 3);
        assert_eq!(m.idx, vec![0, 1, 2]);
        // najdłuższy wymiar znormalizowany do 1
        let xs: Vec<f32> = m.verts.iter().map(|v| v[0]).collect();
        let span = xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
        assert!((span - 1.0).abs() < 1e-5, "{span}");
        assert!((m.verts[0][3] - 1.0).abs() < 1e-6 && (m.verts[0][4] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn every_model_has_a_mission_and_encoded_url() {
        for r in MODELS {
            assert!(crate::missions::index_of(r.mission).is_some(), "{}", r.mission);
            assert!(!r.url().contains(' '), "{}", r.url());
        }
    }
}
