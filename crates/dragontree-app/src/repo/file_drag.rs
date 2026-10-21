//! Glisser des fichiers d'une liste de changements à l'autre : « Non indexés » vers « Indexés » les indexe, et
//! inversement. Un fichier de la sélection entraîne toute la sélection de sa liste.

use dragontree_i18n::trn;
use egui::{Context, Rect, Sense, Ui};

use super::{RepoTab, Side};
use crate::app::Shared;
use crate::icons;
use crate::theme::Palette;

/// Charge portée par le glisser : les fichiers déplacés et la liste d'où ils viennent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileDrag {
    pub from: Side,
    pub paths: Vec<String>,
}

/// La liste qui reçoit un glisser venant de `from` (les conflits ne s'indexent pas par glisser).
fn accepts(target: Side, from: Side) -> bool {
    matches!((from, target), (Side::Unstaged, Side::Staged) | (Side::Staged, Side::Unstaged))
}

impl RepoTab {
    /// Fichiers entraînés quand on glisse la ligne `path` de la liste `side` : toute la sélection si la ligne en
    /// fait partie, sinon cette ligne seule.
    pub(super) fn dragged_paths(&self, side: Side, path: &str) -> Vec<String> {
        let selection = self.selection_paths(side);
        if selection.iter().any(|p| p == path) { selection } else { vec![path.to_string()] }
    }

    /// Zone de dépôt de la liste `side` (`rect`) : cadre au survol d'un glisser acceptable, et indexation ou
    /// désindexation au relâchement.
    pub(super) fn file_drop_zone(&mut self, ui: &mut Ui, shared: &Shared, p: &Palette, side: Side, rect: Rect) {
        let zone = ui.interact(rect, ui.scope_id().with(("file-drop", side)), Sense::hover());
        if let Some(drag) = zone.dnd_hover_payload::<FileDrag>()
            && accepts(side, drag.from)
        {
            ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.5, p.accent), egui::StrokeKind::Inside);
        }
        if let Some(drag) = zone.dnd_release_payload::<FileDrag>()
            && accepts(side, drag.from)
        {
            let paths = drag.paths.clone();
            match side {
                Side::Staged => self.op_stage(shared, paths),
                Side::Unstaged => self.op_unstage(shared, paths),
                Side::Conflict => {}
            }
        }
    }
}

/// Fantôme qui suit le pointeur pendant un glisser de fichiers.
pub(super) fn file_drag_ghost(ctx: &Context) {
    if let Some(drag) = egui::DragAndDrop::payload::<FileDrag>(ctx)
        && let Some(pos) = ctx.pointer_latest_pos()
    {
        let n = drag.paths.len();
        egui::Area::new(egui::Id::unique("file-drag-ghost"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pos + egui::vec2(14.0, 14.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(format!("{} {}", icons::FILE, trn!(n, "{n} fichier", "{n} fichiers")));
                });
            });
    }
}
