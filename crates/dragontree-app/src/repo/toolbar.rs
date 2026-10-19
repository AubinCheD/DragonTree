//! Barre d'outils de l'onglet : Fetch, Pull, Push, Stash, Pop, Branche, ouverture externe.

use dragontree_git::HeadState;
use dragontree_i18n::tr;
use egui::{RichText, Ui};

use super::actions::StashOp;
use super::{Dialog, RepoTab};
use crate::app::{AppAction, Shared};
use crate::icons;
use crate::theme::Palette;
use crate::widgets::toolbar_button;

impl RepoTab {
    pub(super) fn toolbar_ui(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        let busy = |tab: &RepoTab, name: &str| tab.running.iter().any(|o| !o.silent && o.title.starts_with(name));
        let head = self.refs.as_ref().map(|r| r.head.clone());
        let on_branch = matches!(head, Some(HeadState::Branch(_)));
        let has_remote = !self.remotes.is_empty();

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

            if toolbar_button(ui, icons::CLOUD_ARROW_DOWN, "Fetch", has_remote, busy(self, "Fetch"))
                .on_hover_text(tr!("Récupérer les nouveautés de tous les remotes (fetch --all --prune)"))
                .clicked()
            {
                self.op_fetch(shared, None, true, false);
            }
            if toolbar_button(ui, icons::ARROW_LINE_DOWN, "Pull", has_remote && on_branch, busy(self, "Pull"))
                .on_hover_text(tr!("Récupérer et intégrer la branche suivie"))
                .clicked()
            {
                self.open_pull_dialog(shared);
            }
            if toolbar_button(ui, icons::ARROW_LINE_UP, "Push", has_remote && on_branch, busy(self, "Push"))
                .on_hover_text(tr!("Envoyer la branche courante"))
                .clicked()
            {
                self.open_push_dialog();
            }
            ui.separator();
            let dirty = self.status.as_ref().is_some_and(|s| !s.is_clean());
            if toolbar_button(ui, icons::TRAY_ARROW_DOWN, "Stash", dirty, busy(self, "Stash"))
                .on_hover_text(tr!("Mettre de côté les modifications en cours"))
                .clicked()
            {
                self.dialog = Some(Dialog::Stash { message: String::new(), untracked: true, keep_index: false });
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
            if toolbar_button(ui, icons::GIT_BRANCH, tr!("Branche"), self.refs.is_some(), false)
                .on_hover_text(tr!("Créer une branche à partir de HEAD"))
                .clicked()
            {
                self.dialog = Some(Dialog::CreateBranch {
                    start: "HEAD".into(),
                    start_label: "HEAD".into(),
                    name: String::new(),
                    checkout: true,
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let path = self.path.clone();
                if toolbar_button(ui, icons::ARROWS_CLOCKWISE, tr!("Actualiser"), true, false)
                    .on_hover_text(tr!("Actualiser (F5)"))
                    .clicked()
                {
                    self.refresh(shared, super::Refresh::ALL);
                }
                if toolbar_button(ui, icons::LIST, tr!("Journal"), true, false)
                    .on_hover_text(tr!("Journal des commandes git (Ctrl+Maj+L)"))
                    .clicked()
                {
                    shared.actions.push(AppAction::ShowLog);
                }
                if crate::util::vscode_available()
                    && toolbar_button(ui, icons::CODE, "VS Code", true, false)
                        .on_hover_text(tr!("Ouvrir dans VS Code"))
                        .clicked()
                {
                    crate::util::open_in_vscode(&path);
                }
                if toolbar_button(ui, icons::FOLDER_OPEN, tr!("Explorateur"), true, false).clicked() {
                    crate::util::open_in_explorer(&path);
                }
                if toolbar_button(ui, icons::TERMINAL_WINDOW, "Terminal", true, false).clicked() {
                    crate::util::open_terminal(&path);
                }
            });
        });
    }

    /// Checkout demandé depuis la palette Ctrl+P.
    pub fn quick_checkout(&mut self, shared: &mut Shared, name: String) {
        self.op_checkout(shared, name);
    }

    /// Commande de la palette Ctrl+P appliquée à l'onglet.
    pub fn quick_command(&mut self, shared: &mut Shared, cmd: &str) {
        match cmd {
            "fetch" => self.op_fetch(shared, None, true, false),
            "pull" => self.open_pull_dialog(shared),
            "push" => self.open_push_dialog(),
            "branch" => {
                self.dialog = Some(Dialog::CreateBranch {
                    start: "HEAD".into(),
                    start_label: "HEAD".into(),
                    name: String::new(),
                    checkout: true,
                })
            }
            "stash" => self.dialog = Some(Dialog::Stash { message: String::new(), untracked: true, keep_index: false }),
            "refresh" => self.refresh(shared, super::Refresh::ALL),
            "changes" => {
                self.view = super::RepoView::Changes;
                self.ensure_change_selected(shared);
            }
            _ => self.view = super::RepoView::History,
        }
    }

    pub(super) fn open_pull_dialog(&mut self, shared: &Shared) {
        let current = self.refs.as_ref().and_then(|r| r.current_branch().map(str::to_string)).unwrap_or_default();
        let (remote, branch) = self
            .current_upstream()
            .unwrap_or_else(|| (self.default_remote().unwrap_or_else(|| "origin".into()), current));
        let _ = shared;
        self.dialog = Some(Dialog::Pull { remote, branch, rebase: false });
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
