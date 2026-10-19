//! Dialogues des opérations (création de branche, merge, reset, push…).

use std::path::PathBuf;

use dragontree_git::{HunkSelection, MergeMode, Oid, OpState, PatchMode, ResetMode, WorktreeStart};
use dragontree_i18n::{tr, trn};
use egui::{RichText, Ui};

use super::actions::StashOp;
use super::{Refresh, RepoTab};
use crate::app::{AppAction, Shared};
use crate::icons;
use crate::theme::Palette;

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    CheckoutDetached(Oid),
    /// Abandon de la fusion, du rebase, du cherry-pick ou du revert en cours.
    AbortOp(OpState),
    /// Ouvrir le worktree où la branche demandée est déjà sortie.
    OpenWorktree(PathBuf),
}

/// Libellé de l'abandon d'une opération en cours (bandeau et confirmation).
pub fn abort_label(state: OpState) -> &'static str {
    match state {
        OpState::Merge => tr!("Abandonner la fusion"),
        OpState::Rebase { .. } => tr!("Abandonner le rebase"),
        OpState::CherryPick => tr!("Abandonner le cherry-pick"),
        OpState::Revert => tr!("Abandonner le revert"),
        OpState::None => tr!("Abandonner"),
    }
}

/// Dialogue modal de l'onglet. `CreateWorktree::pick` : la branche se choisit dans le dialogue (Dépôt › Créer un
/// worktree…) ; `RemoveWorktree::force` (`--force`) : seulement si la case est cochée.
#[derive(Debug, Clone)]
pub enum Dialog {
    CreateBranch { start: String, start_label: String, name: String, checkout: bool },
    RenameBranch { old: String, new: String },
    DeleteBranch { name: String, force: bool, remote: Option<(String, String)>, delete_remote: bool },
    DeleteRemoteBranch { remote: String, branch: String },
    CheckoutRemote { remote_full: String, local: String },
    CreateTag { rev: String, rev_label: String, name: String, message: String, annotated: bool, push: bool },
    DeleteTag { name: String, remote_too: bool },
    Merge { rev: String, mode: MergeMode },
    Rebase { onto: String },
    Reset { rev: String, label: String, mode: ResetMode },
    CherryPick { ids: Vec<Oid>, label: String },
    Revert { id: Oid, label: String },
    Stash { message: String, untracked: bool, keep_index: bool },
    Push { remote: String, local: String, remote_branch: String, set_upstream: bool, force: bool, tags: bool },
    Pull { remote: String, branch: String, rebase: bool },
    Discard { tracked: Vec<String>, untracked: Vec<String> },
    DiscardLines { selection: Vec<HunkSelection> },
    AddRemote { name: String, url: String },
    RemoveRemote { name: String },
    DropStash { oid: Oid, label: String },
    CreateWorktree { start: WorktreeStart, path: String, pick: bool },
    RemoveWorktree { path: PathBuf, force: bool },
    Confirm { title: String, text: String, action: ConfirmAction },
}

impl Dialog {
    pub fn new_tag(rev: String, rev_label: String) -> Dialog {
        Dialog::CreateTag { rev, rev_label, name: String::new(), message: String::new(), annotated: true, push: false }
    }

