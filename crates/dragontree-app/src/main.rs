#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dragontree_app::settings::ThemeChoice;
use dragontree_app::single_instance::{self, Forwarded, PrimaryInstance, Startup};
use dragontree_app::{DragonTreeApp, StartOptions};
use dragontree_i18n::{Lang, tr};

/// Aide de la ligne de commande.
fn help() -> &'static str {
    tr!("DragonTree — client git natif

Utilisation : dragontree [OPTIONS] [DÉPÔT…]

  --open <dossier>        ouvre un dépôt dans un onglet (répétable, ou dossiers en arguments)
  --profile <dossier>     réglages, cache et état de fenêtre dans ce dossier (tests)
  --add-root <dossier>    ajoute un dossier racine scanné
  --theme light|dark      thème pour cette exécution seulement (non enregistré)
  --lang fr|en            langue de l'interface pour cette exécution seulement (non enregistrée)
  --view history|changes  vue initiale des dépôts ouverts
  --select <n>            sélectionne le n-ième commit de l'historique
  --window accounts|clone|settings|log|ssh|pullrequests  ouvre une fenêtre au démarrage
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

/// Instance unique par profil (RV4b, voir `single_instance`) : si une instance tourne déjà sur ce profil (celui de
/// `--profile`, sinon `%APPDATA%\DragonTree`), elle reçoit les dépôts de la ligne de commande et passe au premier
/// plan, et ce processus s'arrête. Une capture d'écran (`--screenshot`) est toujours une instance à part. `None` :
/// démarrer sans instance unique (capture d'écran, ou échec, signalé sur la sortie d'erreur).
fn claim_instance(start: &StartOptions) -> Option<PrimaryInstance> {
    if start.screenshot.is_some() {
        return None;
    }
    let profile = start.profile_dir.clone().unwrap_or_else(dragontree_app::settings::config_dir);
    let key = single_instance::profile_key(&profile);
    // Chemins absolus : le dossier courant de l'instance déjà ouverte n'est pas celui-ci.
    let open = start.open.iter().map(|p| std::path::absolute(p).unwrap_or_else(|_| p.clone())).collect();
    match single_instance::start(&key, &Forwarded { open }, Duration::from_secs(10)) {
        Startup::Primary(instance) => Some(instance),
        Startup::Forwarded => std::process::exit(0),
        Startup::Alone(e) => {
            // Pas de rattachement à la console ici (`console`) : l'application continue, elle en recevrait les Ctrl+C.
            eprintln!("DragonTree : instance unique impossible ({e}), démarrage sans");
            None
        }
    }
}

fn main() -> eframe::Result {
    // Messages de la ligne de commande dans la langue de Windows (les réglages ne sont pas encore lus).
    dragontree_i18n::set_lang(Lang::system());
    let start = match parse_args() {
        Ok(o) => o,
        Err(msg) => {
            // Exécutable release (application graphique) lancé depuis cmd ou PowerShell : sans cela, l'aide et les
            // erreurs n'iraient nulle part (P4, voir `console`).
            dragontree_app::console::attach_parent_console();
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let instance = claim_instance(&start);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("DragonTree")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([820.0, 520.0])
        // Même dessin que l'icône de l'exécutable (`assets/dragontree.ico`, intégrée par build.rs).
        .with_icon(Arc::new(dragontree_app::app_icon::window_icon()));
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
    eframe::run_native(
        "DragonTree",
        options,
        Box::new(move |cc| {
            let mut app = DragonTreeApp::new(cc, start);
            if let Some(instance) = instance {
                app.set_instance(instance);
            }
            Ok(Box::new(app))
        }),
    )
}
