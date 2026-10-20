//! Recherche de commits dans l'historique : par message, auteur ou SHA (en mémoire, voir `apply_filter`), par contenu
//! des modifications (`git log -G`) ou par chemin (`git log -- <motif>`). Les deux dernières lancent git en
//! arrière-plan, après un délai de frappe, sur la portée courante du graphe (`RepoTab::log_options`) ; leur résultat
//! est un ensemble d'oids, et la liste affichée n'en garde que les lignes.
//!
//! Choix : le contenu est une expression régulière (`-G`, pas `-S` littéral : c'est ce que demande la feuille de
//! route) ; une expression invalide n'est pas cachée, git la refuse et son message est affiché. Les chemins sont
//! séparés par des espaces (pas de guillemets), avec les jokers de git (`src/**/*.rs`). Le mode choisi est propre à
//! l'onglet et à la session (comme les autres filtres de l'historique) ; il ne s'enregistre pas dans les réglages.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dragontree_git::{CancelToken, CommitSearch, GitError, LogScope, Oid};
use dragontree_i18n::tr;

use super::{RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};

/// Délai entre la dernière frappe et le lancement de git.
const DEBOUNCE: Duration = Duration::from_millis(350);

/// Ce que cherche le champ de recherche de l'historique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    /// Message, auteur, SHA : en mémoire, sur les commits chargés.
    #[default]
    Text,
    /// Contenu des modifications : `git log -G`.
    Content,
    /// Chemin ou motif de chemin : `git log -- <motif>`.
    Path,
}

impl SearchMode {
    pub const ALL: [SearchMode; 3] = [SearchMode::Text, SearchMode::Content, SearchMode::Path];

    pub fn label(self) -> &'static str {
        match self {
            SearchMode::Text => tr!("Message, auteur, SHA"),
            SearchMode::Content => tr!("Contenu des modifications"),
            SearchMode::Path => tr!("Chemin"),
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            SearchMode::Text => tr!("Rechercher : message, auteur, SHA"),
            SearchMode::Content => tr!("Rechercher : texte ou expression régulière dans les modifications"),
            SearchMode::Path => tr!("Rechercher : chemins ou motifs, séparés par des espaces"),
        }
    }

    /// Recherche git correspondant au texte saisi (`None` : mode en mémoire, ou saisie vide).
    pub fn search(self, query: &str) -> Option<CommitSearch> {
        let query = query.trim();
        match self {
            SearchMode::Text => None,
            _ if query.is_empty() => None,
            SearchMode::Content => Some(CommitSearch::Content(query.to_string())),
            SearchMode::Path => Some(CommitSearch::Paths(query.split_whitespace().map(String::from).collect())),
        }
    }
}

/// État de la recherche d'un onglet.
#[derive(Default)]
pub struct HistorySearch {
    pub mode: SearchMode,
    /// Dernière saisie vue et son instant : le délai de frappe part de là.
    typed: Option<(SearchMode, String, Instant)>,
    /// Recherche lancée (ou terminée) ; `None` : à relancer (la portée a changé).
    requested: Option<(SearchMode, String)>,
    /// Oids trouvés par la recherche lancée, quand elle est terminée sans erreur.
    pub results: Option<Arc<HashSet<Oid>>>,
    pub error: Option<GitError>,
    pub running: bool,
    cancel: Option<CancelToken>,
    /// Change à chaque arrivée ou effacement de résultat : la liste filtrée se recalcule.
    pub revision: u64,
}

impl HistorySearch {
    /// Abandonne la recherche en cours et oublie ses résultats.
    fn clear(&mut self) {
        if let Some(c) = self.cancel.take() {
            c.cancel();
        }
        let (requested, results, error) = (self.requested.take(), self.results.take(), self.error.take());
        let had = requested.is_some() || results.is_some() || error.is_some();
        if self.running || had {
            self.revision += 1;
        }
        self.running = false;
    }

    /// La recherche lancée est celle de cette saisie.
    fn is_for(&self, mode: SearchMode, query: &str) -> bool {
        self.requested.as_ref().is_some_and(|(m, q)| *m == mode && q == query)
    }

    /// Résultats de la recherche lancée, s'ils sont ceux de cette saisie (sinon : en attente ou en cours).
    pub fn results_for(&self, mode: SearchMode, query: &str) -> Option<&Arc<HashSet<Oid>>> {
        self.results.as_ref().filter(|_| self.is_for(mode, query))
    }

