//! Barre d'outils de l'onglet : Fetch, Pull, Push, Stash, Pop, Branche, ouverture externe.

use dragontree_git::HeadState;
use dragontree_i18n::tr;
use egui::{RichText, Ui};

use super::actions::StashOp;
use super::{Dialog, RepoTab};
use crate::app::{AppAction, Shared};
use crate::commands::Command;
use crate::icons;
use crate::theme::Palette;
use crate::widgets::toolbar_button;

impl RepoTab {
    pub(super) fn toolbar_ui(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        let busy = |tab: &RepoTab, name: &str| tab.running.iter().any(|o| !o.silent && o.title.starts_with(name));
        let head = self.refs.as_ref().map(|r| r.head.clone());
        // Les boutons tirés du registre de commandes ont le même état grisé et la même action que les menus.
        let can = |cmd| self.can_run(cmd);
        let (fetch, pull, push, stash, branch) =
            (can(Command::Fetch), can(Command::Pull), can(Command::Push), can(Command::Stash), can(Command::NewBranch));

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            // Dépôt et branche courante.
            ui.vertical(|ui| {
                ui.set_width(190.0);
                ui.add_space(4.0);
                ui.label(RichText::new(self.title()).strong().size(15.0));
                let branch = match &head {
                    Some(HeadState::Branch(b)) => format!("{}  {b}", icons::GIT_BRANCH),
                    Some(HeadState::Unborn(b)) => {
                        format!("{}  {}", icons::GIT_BRANCH, tr!("{branch} (aucun commit)", branch = b))
                    }
                    Some(HeadState::Detached(oid)) => {
                        format!("{}  {}", icons::WARNING, tr!("HEAD détachée ({sha})", sha = oid.short()))
                    }
                    None => "…".to_string(),
                };
                let mut tip = branch.clone();
                if let Some(s) = &self.status
                    && (s.ahead > 0 || s.behind > 0)
                {
                    tip = format!("{tip}   {}{} {}{}", icons::ARROW_UP, s.ahead, icons::ARROW_DOWN, s.behind);
                }
                ui.add(egui::Label::new(RichText::new(tip).size(12.0).color(p.weak)).truncate());
            });
            ui.separator();

            if toolbar_button(ui, icons::CLOUD_ARROW_DOWN, "Fetch", fetch, busy(self, "Fetch"))
                .on_hover_text(with_shortcut(
                    tr!("Récupérer les nouveautés de tous les remotes (fetch --all --prune)"),
                    Command::Fetch,
                ))
                .clicked()
            {
                self.run_command(Command::Fetch, shared);
            }
            // Ctrl+clic sur Pull ou Push : l'opération rapide, sans dialogue (Fetch n'en a pas).
            let ctrl = |ui: &Ui| ui.input(|i| i.modifiers.command);
            if toolbar_button(ui, icons::ARROW_LINE_DOWN, "Pull", pull, busy(self, "Pull"))
                .on_hover_text(format!(
                    "{}\n{}",
                    with_shortcut(tr!("Récupérer et intégrer la branche suivie"), Command::Pull),
                    tr!("Ctrl+clic : pull rapide, sans dialogue")
                ))
                .clicked()
            {
                self.run_command(if ctrl(ui) { Command::QuickPull } else { Command::Pull }, shared);
            }
            if toolbar_button(ui, icons::ARROW_LINE_UP, "Push", push, busy(self, "Push"))
                .on_hover_text(format!(
                    "{}\n{}",
                    with_shortcut(tr!("Envoyer la branche courante"), Command::Push),
                    tr!("Ctrl+clic : push rapide vers la branche suivie, sans dialogue")
                ))
                .clicked()
            {
                self.run_command(if ctrl(ui) { Command::QuickPush } else { Command::Push }, shared);
            }
            ui.separator();
            if toolbar_button(ui, icons::TRAY_ARROW_DOWN, "Stash", stash, busy(self, "Stash"))
                .on_hover_text(with_shortcut(tr!("Mettre de côté les modifications en cours"), Command::Stash))
                .clicked()
            {
                self.run_command(Command::Stash, shared);
            }
            let has_stash = !self.stashes.is_empty();
            if toolbar_button(ui, icons::TRAY_ARROW_UP, "Pop", has_stash, busy(self, StashOp::Pop.title()))
                .on_hover_text(tr!("Réappliquer le dernier stash et le supprimer"))
                .clicked()
                && let Some(top) = self.stashes.first()
            {
                let (oid, label) = (top.oid, format!("stash@{{0}} {}", top.message));
                self.op_stash(shared, oid, label, StashOp::Pop);
            }
            ui.separator();
            if toolbar_button(ui, icons::GIT_BRANCH, tr!("Branche"), branch, false)
                .on_hover_text(with_shortcut(tr!("Créer une branche à partir de HEAD"), Command::NewBranch))
                .clicked()
            {
                self.run_command(Command::NewBranch, shared);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if toolbar_button(ui, icons::ARROWS_CLOCKWISE, tr!("Actualiser"), true, false)
                    .on_hover_text(tr!("Actualiser (F5)"))
                    .clicked()
                {
                    self.run_command(Command::Refresh, shared);
                }
                if toolbar_button(ui, icons::LIST, tr!("Journal"), true, false)
                    .on_hover_text(tr!("Journal des commandes git (Ctrl+Maj+L)"))
                    .clicked()
                {
                    shared.actions.push(AppAction::ShowLog);
                }
                // Bouton masqué (plutôt que grisé) quand VS Code n'est pas installé.
                if self.can_run(Command::OpenVsCode)
                    && toolbar_button(ui, icons::CODE, "VS Code", true, false)
                        .on_hover_text(tr!("Ouvrir dans VS Code"))
                        .clicked()
                {
                    self.run_command(Command::OpenVsCode, shared);
                }
                if toolbar_button(ui, icons::FOLDER_OPEN, tr!("Explorateur"), true, false).clicked() {
                    self.run_command(Command::OpenExplorer, shared);
                }
                if toolbar_button(ui, icons::TERMINAL_WINDOW, "Terminal", true, false).clicked() {
                    self.run_command(Command::OpenTerminal, shared);
                }
            });
        });
    }

    /// Checkout demandé depuis la palette Ctrl+P.
    pub fn quick_checkout(&mut self, shared: &mut Shared, name: String) {
        self.op_checkout(shared, name);
    }

    pub(super) fn open_pull_dialog(&mut self, shared: &Shared) {
        let current = self.refs.as_ref().and_then(|r| r.current_branch().map(str::to_string)).unwrap_or_default();
        let (remote, branch) = self
            .current_upstream()
            .unwrap_or_else(|| (self.default_remote().unwrap_or_else(|| "origin".into()), current));
        self.dialog = Some(Dialog::Pull { remote, branch, rebase: shared.pull_rebase });
    }

    /// Pull rapide : la branche suivie, dans le dernier mode choisi pendant la session (fusion sinon). Sans
    /// branche suivie, il n'y a rien à deviner : le dialogue s'ouvre.
    pub(super) fn quick_pull(&mut self, shared: &Shared) {
        match self.current_upstream() {
            Some((remote, branch)) => self.op_pull(shared, remote, branch, shared.pull_rebase),
            None => self.open_pull_dialog(shared),
        }
    }

    /// Push rapide : vers la branche suivie, créée sur le remote par défaut s'il n'y en a pas ; sans force ni tags.
    /// Comme `push.default=simple`, une branche qui suit une branche d'un autre nom (`feature` qui suit
    /// `origin/main`) n'est pas poussée sans confirmation : le dialogue s'ouvre, prérempli.
    pub(super) fn quick_push(&mut self, shared: &Shared) {
        match self.default_push() {
            Some(target) if target.remote_branch == target.local => self.push_to(shared, target),
            Some(_) => self.open_push_dialog(),
            None => {}
        }
    }

    pub(super) fn open_push_dialog(&mut self) {
        let local = self.refs.as_ref().and_then(|r| r.current_branch().map(str::to_string)).unwrap_or_default();
        let upstream = self.current_upstream();
        let remote = upstream
            .as_ref()
            .map(|(r, _)| r.clone())
            .or_else(|| self.default_remote())
            .unwrap_or_else(|| "origin".into());
        let remote_branch = upstream.as_ref().map(|(_, b)| b.clone()).unwrap_or_else(|| local.clone());
        self.dialog = Some(Dialog::Push {
            remote,
            local,
            remote_branch,
            set_upstream: upstream.is_none(),
            force: false,
            tags: false,
        });
    }
}

/// Infobulle d'un bouton : le texte suivi du raccourci de la commande, s'il y en a un.
fn with_shortcut(text: impl std::fmt::Display, cmd: Command) -> String {
    match cmd.shortcut_text() {
        Some(shortcut) => format!("{text} ({shortcut})"),
        None => text.to_string(),
    }
}