    fn title_and_ok(&self) -> (String, String, bool) {
        match self {
            Dialog::CreateBranch { .. } => (tr!("Nouvelle branche").into(), tr!("Créer").into(), false),
            Dialog::RenameBranch { old, .. } => (tr!("Renommer « {old} »", old), tr!("Renommer").into(), false),
            Dialog::DeleteBranch { name, .. } => {
                (tr!("Supprimer la branche « {name} »", name), tr!("Supprimer").into(), true)
            }
            Dialog::DeleteRemoteBranch { remote, branch } => {
                (tr!("Supprimer {remote}/{branch}", remote, branch), tr!("Supprimer sur le remote").into(), true)
            }
            Dialog::CheckoutRemote { remote_full, .. } => {
                (tr!("Checkout de {remote_full}", remote_full), "Checkout".into(), false)
            }
            Dialog::CreateTag { .. } => (tr!("Nouveau tag").into(), tr!("Créer").into(), false),
            Dialog::DeleteTag { name, .. } => (tr!("Supprimer le tag « {name} »", name), tr!("Supprimer").into(), true),
            Dialog::Merge { .. } => (tr!("Fusionner").into(), tr!("Fusionner").into(), false),
            Dialog::Rebase { .. } => (tr!("Rebaser").into(), tr!("Rebaser").into(), false),
            Dialog::Reset { .. } => (tr!("Réinitialiser la branche").into(), tr!("Réinitialiser").into(), true),
            Dialog::CherryPick { .. } => ("Cherry-pick".into(), "Cherry-pick".into(), false),
            Dialog::Revert { .. } => ("Revert".into(), tr!("Créer le revert").into(), false),
            Dialog::Stash { .. } => (tr!("Mettre de côté (stash)").into(), "Stash".into(), false),
            Dialog::Push { .. } => ("Push".into(), "Push".into(), false),
            Dialog::Pull { .. } => ("Pull".into(), "Pull".into(), false),
            Dialog::Discard { .. } => {
                (tr!("Annuler des modifications").into(), tr!("Annuler les modifications").into(), true)
            }
            Dialog::DiscardLines { .. } => (tr!("Annuler des lignes").into(), tr!("Annuler les lignes").into(), true),
            Dialog::AddRemote { .. } => (tr!("Ajouter un remote").into(), tr!("Ajouter").into(), false),
            Dialog::RemoveRemote { name } => {
                (tr!("Supprimer le remote « {name} »", name), tr!("Supprimer").into(), true)
            }
            Dialog::DropStash { .. } => (tr!("Supprimer le stash").into(), tr!("Supprimer").into(), true),
            Dialog::CreateWorktree { .. } => (tr!("Créer un worktree").into(), tr!("Créer").into(), false),
            Dialog::RemoveWorktree { .. } => (tr!("Supprimer le worktree").into(), tr!("Supprimer").into(), true),
            Dialog::Confirm { title, action: ConfirmAction::CheckoutDetached(_), .. } => {
                (title.clone(), tr!("Continuer").into(), false)
            }
            Dialog::Confirm { title, action: ConfirmAction::AbortOp(state), .. } => {
                (title.clone(), abort_label(*state).into(), true)
            }
            Dialog::Confirm { title, action: ConfirmAction::OpenWorktree(_), .. } => {
                (title.clone(), tr!("Ouvrir le worktree").into(), false)
            }
        }
    }
}

/// Avertissement d'un dialogue (icône et couleur d'alerte).
pub(super) fn warning(ui: &mut Ui, text: &str, p: &Palette) {
    ui.label(RichText::new(format!("{} {text}", icons::WARNING)).color(p.modified_fg));
}

/// Validation locale d'un nom de branche ou de tag (règles de `git check-ref-format`).
pub fn valid_ref_name(name: &str) -> bool {
    !name.is_empty()
        && name != "@"
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.ends_with(".lock")
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && !name.contains("/.")
        && !name.chars().any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
}

