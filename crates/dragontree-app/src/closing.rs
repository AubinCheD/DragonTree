//! Fermeture de l'application pendant des opérations (RV4a).
//!
//! La demande de fermeture (croix, Alt+F4, Fichier › Quitter, barre des tâches) passe par [`Closing::intercept`],
//! appelée par `App::logic`, qui tourne aussi quand la fenêtre est réduite :
//! - si des opérations visibles attendent ou tournent, la fermeture est suspendue (`ViewportCommand::CancelClose`) et
//!   un dialogue ([`Closing::dialog`]) les nomme : « Attendre » ferme la fenêtre d'elle-même à leur fin, « Quitter
//!   quand même » les annule toutes et ferme, Échap ou un clic à côté renonce à fermer ;
//! - sinon les opérations silencieuses (fetch automatique) sont annulées et la fenêtre se ferme sans question.
//!
//! Une opération compte jusqu'à ce que l'interface ait traité sa fin, pas seulement tant qu'elle est dans sa file
//! ([`TabWork`]) : la suite qu'elle enchaîne (push après un commit, création d'une pull request après son push,
//! proposition du push forcé après un amend) ne part qu'à ce moment, et « Attendre » l'attend aussi ; la création d'une
//! pull request par l'API d'une forge compte aussi. Les événements des tâches sont traités par `App::logic` juste avant
//! [`Closing::intercept`], fenêtre réduite comprise. Si la fin d'une opération ouvre un dialogue qui attend
//! l'utilisateur, la fermeture suspendue y renonce pour le montrer (`decision` de [`Closing::intercept`]) ; un dépôt
//! transmis par une autre instance aussi ([`Closing::cancel`]).
//!
//! Une opération annulée arrête son git et ses sous-processus ; ce qui tournerait encore à la fin du processus meurt
//! avec lui (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, voir le runner de `dragontree_git`). Seul un clonage annulé est
//! attendu à la fin de l'application, 2 s au plus, le temps de supprimer son dossier partiel (`clone::CloneTasks`).

use std::collections::HashSet;
use std::time::Duration;

use dragontree_git::CancelToken;
use dragontree_i18n::{tr, trn};

use crate::app::TabId;
use crate::clone::CloneWindow;
use crate::op_queue::OpQueues;
use crate::theme::Palette;

/// Travail visible d'un onglet tel que l'interface le suit (voir `RepoTab::pending_work`) : une opération lancée dont
/// la fin n'a pas encore été traitée (elle a pu quitter sa file entre-temps), ou un appel à une forge (création d'une
/// pull request).
#[derive(Debug, Clone)]
pub struct TabWork {
    /// Opération d'une file (onglet, numéro) : comptée une seule fois avec celle de la file.
    pub op: Option<(TabId, u64)>,
    pub title: String,
    /// Annulation (`None` : un appel à une forge, qui ne s'annule pas).
    pub cancel: Option<CancelToken>,
}

/// Opérations de l'application que la fermeture regarde : celles des files des dépôts, le travail suivi par les
/// onglets, et un clonage en cours.
pub struct Operations<'a> {
    pub queues: &'a OpQueues,
    pub tabs: Vec<TabWork>,
    pub clone: Option<&'a CancelToken>,
}

