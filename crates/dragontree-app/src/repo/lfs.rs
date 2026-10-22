//! Git LFS de l'onglet : dialogues « Suivre un motif » et « Ne plus suivre », fetch, pull et prune (file d'écriture
//! de l'onglet, avec progression et annulation), et fenêtre des verrous. La logique est dans le moteur
//! (`dragontree_git::lfs`) ; les entrées de menu sont déclarées dans le registre de commandes.

use dragontree_git::{CancelToken, GitError, LfsLock, NetOptions};
use dragontree_i18n::{tr, trn};
use egui::{RichText, Ui};

use super::dialogs::{ConfirmAction, warning};
use super::{Dialog, Refresh, RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::icons;
use crate::theme::Palette;

/// État de la fenêtre des verrous : la liste lue sur le serveur (`git lfs locks --json`) et le chemin à verrouiller.
/// `generation` écarte les réponses périmées ; `after_op` : numéro de l'opération (verrouiller, déverrouiller) dont la
/// fin relance la lecture.
pub struct LfsLocksWindow {
    locks: Vec<LfsLock>,
    loading: bool,
    error: Option<String>,
    path: String,
    generation: u64,
    after_op: Option<u64>,
}

impl RepoTab {
    pub(super) fn open_lfs_track(&mut self) {
        self.dialog = Some(Dialog::LfsTrack { pattern: String::new() });
    }

    pub(super) fn open_lfs_untrack(&mut self) {
        let pattern = self.lfs_patterns.first().cloned().unwrap_or_default();
        self.dialog = Some(Dialog::LfsUntrack { pattern });
    }

    /// Champs du dialogue « Suivre un motif » : les motifs déjà suivis, puis la saisie ; renvoie vrai si elle est
    /// valide.
    pub(super) fn lfs_track_body(&self, ui: &mut Ui, pattern: &mut String, p: &Palette) -> bool {
        ui.label(
            RichText::new(tr!("Les fichiers qui correspondent seront stockés par Git LFS (pointeurs dans le dépôt)."))
                .color(p.weak),
        );
        ui.add(
            egui::TextEdit::singleline(pattern)
                .hint_text(tr!("Motif (ex. *.psd, assets/**/*.bin)"))
                .desired_width(f32::INFINITY),
        )
        .request_focus();
        let wanted = pattern.trim();
        if !self.lfs_patterns.is_empty() {
            ui.add_space(4.0);
            ui.label(tr!("Motifs déjà suivis :"));
            for tracked in &self.lfs_patterns {
                ui.label(RichText::new(format!("{} {tracked}", icons::HARD_DRIVES)).color(p.weak));
            }
        }
        if wanted.starts_with('-') {
            warning(ui, tr!("Un motif ne peut pas commencer par « - »."), p);
            return false;
        }
        if self.lfs_patterns.iter().any(|t| t == wanted) {
            warning(ui, tr!("Ce motif est déjà suivi."), p);
            return false;
        }
        !wanted.is_empty()
    }

    /// Champs du dialogue « Ne plus suivre » : le motif se choisit dans la liste de ceux qui sont suivis.
    pub(super) fn lfs_untrack_body(&self, ui: &mut Ui, pattern: &mut String, p: &Palette) -> bool {
        ui.label(tr!("Motif à ne plus suivre :"));
        for tracked in &self.lfs_patterns {
            ui.radio_value(pattern, tracked.clone(), tracked);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(tr!(
                "Les fichiers déjà commités restent des pointeurs LFS dans l'historique ; seul le suivi futur change."
            ))
            .color(p.weak)
            .size(12.0),
        );
        self.lfs_patterns.contains(pattern)
    }

    pub(super) fn op_lfs_install(&mut self, shared: &Shared) {
        self.run_simple(shared, "Git LFS", Refresh::ALL, |r| {
            r.lfs_install_local()?;
            Ok(Some(tr!("Git LFS est activé dans ce dépôt").into()))
        });
    }

    pub(super) fn op_lfs_track(&mut self, shared: &Shared, pattern: String) {
        self.run_simple(shared, "Git LFS", Refresh::STATUS.with_extras(), move |r| {
            r.lfs_track(&pattern)?;
            Ok(Some(tr!("« {pattern} » est maintenant suivi par Git LFS", pattern = pattern.trim())))
        });
    }

    pub(super) fn op_lfs_untrack(&mut self, shared: &Shared, pattern: String) {
        self.run_simple(shared, "Git LFS", Refresh::STATUS.with_extras(), move |r| {
            r.lfs_untrack(&pattern)?;
            Ok(Some(tr!("« {pattern} » n'est plus suivi par Git LFS", pattern = pattern.trim())))
        });
    }

    /// Fetch, pull et prune : sur la file de l'onglet, avec progression et bouton d'annulation (opération réseau
    /// authentifiée par le compte du dépôt).
    pub(super) fn op_lfs_fetch(&mut self, shared: &Shared) {
        self.run_net(shared, tr!("Fetch LFS"), Refresh::NONE, false, |r, n, on| {
            r.lfs_fetch(n, on)?;
            Ok(Some(tr!("Fetch LFS terminé").into()))
        });
    }

    pub(super) fn op_lfs_pull(&mut self, shared: &Shared) {
        self.run_net(shared, tr!("Pull LFS"), Refresh::STATUS, false, |r, n, on| {
            r.lfs_pull(n, on)?;
            Ok(Some(tr!("Pull LFS terminé").into()))
        });
    }

    pub(super) fn op_lfs_prune(&mut self, shared: &Shared) {
        self.run_net(shared, tr!("Prune LFS"), Refresh::NONE, false, |r, n, on| {
            r.lfs_prune(n, on)?;
            Ok(Some(tr!("Prune LFS terminé").into()))
        });
    }

    // --- Verrous ----------------------------------------------------------------------------

    pub(super) fn open_lfs_locks(&mut self, shared: &Shared) {
        self.lfs_locks = Some(LfsLocksWindow {
            locks: Vec::new(),
            loading: true,
            error: None,
            path: String::new(),
            generation: 0,
            after_op: None,
        });
        self.load_lfs_locks(shared);
    }

    /// Relit les verrous du serveur en tâche de fond (le serveur peut être lent ou refuser).
    fn load_lfs_locks(&mut self, shared: &Shared) {
        let (Some(repo), Some(window)) = (self.repo.clone(), self.lfs_locks.as_mut()) else { return };
        self.gens.lfs_locks += 1;
        window.generation = self.gens.lfs_locks;
        window.loading = true;
        window.error = None;
        let (g, id) = (window.generation, self.id);
        let (account, urls) = (self.net_account(shared), self.remote_urls());
        shared.jobs.spawn("lfs-locks", move || {
            let env = super::actions::auth_env(account.as_ref(), &urls);
            let net = NetOptions { env, cancel: CancelToken::new(), non_interactive: false };
            Some(AppEvent::Repo(id, RepoEvent::LfsLocks(g, repo.lfs_locks(&net))))
        });
    }

    pub(super) fn on_lfs_locks(&mut self, g: u64, result: Result<Vec<LfsLock>, GitError>) {
        let Some(window) = self.lfs_locks.as_mut().filter(|w| w.generation == g) else { return };
        window.loading = false;
        match result {
            Ok(locks) => window.locks = locks,
            Err(e) => window.error = Some(e.stderr_summary()),
        }
    }

    /// Verrouille ou déverrouille sur la file de l'onglet ; la fin de l'opération relance la lecture des verrous.
    fn op_lfs_lock(&mut self, shared: &Shared, path: String, unlock: Option<bool>) {
        let title = if unlock.is_some() { tr!("Déverrouillage LFS") } else { tr!("Verrouillage LFS") };
        self.run_net(shared, title, Refresh::STATUS, false, move |r, n, _| match unlock {
            Some(force) => {
                r.lfs_unlock(&path, force, n)?;
                Ok(Some(tr!("{path} déverrouillé", path)))
            }
            None => {
                r.lfs_lock(&path, n)?;
                Ok(Some(tr!("{path} verrouillé", path)))
            }
        });
        let op = self.next_op_id;
        if let Some(window) = &mut self.lfs_locks {
            window.after_op = Some(op);
        }
    }

    /// Une opération se termine : si c'est un verrouillage ou un déverrouillage de la fenêtre, la liste est relue.
    pub(super) fn after_lfs_op(&mut self, shared: &Shared, op: u64) {
        if self.lfs_locks.as_ref().is_some_and(|w| w.after_op == Some(op)) {
            if let Some(w) = &mut self.lfs_locks {
                w.after_op = None;
            }
            self.load_lfs_locks(shared);
        }
    }

    /// Fenêtre des verrous ; ferme la fenêtre quand l'utilisateur la quitte.
    pub(super) fn lfs_locks_window(&mut self, ctx: &egui::Context, shared: &mut Shared, p: &Palette) {
        let Some(window) = &mut self.lfs_locks else { return };
        let mut open = true;
        let (mut reload, mut lock, mut unlock, mut force) = (false, None, None, None);
        crate::widgets::centered_window(ctx, format!("{} {}", icons::LOCK, tr!("Verrous Git LFS")))
            .id(egui::Id::unique(("lfs-locks", self.id)))
            .open(&mut open)
            .default_size([620.0, 420.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut window.path)
                            .hint_text(tr!("Chemin du fichier à verrouiller"))
                            .desired_width(300.0),
                    );
                    let valid = !window.path.trim().is_empty();
                    if ui
                        .add_enabled(valid, egui::Button::new(format!("{} {}", icons::LOCK, tr!("Verrouiller"))))
                        .clicked()
                    {
                        lock = Some(window.path.trim().replace('\\', "/"));
                        window.path.clear();
                    }
                    if ui.button(format!("{} {}", icons::ARROWS_CLOCKWISE, tr!("Actualiser"))).clicked() {
                        reload = true;
                    }
                });
                if let Some(error) = &window.error {
                    warning(ui, &tr!("Lecture des verrous impossible : {error}", error), p);
                }
                ui.separator();
                if window.loading {
                    ui.spinner();
                } else if window.locks.is_empty() && window.error.is_none() {
                    ui.label(RichText::new(tr!("Aucun verrou sur le serveur.")).color(p.weak));
                } else {
                    ui.label(RichText::new(trn!(window.locks.len(), "{n} verrou", "{n} verrous")).strong());
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for l in &window.locks {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icons::LOCK).color(p.modified_fg));
                            ui.label(RichText::new(&l.path).strong());
                            ui.label(RichText::new(format!("{}  {}", l.owner, l.locked_at)).color(p.weak));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .button(tr!("Forcer…"))
                                    .on_hover_text(tr!("Lève le verrou d'un autre utilisateur (--force)."))
                                    .clicked()
                                {
                                    force = Some(l.path.clone());
                                }
                                if ui.button(format!("{} {}", icons::LOCK_OPEN, tr!("Déverrouiller"))).clicked() {
                                    unlock = Some(l.path.clone());
                                }
                            });
                        });
                    }
                });
            });
        if !open {
            self.lfs_locks = None;
            return;
        }
        if reload {
            self.load_lfs_locks(shared);
        }
        if let Some(path) = lock {
            self.op_lfs_lock(shared, path, None);
        }
        if let Some(path) = unlock {
            self.op_lfs_lock(shared, path, Some(false));
        }
        if let Some(path) = force {
            self.dialog = Some(Dialog::Confirm {
                title: tr!("Forcer le déverrouillage de « {path} »", path = &path),
                text: tr!(
                    "Le verrou de « {path} » sera levé même s'il appartient à un autre utilisateur, qui pourrait perdre le travail qu'il y a fait.",
                    path = &path
                ),
                action: ConfirmAction::LfsForceUnlock(path),
            });
        }
    }

    /// Déverrouillage forcé confirmé dans le dialogue.
    pub(super) fn op_lfs_force_unlock(&mut self, shared: &Shared, path: String) {
        self.op_lfs_lock(shared, path, Some(true));
    }
}
