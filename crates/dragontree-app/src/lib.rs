//! DragonTree — client git graphique natif.

// Les fonctions d'interface reçoivent beaucoup de contexte (ui, état partagé, palette…).
#![allow(clippy::too_many_arguments)]

pub mod accounts;
pub mod ai;
pub mod app;
pub mod clone;
pub mod closed_tabs;
pub mod commands;
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
pub mod quick_open;
pub mod repo;
pub mod repo_list;
pub mod settings;
pub mod settings_ui;
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
