//! IA dans l'onglet de dépôt : « Générer avec Claude » (message de commit) et revue d'une branche. Les appels à la CLI
//! (module `crate::ai`) tournent sur un thread, annulables, et leurs résultats portent un numéro de génération : un
//! résultat périmé (annulation, nouvelle demande, fenêtre fermée) est ignoré. Rien ne part sans un clic.

use dragontree_git::CancelToken;
use dragontree_i18n::{tr, trn};
use egui::{RichText, Ui};

use super::{RepoEvent, RepoTab};
use crate::ai::{self, AiCli, AiError};
use crate::app::{AppEvent, Shared};
use crate::icons;
use crate::theme::Palette;
use crate::widgets;

/// Revue terminée : le texte de Claude et ce qui a été lu.
pub struct ReviewOutcome {
    pub text: String,
    /// Branche de comparaison (nom court).
    pub base: String,
    pub commits: usize,
    /// Taille d'origine du diff s'il a été tronqué avant l'envoi.
    pub truncated_from: Option<usize>,
}

enum ReviewState {
    Loading,
    Done(ReviewOutcome),
    Failed(String),
}

/// Fenêtre « Revue de la branche par Claude ».
pub struct ReviewWindow {
    branch: String,
    state: ReviewState,
    generation: u64,
    cancel: CancelToken,
}

/// État de l'IA d'un onglet.
#[derive(Default)]
pub struct AiState {
    /// Numéro de la dernière génération de message demandée ; `cancel` est présent tant qu'elle tourne.
    commit_generation: u64,
    commit_cancel: Option<CancelToken>,
    commit_error: Option<String>,
    /// Diff tronqué avant l'envoi : taille d'origine, affichée tant que le message généré est là.
    commit_truncated_from: Option<usize>,
    /// Message de la zone de commit au moment du clic (résumé, description) et si la réponse peut l'écraser.
    commit_snapshot: (String, String, bool),
    /// Réponse arrivée alors que l'utilisateur avait écrit entre-temps : proposée, jamais imposée.
    suggestion: Option<(String, String)>,
    review: Option<ReviewWindow>,
    review_generation: u64,
}

impl AiState {
    pub fn generating(&self) -> bool {
        self.commit_cancel.is_some()
    }
}

impl RepoTab {
    /// Clic sur « Générer avec Claude » : envoie le diff indexé (tronqué) à la CLI.
    pub(super) fn generate_commit_message(&mut self, shared: &Shared) {
        let Some(repo) = self.repo.clone() else { return };
        if let Some(old) = &self.ai.commit_cancel {
            old.cancel();
        }
        let cancel = CancelToken::new();
        self.ai.commit_generation += 1;
        self.ai.commit_cancel = Some(cancel.clone());
        self.ai.commit_error = None;
        self.ai.commit_truncated_from = None;
        self.ai.suggestion = None;
        // Une zone vide (ou le modèle intact) est remplie ; un message déjà écrit n'est jamais écrasé sans demander.
        let c = &self.changes;
        let template_only = c.template_prefill.as_ref().is_some_and(|(s, b)| *s == c.subject && *b == c.body);
        let replaceable = c.subject.trim().is_empty() && c.body.trim().is_empty() || template_only;
        self.ai.commit_snapshot = (c.subject.clone(), c.body.clone(), replaceable);

        let (generation, tab) = (self.ai.commit_generation, self.id);
        let (cli, lang) = (AiCli::from_settings(&shared.settings), dragontree_i18n::lang());
        shared.jobs.spawn("ai-commit", move || {
            let result = (|| {
                let diff = repo.staged_diff_text(&cancel).map_err(|e| e.to_string())?;
                if diff.trim().is_empty() {
                    return Err(tr!("Aucun fichier indexé : rien à résumer.").to_string());
                }
                let (sent, truncated_from) = ai::truncate_diff(&diff, ai::MAX_DIFF_BYTES);
                let answer =
                    ai::ask(&cli, &ai::commit_prompt(&sent, lang), &cancel, ai::TIMEOUT).map_err(|e| e.message())?;
                let (subject, body) = ai::parse_commit_message(&answer).ok_or_else(|| AiError::Empty.message())?;
                Ok((subject, body, truncated_from))
            })();
            Some(AppEvent::Repo(tab, RepoEvent::AiCommit(generation, result)))
        });
    }

