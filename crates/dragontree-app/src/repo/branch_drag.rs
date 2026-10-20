//! Glisser une branche locale sur une autre dans la barre latérale : menu « fusionner / rebaser ».

use dragontree_git::MergeMode;
use dragontree_i18n::tr;
use egui::{Context, Pos2, Ui};

use super::{Dialog, RepoTab};
use crate::app::Shared;
use crate::icons;

/// Charge portée par le glisser : la branche locale `S` qu'on déplace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BranchDrag(pub String);

/// Menu ouvert au dépôt de `source` sur `target`, à la position `pos`.
#[derive(Debug, Clone)]
pub(super) struct BranchDrop {
    pub source: String,
    pub target: String,
    pub pos: Pos2,
}

/// Action proposée au dépôt d'une branche sur une autre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DropAction {
    /// Fusionner `rev` dans `into` (la branche courante).
    Merge { rev: String, into: String },
    /// Rebaser `branch` (la branche courante) sur `onto`.
    Rebase { branch: String, onto: String },
    /// Sortir `target`, puis fusionner `rev` dedans.
    CheckoutThenMerge { target: String, rev: String },
}

/// Actions proposées en déposant `source` (S) sur `target` (T), selon la branche courante.
pub(super) fn drop_actions(source: &str, target: &str, current: Option<&str>) -> Vec<DropAction> {
    let (s, t) = (source.to_string(), target.to_string());
    if source == target {
        Vec::new()
    } else if current == Some(target) {
        vec![DropAction::Merge { rev: s.clone(), into: t.clone() }, DropAction::Rebase { branch: t, onto: s }]
    } else if current == Some(source) {
        vec![DropAction::Rebase { branch: s.clone(), onto: t.clone() }, DropAction::Merge { rev: t, into: s }]
    } else {
        vec![DropAction::CheckoutThenMerge { target: t, rev: s }]
    }
}

impl DropAction {
    fn label(&self) -> String {
        match self {
            DropAction::Merge { rev, into } => tr!("Fusionner « {rev} » dans « {into} »…", rev, into),
            DropAction::Rebase { branch, onto } => tr!("Rebaser « {branch} » sur « {onto} »…", branch, onto),
            DropAction::CheckoutThenMerge { target, rev } => {
                tr!("Checkout de « {target} » puis fusionner « {rev} »…", target, rev)
            }
        }
        .to_string()
    }

    fn icon(&self) -> &'static str {
        match self {
            DropAction::Rebase { .. } => icons::FLOW_ARROW,
            _ => icons::GIT_MERGE,
        }
    }
}

impl RepoTab {
    /// Fusion différée : à la fin de l'opération (numéro) réussie, ouvre le dialogue de fusion de la révision.
    pub(super) fn after_checkout_merge(&mut self, op: u64, succeeded: bool) {
        if self.merge_after_op.as_ref().is_some_and(|(pending, _)| *pending == op)
            && let Some((_, rev)) = self.merge_after_op.take()
            && succeeded
            && self.dialog.is_none()
        {
            self.dialog = Some(Dialog::Merge { rev, mode: MergeMode::Default });
        }
    }

    /// Exécute l'action choisie dans le menu de dépôt.
    fn run_drop_action(&mut self, shared: &Shared, action: DropAction) {
        match action {
            DropAction::Merge { rev, .. } => self.dialog = Some(Dialog::Merge { rev, mode: MergeMode::Default }),
            DropAction::Rebase { onto, .. } => self.dialog = Some(Dialog::Rebase { onto }),
            DropAction::CheckoutThenMerge { target, rev } => {
                // Comme le checkout du menu : git refuse une branche sortie ailleurs, on propose son worktree.
                if let Some(path) = self.checked_out_elsewhere(&target).map(|w| w.path.clone()) {
                    self.offer_worktree(target, path);
                    return;
                }
                let before = self.next_op_id;
                self.op_checkout(shared, target);
                if self.next_op_id != before {
                    self.merge_after_op = Some((self.next_op_id, rev));
                }
            }
        }
    }

    /// Menu proposé après le dépôt d'une branche sur une autre, et fantôme qui suit le pointeur pendant le glisser.
    pub(super) fn branch_drag_ui(&mut self, ctx: &Context, shared: &Shared) {
        if let Some(drag) = egui::DragAndDrop::payload::<BranchDrag>(ctx)
            && let Some(pos) = ctx.pointer_latest_pos()
        {
            egui::Area::new(egui::Id::unique("branch-drag-ghost"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pos + egui::vec2(14.0, 14.0))
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(format!("{} {}", icons::GIT_BRANCH, drag.0)));
                });
        }
        let Some(drop) = self.branch_drop.clone() else { return };
        let current = self.refs.as_ref().and_then(|r| r.current_branch().map(str::to_string));
        let actions = drop_actions(&drop.source, &drop.target, current.as_deref());
        let mut chosen = None;
        let area = egui::Area::new(egui::Id::unique("branch-drop-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(drop.pos)
            .show(ctx, |ui: &mut Ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    for action in &actions {
                        if ui.button(format!("{} {}", action.icon(), action.label())).clicked() {
                            chosen = Some(action.clone());
                        }
                    }
                });
            });
        let outside = ctx.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer();
        if chosen.is_some() || outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.branch_drop = None;
        }
        if let Some(action) = chosen {
            self.run_drop_action(shared, action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merge(rev: &str, into: &str) -> DropAction {
        DropAction::Merge { rev: rev.into(), into: into.into() }
    }

    #[test]
    fn dropping_on_the_current_branch_offers_merge_into_it_and_rebase_of_it() {
        assert_eq!(
            drop_actions("feature", "main", Some("main")),
            vec![merge("feature", "main"), DropAction::Rebase { branch: "main".into(), onto: "feature".into() }]
        );
    }

    #[test]
    fn dragging_the_current_branch_offers_rebase_of_it_and_merge_of_the_target() {
        assert_eq!(
            drop_actions("feature", "main", Some("feature")),
            vec![DropAction::Rebase { branch: "feature".into(), onto: "main".into() }, merge("main", "feature")]
        );
    }

    #[test]
    fn two_other_branches_need_a_checkout_first() {
        let expected = vec![DropAction::CheckoutThenMerge { target: "main".into(), rev: "feature".into() }];
        assert_eq!(drop_actions("feature", "main", Some("dev")), expected);
        assert_eq!(drop_actions("feature", "main", None), expected, "HEAD détachée");
    }

    #[test]
    fn a_remote_branch_is_never_current_so_it_gets_the_same_offers_as_another_branch() {
        assert_eq!(
            drop_actions("origin/hotfix", "main", Some("main")),
            vec![
                merge("origin/hotfix", "main"),
                DropAction::Rebase { branch: "main".into(), onto: "origin/hotfix".into() }
            ]
        );
        assert_eq!(
            drop_actions("origin/hotfix", "main", Some("dev")),
            vec![DropAction::CheckoutThenMerge { target: "main".into(), rev: "origin/hotfix".into() }]
        );
    }

    #[test]
    fn dropping_a_branch_on_itself_offers_nothing() {
        assert!(drop_actions("main", "main", Some("main")).is_empty());
        assert!(drop_actions("main", "main", Some("dev")).is_empty());
    }
}
