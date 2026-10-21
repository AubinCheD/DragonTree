//! Filtre de l'historique (champ de recherche) : les lignes de la liste affichée qui correspondent à la saisie. Le
//! calcul parcourt tous les commits : il tourne en arrière-plan, avec un numéro de génération, pour qu'une frappe ne
//! coûte presque rien au thread de l'interface ; un calcul devenu inutile est annulé et son résultat ignoré.
//!
//! - Mode texte : message, auteur, e-mail ou début du SHA, sans casse ni accents.
//! - Recherche git (contenu, chemin : `history_search`) : les lignes dont l'oid figure dans le résultat de git ;
//!   aucune tant que ce résultat n'est pas arrivé.
//!
//! En attendant un résultat, le précédent reste affiché (pas de clignotement d'une frappe à l'autre). Les lignes
//! désignent toujours la liste affichée : quand celle-ci est remplacée (journal relu, fusions repliées :
//! `show_list`), l'ancien résultat est aussitôt ramené aux commits qu'elle contient encore (`carry_filter_over`),
//! puis recalculé sur la nouvelle liste. Un commit défilé pendant le calcul (sur ces lignes provisoires) l'est de
//! nouveau à l'arrivée du résultat (`refollow`). La tâche répond toujours : si elle panique ou ne démarre pas, le
//! calcul n'est plus attendu (`Jobs::spawn_guarded`).

use std::collections::HashSet;
use std::sync::Arc;

use dragontree_git::{CancelToken, Commit, Oid};
use dragontree_i18n::tr;

