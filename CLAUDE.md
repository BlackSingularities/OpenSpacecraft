# OpenSpacecraft – zasady pracy w repozytorium

- **Każda zmiana trafia do `main`**: gałąź robocza → pull request → scalenie.
- **Tylko Windows, tylko Rust** (Rust + wgpu + egui), wygląd i wspólne moduły jak w GRANI (`gran-3d`): `ui/theme.rs`, `feedback.rs`, `i18n.rs`, `updater.rs` – zmieniaj je razem we wszystkich aplikacjach egui.
- Lokalnie kompiluj zestawem MSVC: `cargo +stable-x86_64-pc-windows-msvc build` (domyślny zestaw GNU nie ma `dlltool`). Testy: `cargo test`.
- **Wydania** robi `.github/workflows/build.yml`: push na `main` z nową wersją w `Cargo.toml` (bez tagu `v<wersja>`) → testy, build, podpis (sekrety `WIN_CSC_LINK`, `WIN_CSC_KEY_PASSWORD`), instalator NSIS `OpenSpacecraft-Setup-<wersja>.exe`, `latest.yml`, wydanie. Przy wydaniu podbij wersję w `Cargo.toml` i `Cargo.lock` i dodaj `build/release-notes/<wersja>.md`.
- **Pobieranie i aktualizacje przez redzikowski.com** (projekt `openspacecraft` w portfolio, `releaseSource: 'github'`): program pyta `redzikowski.com/api/releases/repos/BlackSingularities/OpenSpacecraft/releases/latest`, sprawdza SHA-512 z `latest.yml` i uruchamia instalator z UAC. Zgłoszenia: `app: 'openspacecraft'`.
- **Języki**: napisy w kodzie po polsku przez `tr("…")` / `tf!("…", …)`; nowy napis → `lang/en.tsv`. Opisy misji są w danych (`pl`/`en`).
- **Dane**: `data/missions.json` (misje: wydarzenia, model trajektorii `traj`, opcjonalnie `horizons` – identyfikator JPL), `data/vehicles.json`, `data/sites.json`, `data/small_bodies.json`. Nowa misja z identyfikatorem Horizons → `python tools/fetch_horizons.py <id>` (uzupełnia `assets/trajectories.bin`).
- Struktura: `astro/` (czas, VSOP87, Księżyc, Kepler/Lambert, ciała), `missions/` (baza, `traj.rs` – trajektorie), `sats.rs` (CelesTrak, SGP4, starty), `render/` (wgpu, `space.wgsl`), `ui/` (scena, panele, oś czasu, paleta, okna).
- Zrzuty ekranu: `tools/capture.sh <plik> <misja@data> [odległość] [yaw] [pitch] [zakładka]`; plakat: `node ../Homepage/scripts/posters/build.mjs .`.
- Nie używaj `border-left` ani zamienników lewej belki w UI.