impl RepoTab {
    pub(super) fn dialogs_ui(&mut self, ctx: &egui::Context, shared: &mut Shared, p: &Palette) {
        let Some(mut dialog) = self.dialog.take() else { return };
        let (title, ok_label, danger) = dialog.title_and_ok();
        let mut confirmed = false;
        let mut cancelled = false;
        let response = egui::Modal::new(egui::Id::unique(("dialog", self.id))).show(ctx, |ui| {
            ui.set_width(470.0);
            ui.label(RichText::new(title).strong().size(16.0));
            ui.add_space(10.0);
            let valid = self.dialog_body(ui, &mut dialog, p);
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let fill = if danger { p.removed_fg } else { p.accent };
                    let ok = egui::Button::new(RichText::new(&ok_label).strong().color(egui::Color32::WHITE))
                        .fill(fill)
                        .min_size(egui::vec2(110.0, 28.0));
                    if ui.add_enabled(valid, ok).clicked() {
                        confirmed = true;
                    }
                    if ui.add(egui::Button::new(tr!("Annuler")).min_size(egui::vec2(90.0, 28.0))).clicked() {
                        cancelled = true;
                    }
                });
            });
            // Entrée valide les dialogues sans risque ; les destructeurs exigent un clic.
            let enter = ui.ctx().input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
            if enter && valid && !danger && !matches!(dialog, Dialog::CreateTag { annotated: true, .. }) {
                confirmed = true;
            }
        });
        if confirmed {
            self.execute_dialog(dialog, shared);
        } else if !cancelled && !response.should_close() {
            self.dialog = Some(dialog);
        }
    }

    /// Champs du dialogue ; renvoie vrai si la saisie est valide.
    fn dialog_body(&self, ui: &mut Ui, dialog: &mut Dialog, p: &Palette) -> bool {
        let exists = |name: &str| self.refs.as_ref().is_some_and(|r| r.locals.iter().any(|b| b.name == name));
        let warn = |ui: &mut Ui, text: &str| warning(ui, text, p);
        match dialog {
            Dialog::CreateBranch { start_label, name, checkout, .. } => {
                ui.label(RichText::new(tr!("À partir de : {start_label}", start_label)).color(p.weak));
                let resp = ui.add(
                    egui::TextEdit::singleline(name)
                        .hint_text(tr!("nom-de-la-branche (ex. feature/ma-fonction)"))
                        .desired_width(f32::INFINITY),
                );
                resp.request_focus();
                ui.checkbox(checkout, tr!("Basculer sur la nouvelle branche (checkout)"));
                let ok = valid_ref_name(name.trim()) && !exists(name.trim());
                if !name.trim().is_empty() && !valid_ref_name(name.trim()) {
                    warn(ui, tr!("Nom de branche invalide (pas d'espace ni de caractères ~^:?*[\\)."));
                } else if exists(name.trim()) {
                    warn(ui, tr!("Une branche porte déjà ce nom."));
                }
                ok
            }
            Dialog::RenameBranch { old, new } => {
                ui.add(egui::TextEdit::singleline(new).desired_width(f32::INFINITY)).request_focus();
                let n = new.trim();
                valid_ref_name(n) && n != old && !exists(n)
            }
            Dialog::DeleteBranch { name, force, remote, delete_remote } => {
                ui.label(tr!("La branche locale « {name} » sera supprimée.", name));
                ui.checkbox(force, tr!("Forcer (même si elle n'est pas fusionnée)"));
                if let Some((r, b)) = remote {
                    ui.checkbox(delete_remote, tr!("Supprimer aussi {r}/{b} sur le remote", r, b));
                }
                true
            }
            Dialog::DeleteRemoteBranch { remote, branch } => {
                warn(
                    ui,
                    &tr!("La branche {branch} sera supprimée du serveur {remote} pour tout le monde.", branch, remote),
                );
                true
            }
            Dialog::CheckoutRemote { remote_full, local } => {
                ui.label(
                    RichText::new(tr!("Nouvelle branche locale suivant {remote_full} :", remote_full)).color(p.weak),
                );
                ui.add(egui::TextEdit::singleline(local).desired_width(f32::INFINITY)).request_focus();
                let n = local.trim();
                if exists(n) {
                    warn(ui, tr!("Une branche locale porte déjà ce nom : faites plutôt un checkout de celle-ci."));
                }
                valid_ref_name(n) && !exists(n)
            }
            Dialog::CreateTag { rev_label, name, message, annotated, push, .. } => {
                ui.label(RichText::new(tr!("Sur : {rev_label}", rev_label)).color(p.weak));
                ui.add(egui::TextEdit::singleline(name).hint_text("v1.2.3").desired_width(f32::INFINITY))
                    .request_focus();
                ui.checkbox(annotated, tr!("Tag annoté (avec message)"));
                if *annotated {
                    ui.add(
                        egui::TextEdit::multiline(message)
                            .hint_text(tr!("Message du tag"))
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                }
                if self.default_remote().is_some() {
                    ui.checkbox(push, tr!("Pousser le tag vers le remote"));
                }
                let tag_exists = self.refs.as_ref().is_some_and(|r| r.tags.iter().any(|t| t.name == name.trim()));
                if tag_exists {
                    warn(ui, tr!("Ce tag existe déjà."));
                }
                valid_ref_name(name.trim()) && !tag_exists && (!*annotated || !message.trim().is_empty())
            }
            Dialog::DeleteTag { name, remote_too } => {
                ui.label(tr!("Le tag « {name} » sera supprimé localement.", name));
                if let Some(r) = self.default_remote() {
                    ui.checkbox(remote_too, tr!("Le supprimer aussi sur {r}", r));
                }
                true
            }
            Dialog::Merge { rev, mode } => {
                let current =
                    self.refs.as_ref().and_then(|r| r.current_branch().map(str::to_string)).unwrap_or_default();
                ui.label(tr!("Fusionner {rev} dans « {current} »", rev = short_rev(rev), current));
                ui.add_space(4.0);
                ui.radio_value(mode, MergeMode::Default, tr!("Avance rapide si possible, sinon commit de fusion"));
                ui.radio_value(mode, MergeMode::NoFf, tr!("Toujours créer un commit de fusion (--no-ff)"));
                ui.radio_value(mode, MergeMode::FfOnly, tr!("Avance rapide uniquement (--ff-only)"));
                ui.radio_value(mode, MergeMode::Squash, tr!("Squash (modifications indexées, sans commit)"));
                true
            }
            Dialog::Rebase { onto } => {
                let current =
                    self.refs.as_ref().and_then(|r| r.current_branch().map(str::to_string)).unwrap_or_default();
                ui.label(tr!(
                    "Rejouer les commits de « {current} » au-dessus de {onto}.",
                    current,
                    onto = short_rev(onto)
                ));
                warn(ui, tr!("Les commits déjà poussés seront réécrits."));
                true
            }
            Dialog::Reset { label, mode, .. } => {
                let current = self
                    .refs
                    .as_ref()
                    .and_then(|r| r.current_branch().map(str::to_string))
                    .unwrap_or_else(|| "HEAD".into());
                ui.label(tr!("Déplacer « {current} » sur {label}", current, label));
                ui.add_space(4.0);
                ui.radio_value(mode, ResetMode::Soft, tr!("Soft — garder les modifications indexées"));
                ui.radio_value(mode, ResetMode::Mixed, tr!("Mixed — garder les modifications, non indexées"));
                ui.radio_value(mode, ResetMode::Hard, tr!("Hard — tout perdre (modifications locales comprises)"));
                if *mode == ResetMode::Hard {
                    warn(ui, tr!("Les modifications non commitées seront définitivement perdues."));
                }
                true
            }
            Dialog::CherryPick { label, .. } => {
                ui.label(tr!("Appliquer {label} sur la branche courante.", label));
                true
            }
            Dialog::Revert { label, .. } => {
                ui.label(tr!("Créer un commit qui annule {label}.", label));
                true
            }
            Dialog::Stash { message, untracked, keep_index } => {
                ui.add(
                    egui::TextEdit::singleline(message)
                        .hint_text(tr!("Message (facultatif)"))
                        .desired_width(f32::INFINITY),
                )
                .request_focus();
                ui.checkbox(untracked, tr!("Inclure les fichiers non suivis"));
                ui.checkbox(keep_index, tr!("Garder les modifications indexées"));
                true
            }
            Dialog::Push { remote, local, remote_branch, set_upstream, force, tags } => {
                self.remote_combo(ui, remote);
                egui::Grid::new("push-grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label(RichText::new(tr!("Branche locale")).color(p.weak));
                    ui.label(RichText::new(local.as_str()).strong());
                    ui.end_row();
                    ui.label(RichText::new(tr!("Branche distante")).color(p.weak));
                    ui.add(egui::TextEdit::singleline(remote_branch).desired_width(260.0));
                    ui.end_row();
                });
                ui.checkbox(set_upstream, tr!("Suivre cette branche distante (--set-upstream)"));
                ui.checkbox(tags, tr!("Pousser aussi tous les tags"));
                ui.checkbox(force, tr!("Forcer (--force-with-lease)"));
                if *force {
                    warn(ui, tr!("Le push forcé réécrit l'historique distant (protégé par --force-with-lease)."));
                }
                !remote.is_empty() && valid_ref_name(remote_branch.trim())
            }
            Dialog::Pull { remote, branch, rebase } => {
                self.remote_combo(ui, remote);
                egui::Grid::new("pull-grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label(RichText::new(tr!("Branche distante")).color(p.weak));
                    ui.add(egui::TextEdit::singleline(branch).desired_width(260.0));
                    ui.end_row();
                });
                ui.checkbox(rebase, tr!("Rebaser au lieu de fusionner (--rebase)"));
                !remote.is_empty() && !branch.trim().is_empty()
            }
            Dialog::Discard { tracked, untracked } => {
                let total = tracked.len() + untracked.len();
                warn(
                    ui,
                    &trn!(
                        total,
                        "Les modifications de {n} fichier seront définitivement perdues.",
                        "Les modifications de {n} fichiers seront définitivement perdues."
                    ),
                );
                egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    for f in tracked.iter().take(50) {
                        ui.label(RichText::new(f).monospace().size(12.0));
                    }
                    for f in untracked.iter().take(50) {
                        ui.label(
                            RichText::new(tr!("{f}  (non suivi : supprimé)", f))
                                .monospace()
                                .size(12.0)
                                .color(p.removed_fg),
                        );
                    }
                    if total > 100 {
                        ui.label(RichText::new(trn!(total - 100, "… et {n} autre", "… et {n} autres")).color(p.weak));
                    }
                });
                true
            }
            Dialog::DiscardLines { selection } => {
                let lines: usize = selection.iter().map(|s| s.lines.as_ref().map_or(0, Vec::len)).sum();
                let text = if lines == 0 {
                    tr!("Le hunk sera annulé dans la copie de travail.").to_string()
                } else {
                    trn!(
                        lines,
                        "{n} ligne sera annulée dans la copie de travail.",
                        "{n} lignes seront annulées dans la copie de travail."
                    )
                };
                warn(ui, &text);
                true
            }
            Dialog::AddRemote { name, url } => {
                egui::Grid::new("remote-grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label(tr!("Nom"));
                    ui.add(egui::TextEdit::singleline(name).hint_text("origin").desired_width(300.0));
                    ui.end_row();
                    ui.label("URL");
                    ui.add(egui::TextEdit::singleline(url).hint_text(tr!("https://… ou git@…")).desired_width(300.0));
                    ui.end_row();
                });
                let taken = self.remotes.iter().any(|r| r.name == name.trim());
                if taken {
                    warn(ui, tr!("Un remote porte déjà ce nom."));
                }
                valid_ref_name(name.trim()) && !url.trim().is_empty() && !taken
            }
            Dialog::RemoveRemote { name } => {
                warn(
                    ui,
                    &tr!("Le remote « {name} » et ses branches distantes locales seront retirés de ce dépôt.", name),
                );
                true
            }
            Dialog::DropStash { label, .. } => {
                warn(ui, &tr!("{label} sera définitivement supprimé.", label));
                true
            }
            Dialog::CreateWorktree { start, path, pick } => self.create_worktree_body(ui, start, path, *pick, p),
            Dialog::RemoveWorktree { path, force } => Self::remove_worktree_body(ui, path, force, p),
            Dialog::Confirm { text, .. } => {
                ui.label(text.as_str());
                true
            }
        }
    }

    fn remote_combo(&self, ui: &mut Ui, remote: &mut String) {
        ui.horizontal(|ui| {
            ui.label("Remote");
            egui::ComboBox::from_id_salt(("remote-combo", self.id)).selected_text(remote.as_str()).show_ui(ui, |ui| {
                for r in self.remotes.iter() {
                    ui.selectable_value(remote, r.name.clone(), &r.name).on_hover_text(&r.fetch_url);
                }
            });
        });
    }

    fn execute_dialog(&mut self, dialog: Dialog, shared: &mut Shared) {
        match dialog {
            Dialog::CreateBranch { start, name, checkout, .. } => {
                self.op_create_branch(shared, name.trim().to_string(), start, checkout)
            }
            Dialog::RenameBranch { old, new } => {
                let new = new.trim().to_string();
                self.run_simple(shared, tr!("Renommage"), Refresh::REFS, move |r| {
                    r.rename_branch(&old, &new)?;
                    Ok(Some(tr!("Branche renommée en {new}", new)))
                });
            }
            Dialog::DeleteBranch { name, force, remote, delete_remote } => {
                let remote = if delete_remote { remote } else { None };
                self.run_net(shared, tr!("Suppression de branche"), Refresh::REFS, false, move |r, n, on| {
                    r.delete_branch(&name, force)?;
                    if let Some((rem, br)) = &remote {
                        r.delete_remote_branch(rem, br, n, on)?;
                    }
                    Ok(Some(tr!("Branche {name} supprimée", name)))
                });
            }
            Dialog::DeleteRemoteBranch { remote, branch } => {
                self.run_net(shared, tr!("Suppression distante"), Refresh::REFS, false, move |r, n, on| {
                    r.delete_remote_branch(&remote, &branch, n, on)?;
                    Ok(Some(tr!("{remote}/{branch} supprimée", remote, branch)))
                });
            }
            Dialog::CheckoutRemote { remote_full, local } => {
                self.op_checkout_remote(shared, remote_full, local.trim().to_string())
            }
            Dialog::CreateTag { rev, name, message, annotated, push, .. } => {
                let remote = self.default_remote();
                let name = name.trim().to_string();
                self.run_net(shared, tr!("Création du tag"), Refresh::REFS, false, move |r, n, on| {
                    r.create_tag(&name, &rev, annotated.then_some(message.as_str()))?;
                    if push && let Some(remote) = &remote {
                        r.push_tag(remote, &name, n, on)?;
                    }
                    Ok(Some(tr!("Tag {name} créé", name)))
                });
            }
            Dialog::DeleteTag { name, remote_too } => {
                let remote = if remote_too { self.default_remote() } else { None };
                self.run_net(shared, tr!("Suppression du tag"), Refresh::REFS, false, move |r, n, on| {
                    r.delete_tag(&name)?;
                    if let Some(remote) = &remote {
                        r.delete_remote_tag(remote, &name, n, on)?;
                    }
                    Ok(Some(tr!("Tag {name} supprimé", name)))
                });
            }
            Dialog::Merge { rev, mode } => self.op_merge(shared, rev, mode),
            Dialog::Rebase { onto } => self.op_rebase(shared, onto),
            Dialog::Reset { rev, mode, .. } => self.op_reset(shared, rev, mode),
            Dialog::CherryPick { ids, .. } => self.op_cherry_pick(shared, ids),
            Dialog::Revert { id, .. } => self.op_revert(shared, id),
            Dialog::Stash { message, untracked, keep_index } => {
                self.op_stash_save(shared, message, untracked, keep_index)
            }
            Dialog::Push { remote, local, remote_branch, set_upstream, force, tags } => {
                self.op_push(shared, remote, local, remote_branch.trim().to_string(), set_upstream, force, tags)
            }
            Dialog::Pull { remote, branch, rebase } => {
                // Mode retenu pour la session : repris par le prochain dialogue et par le pull rapide.
                shared.pull_rebase = rebase;
                self.op_pull(shared, remote, branch.trim().to_string(), rebase)
            }
            Dialog::Discard { tracked, untracked } => self.op_discard(shared, tracked, untracked),
            Dialog::DiscardLines { selection } => {
                self.op_patch(shared, PatchMode::Discard, selection);
                self.changes.diff_state.selected.clear();
            }
            Dialog::AddRemote { name, url } => {
                let (name, url) = (name.trim().to_string(), url.trim().to_string());
                self.run_simple(shared, tr!("Ajout du remote"), Refresh::REFS.with_extras(), move |r| {
                    r.add_remote(&name, &url)?;
                    Ok(Some(tr!("Remote {name} ajouté", name)))
                });
            }
            Dialog::RemoveRemote { name } => {
                self.run_simple(shared, tr!("Suppression du remote"), Refresh::REFS.with_extras(), move |r| {
                    r.remove_remote(&name)?;
                    Ok(Some(tr!("Remote {name} supprimé", name)))
                });
            }
            Dialog::DropStash { oid, label } => self.op_stash(shared, oid, label, StashOp::Drop),
            Dialog::CreateWorktree { start, path, .. } => {
                self.op_add_worktree(shared, PathBuf::from(path.trim()), start)
            }
            // L'application ferme d'abord les onglets ouverts sur ce worktree, puis rend la main à cet onglet.
            Dialog::RemoveWorktree { path, force } => {
                shared.actions.push(AppAction::RemoveWorktree { tab: self.id, path, force })
            }
            Dialog::Confirm { action, .. } => match action {
                ConfirmAction::CheckoutDetached(oid) => self.op_checkout(shared, oid.to_hex()),
                ConfirmAction::AbortOp(state) => {
                    self.run_simple(shared, abort_label(state), Refresh::ALL, move |r| {
                        r.abort_op(state)?;
                        Ok(Some(tr!("Opération abandonnée").into()))
                    });
                }
                ConfirmAction::OpenWorktree(path) => shared.actions.push(AppAction::OpenRepo(path)),
            },
        }
    }
}

fn short_rev(rev: &str) -> String {
    if rev.len() == 40 && rev.chars().all(|c| c.is_ascii_hexdigit()) {
        rev[..7].to_string()
    } else {
        tr!("« {rev} »", rev)
    }
}

#[cfg(test)]
mod tests {
    use super::valid_ref_name;

    #[test]
    fn ref_names() {
        for ok in ["main", "feature/x-1", "v1.2.3", "fix_é", "a/b/c"] {
            assert!(valid_ref_name(ok), "{ok}");
        }
        for bad in [
            "", "a b", "-x", "/x", "x/", "x.", "a..b", "a//b", "x.lock", "a@{b", "a~1", "a^", "a:b", "a?", "a*", "a[b",
            "a\\b", "@", ".x", "a/.b",
        ] {
            assert!(!valid_ref_name(bad), "{bad}");
        }
    }
}
