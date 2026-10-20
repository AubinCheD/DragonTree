//! Changements locaux : fichiers non indexés / indexés / en conflit, diff, zone de commit.

use std::sync::Arc;

use dragontree_git::{ConflictKind, FileState, OpState, PatchMode, StatusEntry, StatusSnapshot};
use dragontree_i18n::{tr, trn};
use egui::{Frame, Key, RichText, Ui};

use super::commit_message::{self, LengthLevel};
use super::details::file_row;
use super::diff_view::{self, DiffMode};
use super::patches::PatchSource;
use super::{Dialog, RepoTab, Side};
use crate::app::{AppAction, Shared};
use crate::commands::Command;
use crate::external_tools::ExternalDiff;
use crate::icons;
use crate::settings::{RECENT_MESSAGES_MAX, RecentMessage};
use crate::theme::Palette;
use crate::widgets;

/// Auteurs listés au plus dans le menu « Co-auteur » (la recherche restreint la liste).
const COAUTHORS_SHOWN: usize = 50;

/// Résumé raccourci pour un élément de menu.
fn ellipsized(subject: &str) -> String {
    const MAX: usize = 70;
    if subject.chars().count() <= MAX {
        subject.to_string()
    } else {
        format!("{}…", subject.chars().take(MAX - 1).collect::<String>())
    }
}

const ROW_H: f32 = 22.0;

fn status_letter(e: &StatusEntry, side: Side, p: &Palette) -> (&'static str, egui::Color32, &'static str) {
    if side == Side::Conflict {
        let label = match e.conflict {
            Some(ConflictKind::BothModified) => tr!("modifié des deux côtés"),
            Some(ConflictKind::AddedByUs) => tr!("ajouté de notre côté"),
            Some(ConflictKind::AddedByThem) => tr!("ajouté de leur côté"),
            Some(ConflictKind::DeletedByUs) => tr!("supprimé de notre côté"),
            Some(ConflictKind::DeletedByThem) => tr!("supprimé de leur côté"),
            Some(ConflictKind::BothAdded) => tr!("ajouté des deux côtés"),
            Some(ConflictKind::BothDeleted) => tr!("supprimé des deux côtés"),
            None => tr!("en conflit"),
        };
        return ("!", p.conflict_fg, label);
    }
    let state = if side == Side::Staged { e.index } else { e.worktree };
    match state {
        FileState::Added => ("A", p.added_fg, tr!("ajouté")),
        FileState::Untracked => ("N", p.added_fg, tr!("nouveau (non suivi)")),
        FileState::Deleted => ("D", p.removed_fg, tr!("supprimé")),
        FileState::Renamed => ("R", p.renamed_fg, tr!("renommé")),
        FileState::Copied => ("C", p.renamed_fg, tr!("copié")),
        FileState::TypeChanged => ("T", p.modified_fg, tr!("type modifié")),
        _ => ("M", p.modified_fg, tr!("modifié")),
    }
}

/// Les chemins choisis plus l'ancien nom des renommages indexés qu'ils contiennent : sans lui, la suppression de
/// l'ancien nom resterait en place (stash à moitié fait, patch qui n'efface pas l'ancien fichier).
fn with_rename_origins(paths: &[String], status: &StatusSnapshot) -> Vec<String> {
    let mut all = paths.to_vec();
    for entry in &status.entries {
        if let Some(orig) = &entry.orig_path
            && paths.contains(&entry.path)
            && !all.contains(orig)
        {
            all.push(orig.clone());
        }
    }
    all
}

impl RepoTab {
    /// Fichiers sélectionnables d'une liste (les fichiers ignorés n'en font jamais partie : ils s'affichent à part
    /// et ne s'indexent que par « Indexer quand même »). `hide_untracked` écarte les non suivis des non indexés.
    pub(super) fn entries(status: &StatusSnapshot, side: Side, hide_untracked: bool) -> Vec<StatusEntry> {
        let v = match side {
            Side::Unstaged if hide_untracked => {
                status.unstaged().into_iter().filter(|e| e.worktree != FileState::Untracked).collect()
            }
            Side::Unstaged => status.unstaged(),
            Side::Staged => status.staged(),
            Side::Conflict => status.conflicted(),
        };
        v.into_iter().cloned().collect()
    }

    /// Sélectionne le premier fichier modifié si rien n'est sélectionné.
    pub(super) fn ensure_change_selected(&mut self, shared: &Shared) {
        if self.changes.selected.is_some() {
            return;
        }
        let Some(status) = self.status.clone() else { return };
        let first = [Side::Conflict, Side::Unstaged, Side::Staged].into_iter().find_map(|side| {
            Self::entries(&status, side, shared.settings.hide_untracked_files).first().map(|e| (side, e.path.clone()))
        });
        if let Some((side, path)) = first {
            self.select_change(shared, side, path);
        }
    }