    pub(super) fn cancel_commit_generation(&mut self) {
        if let Some(cancel) = self.ai.commit_cancel.take() {
            cancel.cancel();
        }
        // Le résultat qui arriverait quand même est périmé.
        self.ai.commit_generation += 1;
    }

    /// Le message part avec un commit : une réponse encore attendue ou mise de côté ne doit pas remplir le suivant.
    pub(super) fn forget_ai_commit(&mut self) {
        self.cancel_commit_generation();
        self.ai.suggestion = None;
        self.ai.commit_error = None;
        self.ai.commit_truncated_from = None;
    }

    pub(super) fn on_ai_commit(&mut self, generation: u64, result: Result<(String, String, Option<usize>), String>) {
        if generation != self.ai.commit_generation {
            return;
        }
        self.ai.commit_cancel = None;
        match result {
            Err(message) => self.ai.commit_error = Some(message),
            Ok((subject, body, truncated_from)) => {
                self.ai.commit_truncated_from = truncated_from;
                let (before_subject, before_body, replaceable) = &self.ai.commit_snapshot;
                let untouched = *before_subject == self.changes.subject && *before_body == self.changes.body;
                if *replaceable && untouched {
                    self.apply_suggestion(subject, body);
                } else {
                    self.ai.suggestion = Some((subject, body));
                }
            }
        }
    }

    fn apply_suggestion(&mut self, subject: String, body: String) {
        self.changes.subject = subject;
        self.changes.body = body;
        // Le message n'est plus celui du modèle : un modèle ou un message de fusion ne le remplacera plus.
        self.changes.template_prefill = None;
        self.ai.suggestion = None;
    }

    /// Bouton, erreur et suggestion de l'IA sous le message de commit.
    pub(super) fn ai_commit_controls(&mut self, ui: &mut Ui, shared: &Shared, p: &Palette, can_generate: bool) {
        if !shared.settings.ai_enabled {
            return;
        }
        if self.ai.generating() {
            ui.spinner();
            ui.label(RichText::new(tr!("Claude rédige le message…")).color(p.weak));
            if ui.button(tr!("Annuler")).clicked() {
                self.cancel_commit_generation();
            }
        } else {
            let label = format!("{} {}", icons::SPARKLE, tr!("Générer avec Claude"));
            let hint = tr!("Envoie le diff indexé à la CLI Claude Code pour rédiger le message");
            if ui.add_enabled(can_generate, egui::Button::new(label)).on_hover_text(hint).clicked() {
                self.generate_commit_message(shared);
            }
        }
    }

