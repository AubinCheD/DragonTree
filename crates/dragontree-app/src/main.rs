#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dragontree_app::settings::ThemeChoice;
use dragontree_app::{DragonTreeApp, StartOptions};
use dragontree_i18n::{Lang, tr};

/// Aide de la ligne de commande.
fn help() -> &'static str {
    tr!("DragonTree — client git natif

Utilisation : dragontree [OPTIONS] [DÉPÔT…]

  --open <dossier>        ouvre un dépôt dans un onglet (répétable, ou dossiers en arguments)
  --profile <dossier>     réglages, cache et état de fenêtre dans ce dossier (tests)
  --add-root <dossier>    ajoute un dossier racine scanné
  --theme light|dark      force le thème
  --lang fr|en            force la langue de l'interface
  --view history|changes  vue initiale des dépôts ouverts
  --select <n>            sélectionne le n-ième commit de l'historique
  --window accounts|clone|settings|log|ssh  ouvre une fenêtre au démarrage
  --screenshot <png>      capture la fenêtre puis quitte
  --after-ms <n>          délai avant la capture (défaut 2500)
")
}

fn parse_args() -> Result<StartOptions, String> {
    let mut o = StartOptions { screenshot_after: Duration::from_millis(2500), ..Default::default() };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| tr!("{name} attend une valeur", name));
        match a.as_str() {
            "--open" => o.open.push(PathBuf::from(value("--open")?)),
            "--profile" => o.profile_dir = Some(PathBuf::from(value("--profile")?)),
            "--add-root" => o.add_root = Some(PathBuf::from(value("--add-root")?)),
            "--theme" => {
                o.theme = Some(match value("--theme")?.as_str() {
                    "dark" | "sombre" => ThemeChoice::Dark,
                    "light" | "clair" => ThemeChoice::Light,
                    _ => ThemeChoice::System,
                })
            }
            "--lang" => {
                let code = value("--lang")?;
                let lang = Lang::from_locale(&code).ok_or_else(|| tr!("--lang : langue inconnue « {code} »", code))?;
                // L'aide et les erreurs qui suivent s'affichent dans la langue demandée.
                dragontree_i18n::set_lang(lang);
                o.lang = Some(lang);
            }
            "--view" => o.view = Some(value("--view")?),
            "--select" => o.select_commit = value("--select")?.parse().ok(),
            "--window" => o.window = Some(value("--window")?),
            "--screenshot" => o.screenshot = Some(PathBuf::from(value("--screenshot")?)),
            "--after-ms" => {
                o.screenshot_after =
                    Duration::from_millis(value("--after-ms")?.parse().map_err(|_| tr!("--after-ms : nombre attendu"))?)
            }
            "-h" | "--help" => return Err(help().to_string()),
            other if !other.starts_with('-') => o.open.push(PathBuf::from(other)),
            other => return Err(format!("{}\n\n{}", tr!("option inconnue : {other}", other), help())),
        }
    }
    Ok(o)
}

/// Icône de fenêtre dessinée en code : trois nœuds reliés sur un disque bleu.
fn window_icon() -> egui::IconData {
    const N: usize = 64;
    let mut rgba = vec![0u8; N * N * 4];
    let blend = |px: &mut [u8], c: [u8; 3], a: f32| {
        let a = a.clamp(0.0, 1.0);
        for k in 0..3 {
            px[k] = (px[k] as f32 * (1.0 - a) + c[k] as f32 * a) as u8;
        }
        px[3] = (px[3] as f32 + (255.0 - px[3] as f32) * a) as u8;
    };
    let seg_dist = |p: (f32, f32), a: (f32, f32), b: (f32, f32)| {
        let (vx, vy) = (b.0 - a.0, b.1 - a.1);
        let t = (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
        ((p.0 - a.0 - t * vx).powi(2) + (p.1 - a.1 - t * vy).powi(2)).sqrt()
    };
    let (bottom, left, right, mid) = ((32.0, 50.0), (20.0, 14.0), (44.0, 14.0), (32.0, 34.0));
    for y in 0..N {
        for x in 0..N {
            let p = (x as f32 + 0.5, y as f32 + 0.5);
            let i = (y * N + x) * 4;
            let px = &mut rgba[i..i + 4];
            let d = ((p.0 - 32.0).powi(2) + (p.1 - 32.0).powi(2)).sqrt();
            blend(px, [0x1F, 0x6F, 0xEB], 31.5 - d);
            let line = seg_dist(p, bottom, mid).min(seg_dist(p, mid, left)).min(seg_dist(p, mid, right));
            blend(px, [255, 255, 255], 3.2 - line);
            for c in [bottom, left, right] {
                let dc = ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt();
                blend(px, [255, 255, 255], 6.5 - dc);
            }
        }
    }
    egui::IconData { rgba, width: N as u32, height: N as u32 }
}

fn main() -> eframe::Result {
    // Messages de la ligne de commande dans la langue de Windows (les réglages ne sont pas encore lus).
    dragontree_i18n::set_lang(Lang::system());
    let start = match parse_args() {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("DragonTree")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([820.0, 520.0])
        .with_icon(Arc::new(window_icon()));
    if start.screenshot.is_some() {
        // Taille fixe pour des captures comparables.
        viewport = viewport.with_inner_size([1440.0, 900.0]);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        persist_window: start.screenshot.is_none(),
        persistence_path: start.profile_dir.as_ref().map(|d| d.join("window.ron")),
        ..Default::default()
    };
    eframe::run_native("DragonTree", options, Box::new(move |cc| Ok(Box::new(DragonTreeApp::new(cc, start)))))
}