impl<'a> Operations<'a> {
    pub fn new(queues: &'a OpQueues, tabs: Vec<TabWork>, clone: Option<&'a CloneWindow>) -> Operations<'a> {
        Operations { queues, tabs, clone: clone.and_then(CloneWindow::running) }
    }

    /// Titres des opérations visibles en attente ou en cours.
    fn visible(&self) -> Vec<String> {
        let queued: Vec<_> = self.queues.pending().into_iter().filter(|op| !op.silent).collect();
        let in_queues: HashSet<(TabId, u64)> = queued.iter().map(|op| (op.tab, op.id)).collect();
        let tabs = self.tabs.iter().filter(|w| w.op.is_none_or(|key| !in_queues.contains(&key)));
        let clone = self.clone.map(|_| tr!("Clonage").to_string());
        queued.into_iter().map(|op| op.title).chain(tabs.map(|w| w.title.clone())).chain(clone).collect()
    }

    /// Annule toutes les opérations, silencieuses comprises.
    pub fn stop_all(&self) {
        for op in self.queues.pending() {
            op.cancel.cancel();
        }
        for cancel in self.tabs.iter().filter_map(|w| w.cancel.as_ref()) {
            cancel.cancel();
        }
        if let Some(cancel) = self.clone {
            cancel.cancel();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    #[default]
    No,
    /// Des opérations visibles tournent : le dialogue demande d'attendre leur fin ou de quitter quand même.
    Asking,
    /// « Attendre » : la fenêtre se ferme d'elle-même à la fin des opérations visibles.
    Waiting,
    /// Fermeture décidée : la demande de fermeture suivante n'est plus suspendue, aucune opération ne démarre plus.
    Quitting,
}

/// État de la fermeture de l'application.
#[derive(Debug, Default)]
pub struct Closing {
    state: State,
}

impl Closing {
    /// Fermeture décidée : plus de rafraîchissement ni de fetch automatique.
    pub fn quitting(&self) -> bool {
        self.state == State::Quitting
    }

    /// Ferme pour de bon : opérations arrêtées, puis `ViewportCommand::Close`, que `intercept` laisse passer.
    fn quit_now(&mut self, ctx: &egui::Context, ops: &Operations) {
        ops.stop_all();
        self.state = State::Quitting;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Renonce à une fermeture suspendue (question posée ou attente de la fin des opérations) : un dépôt transmis par
    /// une autre instance (RV4b) vient de s'ouvrir, la fenêtre reste.
    pub fn cancel(&mut self) {
        if matches!(self.state, State::Asking | State::Waiting) {
            self.state = State::No;
        }
    }

    /// Rend visible et met au premier plan la fenêtre, réduite peut-être, pour une question.
    fn show_window(ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().minimized == Some(true)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// À chaque image, après le traitement des événements des tâches : suspend la demande de fermeture de la fenêtre
    /// s'il le faut (voir la documentation du module). Quand il n'y a plus d'opération visible, la question n'a plus
    /// d'objet : la fenêtre se ferme. `forced` : une fermeture qui n'est jamais suspendue (celle que demande une
    /// capture d'écran). `decision` : la fin d'une opération vient d'ouvrir un dialogue qui attend l'utilisateur
    /// (proposition du push forcé après un amend…) ; une fermeture suspendue, ou demandée dans cette image, y renonce
    /// pour le montrer.
    pub fn intercept(&mut self, ctx: &egui::Context, ops: &Operations, forced: bool, decision: bool) {
        let requested = ctx.input(|i| i.viewport().close_requested());
        let suspended = matches!(self.state, State::Asking | State::Waiting);
        if decision && !forced && (suspended || requested && self.state == State::No) {
            if requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            self.state = State::No;
            Self::show_window(ctx);
            return;
        }
        if suspended {
            if ops.visible().is_empty() {
                self.quit_now(ctx, ops);
            } else {
                // La fin d'une opération réveille l'interface avant qu'elle ne quitte sa file : on revérifie.
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
        if !requested || self.state == State::Quitting || forced {
            return;
        }
        if ops.visible().is_empty() {
            ops.stop_all();
            self.state = State::Quitting;
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        self.state = State::Asking;
        // Fermée depuis la barre des tâches : la fenêtre réduite est rendue visible pour la question.
        Self::show_window(ctx);
    }

    /// Dialogue de fermeture pendant des opérations (après « Attendre » : attente de leur fin). Destructeur : Entrée
    /// ne quitte pas ; Échap ou un clic à côté renonce à fermer.
    pub fn dialog(&mut self, ctx: &egui::Context, ops: &Operations, p: &Palette) {
        if !matches!(self.state, State::Asking | State::Waiting) {
            return;
        }
        let waiting = self.state == State::Waiting;
        let visible = ops.visible();
        const SHOWN: usize = 8;
        enum Choice {
            Quit,
            Wait,
            Stay,
        }
        let mut choice = None;
        let response = egui::Modal::new(egui::Id::unique("closing-dialog")).show(ctx, |ui| {
            ui.set_width(420.0);
            let title = format!("{} {}", crate::icons::WARNING, tr!("Des opérations sont en cours"));
            ui.label(egui::RichText::new(title).strong().size(16.0));
            ui.add_space(8.0);
            for title in visible.iter().take(SHOWN) {
                ui.label(format!("• {title}"));
            }
            if visible.len() > SHOWN {
                ui.label(trn!(visible.len() - SHOWN, "… et {n} autre", "… et {n} autres"));
            }
            ui.add_space(8.0);
            if waiting {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr!("DragonTree se fermera à la fin des opérations."));
                });
            } else {
                let risk = tr!(
                    "Quitter maintenant les interrompt : un dépôt peut rester dans un état intermédiaire (opération à reprendre, fichier index.lock à supprimer)."
                );
                ui.label(egui::RichText::new(risk).color(p.weak));
            }
            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let quit = egui::Button::new(
                    egui::RichText::new(tr!("Quitter quand même")).strong().color(egui::Color32::WHITE),
                )
                .fill(p.removed_fg)
                .min_size(egui::vec2(140.0, 28.0));
                if ui.add(quit).clicked() {
                    choice = Some(Choice::Quit);
                }
                let (label, other) =
                    if waiting { (tr!("Ne pas quitter"), Choice::Stay) } else { (tr!("Attendre"), Choice::Wait) };
                if ui.add(egui::Button::new(label).min_size(egui::vec2(110.0, 28.0))).clicked() {
                    choice = Some(other);
                }
            });
        });
        match choice {
            Some(Choice::Quit) => self.quit_now(ctx, ops),
            Some(Choice::Wait) => self.state = State::Waiting,
            Some(Choice::Stay) => self.state = State::No,
            None if response.should_close() => self.state = State::No,
            None => {}
        }
    }
}