    /// Erreur de git (expression invalide…) pour cette saisie.
    pub fn error_for(&self, mode: SearchMode, query: &str) -> Option<&GitError> {
        self.error.as_ref().filter(|_| self.is_for(mode, query))
    }

    /// La saisie est une recherche git qui n'a encore ni résultat ni erreur : délai de frappe ou git au travail.
    pub fn is_searching(&self, mode: SearchMode, query: &str) -> bool {
        mode.search(query).is_some() && !(self.is_for(mode, query) && !self.running)
    }
}

impl RepoTab {
    /// À appeler à chaque image : lance, relance ou abandonne la recherche git selon le mode et la saisie.
    pub(super) fn drive_search(&mut self, ctx: &egui::Context, shared: &Shared) {
        let mode = self.history.search.mode;
        let query = self.history.filter.trim().to_string();
        let Some(search) = mode.search(&query) else {
            self.history.search.clear();
            self.history.search.typed = None;
            return;
        };
        let key = (mode, query);
        let state = &mut self.history.search;
        if state.requested.as_ref() == Some(&key) {
            return;
        }
        let typed_at = match &state.typed {
            Some((m, q, at)) if (*m, q) == (key.0, &key.1) => *at,
            _ => {
                let now = Instant::now();
                state.typed = Some((key.0, key.1.clone(), now));
                now
            }
        };
        let wait = DEBOUNCE.saturating_sub(typed_at.elapsed());
        if !wait.is_zero() {
            ctx.request_repaint_after(wait);
            return;
        }
        self.start_search(shared, key, search);
    }

    fn start_search(&mut self, shared: &Shared, key: (SearchMode, String), search: CommitSearch) {
        let Some(repo) = self.repo.clone() else { return };
        // La recherche précédente est annulée (dépôt volumineux) et ses résultats oubliés.
        self.history.search.clear();
        let cancel = CancelToken::new();
        self.gens.search += 1;
        let (generation, id) = (self.gens.search, self.id);
        let state = &mut self.history.search;
        state.cancel = Some(cancel.clone());
        state.requested = Some(key);
        state.running = true;
        state.revision += 1;
        let mut options = self.log_options(shared);
        let with_lost = self.shows_lost();
        shared.jobs.spawn("history-search", move || {
            // Même portée que le graphe, commits perdus compris.
            if with_lost {
                match repo.lost_commits(&cancel) {
                    Ok(found) => {
                        if let LogScope::Refs(selection) = &mut options.scope {
                            selection.extra.extend(found.tips.iter().map(Oid::to_hex));
                        }
                    }
                    Err(GitError::Cancelled) => return None,
                    // Sans eux, la recherche porte sur le reste du graphe.
                    Err(_) => {}
                }
            }
            match repo.search_commits(&options, &search, &cancel) {
                Err(GitError::Cancelled) => None,
                result => Some(AppEvent::Repo(id, RepoEvent::Searched { generation, result })),
            }
        });
    }

    /// Résultat d'une recherche git ; ignoré s'il n'est plus celui de la dernière demande.
    pub(super) fn search_done(&mut self, generation: u64, result: Result<HashSet<Oid>, GitError>) {
        if generation != self.gens.search {
            return;
        }
        let state = &mut self.history.search;
        state.running = false;
        state.revision += 1;
        match result {
            Ok(found) => (state.results, state.error) = (Some(Arc::new(found)), None),
            Err(e) => (state.results, state.error) = (None, Some(e)),
        }
    }

    /// La portée du graphe a changé (journal relu) : la recherche est à refaire.
    pub(super) fn search_scope_changed(&mut self) {
        self.history.search.requested = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_build_their_search() {
        assert_eq!(SearchMode::Text.search("abc"), None);
        assert_eq!(SearchMode::Content.search("   "), None);
        assert_eq!(SearchMode::Content.search(" foo(\\d+) "), Some(CommitSearch::Content("foo(\\d+)".into())));
        assert_eq!(
            SearchMode::Path.search("src/*.rs  docs"),
            Some(CommitSearch::Paths(vec!["src/*.rs".into(), "docs".into()]))
        );
    }

    #[test]
    fn clearing_forgets_results_and_bumps_the_revision_once() {
        let mut s = HistorySearch::default();
        s.clear();
        assert_eq!(s.revision, 0, "rien à effacer");
        s.requested = Some((SearchMode::Content, "x".into()));
        s.results = Some(Arc::default());
        s.clear();
        assert!(s.results.is_none() && s.requested.is_none());
        assert_eq!(s.revision, 1);
    }
}
