//! « Ouvrir dans » : le dépôt ouvert dans un éditeur externe (`crate::editors`), avec le choix de la solution quand
//! Visual Studio en trouve plusieurs.

use std::path::PathBuf;

use dragontree_i18n::tr;
use egui::{RichText, Ui};

use super::{Dialog, RepoTab};
use crate::app::Shared;
use crate::editors::{self, Editor, OpenOutcome};
use crate::theme::Palette;
use crate::toasts::Toast;

/// Notification d'un éditeur qui ne démarre pas.
fn launch_failed(editor: &Editor, error: String) -> Toast {
    Toast::error(tr!("{editor} n'a pas pu être lancé", editor = editor.kind.label()), error)
}

impl RepoTab {
    /// Ouvre le dépôt dans l'éditeur de `Shared::editors` à cette position, et le retient comme dernier utilisé
    /// (clic principal de la barre d'outils).
    pub(super) fn open_in_editor(&mut self, shared: &mut Shared, index: usize) {
        let Some(editor) = shared.editors.get(index).cloned() else { return };
        shared.settings.last_editor = Some(editor.kind.id().to_string());
        shared.settings_dirty = true;
        match editors::open(&editor, &self.path) {
            Ok(OpenOutcome::Launched) => {}
            Ok(OpenOutcome::Choose(solutions)) => {
                self.dialog = Some(Dialog::OpenSolution { editor, solutions, chosen: 0 });
            }
            Err(e) => shared.toast(launch_failed(&editor, e)),
        }
    }

    /// Corps du dialogue de choix de la solution ; toujours valide.
    pub(super) fn open_solution_body(
        &self,
        ui: &mut Ui,
        solutions: &[PathBuf],
        chosen: &mut usize,
        p: &Palette,
    ) -> bool {
        ui.label(RichText::new(tr!("Plusieurs solutions dans ce dépôt :")).color(p.weak));
        for (i, solution) in solutions.iter().enumerate() {
            let shown = solution.strip_prefix(&self.path).unwrap_or(solution);
            ui.radio_value(chosen, i, shown.display().to_string());
        }
        true
    }

    /// Solution choisie dans le dialogue.
    pub(super) fn open_solution(&mut self, shared: &mut Shared, editor: &Editor, solutions: &[PathBuf], chosen: usize) {
        if let Some(solution) = solutions.get(chosen)
            && let Err(e) = editors::launch(editor, solution)
        {
            shared.toast(launch_failed(editor, e));
        }
    }
}
