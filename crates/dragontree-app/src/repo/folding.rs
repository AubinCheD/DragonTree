//! Repli des fusions de l'historique : état de l'onglet, liste affichée recalculée en arrière-plan à partir du
//! journal chargé (`dragontree_git::collapse_merges`), sélection et recherche gardées cohérentes.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dragontree_git::Oid;

use super::{CommitList, RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};

/// Fusions repliées d'un onglet, par oid : elles le restent quand le journal est relu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeFolding {
    /// Toutes les fusions sont repliées, sauf celles de `toggled` ; sinon, seules celles de `toggled` le sont.
    all: bool,
    toggled: HashSet<Oid>,
}

impl MergeFolding {
    /// Toutes les fusions repliées, y compris celles des commits chargés plus tard.
    pub fn all() -> MergeFolding {
        MergeFolding { all: true, toggled: HashSet::new() }
    }

    pub fn is_collapsed(&self, merge: Oid) -> bool {
        self.all != self.toggled.contains(&merge)
    }

    pub fn set(&mut self, merge: Oid, collapsed: bool) {
        if self.is_collapsed(merge) != collapsed && !self.toggled.remove(&merge) {
            self.toggled.insert(merge);
        }
    }

    /// Aucune fusion n'est repliée.
    pub fn is_none(&self) -> bool {
        !self.all && self.toggled.is_empty()
    }

    /// Toutes les fusions sont repliées.
    pub fn is_all(&self) -> bool {
        self.all && self.toggled.is_empty()
    }
}

impl RepoTab {
    /// Recalcule la liste affichée à partir du journal chargé : immédiatement s'il n'y a rien à replier, sinon en
    /// arrière-plan (avec génération) ; la liste précédente reste affichée en attendant.
    pub(super) fn refold(&mut self, shared: &Shared, follow_selection: bool) {
        self.gens.folding += 1;
        if self.history.folding.is_none() {
            let list = self.history.loaded.clone();
            self.show_list(shared, list, HashMap::new(), &HashMap::new(), follow_selection);
            return;
        }
        let (generation, id) = (self.gens.folding, self.id);
        let (loaded, folding) = (self.history.loaded.clone(), self.history.folding.clone());
        let roots = self.fold_roots();
        shared.jobs.spawn("folding", move || {
            let view =
                dragontree_git::collapse_merges(&loaded.commits, &loaded.index, |m| folding.is_collapsed(m), &roots);
            Some(AppEvent::Repo(id, RepoEvent::Folded { generation, view: Box::new(view), follow_selection }))
        });
    }

    /// Commits jamais masqués par une fusion repliée : HEAD seulement (branche fusionnée sur le serveur puis
    /// récupérée, sur laquelle on travaille encore). Comme une extrémité, il garde aussi ses ancêtres affichés. Les
    /// autres étiquettes (branches fusionnées encore présentes, `origin/x`…) sont repliées avec leur fusion : c'est le
    /// but du repli. Relu à chaque calcul : un HEAD qui change relit le journal, donc refait le repli.
    fn fold_roots(&self) -> HashSet<Oid> {
        self.refs.as_ref().and_then(|r| r.head_oid).into_iter().collect()
    }

    /// Replie (`true`) ou déplie la fusion `merge`.
    pub(super) fn set_merge_collapsed(&mut self, shared: &Shared, merge: Oid, collapsed: bool) {
        if self.history.folding.is_collapsed(merge) != collapsed {
            self.history.folding.set(merge, collapsed);
            self.refold(shared, false);
        }
    }

    /// Replie toutes les fusions (« Replier toutes les fusions »).
    pub(super) fn collapse_all_merges(&mut self, shared: &Shared) {
        self.history.folding = MergeFolding::all();
        self.refold(shared, true);
    }

    /// Déplie toutes les fusions : le journal chargé est réaffiché tel quel, sans calcul.
    pub(super) fn expand_all_merges(&mut self, shared: &Shared) {
        self.history.folding = MergeFolding::default();
        self.refold(shared, true);
    }