use super::history_search::SearchMode;
use super::{RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::toasts::Toast;
use crate::widgets;

/// Nombre de commits examinés entre deux vérifications de l'annulation.
const CANCEL_CHECK: usize = 4096;

/// Ce que retient le filtre.
pub(super) enum FilterQuery {
    /// Texte déjà replié (`widgets::fold`) : message, auteur, e-mail, début du SHA.
    Text(String),
    /// Oids trouvés par une recherche git.
    Oids(Arc<HashSet<Oid>>),
}

/// Lignes de `commits` retenues par `query`, dans l'ordre de la liste ; `None` si `cancel` a interrompu le calcul.
pub(super) fn filter_rows(commits: &[Commit], query: &FilterQuery, cancel: &CancelToken) -> Option<Vec<usize>> {
    let mut rows = Vec::new();
    match query {
        FilterQuery::Text(needle) => {
            // Un SHA ne s'écrit qu'en chiffres hexadécimaux : sinon, inutile de le calculer pour chaque commit.
            let hex = needle.len() <= 40 && needle.bytes().all(|b| b.is_ascii_hexdigit());
            let mut folded = String::new();
            let mut contains = |text: &str| {
                widgets::fold_into(&mut folded, text);
                folded.contains(needle.as_str())
            };
            for (i, c) in commits.iter().enumerate() {
                if i % CANCEL_CHECK == 0 && cancel.is_cancelled() {
                    return None;
                }
                if (hex && c.id.to_hex().starts_with(needle.as_str()))
                    || contains(&c.summary)
                    || contains(&c.author)
                    || contains(&c.author_email)
                {
                    rows.push(i);
                }
            }
        }
        FilterQuery::Oids(found) => {
            for (i, c) in commits.iter().enumerate() {
                if i % CANCEL_CHECK == 0 && cancel.is_cancelled() {
                    return None;
                }
                if found.contains(&c.id) {
                    rows.push(i);
                }
            }
        }
    }
    Some(rows)
}

impl RepoTab {
    /// À appeler à chaque image (et quand la liste affichée change) : lance le calcul du filtre si la saisie, le mode
    /// ou les résultats de la recherche git ont changé. Seul le lancement a lieu ici, jamais le parcours des commits.
    pub(super) fn apply_filter(&mut self, shared: &Shared) {
        let mode = self.history.search.mode;
        let revision = self.history.search.revision;
        let query = self.history.filter.trim();
        if self.history.filter_applied.as_ref().is_some_and(|(m, q, r)| (*m, q.as_str(), *r) == (mode, query, revision))
        {
            return;
        }
        let query = query.to_string();
        self.history.filter_applied = Some((mode, query.clone(), revision));
        // Le calcul précédent n'a plus d'objet : annulé, et son résultat éventuel ignoré.
        if let Some(cancel) = self.history.filter_cancel.take() {
            cancel.cancel();
        }
        self.gens.filter += 1;
        self.history.filter_pending = false;
        let what = if query.is_empty() {
            self.history.filtered = None;
            self.refollow();
            return;
        } else if mode == SearchMode::Text {
            FilterQuery::Text(widgets::fold(&query))
        } else {
            match self.history.search.results_for(mode, &query) {
                Some(found) => FilterQuery::Oids(found.clone()),
                // Recherche git pas encore terminée : aucune ligne.
                None => {
                    self.history.filtered = Some(Arc::new(Vec::new()));
                    self.refollow();
                    return;
                }
            }
        };
        let cancel = CancelToken::new();
        self.history.filter_cancel = Some(cancel.clone());
        self.history.filter_pending = true;
        let (generation, id, commits) = (self.gens.filter, self.id, self.history.commits.clone());
        // Une tâche qui panique, ou qui ne démarre pas, répond quand même : le filtre n'attend jamais en vain.
        let failed = AppEvent::Repo(id, RepoEvent::Filtered { generation, rows: None });
        shared.jobs.spawn_guarded("history-filter", failed, move || {
            let rows = filter_rows(&commits, &what, &cancel)?;
            Some(AppEvent::Repo(id, RepoEvent::Filtered { generation, rows: Some(rows) }))
        });
    }

    /// Résultat du filtre ; ignoré s'il n'est plus celui de la dernière saisie ou de la liste affichée. En cas d'échec
    /// de sa tâche (`None`), le résultat précédent reste affiché.
    pub(super) fn filter_done(&mut self, shared: &mut Shared, generation: u64, rows: Option<Vec<usize>>) {
        if generation != self.gens.filter {
            return;
        }
        self.history.filter_pending = false;
        self.history.filter_cancel = None;
        match rows {
            Some(rows) => self.history.filtered = Some(Arc::new(rows)),
            None => shared.toast(Toast::error(tr!("Le filtre de l'historique n'a pas pu être calculé"), "")),
        }
        self.refollow();
    }

    /// Le filtre n'est plus en calcul : un commit défilé entre-temps, sur des lignes provisoires, l'est de nouveau sur
    /// les lignes affichées, s'il est toujours sélectionné (un clic ou une flèche a pu en choisir un autre).
    fn refollow(&mut self) {
        if let Some(oid) = self.history.follow.take().filter(|oid| self.history.selected == Some(*oid)) {
            self.scroll_to_commit(oid);
        }
    }

    /// La liste affichée vient de remplacer `old` : le résultat du filtre (des lignes de `old`) est ramené aux
    /// commits que la nouvelle liste contient encore, dans son ordre, en attendant le calcul complet.
    pub(super) fn carry_filter_over(&mut self, old: &[Commit]) {
        let Some(rows) = &self.history.filtered else { return };
        let index = &self.history.index;
        let mut kept: Vec<usize> =
            rows.iter().filter_map(|&r| old.get(r).and_then(|c| index.get(&c.id).copied())).collect();
        kept.sort_unstable();
        self.history.filtered = Some(Arc::new(kept));
    }

    /// Position de la ligne `row` de la liste affichée parmi les lignes affichées (les résultats du filtre s'il est
    /// actif) ; `None` si le filtre l'écarte.
    pub(crate) fn shown_position(&self, row: usize) -> Option<usize> {
        match &self.history.filtered {
            // Les résultats sont dans l'ordre de la liste.
            Some(rows) => rows.binary_search(&row).ok(),
            None => Some(row),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::repo::CommitList;
    use crate::settings::SettingsFile;

    fn oid(i: usize) -> Oid {
        let mut bytes = [0xab; 20];
        bytes[..8].copy_from_slice(&(i as u64).to_be_bytes());
        Oid(bytes)
    }

    /// Liste synthétique de `n` commits (un gros dépôt), sans git.
    fn commits(n: usize) -> Vec<Commit> {
        let authors: Vec<(Arc<str>, Arc<str>)> = ["Ada Lovelace", "Grace Hopper", "Linus Contoso", "Émile Fabrikam"]
            .iter()
            .map(|a| (Arc::from(*a), Arc::from(format!("{}@northwind.test", a.to_lowercase().replace(' ', ".")))))
            .collect();
        (0..n)
            .map(|i| {
                let (author, email) = authors[i % authors.len()].clone();
                Commit {
                    id: oid(i),
                    parents: Box::new([oid(i + 1)]),
                    author: author.clone(),
                    author_email: email,
                    author_time: 1_700_000_000 - i as i64,
                    committer: author,
                    committer_time: 1_700_000_000 - i as i64,
                    summary: format!("Corrige le numéro {i} du module de facturation").into(),
                }
            })
            .collect()
    }

    fn list_of(commits: Vec<Commit>) -> CommitList {
        let index = commits.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
        CommitList { commits: Arc::new(commits), index: Arc::new(index), ..CommitList::default() }
    }

    type Events = std::sync::mpsc::Receiver<AppEvent>;

    fn tab_with(n: usize) -> (RepoTab, Shared, Events, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let (store, settings, _) = SettingsFile::load(&dir.path().join("settings.json"));
        let (mut shared, events) = Shared::new(egui::Context::default(), settings, store, None, None);
        let mut tab = RepoTab::new(1, dir.path().to_path_buf(), &mut shared);
        let list = list_of(commits(n));
        tab.history.commits = list.commits;
        tab.history.index = list.index;
        (tab, shared, events, dir)
    }

    /// Fait arriver les événements des tâches de l'onglet jusqu'à la fin du calcul du filtre (au plus 20 s).
    fn until_filtered(tab: &mut RepoTab, shared: &mut Shared, events: &Events) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while tab.history.filter_pending {
            let left = deadline.saturating_duration_since(Instant::now());
            match events.recv_timeout(left) {
                Ok(AppEvent::Repo(_, ev)) => tab.handle_event(ev, shared),
                Ok(_) => {}
                Err(_) => panic!("délai dépassé en attendant le filtre"),
            }
        }
    }

    fn summaries(tab: &RepoTab) -> Vec<String> {
        let rows = tab.history.filtered.as_ref().expect("filtre actif");
        rows.iter().map(|&r| tab.history.commits[r].summary.to_string()).collect()
    }

    fn numbered(ns: impl IntoIterator<Item = usize>) -> Vec<String> {
        ns.into_iter().map(|i| format!("Corrige le numéro {i} du module de facturation")).collect()
    }

    /// Critère de RV1 : une frappe dans le filtre coûte moins de 16 ms au thread de l'interface, sur une liste de
    /// 100 000 commits (le calcul lui-même part en arrière-plan).
    #[test]
    fn a_keystroke_in_the_filter_costs_less_than_a_frame_on_the_interface_thread() {
        let (mut tab, shared, _events, _dir) = tab_with(100_000);
        let mut worst = Duration::ZERO;
        for typed in ["n", "nu", "num", "nume", "numér", "numéro", "numéro 4", "numéro 42", "numéro 424"] {
            tab.history.filter = typed.into();
            let start = Instant::now();
            tab.apply_filter(&shared);
            worst = worst.max(start.elapsed());
        }
        assert!(worst < Duration::from_millis(16), "frappe la plus lente : {worst:?}");
    }

    #[test]
    fn the_filter_result_is_applied_when_its_job_returns_and_a_stale_one_is_ignored() {
        let (mut tab, mut shared, events, _dir) = tab_with(50_000);
        tab.history.filter = "numéro 4242".into();
        tab.apply_filter(&shared);
        assert!(tab.history.filter_pending && tab.history.filtered.is_none(), "rien n'est calculé sur place");
        until_filtered(&mut tab, &mut shared, &events);
        assert_eq!(summaries(&tab), numbered([4242].into_iter().chain(42420..=42429)));

        // Deux frappes rapprochées : seul le résultat de la dernière est appliqué, l'autre est ignoré à son arrivée.
        tab.history.filter = "NUMERO 1234".into();
        tab.apply_filter(&shared);
        let stale = tab.gens.filter;
        tab.history.filter = "numéro 12345".into();
        tab.apply_filter(&shared);
        assert_eq!(summaries(&tab), numbered([4242].into_iter().chain(42420..=42429)), "l'ancien reste affiché");
        tab.handle_event(RepoEvent::Filtered { generation: stale, rows: Some(vec![0, 1, 2]) }, &mut shared);
        assert!(tab.history.filter_pending, "résultat périmé ignoré");
        until_filtered(&mut tab, &mut shared, &events);
        assert_eq!(summaries(&tab), numbered([12345]));

        // Auteur et e-mail, sans casse ni accents ; début du SHA.
        tab.history.filter = "emile".into();
        tab.apply_filter(&shared);
        until_filtered(&mut tab, &mut shared, &events);
        assert_eq!(tab.history.filtered.as_ref().unwrap().len(), 50_000 / 4);
        tab.history.filter = oid(7).to_hex()[..16].to_string();
        tab.apply_filter(&shared);
        until_filtered(&mut tab, &mut shared, &events);
        assert_eq!(summaries(&tab), numbered([7]));

        // Champ vidé : plus de filtre, sur-le-champ.
        tab.history.filter = "  ".into();
        tab.apply_filter(&shared);
        assert!(tab.history.filtered.is_none() && !tab.history.filter_pending);
    }

    /// V8 : quand la liste affichée change (fusions repliées, journal relu), les lignes du filtre restent celles de
    /// la liste affichée, tout de suite puis après le nouveau calcul.
    #[test]
    fn the_filter_follows_the_displayed_list_when_it_is_replaced() {
        let (mut tab, mut shared, events, _dir) = tab_with(2_000);
        tab.history.filter = "numéro 1".into();
        tab.apply_filter(&shared);
        until_filtered(&mut tab, &mut shared, &events);
        let before = summaries(&tab);

        // Nouvelle liste : un commit sur deux (les autres sont repliés), plus un nouveau en tête.
        let mut shown: Vec<Commit> = commits(2_000).into_iter().step_by(2).collect();
        let mut newer = commits(2_001).pop().unwrap();
        newer.summary = "Corrige le numéro 1 du nouveau module".into();
        shown.insert(0, newer);
        tab.show_list(&shared, list_of(shown), HashMap::new(), &HashMap::new(), false);
        let kept: Vec<String> = before
            .iter()
            .filter(|s| s.split(' ').nth(3).and_then(|n| n.parse::<usize>().ok()).is_some_and(|n| n % 2 == 0))
            .cloned()
            .collect();
        assert_eq!(summaries(&tab), kept, "aussitôt : l'ancien résultat, ramené à la nouvelle liste");
        until_filtered(&mut tab, &mut shared, &events);
        let mut expected = vec!["Corrige le numéro 1 du nouveau module".to_string()];
        expected.extend(kept);
        assert_eq!(summaries(&tab), expected, "puis le calcul sur la nouvelle liste");
    }

    /// Un commit choisi pendant le calcul du filtre (recherche, « Aller à HEAD »…) est défilé sur les lignes
    /// provisoires, puis de nouveau à l'arrivée du résultat : sa position y a changé.
    #[test]
    fn the_scroll_follows_the_selected_commit_when_the_filter_result_arrives() {
        let (mut tab, mut shared, events, _dir) = tab_with(2_000);
        tab.history.filter = "numéro 1".into();
        tab.apply_filter(&shared);
        until_filtered(&mut tab, &mut shared, &events);
        tab.history.filter = "numéro 15".into();
        tab.apply_filter(&shared);
        assert!(tab.history.filter_pending);
        // Le commit 1500 est la ligne 1 + 10 + 100 + 500 des résultats provisoires (« numéro 1… »).
        tab.select_commit(&shared, oid(1500), true);
        assert_eq!(tab.history.scroll_to.take(), Some(611), "défilé sur les lignes provisoires");
        until_filtered(&mut tab, &mut shared, &events);
        // Résultat de « numéro 15 » : 15, 150…159, puis 1500…1599.
        assert_eq!(tab.history.scroll_to, Some(11), "défilé de nouveau sur le résultat");

        // Un autre commit choisi entre-temps (clic, clavier) : le défilement n'y revient plus.
        tab.history.filter = "numéro 1".into();
        tab.apply_filter(&shared);
        tab.select_commit(&shared, oid(1500), true);
        tab.history.selected = Some(oid(15));
        tab.history.scroll_to = None;
        until_filtered(&mut tab, &mut shared, &events);
        assert_eq!(tab.history.scroll_to, None);
    }

    /// Une tâche du filtre qui panique (ou ne démarre pas) répond quand même : le calcul n'est plus attendu, le
    /// résultat précédent reste affiché.
    #[test]
    fn a_failed_filter_job_does_not_leave_the_filter_pending() {
        let (mut tab, mut shared, _events, _dir) = tab_with(100);
        tab.history.filter = "numéro 4".into();
        tab.apply_filter(&shared);
        assert!(tab.history.filter_pending);
        tab.handle_event(RepoEvent::Filtered { generation: tab.gens.filter, rows: None }, &mut shared);
        assert!(!tab.history.filter_pending, "plus de calcul attendu");
        assert!(shared.toasts.has_error(), "l'échec est signalé");
    }

    #[test]
    fn a_git_search_keeps_the_rows_of_the_commits_it_found() {
        let list = commits(10);
        let found: HashSet<Oid> = [oid(8), oid(3), oid(42)].into_iter().collect();
        let rows = filter_rows(&list, &FilterQuery::Oids(Arc::new(found)), &CancelToken::new());
        assert_eq!(rows, Some(vec![3, 8]));
        let cancelled = CancelToken::new();
        cancelled.cancel();
        assert_eq!(filter_rows(&list, &FilterQuery::Text("numero".into()), &cancelled), None);
    }
}
