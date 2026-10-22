//! DragonTree — client git graphique natif.

// Les fonctions d'interface reçoivent beaucoup de contexte (ui, état partagé, palette…).
#![allow(clippy::too_many_arguments)]

pub mod accounts;
pub mod ai;
pub mod app;
pub mod app_icon;
pub mod clone;
pub mod closed_tabs;
pub mod closing;
pub mod commands;
pub mod console;
pub mod custom_commands;
pub mod diff_style;
pub mod editors;
pub mod external_tools;
pub mod git_identity;
pub mod help;
pub mod highlight;
pub mod icons;
pub mod jobs;
pub mod menu_bar;
pub mod op_queue;
pub mod pull_requests;
pub mod quick_open;
pub mod repo;
pub mod repo_list;
pub mod settings;
pub mod settings_ui;
pub mod single_instance;
pub mod ssh_keys;
pub mod tabbar;
pub mod terminal;
pub mod theme;
pub mod tickets;
pub mod toasts;
pub mod util;
pub mod welcome;
pub mod widgets;
pub mod workspace_dialog;
pub mod workspaces;

pub use app::{DragonTreeApp, StartOptions};

/// Tests unitaires : configuration git globale isolée, jamais celle de l'utilisateur (contrainte 6 du plan) ; même
/// fichier que les tests d'intégration de ce crate et du moteur.
#[cfg(test)]
#[path = "../../dragontree-git/tests/common/global_config.rs"]
pub(crate) mod test_git_config;

/// git des tests unitaires, configuration globale isolée : à utiliser à la place de `Git::discover`.
#[cfg(test)]
pub(crate) fn test_git() -> dragontree_git::Git {
    test_git_config::isolate_git_config_for_this_test();
    dragontree_git::Git::discover().expect("git doit être installé pour les tests")
}

#[cfg(test)]
mod tests {
    /// Contrainte 6 du plan : git des tests unitaires lit une configuration globale temporaire.
    #[test]
    fn unit_tests_never_read_the_users_global_git_config() {
        let git = crate::test_git();
        // Vérifié avant toute écriture : en cas d'échec, rien n'est écrit dans la vraie configuration.
        let global = crate::test_git_config::isolated_global_file().expect("GIT_CONFIG_GLOBAL et HOME temporaires");
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| git.cmd(dir.path()).args(args).run().unwrap().stdout_str();
        run(&["config", "--global", "dragontree.isolation", "oui"]);
        let out = run(&["config", "--global", "--list", "--show-origin"]);
        assert!(out.lines().any(|l| l.ends_with("dragontree.isolation=oui")), "{out}");
        let from_file = |l: &str| crate::test_git_config::origin_is(l, &global);
        assert!(out.lines().all(from_file), "portée globale lue hors de {global:?} : {out}");
    }
}