    /// Remplace la liste affichée. La recherche porte sur la nouvelle liste ; un commit sélectionné qui vient d'être
    /// masqué cède la sélection à la fusion repliée qui le contient (`hidden_in`) ; avec `follow_selection` (repli ou
    /// dépli de toutes les fusions, qui déplace aussi les lignes au-dessus d'elle), la sélection reste dans la vue.
    pub(super) fn show_list(
        &mut self,
        shared: &Shared,
        list: CommitList,
        folded: HashMap<Oid, usize>,
        hidden_in: &HashMap<Oid, Oid>,
        follow_selection: bool,
    ) {
        let selected = self.history.selected;
        let row_before = selected.and_then(|oid| self.history.index.get(&oid).copied());
        self.history.commits = list.commits;
        self.history.graph = list.graph;
        self.history.index = list.index;
        self.history.folded = Arc::new(folded);
        self.history.filter_applied.clear();
        self.history.filtered = None;
        self.apply_filter();
        let Some(oid) = selected else { return };
        match self.history.index.get(&oid).copied() {
            Some(row) if follow_selection && Some(row) != row_before => self.scroll_to_commit(oid),
            Some(_) => {}
            None => {
                if let Some(&merge) = hidden_in.get(&oid) {
                    self.select_commit(shared, merge, true);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(n: u8) -> Oid {
        Oid([n; 20])
    }

    #[test]
    fn merges_are_collapsed_one_by_one_or_all_at_once() {
        let mut folding = MergeFolding::default();
        assert!(folding.is_none() && !folding.is_collapsed(oid(1)));
        folding.set(oid(1), true);
        assert!(folding.is_collapsed(oid(1)) && !folding.is_collapsed(oid(2)));
        folding.set(oid(1), true);
        folding.set(oid(1), false);
        assert!(folding.is_none(), "replier puis déplier : rien n'est replié");

        // Tout replier vaut aussi pour les fusions chargées plus tard ; une fusion dépliée ensuite est l'exception.
        let mut all = MergeFolding::all();
        assert!(all.is_all() && all.is_collapsed(oid(9)));
        all.set(oid(2), false);
        assert!(!all.is_collapsed(oid(2)) && all.is_collapsed(oid(3)));
        assert!(!all.is_all() && !all.is_none());
        all.set(oid(2), true);
        assert!(all.is_all());
    }

    /// Fait arriver les événements des tâches de l'onglet jusqu'à ce que `done` soit vrai (au plus 20 s).
    fn pump(
        tab: &mut RepoTab,
        shared: &mut Shared,
        events: &std::sync::mpsc::Receiver<AppEvent>,
        what: &str,
        done: impl Fn(&RepoTab) -> bool,
    ) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !done(tab) {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match events.recv_timeout(left) {
                Ok(AppEvent::Repo(_, ev)) => tab.handle_event(ev, shared),
                Ok(_) => {}
                Err(_) => panic!("délai dépassé en attendant : {what}"),
            }
        }
    }

    /// Le commit de message `summary` figure dans la liste affichée.
    fn shown(tab: &RepoTab, summary: &str) -> bool {
        tab.history.commits.iter().any(|c| &*c.summary == summary)
    }

    #[test]
    fn a_fold_requested_before_the_first_refs_spares_head_once_they_arrive() {
        use dragontree_git::{Git, Repo};

        use crate::settings::SettingsFile;

        // HEAD sur `feature`, que main a fusionnée ; `ancienne`, fusionnée aussi, existe encore.
        let dir = tempfile::tempdir().unwrap();
        let git = Git::discover().unwrap();
        let run = |args: &[&str]| {
            let identity = ["-c", "user.name=Testeur", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"];
            git.cmd(dir.path()).args(identity).args(args).run().unwrap_or_else(|e| panic!("git {args:?} : {e}"));
        };
        let commit = |file: &str, msg: &str| {
            std::fs::write(dir.path().join(file), msg).unwrap();
            run(&["add", "-A"]);
            run(&["commit", "-q", "--no-verify", "-m", msg]);
        };
        run(&["init", "-q", "-b", "main"]);
        commit("base.txt", "Base");
        run(&["checkout", "-q", "-b", "ancienne"]);
        commit("a.txt", "Ancien travail");
        run(&["checkout", "-q", "main"]);
        run(&["merge", "-q", "--no-ff", "--no-verify", "-m", "Fusion d'ancienne", "ancienne"]);
        run(&["checkout", "-q", "-b", "feature"]);
        commit("f1.txt", "Travail 1");
        commit("f2.txt", "Travail 2");
        run(&["checkout", "-q", "main"]);
        commit("main.txt", "Suite de main");
        run(&["merge", "-q", "--no-ff", "--no-verify", "-m", "Fusion de feature", "feature"]);
        run(&["checkout", "-q", "feature"]);

        let (store, settings, _) = SettingsFile::load(&dir.path().join("settings.json"));
        let (mut shared, events) = Shared::new(egui::Context::default(), settings, store, None, None);
        let mut tab = RepoTab::new(1, dir.path().to_path_buf(), &mut shared);
        let repo = Arc::new(Repo::open(&git, dir.path()).unwrap());
        tab.repo = Some(repo.clone());
        tab.load_log(&shared);
        pump(&mut tab, &mut shared, &events, "journal", |t| !t.history.loading && shown(t, "Travail 2"));

        // Tout replier avant la lecture des références : HEAD inconnu, sa branche est repliée avec les autres.
        tab.collapse_all_merges(&shared);
        pump(&mut tab, &mut shared, &events, "repli", |t| !shown(t, "Ancien travail"));
        assert!(!shown(&tab, "Travail 2"), "sans HEAD connu, rien n'est épargné");

        // Les références arrivent : le repli est refait, HEAD et sa branche réapparaissent ; `ancienne` reste repliée.
        tab.handle_event(RepoEvent::Refs(tab.gens.refs, Ok(repo.refs().unwrap())), &mut shared);
        pump(&mut tab, &mut shared, &events, "HEAD épargné", |t| shown(t, "Travail 2") && shown(t, "Travail 1"));
        assert!(!shown(&tab, "Ancien travail"));

        // Les relectures suivantes ne le refont pas (un HEAD qui change relit le journal, qui refait le repli).
        let after = tab.gens.folding;
        tab.handle_event(RepoEvent::Refs(tab.gens.refs, Ok(repo.refs().unwrap())), &mut shared);
        assert_eq!(tab.gens.folding, after);
    }
}
