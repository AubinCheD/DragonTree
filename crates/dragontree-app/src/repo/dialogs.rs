//! Dialogues des opérations (création de branche, merge, reset, push…).

use std::path::PathBuf;

use dragontree_git::{
    FlowKind, GitFlowConfig, HunkSelection, MergeMode, Oid, OpState, PatchMode, ResetMode, WorktreeStart,
};
use dragontree_i18n::{tr, trn};
use egui::{RichText, Ui};

use super::actions::{OpOptions, StashOp};
use super::patches::{PatchPreview, PatchTarget};
use super::{Refresh, RepoTab};
use crate::app::{AppAction, Shared};
use crate::custom_commands::{CommandKind, Prepared};
use crate::icons;
use crate::theme::Palette;

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    CheckoutDetached(Oid),
    /// Abandon de la fusion, du rebase, du cherry-pick ou du revert en cours.
    AbortOp(OpState),
    /// Ouvrir le worktree où la branche demandée est déjà sortie.
    OpenWorktree(PathBuf),
    /// Lever le verrou LFS d'un fichier même s'il appartient à quelqu'un d'autre (`git lfs unlock --force`).
    LfsForceUnlock(String),
    /// Pousser en forçant `amended`, le commit modifié par un amend alors qu'il était déjà poussé (L5), avec un bail
    /// fixé à `replaced`, le commit remplacé. Annuler (ou fermer) garde le commit en local et le notifie.
    ForcePushAmended {
        remote: String,
        remote_branch: String,
        replaced: Oid,
        amended: Oid,
    },
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
/// `Stash::snapshot` : enregistre un stash sans toucher aux modifications (`untracked` et `keep_index` sans objet) ;
/// `StashFiles` : seulement les fichiers choisis dans la vue Changements ; `ApplyPatch` : aperçu d'un patch lu (fichier
/// déposé, texte collé) et cible choisie ; `PastePatch` : zone où coller le texte d'un patch ; `EditRemoteUrl` :
/// `push_url` est un champ séparé, seulement si le remote a une URL de push distincte ; `RepoSettings` : identité
/// locale, compte, modèle de message, liens des tickets ; `AddSubmodule` : `path` vide reprend le nom du dépôt cloné,
/// `branch` est facultative ; `EditSubmoduleUrl`, `MoveSubmodule` et `RemoveSubmodule` portent sur le sous-module installé
/// dans `path` (`old` pour le déplacement) ; `Ignore` : motif, fichier cible et aperçu des fichiers concernés ;
/// `GitFlowInit` : réglages de Git Flow à écrire ; `GitFlowStart` : nature et nom court de la branche à créer ;
/// `GitFlowFinish` : branche de travail à terminer (nature, nom court), `delete_branch` demande de la supprimer ensuite
/// (jamais forcé) ; `LfsTrack` : motif à suivre avec Git LFS ; `LfsUntrack` : motif suivi à retirer (choisi dans la
/// liste des motifs du `.gitattributes`) ; `CustomCommand` : commande personnalisée qui demande confirmation, sa ligne
/// (ou son URL) complète affichée ; `UpdateBranches`, `PushBranches` et `DeleteStaleBranches` : branches à cocher (voir
/// `branch_batch`), `DeleteStaleBranches::items` vaut `None` tant que la liste est lue et `force` demande `-D` ;
/// `RestoreFile` : fichier à remettre dans son état à `rev` (`label` décrit la version), demandé seulement quand il a
/// des modifications locales ; `PullRequest` : formulaire de création d'une pull request, qui reste ouvert pendant le push
/// et l'appel à la forge (voir `pull_request`) ; `OpenSolution` : solution à ouvrir dans `editor` (Visual Studio) quand le
/// dépôt en contient plusieurs, `chosen` est sa position dans `solutions` ; `UseAccount` : comptes proposés pour le
/// dépôt (voir `repo_account`), `chosen` est l'identifiant du compte coché.
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
    Stash { message: String, untracked: bool, keep_index: bool, snapshot: bool },
    StashFiles { message: String, paths: Vec<String> },
    RenameStash { oid: Oid, label: String, message: String },
    ApplyPatch { preview: Box<PatchPreview>, target: PatchTarget },
    PastePatch { text: String },
    Push { remote: String, local: String, remote_branch: String, set_upstream: bool, force: bool, tags: bool },
    Pull { remote: String, branch: String, rebase: bool },
    Discard { tracked: Vec<String>, untracked: Vec<String> },
    DiscardLines { selection: Vec<HunkSelection> },
    AddRemote { name: String, url: String },
    EditRemoteUrl { name: String, url: String, push_url: Option<String> },
    RemoveRemote { name: String },
    DropStash { oid: Oid, label: String },
    CreateWorktree { start: WorktreeStart, path: String, pick: bool },
    RemoveWorktree { path: PathBuf, force: bool },
    AddSubmodule { url: String, path: String, branch: String },
    EditSubmoduleUrl { path: String, url: String },
    MoveSubmodule { old: String, new: String },
    RemoveSubmodule { path: String },
    Ignore(Box<super::ignore::IgnoreForm>),
    GitFlowInit(Box<GitFlowConfig>),
    GitFlowStart { kind: FlowKind, name: String },
    GitFlowFinish { kind: FlowKind, name: String, delete_branch: bool },
    LfsTrack { pattern: String },
    LfsUntrack { pattern: String },
    RestoreFile { path: String, rev: String, label: String },
    PullRequest(Box<super::pull_request::PullRequestForm>),
    UpdateBranches { items: Vec<super::branch_batch::BranchChoice> },
    PushBranches { items: Vec<super::branch_batch::BranchChoice> },
    DeleteStaleBranches { items: Option<Vec<super::branch_batch::BranchChoice>>, force: bool },
    RepoSettings(Box<super::repo_settings::RepoSettingsForm>),
    UseAccount { choices: Vec<super::repo_account::AccountChoice>, chosen: Option<String> },
    CustomCommand(Box<Prepared>),
    OpenSolution { editor: crate::editors::Editor, solutions: Vec<PathBuf>, chosen: usize },
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
            Dialog::Stash { snapshot: true, .. } => {
                (tr!("Enregistrer un snapshot").into(), tr!("Enregistrer").into(), false)
            }
            Dialog::Stash { .. } => (tr!("Mettre de côté (stash)").into(), "Stash".into(), false),
            Dialog::StashFiles { .. } => {
                (tr!("Mettre de côté les fichiers").into(), tr!("Mettre de côté").into(), false)
            }
            Dialog::RenameStash { .. } => (tr!("Renommer le stash").into(), tr!("Renommer").into(), false),
            Dialog::ApplyPatch { target: PatchTarget::Am, .. } => {
                (tr!("Appliquer un patch").into(), tr!("Créer les commits").into(), false)
            }
            Dialog::ApplyPatch { .. } => (tr!("Appliquer un patch").into(), tr!("Appliquer").into(), false),
            Dialog::PastePatch { .. } => (tr!("Coller un patch").into(), tr!("Aperçu").into(), false),
            Dialog::Push { .. } => ("Push".into(), "Push".into(), false),
            Dialog::Pull { .. } => ("Pull".into(), "Pull".into(), false),
            Dialog::Discard { .. } => {
                (tr!("Annuler des modifications").into(), tr!("Annuler les modifications").into(), true)
            }
            Dialog::DiscardLines { .. } => (tr!("Annuler des lignes").into(), tr!("Annuler les lignes").into(), true),
            Dialog::AddRemote { .. } => (tr!("Ajouter un remote").into(), tr!("Ajouter").into(), false),
            Dialog::EditRemoteUrl { name, .. } => {
                (tr!("Modifier l'URL de « {name} »", name), tr!("Enregistrer").into(), false)
            }
            Dialog::RemoveRemote { name } => {
                (tr!("Supprimer le remote « {name} »", name), tr!("Supprimer").into(), true)
            }
            Dialog::DropStash { .. } => (tr!("Supprimer le stash").into(), tr!("Supprimer").into(), true),
            Dialog::CreateWorktree { .. } => (tr!("Créer un worktree").into(), tr!("Créer").into(), false),
            Dialog::RemoveWorktree { .. } => (tr!("Supprimer le worktree").into(), tr!("Supprimer").into(), true),
            Dialog::AddSubmodule { .. } => (tr!("Ajouter un sous-module").into(), tr!("Ajouter").into(), false),
            Dialog::EditSubmoduleUrl { path, .. } => {
                (tr!("Modifier l'URL de « {path} »", path), tr!("Enregistrer").into(), false)
            }
            Dialog::MoveSubmodule { old, .. } => {
                (tr!("Déplacer le sous-module « {old} »", old), tr!("Déplacer").into(), false)
            }
            Dialog::RemoveSubmodule { path } => {
                (tr!("Supprimer le sous-module « {path} »", path), tr!("Supprimer").into(), true)
            }
            Dialog::Ignore(_) => (tr!("Ignorer des fichiers").into(), tr!("Ignorer").into(), false),
            Dialog::GitFlowInit(_) => (tr!("Initialiser Git Flow").into(), tr!("Initialiser").into(), false),
            Dialog::GitFlowStart { kind, .. } => {
                let (title, ok, _) = super::gitflow::start_texts(*kind);
                (title.into(), ok.into(), false)
            }
            Dialog::GitFlowFinish { .. } => {
                (tr!("Terminer la branche Git Flow").into(), tr!("Terminer la branche").into(), false)
            }
            Dialog::LfsTrack { .. } => (tr!("Suivre un motif avec Git LFS").into(), tr!("Suivre").into(), false),
            Dialog::LfsUntrack { .. } => {
                (tr!("Ne plus suivre avec Git LFS").into(), tr!("Ne plus suivre").into(), false)
            }
            Dialog::RestoreFile { path, .. } => {
                (tr!("Remettre « {path} »", path), tr!("Remettre le fichier").into(), true)
            }
            Dialog::PullRequest(form) => {
                let (title, ok) = form.title_and_ok();
                (title, ok, false)
            }
            Dialog::UpdateBranches { .. } => {
                (tr!("Mettre à jour les branches locales").into(), tr!("Mettre à jour").into(), false)
            }
            Dialog::PushBranches { .. } => (tr!("Pousser plusieurs branches").into(), "Push".into(), false),
            Dialog::DeleteStaleBranches { .. } => {
                (tr!("Supprimer les branches périmées").into(), tr!("Supprimer").into(), true)
            }
            Dialog::RepoSettings(_) => (tr!("Réglages du dépôt").into(), tr!("Enregistrer").into(), false),
            Dialog::UseAccount { .. } => {
                (tr!("Utiliser un compte pour ce dépôt").into(), tr!("Utiliser").into(), false)
            }
            Dialog::OpenSolution { editor, .. } => {
                (tr!("Ouvrir dans {editor}", editor = editor.kind.label()), tr!("Ouvrir").into(), false)
            }
            // Un script peut tout faire : sa confirmation exige un clic.
            Dialog::CustomCommand(c) if c.kind == CommandKind::Script => {
                (tr!("Exécuter « {name} » ?", name = c.name), tr!("Exécuter").into(), true)
            }
            Dialog::CustomCommand(c) => (tr!("Ouvrir « {name} » ?", name = c.name), tr!("Ouvrir").into(), false),
            Dialog::Confirm { title, action: ConfirmAction::CheckoutDetached(_), .. } => {
                (title.clone(), tr!("Continuer").into(), false)
            }
            Dialog::Confirm { title, action: ConfirmAction::AbortOp(state), .. } => {
                (title.clone(), abort_label(*state).into(), true)
            }
            Dialog::Confirm { title, action: ConfirmAction::LfsForceUnlock(_), .. } => {
                (title.clone(), tr!("Forcer le déverrouillage").into(), true)
            }
            Dialog::Confirm { title, action: ConfirmAction::OpenWorktree(_), .. } => {
                (title.clone(), tr!("Ouvrir le worktree").into(), false)
            }
            Dialog::Confirm { title, action: ConfirmAction::ForcePushAmended { .. }, .. } => {
                (title.clone(), tr!("Pousser en forçant").into(), true)
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
            ui.set_width(if matches!(dialog, Dialog::RepoSettings(_)) { 640.0 } else { 470.0 });
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
            if enter
                && valid
                && !danger
                && !matches!(dialog, Dialog::CreateTag { annotated: true, .. } | Dialog::PullRequest(_))
            {
                confirmed = true;
            }
        });
        if let Dialog::Ignore(form) = &mut dialog
            && !confirmed
            && !cancelled
        {
            self.request_ignore_preview(shared, form);
        }
        if confirmed {
            self.execute_dialog(dialog, shared);
        } else if cancelled || response.should_close() {
            Self::dialog_dismissed(dialog, shared);
        } else {
            self.dialog = Some(dialog);
        }
    }

    /// Dialogue fermé sans être validé (Annuler, Échap, clic hors du dialogue).
    fn dialog_dismissed(dialog: Dialog, shared: &mut Shared) {
        // Push forcé du commit modifié écarté (L5) : le commit reste en local, comme quand le push n'est pas proposé.
        if let Dialog::Confirm { action: ConfirmAction::ForcePushAmended { remote, remote_branch, .. }, .. } = dialog {
            super::actions::commit_not_pushed(shared, super::actions::amended_not_pushed(&remote, &remote_branch));
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
            Dialog::Stash { message, untracked, keep_index, snapshot } => {
                ui.add(
                    egui::TextEdit::singleline(message)
                        .hint_text(tr!("Message (facultatif)"))
                        .desired_width(f32::INFINITY),
                )
                .request_focus();
                ui.add_enabled_ui(!*snapshot, |ui| {
                    ui.checkbox(untracked, tr!("Inclure les fichiers non suivis"));
                    ui.checkbox(keep_index, tr!("Garder les modifications indexées"));
                });
                ui.checkbox(snapshot, tr!("Snapshot : garder les modifications en place")).on_hover_text(tr!(
                    "Enregistre un stash sans toucher à la copie de travail ni à l'index (fichiers suivis seulement)."
                ));
                true
            }
            Dialog::StashFiles { message, paths } => {
                ui.label(
                    RichText::new(trn!(
                        paths.len(),
                        "{n} fichier sera mis de côté, les autres modifications restent en place.",
                        "{n} fichiers seront mis de côté, les autres modifications restent en place."
                    ))
                    .color(p.weak),
                );
                ui.add(
                    egui::TextEdit::singleline(message)
                        .hint_text(tr!("Message (facultatif)"))
                        .desired_width(f32::INFINITY),
                )
                .request_focus();
                !paths.is_empty()
            }
            Dialog::RenameStash { label, message, .. } => {
                ui.label(RichText::new(tr!("Nouveau message pour {label}", label)).color(p.weak));
                ui.add(egui::TextEdit::singleline(message).desired_width(f32::INFINITY)).request_focus();
                ui.label(
                    RichText::new(tr!(
                        "git n'a pas de renommage : le stash est supprimé puis recréé avec le même contenu, à la même place."
                    ))
                    .color(p.weak)
                    .size(12.0),
                );
                !message.trim().is_empty()
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
                // Diff sans les espaces (L6) : un hunk entier emporte aussi les changements d'espaces de ses lignes.
                if self.changes.diff_full.is_some() && selection.iter().any(|s| s.lines.is_none()) {
                    ui.label(
                        RichText::new(tr!("Ses changements d'espaces, masqués par « Espaces », seront aussi annulés."))
                            .color(p.weak),
                    );
                }
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
            Dialog::EditRemoteUrl { url, push_url, .. } => {
                egui::Grid::new("remote-url-grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("URL");
                    let field = ui
                        .add(egui::TextEdit::singleline(url).hint_text(tr!("https://… ou git@…")).desired_width(340.0));
                    // Le focus n'est pris qu'à l'ouverture : il ne doit pas revenir ici quand on passe au champ de push.
                    if ui.memory(|m| m.focused().is_none()) {
                        field.request_focus();
                    }
                    ui.end_row();
                    if let Some(push) = push_url {
                        ui.label(tr!("URL de push"));
                        ui.add(egui::TextEdit::singleline(push).desired_width(340.0))
                            .on_hover_text(tr!("Vide : le push utilise l'URL ci-dessus."));
                        ui.end_row();
                    }
                });
                !url.trim().is_empty()
            }
            Dialog::RemoveRemote { name } => {
                warn(
                    ui,
                    &tr!("Le remote « {name} » et ses branches distantes locales seront retirés de ce dépôt.", name),
                );
                true
            }
            Dialog::ApplyPatch { preview, target } => Self::apply_patch_body(ui, preview, target, p),
            Dialog::PastePatch { text } => Self::paste_patch_body(ui, text, p),
            Dialog::DropStash { label, .. } => {
                warn(ui, &tr!("{label} sera définitivement supprimé.", label));
                true
            }
            Dialog::CreateWorktree { start, path, pick } => self.create_worktree_body(ui, start, path, *pick, p),
            Dialog::RemoveWorktree { path, force } => Self::remove_worktree_body(ui, path, force, p),
            Dialog::AddSubmodule { url, path, branch } => self.add_submodule_body(ui, url, path, branch, p),
            Dialog::EditSubmoduleUrl { url, .. } => Self::edit_submodule_url_body(ui, url),
            Dialog::MoveSubmodule { old, new } => self.move_submodule_body(ui, old, new, p),
            Dialog::RemoveSubmodule { path } => Self::remove_submodule_body(ui, path, p),
            Dialog::Ignore(form) => Self::ignore_body(ui, form, p),
            Dialog::GitFlowInit(cfg) => self.gitflow_init_body(ui, cfg, p),
            Dialog::GitFlowStart { kind, name } => self.gitflow_start_body(ui, *kind, name, p),
            Dialog::GitFlowFinish { kind, name, delete_branch } => {
                self.gitflow_finish_body(ui, *kind, name, delete_branch, p)
            }
            Dialog::LfsTrack { pattern } => self.lfs_track_body(ui, pattern, p),
            Dialog::LfsUntrack { pattern } => self.lfs_untrack_body(ui, pattern, p),
            Dialog::RestoreFile { path, label, .. } => Self::restore_file_body(ui, path, label, p),
            Dialog::PullRequest(form) => Self::pull_request_body(ui, form, p),
            Dialog::UpdateBranches { items } => Self::update_branches_body(ui, items, p),
            Dialog::PushBranches { items } => Self::push_branches_body(ui, items, p),
            Dialog::DeleteStaleBranches { items, force } => Self::delete_stale_body(ui, items, force, p),
            Dialog::RepoSettings(form) => Self::repo_settings_body(ui, form, p),
            Dialog::UseAccount { choices, chosen } => Self::use_account_body(ui, choices, chosen, p),
            Dialog::CustomCommand(prepared) => self.custom_command_body(ui, prepared, p),
            Dialog::OpenSolution { solutions, chosen, .. } => self.open_solution_body(ui, solutions, chosen, p),
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

    pub(super) fn execute_dialog(&mut self, dialog: Dialog, shared: &mut Shared) {
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
            Dialog::Stash { message, snapshot: true, .. } => self.op_stash_snapshot(shared, message),
            Dialog::Stash { message, untracked, keep_index, .. } => {
                self.op_stash_save(shared, message, untracked, keep_index)
            }
            Dialog::StashFiles { message, paths } => self.op_stash_files(shared, message, paths),
            Dialog::RenameStash { oid, label, message } => self.op_stash_rename(shared, oid, label, message),
            Dialog::ApplyPatch { preview, target } => self.op_apply_patch(shared, preview, target),
            Dialog::PastePatch { text } => self.open_patch_text(shared, tr!("Presse-papiers").into(), text),
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
            Dialog::EditRemoteUrl { name, url, push_url } => {
                let (url, push_url) = (url.trim().to_string(), push_url.map(|p| p.trim().to_string()));
                self.run_simple(shared, tr!("Modification de l'URL"), Refresh::REFS.with_extras(), move |r| {
                    r.set_remote_urls(&name, &url, push_url.as_deref())?;
                    Ok(Some(tr!("URL du remote {name} modifiée", name)))
                });
            }
            Dialog::RemoveRemote { name } => {
                // Supprime ses refs distantes : pas en même temps qu'un fetch automatique (voir `op_queue`).
                let opts = OpOptions { remote_refs: true, ..OpOptions::default() };
                let title = tr!("Suppression du remote");
                self.run_op(
                    shared,
                    title,
                    Refresh::REFS.with_extras(),
                    opts,
                    Box::new(move |r, _, _| {
                        r.remove_remote(&name)?;
                        Ok(Some(tr!("Remote {name} supprimé", name)))
                    }),
                );
            }
            Dialog::DropStash { oid, label } => self.op_stash(shared, oid, label, StashOp::Drop),
            Dialog::CreateWorktree { start, path, .. } => {
                self.op_add_worktree(shared, PathBuf::from(path.trim()), start)
            }
            // L'application ferme d'abord les onglets ouverts sur ce worktree, puis rend la main à cet onglet.
            Dialog::RemoveWorktree { path, force } => {
                shared.actions.push(AppAction::RemoveWorktree { tab: self.id, path, force })
            }
            Dialog::AddSubmodule { url, path, branch } => self.op_add_submodule(shared, url, path, branch),
            Dialog::EditSubmoduleUrl { path, url } => self.op_set_submodule_url(shared, path, url),
            Dialog::MoveSubmodule { old, new } => self.op_move_submodule(shared, old, new),
            Dialog::RemoveSubmodule { path } => self.op_remove_submodule(shared, path),
            Dialog::Ignore(form) => {
                let target = form.target();
                self.op_add_ignore(shared, form.pattern, target);
            }
            Dialog::GitFlowInit(cfg) => self.op_gitflow_init(shared, *cfg),
            Dialog::GitFlowStart { kind, name } => self.op_gitflow_start(shared, kind, name.trim().to_string()),
            Dialog::GitFlowFinish { kind, name, delete_branch } => {
                self.op_gitflow_finish(shared, kind, name, delete_branch)
            }
            Dialog::LfsTrack { pattern } => self.op_lfs_track(shared, pattern),
            Dialog::LfsUntrack { pattern } => self.op_lfs_untrack(shared, pattern),
            Dialog::RestoreFile { path, rev, label } => self.op_restore_file(shared, path, rev, label, true),
            Dialog::PullRequest(form) => self.submit_pull_request(shared, *form),
            Dialog::UpdateBranches { items } => self.op_update_branches(shared, items),
            Dialog::PushBranches { items } => self.op_push_branches(shared, items),
            Dialog::DeleteStaleBranches { items, force } => {
                self.op_delete_stale(shared, items.unwrap_or_default(), force)
            }
            Dialog::RepoSettings(form) => self.save_repo_settings(shared, form),
            Dialog::UseAccount { chosen, .. } => self.set_account(shared, chosen),
            Dialog::CustomCommand(prepared) => self.launch_custom(shared, *prepared),
            Dialog::OpenSolution { editor, solutions, chosen } => {
                self.open_solution(shared, &editor, &solutions, chosen)
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
                ConfirmAction::LfsForceUnlock(path) => self.op_lfs_force_unlock(shared, path),
                ConfirmAction::ForcePushAmended { remote, remote_branch, replaced, amended } => {
                    self.op_force_push_amended(shared, remote, remote_branch, replaced, amended)
                }
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