    fn select_change(&mut self, shared: &Shared, side: Side, path: String) {
        self.changes.selected = Some((side, path.clone()));
        self.changes.multi = [(side, path.clone())].into();
        if side == Side::Conflict {
            self.changes.diff = None;
            self.changes.diff_key = None;
        } else {
            self.load_work_diff(shared, side, path, false);
        }
    }

    pub(super) fn changes_ui(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        let Some(status) = self.status.clone() else {
            ui.centered_and_justified(|ui| ui.spinner());
            return;
        };
        // Le modèle de message intact compte pour une zone vide : le message de la fusion le remplace.
        let c = &self.changes;
        let template_only = c.template_prefill.as_ref().is_some_and(|(s, b)| *s == c.subject && *b == c.body);
        if self.op_state == OpState::Merge && (c.subject.is_empty() || template_only) && !c.merge_msg_loaded {
            self.changes.merge_msg_loaded = true;
            if let Some(repo) = &self.repo
                && let Ok(msg) = std::fs::read_to_string(repo.git_dir.join("MERGE_MSG"))
            {
                let cleaned: String = msg.lines().filter(|l| !l.starts_with('#')).collect::<Vec<_>>().join("\n");
                let (s, b) = super::split_message(&cleaned);
                self.changes.subject = s;
                self.changes.body = b;
                self.changes.template_prefill = None;
            }
        }
        if self.op_state == OpState::None {
            self.changes.merge_msg_loaded = false;
        }
        self.changes_keyboard(ui, shared, &status);

        let avail = ui.available_height();
        egui::Panel::left(("changes-lists", self.id))
            .resizable(true)
            .default_size(440.0)
            .size_range(260.0..=1000.0)
            .frame(Frame::new().fill(p.panel).inner_margin(egui::Margin::symmetric(6, 4)))
            .show(ui, |ui| {
                egui::Panel::bottom(("commit-box", self.id))
                    .frame(Frame::new().fill(p.panel).inner_margin(egui::Margin::symmetric(2, 6)))
                    .show(ui, |ui| self.commit_box(ui, shared, p, &status));
                egui::Panel::bottom(("staged-list", self.id))
                    .resizable(true)
                    .default_size(((avail - 200.0) / 2.0).max(120.0))
                    .size_range(70.0..=(avail - 160.0).max(80.0))
                    .frame(Frame::new().fill(p.panel))
                    .show(ui, |ui| self.file_list(ui, shared, p, &status, Side::Staged));
                egui::CentralPanel::default().frame(Frame::new().fill(p.panel)).show(ui, |ui| {
                    if !status.conflicted().is_empty() {
                        let h = (status.conflicted().len() as f32 * ROW_H + 30.0).min(ui.available_height() / 2.0);
                        egui::Panel::top(("conflict-list", self.id))
                            .exact_size(h)
                            .frame(Frame::new().fill(p.panel))
                            .show(ui, |ui| self.file_list(ui, shared, p, &status, Side::Conflict));
                    }
                    self.file_list(ui, shared, p, &status, Side::Unstaged);
                });
            });

        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg))
            .show(ui, |ui| self.change_detail(ui, shared, p, &status));
    }

    fn changes_keyboard(&mut self, ui: &Ui, shared: &mut Shared, status: &Arc<StatusSnapshot>) {
        if self.dialog.is_some() || ui.ctx().memory(|m| m.focused().is_some()) {
            return;
        }
        let Some((side, path)) = self.changes.selected.clone() else { return };
        let (space, enter, del, down, up) = ui.ctx().input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Delete),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::ArrowUp),
            )
        });
        if space || enter {
            self.toggle_stage_selection(shared);
        }
        if del && side == Side::Unstaged {
            self.ask_discard_selection(status);
        }
        if down || up {
            let entries = Self::entries(status, side, shared.settings.hide_untracked_files);
            if let Some(i) = entries.iter().position(|e| e.path == path) {
                let next = if down { (i + 1).min(entries.len().saturating_sub(1)) } else { i.saturating_sub(1) };
                if next != i {
                    self.changes.anchor = Some((side, entries[next].path.clone()));
                    self.select_change(shared, side, entries[next].path.clone());
                }
            }
        }
    }

    fn selection_paths(&self, side: Side) -> Vec<String> {
        self.changes.multi.iter().filter(|(s, _)| *s == side).map(|(_, p)| p.clone()).collect()
    }

    fn toggle_stage_selection(&mut self, shared: &Shared) {
        let Some((side, _)) = self.changes.selected.clone() else { return };
        let paths = self.selection_paths(side);
        match side {
            Side::Unstaged => self.op_stage(shared, paths),
            Side::Staged => self.op_unstage(shared, paths),
            Side::Conflict => self.op_stage(shared, paths),
        }
    }

    fn ask_discard_selection(&mut self, status: &StatusSnapshot) {
        let paths = self.selection_paths(Side::Unstaged);
        let (mut tracked, mut untracked) = (Vec::new(), Vec::new());
        for path in paths {
            let is_untracked = status.entries.iter().any(|e| e.path == path && e.worktree == FileState::Untracked);
            if is_untracked { untracked.push(path) } else { tracked.push(path) }
        }
        if !tracked.is_empty() || !untracked.is_empty() {
            self.dialog = Some(Dialog::Discard { tracked, untracked });
        }
    }

    fn file_list(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette, status: &Arc<StatusSnapshot>, side: Side) {
        let entries = Self::entries(status, side, shared.settings.hide_untracked_files);
        // Fichiers ignorés : en fin de la liste des non indexés, seulement si le statut les a demandés.
        let ignored: Vec<StatusEntry> = if side == Side::Unstaged && shared.settings.show_ignored_files {
            status.ignored().into_iter().cloned().collect()
        } else {
            Vec::new()
        };
        ui.horizontal(|ui| {
            ui.set_height(26.0);
            let (title, icon) = match side {
                Side::Unstaged => (tr!("Non indexés"), icons::FILE),
                Side::Staged => (tr!("Indexés"), icons::CHECK_CIRCLE),
                Side::Conflict => (tr!("Conflits"), icons::WARNING),
            };
            let color = if side == Side::Conflict { p.conflict_fg } else { p.text };
            ui.label(RichText::new(format!("{icon} {title}")).strong().color(color));
            widgets::count_pill(ui, entries.len(), p);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if side == Side::Unstaged {
                    self.display_options_menu(ui, shared);
                }
                match side {
                    Side::Unstaged if !entries.is_empty() => {
                        if ui.button(format!("{} {}", icons::PLUS, tr!("Tout indexer"))).clicked() {
                            let all = entries.iter().map(|e| e.path.clone()).collect();
                            self.op_stage(shared, all);
                        }
                        if ui
                            .small_button(icons::ARROW_COUNTER_CLOCKWISE)
                            .on_hover_text(tr!("Annuler toutes les modifications non indexées…"))
                            .clicked()
                        {
                            let (mut tracked, mut untracked) = (Vec::new(), Vec::new());
                            for e in &entries {
                                if e.worktree == FileState::Untracked {
                                    untracked.push(e.path.clone());
                                } else {
                                    tracked.push(e.path.clone());
                                }
                            }
                            self.dialog = Some(Dialog::Discard { tracked, untracked });
                        }
                    }
                    Side::Staged
                        if !entries.is_empty()
                            && ui.button(format!("{} {}", icons::MINUS, tr!("Tout désindexer"))).clicked() =>
                    {
                        let all = entries.iter().map(|e| e.path.clone()).collect();
                        self.op_unstage(shared, all);
                    }
                    _ => {}
                }
            });
        });
        if entries.is_empty() && ignored.is_empty() {
            let msg = match side {
                Side::Unstaged => tr!("Aucune modification non indexée"),
                Side::Staged => tr!("Aucun fichier indexé"),
                Side::Conflict => "",
            };
            ui.label(RichText::new(msg).color(p.weak).size(12.0));
            return;
        }
        let modifiers = ui.ctx().input(|i| i.modifiers);
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut clicked: Option<(usize, bool)> = None;
        egui::ScrollArea::vertical().id_salt(("files", side, self.id)).auto_shrink([false, false]).show_rows(
            ui,
            ROW_H,
            entries.len() + ignored.len(),
            |ui, range| {
                for i in range {
                    if let Some(e) = i.checked_sub(entries.len()).map(|j| &ignored[j]) {
                        self.ignored_row(ui, shared, p, e);
                        continue;
                    }
                    let e = &entries[i];
                    let key = (side, e.path.clone());
                    let selected = self.changes.multi.contains(&key);
                    let (letter, color, label) = status_letter(e, side, p);
                    let orig = if side == Side::Staged { e.orig_path.as_deref() } else { None };
                    let resp = file_row(ui, (side, &e.path), letter, color, &e.path, orig, selected, p);
                    // Fichier géré par Git LFS : icône à droite de la ligne.
                    let resp = if self.lfs_files.contains(&e.path) {
                        ui.painter().text(
                            resp.rect.right_center() - egui::vec2(14.0, 0.0),
                            egui::Align2::CENTER_CENTER,
                            icons::HARD_DRIVES,
                            egui::FontId::proportional(13.0),
                            p.weak,
                        );
                        resp.on_hover_text(format!(
                            "{label} — {}",
                            tr!("fichier géré par Git LFS (pointeur dans le dépôt)")
                        ))
                    } else {
                        resp.on_hover_text(label)
                    };
                    if resp.double_clicked() {
                        clicked = Some((i, true));
                    } else if resp.clicked() || resp.secondary_clicked() && !selected {
                        clicked = Some((i, false));
                    }
                    let path = e.path.clone();
                    let untracked = e.worktree == FileState::Untracked;
                    resp.context_menu(|ui| self.change_menu(ui, shared, side, &path, untracked, status));
                }
            },
        );
        if let Some((i, double)) = clicked {
            let path = entries[i].path.clone();
            if double {
                match side {
                    Side::Unstaged => self.op_stage(shared, vec![path]),
                    Side::Staged => self.op_unstage(shared, vec![path]),
                    Side::Conflict => {}
                }
                return;
            }
            let key = (side, path.clone());
            if modifiers.ctrl {
                if !self.changes.multi.remove(&key) {
                    self.changes.multi.insert(key);
                }
                self.changes.multi.retain(|(s, _)| *s == side);
                self.changes.anchor = Some((side, path.clone()));
                self.changes.selected = Some((side, path.clone()));
                if side != Side::Conflict {
                    self.load_work_diff(shared, side, path, false);
                }
            } else if modifiers.shift
                && let Some((aside, apath)) = &self.changes.anchor
                && *aside == side
                && let Some(a) = entries.iter().position(|e| &e.path == apath)
            {
                let (lo, hi) = if a <= i { (a, i) } else { (i, a) };
                self.changes.multi = entries[lo..=hi].iter().map(|e| (side, e.path.clone())).collect();
                self.changes.selected = Some((side, path.clone()));
                if side != Side::Conflict {
                    self.load_work_diff(shared, side, path, false);
                }
            } else {
                self.changes.anchor = Some((side, path.clone()));
                self.select_change(shared, side, path);
            }
        }
    }

    /// Menu « … » de la liste des non indexés : les mêmes options que le menu Affichage.
    fn display_options_menu(&mut self, ui: &mut Ui, shared: &mut Shared) {
        ui.menu_button(icons::DOTS_THREE, |ui| {
            for (cmd, on) in [
                (Command::ShowIgnoredFiles, shared.settings.show_ignored_files),
                (Command::HideUntrackedFiles, shared.settings.hide_untracked_files),
            ] {
                let mut checked = on;
                if ui.checkbox(&mut checked, cmd.label()).clicked() {
                    shared.actions.push(AppAction::RunCommand(cmd));
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text(tr!("Options d'affichage"));
    }

    /// Ligne d'un fichier ignoré : atténuée, icône propre, sans sélection ni indexation en bloc ;
    /// seul le menu contextuel propose de l'indexer (`add -f`).
    fn ignored_row(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette, e: &StatusEntry) {
        let resp = ui
            .scope(|ui| {
                ui.multiply_opacity(0.55);
                file_row(ui, (Side::Unstaged, "ignored", &e.path), icons::EYE_SLASH, p.weak, &e.path, None, false, p)
            })
            .inner
            .on_hover_text(tr!("ignoré (.gitignore)"));
        let path = e.path.clone();
        resp.context_menu(|ui| {
            if ui.button(format!("{} {}", icons::PLUS, tr!("Indexer quand même"))).clicked() {
                self.op_stage_force(shared, vec![path.clone()]);
                ui.close();
            }
        });
    }

    fn change_menu(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        side: Side,
        path: &str,
        untracked: bool,
        status: &StatusSnapshot,
    ) {
        let mut paths = self.selection_paths(side);
        if !paths.iter().any(|p| p == path) {
            paths = vec![path.to_string()];
        }
        let n = paths.len();
        match side {
            Side::Unstaged => {
                if ui.button(format!("{} {}", icons::PLUS, trn!(n, "Indexer", "Indexer ({n} fichiers)"))).clicked() {
                    self.op_stage(shared, paths.clone());
                    ui.close();
                }
                let discard = trn!(n, "Annuler les modifications…", "Annuler les modifications ({n} fichiers)…");
                if ui.button(format!("{} {discard}", icons::ARROW_COUNTER_CLOCKWISE)).clicked() {
                    let (mut tracked, mut untracked_list) = (Vec::new(), Vec::new());
                    for p in &paths {
                        if status.entries.iter().any(|e| &e.path == p && e.worktree == FileState::Untracked) {
                            untracked_list.push(p.clone());
                        } else {
                            tracked.push(p.clone());
                        }
                    }
                    self.dialog = Some(Dialog::Discard { tracked, untracked: untracked_list });
                    ui.close();
                }
                if untracked {
                    ui.menu_button(format!("{} {}", icons::EYE_SLASH, tr!("Ignorer")), |ui| {
                        let mut patterns = vec![path.to_string()];
                        if let Some(ext) = std::path::Path::new(path).extension() {
                            patterns.push(format!("*.{}", ext.to_string_lossy()));
                        }
                        if let Some((dir, _)) = path.rsplit_once('/') {
                            patterns.push(format!("{dir}/"));
                        }
                        for pat in patterns {
                            if ui.button(tr!("Ajouter « {pat} » au .gitignore", pat)).clicked() {
                                self.add_to_gitignore(shared, &pat);
                                ui.close();
                            }
                        }
                    });
                    if ui.button(format!("{} {}", icons::EYE_SLASH, tr!("Ignorer…"))).clicked() {
                        self.open_ignore(Some(path.to_string()));
                        ui.close();
                    }
                }
            }
            Side::Staged => {
                if ui
                    .button(format!("{} {}", icons::MINUS, trn!(n, "Désindexer", "Désindexer ({n} fichiers)")))
                    .clicked()
                {
                    self.op_unstage(shared, paths.clone());
                    ui.close();
                }
            }
            Side::Conflict => {
                if ui
                    .button(format!("{} {}", icons::ARROW_BEND_DOWN_RIGHT, tr!("Résoudre avec notre version")))
                    .clicked()
                {
                    self.resolve_side(shared, paths.clone(), true);
                    ui.close();
                }
                if ui
                    .button(format!("{} {}", icons::ARROW_BEND_DOWN_RIGHT, tr!("Résoudre avec leur version")))
                    .clicked()
                {
                    self.resolve_side(shared, paths.clone(), false);
                    ui.close();
                }
                if ui.button(format!("{} {}", icons::GIT_DIFF, tr!("Ouvrir l'éditeur de conflits"))).clicked() {
                    self.open_conflict_editor(path.to_string());
                    ui.close();
                }
                if ui
                    .button(format!(
                        "{} {}",
                        icons::CHECK,
                        trn!(n, "Marquer comme résolu", "Marquer comme résolu ({n} fichiers)")
                    ))
                    .clicked()
                {
                    self.op_stage(shared, paths.clone());
                    ui.close();
                }
            }
        }
        if side != Side::Conflict {
            ui.separator();
            if ui.button(format!("{} {}", icons::TRAY_ARROW_DOWN, tr!("Stash des fichiers sélectionnés…"))).clicked()
            {
                self.dialog =
                    Some(Dialog::StashFiles { message: String::new(), paths: with_rename_origins(&paths, status) });
                ui.close();
            }
        }
        if side != Side::Conflict {
            let is_untracked =
                |p: &String| status.entries.iter().any(|e| &e.path == p && e.worktree == FileState::Untracked);
            let (untracked_list, tracked): (Vec<String>, Vec<String>) = paths.iter().cloned().partition(is_untracked);
            // Renommage indexé : l'ancien nom aussi, pour que le patch supprime l'ancien fichier.
            let tracked = with_rename_origins(&tracked, status);
            self.patch_menu_entries(
                ui,
                shared,
                PatchSource::Files { tracked, untracked: untracked_list, staged: side == Side::Staged },
            );
        }
        if side != Side::Conflict {
            // Renommage indexé : la version de HEAD est celle de l'ancien nom.
            let old_path = status.entries.iter().find(|e| e.path == path).and_then(|e| e.orig_path.clone());
            let staged = side == Side::Staged;
            let diff = ExternalDiff::Working { path: path.to_string(), staged, old_path: old_path.filter(|_| staged) };
            self.external_diff_entry(ui, shared, diff);
        }
        ui.separator();
        self.file_menu(ui, shared, path, None, None);
    }

    fn add_to_gitignore(&mut self, shared: &Shared, pattern: &str) {
        let pattern = pattern.to_string();
        self.run_simple(shared, tr!("Ignorer"), super::Refresh::STATUS, move |r| {
            r.add_to_gitignore(&pattern)?;
            Ok(Some(tr!("« {pattern} » ajouté au .gitignore", pattern)))
        });
    }

    pub(super) fn resolve_side(&mut self, shared: &Shared, paths: Vec<String>, ours: bool) {
        self.run_simple(shared, tr!("Résolution"), super::Refresh::STATUS, move |r| {
            for p in &paths {
                r.checkout_side(p, ours)?;
            }
            r.stage(&paths)?;
            Ok(Some(trn!(paths.len(), "{n} fichier résolu", "{n} fichiers résolus")))
        });
    }

    fn change_detail(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette, status: &Arc<StatusSnapshot>) {
        let Some((side, path)) = self.changes.selected.clone() else {
            ui.centered_and_justified(|ui| {
                let msg = if status.is_clean() {
                    tr!("Aucune modification locale — tout est commité.")
                } else {
                    tr!("Sélectionnez un fichier")
                };
                ui.label(RichText::new(msg).color(p.weak));
            });
            return;
        };
        if side == Side::Conflict {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(icons::WARNING).size(36.0).color(p.conflict_fg));
                ui.label(RichText::new(&path).strong().size(15.0));
                let e = status.entries.iter().find(|e| e.path == path);
                if let Some(e) = e {
                    ui.label(RichText::new(status_letter(e, Side::Conflict, p).2).color(p.weak));
                }
                ui.add_space(12.0);
                if ui
                    .button(
                        RichText::new(format!("{}  {}", icons::GIT_DIFF, tr!("Ouvrir l'éditeur de conflits")))
                            .size(14.0),
                    )
                    .clicked()
                {
                    self.open_conflict_editor(path.clone());
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 470.0).max(0.0) / 2.0);
                    if ui.button(tr!("Garder notre version")).clicked() {
                        self.resolve_side(shared, vec![path.clone()], true);
                    }
                    if ui.button(tr!("Garder leur version")).clicked() {
                        self.resolve_side(shared, vec![path.clone()], false);
                    }
                    if ui.button(format!("{} {}", icons::CHECK, tr!("Marquer comme résolu"))).clicked() {
                        self.op_stage(shared, vec![path.clone()]);
                    }
                });
                ui.add_space(4.0);
                if ui
                    .button(format!("{} {}", icons::ARROW_SQUARE_OUT, tr!("Outil de merge externe")))
                    .on_hover_text(tr!("L'outil choisi dans Réglages › Outils externes (git mergetool par défaut)"))
                    .clicked()
                {
                    self.run_external_merge(shared, path.clone());
                }
            });
            return;
        }

        let Some(diff) =
            self.changes.diff.clone().filter(|_| self.changes.diff_key.as_ref() == Some(&(side, path.clone())))
        else {
            ui.centered_and_justified(|ui| ui.spinner());
            return;
        };
        self.diff_header(ui, shared, &diff, (side == Side::Staged).then_some(tr!("(indexé)")), p);
        if diff.is_binary && super::image_diff::is_image(&path) {
            use super::image_diff::ImageSource;
            let workdir = self.repo.as_ref().map(|r| r.workdir.clone()).unwrap_or_default();
            let (old, new) = if side == Side::Staged {
                (ImageSource::Blob("HEAD".into(), path.clone()), ImageSource::Blob(String::new(), path.clone()))
            } else {
                (ImageSource::Blob(String::new(), path.clone()), ImageSource::File(workdir.join(&path)))
            };
            // La clé inclut l'état courant : l'aperçu se recharge quand le fichier change.
            let stamp = std::fs::metadata(workdir.join(&path))
                .and_then(|m| m.modified())
                .map(|t| format!("{t:?}"))
                .unwrap_or_default();
            let key = format!("{side:?}:{path}:{stamp}:{}", status.staged().len());
            self.image_preview_ui(ui, shared, false, key, old, new, p);
            return;
        }
        let mode = if side == Side::Staged { DiffMode::Staged } else { DiffMode::Unstaged };
        let renamed_staged = side == Side::Staged && diff.old_path.is_some() && diff.old_path != diff.new_path;
        // Un diff calculé sans les espaces (-w) ne correspond pas au fichier : on ne peut pas en
        // tirer un patch fiable pour indexer ou annuler des lignes.
        let ignore_ws = shared.settings.ignore_whitespace;
        if ignore_ws {
            ui.label(
                RichText::new(format!(
                    "{} {}",
                    icons::INFO,
                    tr!("Lecture seule : désactivez « Espaces » (ignorer les espaces) pour indexer ou annuler des lignes.")
                ))
                .color(p.weak)
                .size(12.0),
            );
        }
        let mode = if renamed_staged || ignore_ws { DiffMode::ReadOnly } else { mode };
        if self.changes.highlighter.is_dark() != p.dark {
            self.changes.highlighter = crate::highlight::Highlighter::new(p.dark);
        }
        let hl = shared.settings.syntax_highlight.then_some(&mut self.changes.highlighter);
        let action = diff_view::show(
            ui,
            &diff,
            mode,
            &mut self.changes.diff_state,
            shared.settings.split_diff,
            &crate::diff_style::DiffStyle::from_settings(&shared.settings),
            hl,
            p,
            ("work", self.id),
        );
        if let Some(a) = action {
            if a.mode == PatchMode::Discard {
                self.dialog = Some(Dialog::DiscardLines { selection: a.selection });
            } else {
                self.op_patch(shared, a.mode, a.selection);
                self.changes.diff_state.selected.clear();
            }
        }
    }

    /// Contenu du menu « Messages récents » : messages mémorisés pour ce dépôt, puis sujets de l'historique chargé.
    /// Renvoie le choix et vrai s'il remplace aussi la description (message mémorisé) ; un sujet de l'historique ne
    /// remplace que le résumé : la description saisie n'est jamais effacée.
    fn recent_messages_menu(&self, ui: &mut Ui, shared: &Shared, p: &Palette) -> Option<(RecentMessage, bool)> {
        let stored = shared.settings.recent_messages_of(&self.path);
        let history = commit_message::recent_subjects(&self.history.loaded.commits, stored, RECENT_MESSAGES_MAX);
        let mut picked = None;
        if stored.is_empty() && history.is_empty() {
            ui.label(RichText::new(tr!("Aucun message pour l'instant.")).color(p.weak));
        }
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for message in stored {
                let mut response = ui.button(ellipsized(&message.subject));
                if !message.body.is_empty() {
                    response = response.on_hover_text(&message.body);
                }
                if response.clicked() {
                    picked = Some((message.clone(), true));
                    ui.close();
                }
            }
            if !history.is_empty() {
                if !stored.is_empty() {
                    ui.separator();
                }
                ui.label(RichText::new(tr!("Sujets de l'historique")).color(p.weak).size(11.5));
                for subject in &history {
                    if ui.button(ellipsized(subject)).clicked() {
                        picked = Some((RecentMessage { subject: subject.clone(), body: String::new() }, false));
                        ui.close();
                    }
                }
            }
        });
        picked
    }

    /// Contenu du menu « Co-auteur » : auteurs de l'historique chargé, avec recherche.
    fn coauthors_menu(&mut self, ui: &mut Ui, p: &Palette) -> Option<commit_message::Author> {
        let commits = &self.history.loaded.commits;
        if self.changes.authors.as_ref().is_none_or(|(loaded, _)| !Arc::ptr_eq(loaded, commits)) {
            self.changes.authors = Some((Arc::clone(commits), commit_message::known_authors(commits)));
        }
        let authors = self.changes.authors.as_ref().map_or(&[][..], |(_, list)| list.as_slice());
        ui.add(
            egui::TextEdit::singleline(&mut self.changes.coauthor_query)
                .hint_text(tr!("Rechercher un auteur"))
                .desired_width(280.0),
        );
        let mut picked = None;
        let mut shown = 0;
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for author in authors.iter().filter(|a| a.matches(&self.changes.coauthor_query)).take(COAUTHORS_SHOWN) {
                shown += 1;
                if ui.button(format!("{} <{}>", author.name, author.email)).clicked() {
                    picked = Some(author.clone());
                    ui.close();
                }
            }
        });
        if shown == 0 {
            ui.label(RichText::new(tr!("Aucun auteur trouvé.")).color(p.weak));
        }
        picked
    }

    fn commit_box(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette, status: &Arc<StatusSnapshot>) {
        let staged = status.staged().len();
        let conflicts = status.conflicted().len();
        let merging = self.op_state == OpState::Merge;
        let committing = self.running.iter().any(|o| o.title.starts_with("Commit"));

        let subject = ui.add(
            egui::TextEdit::singleline(&mut self.changes.subject)
                .id_salt(("subject", self.id))
                .hint_text(tr!("Résumé du commit (obligatoire)"))
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Body),
        );
        let soft = shared.settings.commit_soft_limit;
        let hard = shared.settings.commit_hard_limit.max(soft);
        let len = self.changes.subject.chars().count();
        let (mut picked, mut coauthor) = (None, None);
        ui.horizontal(|ui| {
            ui.menu_button(format!("{} {}", tr!("Messages récents"), icons::CARET_DOWN), |ui| {
                picked = self.recent_messages_menu(ui, shared, p);
            });
            ui.menu_button(format!("{} {}", tr!("Co-auteur"), icons::CARET_DOWN), |ui| {
                coauthor = self.coauthors_menu(ui, p);
            });
            // Le diff indexé est ce que Claude lit : rien à résumer sans fichier indexé, ni en modifiant un commit.
            self.ai_commit_controls(ui, shared, p, staged > 0 && !self.changes.amend && !committing);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let level = commit_message::length_level(len, soft, hard);
                let color = match level {
                    LengthLevel::Normal => p.weak,
                    LengthLevel::Warning => p.modified_fg,
                    LengthLevel::Error => p.removed_fg,
                };
                // Sous la première limite on compte vers elle, au-delà vers la seconde.
                let shown = if level == LengthLevel::Normal { soft } else { hard };
                ui.label(RichText::new(format!("{len}/{shown}")).size(11.0).color(color)).on_hover_text(tr!(
                    "Résumé : {soft} caractères conseillés, {hard} au plus",
                    soft,
                    hard
                ));
            });
        });
        self.ai_commit_notices(ui, p);
        if let Some((message, with_body)) = picked {
            self.changes.subject = message.subject;
            if with_body {
                self.changes.body = message.body;
            }
            // Le message est celui de l'utilisateur : le modèle ne le remplacera plus.
            self.changes.template_prefill = None;
        }
        if let Some(author) = coauthor {
            commit_message::add_coauthor(&mut self.changes.body, &author);
        }
        // Police à chasse fixe : la colonne guide tombe sur un nombre de caractères exact.
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let margin = egui::Margin::symmetric(4, 2);
        let body = ui.add(
            egui::TextEdit::multiline(&mut self.changes.body)
                .id_salt(("body", self.id))
                .hint_text(tr!("Description (facultatif)"))
                .font(font.clone())
                .margin(margin)
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );
        let char_width = ui.ctx().fonts_mut(|f| f.glyph_width(&font, '0'));
        let guide_x = body.rect.left() + f32::from(margin.left) + char_width * hard as f32;
        if guide_x < body.rect.right() - f32::from(margin.right) {
            let line = [egui::pos2(guide_x, body.rect.top() + 1.0), egui::pos2(guide_x, body.rect.bottom() - 1.0)];
            ui.painter()
                .with_clip_rect(body.rect)
                .line_segment(line, egui::Stroke::new(1.0, p.weak.gamma_multiply(0.45)));
        }
        ui.add_space(4.0);
        if conflicts > 0 {
            ui.label(
                RichText::new(format!("{} {}", icons::WARNING, tr!("Résolvez les conflits avant de valider")))
                    .color(p.conflict_fg)
                    .size(11.5),
            );
        }
        let mut do_commit = false;
        ui.horizontal(|ui| {
            let before = self.changes.amend;
            ui.checkbox(&mut self.changes.amend, "Amend").on_hover_text(tr!("Modifier le dernier commit"));
            if self.changes.amend != before {
                if self.changes.amend {
                    self.changes.saved_message = Some((self.changes.subject.clone(), self.changes.body.clone()));
                    self.load_last_message(shared);
                } else if let Some((s, b)) = self.changes.saved_message.take() {
                    self.changes.subject = s;
                    self.changes.body = b;
                }
            }
            if ui.checkbox(&mut self.changes.push_after, tr!("Pousser après le commit")).changed() {
                shared.settings.push_after_commit = self.changes.push_after;
                shared.settings_dirty = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can = !self.changes.subject.trim().is_empty()
                    && (staged > 0 || self.changes.amend || merging)
                    && conflicts == 0
                    && !committing;
                let label = if self.changes.amend {
                    tr!("Modifier le commit").to_string()
                } else if merging {
                    tr!("Valider la fusion").to_string()
                } else {
                    trn!(staged, "Commit ({n} fichier)", "Commit ({n} fichiers)")
                };
                let btn =
                    egui::Button::new(RichText::new(format!("{}  {label}", icons::CHECK)).strong().color(if can {
                        egui::Color32::WHITE
                    } else {
                        p.weak
                    }))
                    .fill(if can { p.accent } else { p.hover })
                    .min_size(egui::vec2(170.0, 30.0));
                let resp = ui.add_enabled(can, btn).on_hover_text(tr!("Ctrl+Entrée"));
                if resp.clicked() {
                    do_commit = true;
                }
            });
        });
        let ctrl_enter = ui.ctx().input(|i| i.modifiers.ctrl && i.key_pressed(Key::Enter));
        if ctrl_enter && (subject.has_focus() || body.has_focus()) {
            let can = !self.changes.subject.trim().is_empty()
                && (staged > 0 || self.changes.amend || merging)
                && conflicts == 0
                && !committing;
            do_commit |= can;
        }
        if do_commit {
            let mut msg = self.changes.subject.trim().to_string();
            if !self.changes.body.trim().is_empty() {
                msg.push_str("\n\n");
                msg.push_str(self.changes.body.trim_end());
            }
            let (amend, push) = (self.changes.amend, self.changes.push_after);
            self.forget_ai_commit();
            self.op_commit(shared, msg, amend, push);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, orig: Option<&str>) -> StatusEntry {
        StatusEntry {
            path: path.into(),
            orig_path: orig.map(str::to_string),
            index: FileState::Renamed,
            worktree: FileState::Unmodified,
            conflict: None,
            is_submodule: false,
        }
    }

    #[test]
    fn a_staged_rename_brings_its_old_name() {
        let entries = vec![entry("neuf.txt", Some("vieux.txt")), entry("b.txt", None)];
        let status = StatusSnapshot { entries, ..Default::default() };
        let chosen = ["neuf.txt".to_string(), "b.txt".to_string()];
        assert_eq!(with_rename_origins(&chosen, &status), ["neuf.txt", "b.txt", "vieux.txt"]);
        // Un renommage non choisi n'ajoute rien.
        assert_eq!(with_rename_origins(&["b.txt".to_string()], &status), ["b.txt"]);
    }
}
