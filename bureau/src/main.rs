//! Gestion CSV — application de bureau.
//!
//! Même moteur que l'interface Web, dans une fenêtre native (egui) : aucun
//! navigateur ni serveur n'est nécessaire.

// Pas de console noire derrière la fenêtre sous Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod panels;
mod widgets;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

/// Répertoire des données : `GESTION_CSV_DATA`, sinon le dossier de données
/// local de l'utilisateur (`%LOCALAPPDATA%\GestionCSV` sous Windows).
fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("GESTION_CSV_DATA") {
        return PathBuf::from(d);
    }
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("GestionCSV")
}

/// Icône dessinée par le programme : un tableau blanc sur fond bleu.
fn icon() -> egui::IconData {
    const S: usize = 64;
    let mut rgba = vec![0u8; S * S * 4];
    for y in 0..S {
        for x in 0..S {
            let i = (y * S + x) * 4;
            // Coins arrondis (rayon 12).
            let (cx, cy) = (x.clamp(12, S - 13) as f32, y.clamp(12, S - 13) as f32);
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            if d > 12.0 {
                continue;
            }
            let (mut r, mut g, mut b) = (37u8, 99u8, 235u8);
            let inside = (12..52).contains(&x) && (14..50).contains(&y);
            if inside {
                let header = y < 22;
                let line = (y - 14) % 9 == 0 || y == 49 || x == 12 || x == 51 || x == 27 || x == 39;
                if header || line {
                    (r, g, b) = (255, 255, 255);
                } else {
                    (r, g, b) = (219, 231, 255);
                }
            }
            rgba[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    egui::IconData { rgba, width: S as u32, height: S as u32 }
}

/// Variable d'environnement choisissant le moteur de rendu (`opengl` pour forcer OpenGL).
const RENDER_VAR: &str = "GESTION_CSV_RENDU";

fn error_box(text: String) {
    rfd::MessageDialog::new().set_title("Gestion CSV").set_description(text).set_level(rfd::MessageLevel::Error).show();
}

fn main() {
    let forced_gl = std::env::var(RENDER_VAR).is_ok_and(|v| v.eq_ignore_ascii_case("opengl"));
    let renderer = if forced_gl { eframe::Renderer::Glow } else { eframe::Renderer::Wgpu };
    let Err(e) = run(renderer) else { return };
    if !forced_gl {
        // Direct3D / Vulkan indisponible (machine virtuelle, bureau à distance, vieux pilote…) :
        // on relance le programme avec le rendu OpenGL, une boucle d'événements ne pouvant
        // être recréée dans le même processus.
        let relaunched = std::env::current_exe()
            .and_then(|exe| std::process::Command::new(exe).args(std::env::args_os().skip(1)).env(RENDER_VAR, "opengl").status());
        if relaunched.is_ok_and(|s| s.success()) {
            return;
        }
    }
    error_box(format!("Impossible d'ouvrir la fenêtre de l'application :\n{e}"));
    std::process::exit(1);
}

fn run(renderer: eframe::Renderer) -> eframe::Result<()> {
    let dir = data_dir();
    let sort_mb: usize = std::env::var("SORT_MEMORY_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(256);
    let state = match moteur::AppState::open(&dir, sort_mb.max(16) * 1024 * 1024) {
        Ok(s) => s,
        Err(e) => {
            error_box(format!("Impossible de préparer le dossier de données {} :\n{e}", dir.display()));
            std::process::exit(1);
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Gestion CSV")
            .with_app_id("gestion-csv")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([900.0, 560.0])
            .with_drag_and_drop(true)
            .with_icon(Arc::new(icon())),
        renderer,
        ..Default::default()
    };
    eframe::run_native(
        "Gestion CSV",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = true);
            // Police à chasse fixe en repli de la police principale : flèches et symboles
            // (→ ↑ ↓ ≥ ≤) disponibles même sans police système adaptée.
            let mut fonts = egui::FontDefinitions::default();
            if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                list.push("Hack".to_owned());
            }
            cc.egui_ctx.set_fonts(fonts);
            cc.egui_ctx.all_styles_mut(|s| s.spacing.item_spacing = egui::vec2(8.0, 6.0));
            let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).filter(|p| p.is_file()).collect();
            Ok(Box::new(app::GestionCsv::new(state, dir, files)))
        }),
    )
}