    /// Erreur de la dernière génération et réponse mise de côté.
    pub(super) fn ai_commit_notices(&mut self, ui: &mut Ui, p: &Palette) {
        if let Some(error) = self.ai.commit_error.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{} {error}", icons::WARNING)).color(p.removed_fg).size(11.5));
                if ui.small_button(icons::X).on_hover_text(tr!("Masquer")).clicked() {
                    self.ai.commit_error = None;
                }
            });
        }
        if let Some(size) = self.ai.commit_truncated_from {
            let note = tr!(
                "Diff tronqué à {max} Ko sur {size} Ko avant l'envoi.",
                max = ai::MAX_DIFF_BYTES / 1000,
                size = size / 1000
            );
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{} {note}", icons::WARNING)).color(p.modified_fg).size(11.5));
                if ui.small_button(icons::X).on_hover_text(tr!("Masquer")).clicked() {
                    self.ai.commit_truncated_from = None;
                }
            });
        }
        if let Some((subject, body)) = self.ai.suggestion.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr!("Suggestion de Claude : {subject}", subject)).color(p.weak).size(11.5));
                if ui.small_button(tr!("Remplacer le message")).clicked() {
                    self.apply_suggestion(subject, body);
                }
                if ui.small_button(icons::X).on_hover_text(tr!("Ignorer")).clicked() {
                    self.ai.suggestion = None;
                }
            });
        }
    }

    /// Ouvre la fenêtre de revue de `branch` et lance l'appel : diff depuis le merge-base avec la branche par défaut.
    pub(super) fn open_ai_review(&mut self, shared: &Shared, branch: String) {
        let Some(repo) = self.repo.clone() else { return };
        if let Some(old) = &self.ai.review {
            old.cancel.cancel();
        }
        self.ai.review_generation += 1;
        let cancel = CancelToken::new();
        let generation = self.ai.review_generation;
        self.ai.review = Some(ReviewWindow {
            branch: branch.clone(),
            state: ReviewState::Loading,
            generation,
            cancel: cancel.clone(),
        });
        let (tab, cli, lang) = (self.id, AiCli::from_settings(&shared.settings), dragontree_i18n::lang());
        shared.jobs.spawn("ai-review", move || {
            let result = (|| {
                let refs = repo.refs().map_err(|e| e.to_string())?;
                let base = repo
                    .default_branch(&refs)
                    .ok_or_else(|| tr!("Branche par défaut introuvable : rien pour comparer.").to_string())?;
                if base == branch {
                    return Err(tr!("C'est la branche par défaut : il n'y a rien à comparer.").to_string());
                }
                // La branche locale si elle existe, sinon sa copie distante.
                let base_ref = if refs.local(&base).is_some() {
                    base.clone()
                } else {
                    refs.remotes
                        .iter()
                        .find(|r| r.name == base && !r.is_symbolic)
                        .map(|r| r.full.clone())
                        .ok_or_else(|| tr!("Branche par défaut introuvable : rien pour comparer.").to_string())?
                };
                let review = repo.branch_review(&branch, &base_ref, &cancel).map_err(|e| e.to_string())?;
                if review.diff.trim().is_empty() {
                    return Err(tr!("Cette branche n'ajoute rien à {base}.", base).to_string());
                }
                let (diff, _) = ai::truncate_diff(&review.diff, ai::MAX_DIFF_BYTES);
                let truncated_from = (diff.len() != review.diff.len()).then_some(review.diff.len());
                let prompt = ai::review_prompt(&branch, &base, &review.commits, &diff, lang);
                let text = ai::ask(&cli, &prompt, &cancel, ai::TIMEOUT).map_err(|e| e.message())?;
                Ok(ReviewOutcome { text, base, commits: review.commits.len(), truncated_from })
            })();
            Some(AppEvent::Repo(tab, RepoEvent::AiReview(generation, result.map(Box::new))))
        });
    }

    pub(super) fn on_ai_review(&mut self, generation: u64, result: Result<Box<ReviewOutcome>, String>) {
        if let Some(w) = &mut self.ai.review
            && w.generation == generation
        {
            w.state = match result {
                Ok(outcome) => ReviewState::Done(*outcome),
                Err(message) => ReviewState::Failed(message),
            };
        }
    }

    pub(super) fn ai_review_window(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(window) = &mut self.ai.review else { return };
        let mut open = true;
        let title = format!("{} {}", icons::SPARKLE, tr!("Revue de {branch} par Claude", branch = window.branch));
        widgets::centered_window(ctx, title)
            .id(egui::Id::unique(("ai-review", self.id)))
            .open(&mut open)
            .default_size([760.0, 620.0])
            .show(ctx, |ui| match &window.state {
                ReviewState::Loading => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr!("Claude relit la branche…"));
                        if ui.button(tr!("Annuler")).clicked() {
                            window.cancel.cancel();
                            window.state = ReviewState::Failed(AiError::Cancelled.message());
                            window.generation += 1;
                        }
                    });
                }
                ReviewState::Failed(message) => {
                    ui.label(RichText::new(format!("{} {message}", icons::WARNING)).color(p.removed_fg));
                }
                ReviewState::Done(outcome) => {
                    let base = &outcome.base;
                    let summary =
                        trn!(outcome.commits, "{n} commit comparé à {base}", "{n} commits comparés à {base}", base);
                    ui.label(RichText::new(summary).color(p.weak).size(12.0));
                    if let Some(size) = outcome.truncated_from {
                        let note = tr!(
                            "Diff tronqué à {max} Ko sur {size} Ko avant l'envoi.",
                            max = ai::MAX_DIFF_BYTES / 1000,
                            size = size / 1000
                        );
                        ui.label(RichText::new(format!("{} {note}", icons::WARNING)).color(p.modified_fg).size(12.0));
                    }
                    if ui.button(format!("{} {}", icons::COPY, tr!("Copier la revue"))).clicked() {
                        ui.ctx().copy_text(outcome.text.clone());
                    }
                    ui.separator();
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        // Texte en lecture seule mais sélectionnable : un `&str` comme tampon.
                        let mut text = outcome.text.as_str();
                        ui.add(
                            egui::TextEdit::multiline(&mut text)
                                .desired_width(f32::INFINITY)
                                .desired_rows(24)
                                .frame(egui::Frame::NONE),
                        );
                    });
                }
            });
        if !open {
            window.cancel.cancel();
            self.ai.review = None;
        }
    }
}
