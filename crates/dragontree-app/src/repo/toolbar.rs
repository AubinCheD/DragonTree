//! Barre d'outils de l'onglet : Fetch, Pull, Push, Stash, Pop, Branche, ouverture externe.

use dragontree_git::HeadState;
use dragontree_i18n::tr;
use egui::{RichText, Ui};

use super::actions::{OpKind, StashOp};
use super::{Dialog, RepoTab};
use crate::app::{AppAction, Shared};
use crate::commands::Command;
use crate::icons;
use crate::theme::Palette;
use crate::widgets::toolbar_button;

impl RepoTab {
    pub(super) fn toolbar_ui(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        // Bouton « occupé » pendant sa propre opération, reconnue par son type (jamais par son titre, traduit).
        let busy = |tab: &RepoTab, kind| tab.op_running(kind);
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

            if toolbar_button(ui, icons::CLOUD_ARROW_DOWN, "Fetch", fetch, busy(self, OpKind::Fetch))
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
            if toolbar_button(ui, icons::ARROW_LINE_DOWN, "Pull", pull, busy(self, OpKind::Pull))
                .on_hover_text(format!(
                    "{}\n{}",
                    with_shortcut(tr!("Récupérer et intégrer la branche suivie"), Command::Pull),
                    tr!("Ctrl+clic : pull rapide, sans dialogue")
                ))
                .clicked()
            {
                self.run_command(if ctrl(ui) { Command::QuickPull } else { Command::Pull }, shared);
            }
            if toolbar_button(ui, icons::ARROW_LINE_UP, "Push", push, busy(self, OpKind::Push))
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
            if toolbar_button(ui, icons::TRAY_ARROW_DOWN, "Stash", stash, busy(self, OpKind::Stash))
                .on_hover_text(with_shortcut(tr!("Mettre de côté les modifications en cours"), Command::Stash))
                .clicked()
            {
                self.run_command(Command::Stash, shared);
            }
            let has_stash = !self.stashes.is_empty();
            if toolbar_button(ui, icons::TRAY_ARROW_UP, "Pop", has_stash, busy(self, OpKind::StashPop))
                .on_hover_text(tr!("Réappliquer le dernier stash et le supprimer"))
                .clicked()
                && let Some(top) = self.stashes.first()
            {
                let (oid, label) = (top.oid, format!("stash@{{0}} {}", top.message));
                self.op_stash(shared, oid, label, StashOp::Pop);
            }
            // Flèche ▾ : choisir un autre stash (Pop, Appliquer… comme dans la barre latérale).
            self.pop_dropdown(ui, shared, has_stash);
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
                // « Ouvrir dans ▾ » : le clic principal reprend le dernier éditeur utilisé, la flèche liste les autres.
                // Dessinés de droite à gauche : la flèche d'abord.
                self.open_in_dropdown(ui, shared);
                let main = crate::editors::main_editor(&shared.editors, shared.settings.last_editor.as_deref());
                let editor = shared.editors.get(main).map(|e| e.kind);
                let (icon, name) = editor.map_or((icons::CODE, ""), |k| (k.icon(), k.label()));
                if self.can_run(Command::OpenInEditor)
                    && toolbar_button(ui, icon, name, true, false)
                        .on_hover_text(tr!("Ouvrir dans {name}", name))
                        .clicked()
                {
                    self.run_command(Command::OpenInEditor, shared);
                }
                if toolbar_button(ui, icons::FOLDER_OPEN, tr!("Explorateur"), true, false).clicked() {
                    self.run_command(Command::OpenExplorer, shared);
                }
                if toolbar_button(ui, icons::TERMINAL_WINDOW, "Terminal", true, false).clicked() {
                    self.run_command(Command::OpenTerminal, shared);
                }
                self.account_chip(ui, shared, p);
            });
        });
    }

    /// Compte associé au dépôt, s'il y en a un : son jeton authentifie fetch, pull et push ; un clic ouvre le choix
    /// du compte.
    fn account_chip(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        let Some((kind, title)) = self.account(&shared.settings).map(|a| (a.kind, a.title())) else { return };
        ui.separator();
        let text = RichText::new(format!("{} {title}", crate::accounts::kind_icon(kind))).size(12.0).color(p.weak);
        let chip = ui
            .add_enabled(self.can_run(Command::UseAccount), egui::Button::new(text).frame(false).truncate())
            .on_hover_text(tr!("Fetch, pull et push s'authentifient avec ce compte. Clic : choisir un autre compte."));
        if chip.clicked() {
            self.run_command(Command::UseAccount, shared);
        }
    }

    /// Flèche à côté de Pop : la liste des stashs, chacun avec le menu d'un stash (Pop, Appliquer, Renommer…).
    fn pop_dropdown(&mut self, ui: &mut Ui, shared: &mut Shared, enabled: bool) {
        let label = tr!("Choisir le stash");
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(16.0, 46.0), egui::Sense::click());
        crate::widgets::a11y(&resp, label, enabled);
        let p = Palette::current(ui.ctx());
        if enabled && resp.hovered() {
            ui.painter().rect_filled(rect, egui::CornerRadius::same(6), p.hover);
        }
        ui.painter().text(
            rect.center_top() + egui::vec2(0.0, 15.0),
            egui::Align2::CENTER_CENTER,
            icons::CARET_DOWN,
            egui::FontId::proportional(13.0),
            if enabled { p.text } else { p.weak.gamma_multiply(0.6) },
        );
        if !enabled {
            return;
        }
        let stashes = self.stashes.clone();
        egui::Popup::menu(&resp.on_hover_text(tr!("Choisir le stash à réappliquer"))).show(|ui| {
            for s in stashes.iter() {
                let label = super::scope::stash_label(s.index, &s.message);
                ui.menu_button(label.clone(), |ui| self.stash_menu(ui, shared, s.oid, &label));
            }
        });
    }

    /// Flèche à côté du bouton « Ouvrir dans » : tous les éditeurs détectés.
    fn open_in_dropdown(&mut self, ui: &mut Ui, shared: &mut Shared) {
        let label = tr!("Choisir l'éditeur");
        let enabled = self.can_run(Command::OpenInEditor);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(16.0, 46.0), egui::Sense::click());
        crate::widgets::a11y(&resp, label, enabled);
        let p = Palette::current(ui.ctx());
        if enabled && resp.hovered() {
            ui.painter().rect_filled(rect, egui::CornerRadius::same(6), p.hover);
        }
        ui.painter().text(
            rect.center_top() + egui::vec2(0.0, 15.0),
            egui::Align2::CENTER_CENTER,
            icons::CARET_DOWN,
            egui::FontId::proportional(13.0),
            if enabled { p.text } else { p.weak.gamma_multiply(0.6) },
        );
        if !enabled {
            return;
        }
        let mut chosen = None;
        egui::Popup::menu(&resp.on_hover_text(tr!("Ouvrir le dépôt dans un autre éditeur"))).show(|ui| {
            for (i, editor) in shared.editors.iter().enumerate() {
                if ui.button(format!("{} {}", editor.kind.icon(), editor.kind.label())).clicked() {
                    chosen = Some(i);
                    ui.close();
                }
            }
        });
        if let Some(i) = chosen {
            self.run_command(Command::OpenIn(i), shared);
        }
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
