//! Repli des fusions de l'historique : les commits chargés qu'une fusion repliée masque, et la liste affichée
//! (commits, graphe, index) sans eux.
//!
//! Un commit reste affiché s'il est une extrémité de la liste (aucun enfant chargé), une racine imposée (HEAD) ou le
//! parent d'un commit affiché ; une fusion repliée ne compte que son premier parent. Une fusion repliée masque donc
//! les commits que seuls ses parents 2+ apportent : ni son premier parent, ni une autre ligne affichée (branche qui
//! continue après la fusion, HEAD encore sur la branche fusionnée, fusion de la branche principale dans l'autre sens,
//! autre fusion dépliée) ne les atteint. Les étiquettes de références (branche fusionnée encore présente, `origin/x`)
//! sont repliées avec leurs commits. Aucun commit affiché n'a ainsi de parent masqué : le graphe se reconstruit sur
//! la liste affichée sans réécrire de lien.

use std::collections::{HashMap, HashSet};

use crate::graph::{GraphBuilder, GraphLayout};
use crate::log::Commit;
use crate::oid::Oid;

/// Liste affichée d'un journal dont des fusions sont repliées.
#[derive(Debug, Clone, Default)]
pub struct CollapsedLog {
    /// Commits affichés, dans l'ordre de la liste complète (avec leurs vrais parents).
    pub commits: Vec<Commit>,
    /// Graphe des commits affichés : une fusion repliée n'y garde que son premier parent.
    pub graph: GraphLayout,
    /// oid → ligne de `commits`.
    pub index: HashMap<Oid, usize>,
    /// Fusion repliée affichée → nombre de commits masqués derrière elle : ceux que ses parents 2+ atteignent sans
    /// passer par un commit affiché (y compris ceux d'une fusion repliée qu'elle contient).
    pub folded: HashMap<Oid, usize>,
    /// Commit masqué → fusion repliée affichée qui le contient (la première dans l'ordre de la liste).
    pub hidden_in: HashMap<Oid, Oid>,
}

/// Replie les fusions de `commits` pour lesquelles `collapsed` est vrai.
///
/// `commits` : liste complète, enfants avant parents (ordre de `git log --date-order` ou `--topo-order`) ;
/// `index` : oid → ligne de `commits`. Les parents absents de la liste sont ignorés (liens ouverts du graphe).
/// `roots` : commits toujours affichés comme une extrémité, même si seule une fusion repliée les atteint (HEAD dans
/// l'application) ; leurs ancêtres suivent la règle commune.
pub fn collapse_merges(
    commits: &[Commit],
    index: &HashMap<Oid, usize>,
    collapsed: impl Fn(Oid) -> bool,
    roots: &HashSet<Oid>,
) -> CollapsedLog {
    const HAS_CHILD: u8 = 1;
    const REACHED: u8 = 2;
    const SHOWN: u8 = 4;
    const FOLDED: u8 = 8;
    // Lignes des parents chargés, calculées une fois (la suite les parcourt plusieurs fois).
    let parents: Vec<Vec<u32>> =
        commits.iter().map(|c| c.parents.iter().filter_map(|p| index.get(p).map(|&r| r as u32)).collect()).collect();
    let first_parent = |row: usize| commits[row].parents.first().and_then(|p| index.get(p)).map(|&r| r as u32);

    // Un seul passage : les enfants précèdent leurs parents, l'état d'une ligne est donc connu quand on l'atteint.
    let mut flags = vec![0u8; commits.len()];
    for (row, c) in commits.iter().enumerate() {
        let shown = flags[row] & HAS_CHILD == 0 || flags[row] & REACHED != 0 || roots.contains(&c.id);
        let folded = shown && c.parents.len() > 1 && collapsed(c.id);
        if shown {
            flags[row] |= SHOWN | if folded { FOLDED } else { 0 };
        }
        let first = first_parent(row);
        for &p in &parents[row] {
            let followed = shown && (!folded || Some(p) == first);
            flags[p as usize] |= HAS_CHILD | if followed { REACHED } else { 0 };
        }
    }
    let is_shown = |row: u32| flags[row as usize] & SHOWN != 0;

    // Commits masqués derrière chaque fusion repliée affichée : parcours des commits masqués depuis ses parents 2+.
    // `owner` : première fusion (dans l'ordre de la liste) qui atteint le commit ; `seen` : marque du parcours en
    // cours (la ligne de la fusion), pour ne compter qu'une fois un commit atteint par plusieurs chemins.
    let mut folded = HashMap::new();
    let mut owner = vec![u32::MAX; commits.len()];
    let mut seen = vec![u32::MAX; commits.len()];
    let mut stack = Vec::new();
    for (row, c) in commits.iter().enumerate().filter(|(row, _)| flags[*row] & FOLDED != 0) {
        let mark = row as u32;
        let first = first_parent(row);
        for &p in &parents[row] {
            if Some(p) != first && !is_shown(p) && seen[p as usize] != mark {
                seen[p as usize] = mark;
                stack.push(p);
            }
        }
        let mut hidden = 0;
        while let Some(r) = stack.pop() {
            hidden += 1;
            if owner[r as usize] == u32::MAX {
                owner[r as usize] = mark;
            }
            for &p in &parents[r as usize] {
                if !is_shown(p) && seen[p as usize] != mark {
                    seen[p as usize] = mark;
                    stack.push(p);
                }
            }
        }
        folded.insert(c.id, hidden);
    }

    let mut out = CollapsedLog { folded, ..CollapsedLog::default() };
    let mut graph = GraphBuilder::new();
    for (row, c) in commits.iter().enumerate() {
        if !is_shown(row as u32) {
            if let Some(&m) = owner.get(row).filter(|&&m| m != u32::MAX) {
                out.hidden_in.insert(c.id, commits[m as usize].id);
            }
            continue;
        }
        let graph_parents = if flags[row] & FOLDED != 0 { &c.parents[..1] } else { &c.parents[..] };
        graph.push(c.id, graph_parents);
        out.index.insert(c.id, out.commits.len());
        out.commits.push(c.clone());
    }
    out.graph = graph.finish();
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::graph::UNKNOWN_ROW;

    fn oid(n: u32) -> Oid {
        let mut b = [0u8; 20];
        b[..4].copy_from_slice(&n.to_be_bytes());
        b[19] = 1;
        Oid(b)
    }

    fn commit(id: u32, parents: &[u32]) -> Commit {
        Commit {
            id: oid(id),
            parents: parents.iter().map(|p| oid(*p)).collect(),
            author: "a".into(),
            author_email: "a@contoso.com".into(),
            author_time: 0,
            committer: "a".into(),
            committer_time: 0,
            summary: format!("commit {id}").into(),
        }
    }

    /// Liste (enfants d'abord) et son index.
    fn log(list: &[(u32, &[u32])]) -> (Vec<Commit>, HashMap<Oid, usize>) {
        let commits: Vec<Commit> = list.iter().map(|(id, parents)| commit(*id, parents)).collect();
        let index = commits.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
        (commits, index)
    }

    fn fold(list: &[(u32, &[u32])], collapsed: &[u32]) -> CollapsedLog {
        fold_with_roots(list, collapsed, &[])
    }

    /// Comme `fold`, avec des racines imposées (HEAD).
    fn fold_with_roots(list: &[(u32, &[u32])], collapsed: &[u32], roots: &[u32]) -> CollapsedLog {
        let (commits, index) = log(list);
        let set: HashSet<Oid> = collapsed.iter().map(|n| oid(*n)).collect();
        let roots: HashSet<Oid> = roots.iter().map(|n| oid(*n)).collect();
        collapse_merges(&commits, &index, |m| set.contains(&m), &roots)
    }

    /// Commits affichés (numéros).
    fn shown(view: &CollapsedLog) -> Vec<u32> {
        view.commits.iter().map(|c| u32::from_be_bytes(c.id.0[..4].try_into().unwrap())).collect()
    }

    fn count(view: &CollapsedLog, merge: u32) -> Option<usize> {
        view.folded.get(&oid(merge)).copied()
    }

    /// Le graphe et l'index décrivent exactement la liste affichée : aucun lien vers un commit masqué.
    fn assert_coherent(view: &CollapsedLog) {
        assert_eq!(view.graph.rows.len(), view.commits.len());
        for (i, c) in view.commits.iter().enumerate() {
            assert_eq!(view.index.get(&c.id), Some(&i));
        }
        assert_eq!(view.index.len(), view.commits.len());
        for e in &view.graph.edges {
            assert!(e.parent_row != UNKNOWN_ROW, "lien vers un commit absent de la liste affichée : {e:?}");
        }
    }

    // Fusion simple : M(B, F2) ; la branche F1 → F2 part de A.
    const SIMPLE: &[(u32, &[u32])] = &[(10, &[3, 12]), (12, &[11]), (3, &[1]), (11, &[1]), (1, &[])];

    #[test]
    fn a_collapsed_merge_hides_the_commits_of_the_merged_branch() {
        let view = fold(SIMPLE, &[10]);
        assert_eq!(shown(&view), [10, 3, 1]);
        assert_eq!(count(&view, 10), Some(2));
        assert_eq!(view.hidden_in.get(&oid(12)), Some(&oid(10)));
        assert_eq!(view.hidden_in.get(&oid(11)), Some(&oid(10)));
        assert_coherent(&view);
        // La fusion repliée ne garde que son premier parent : deux liens, M → B → A.
        assert_eq!(view.graph.edges.len(), 2);
        assert!(!view.graph.rows[0].is_merge);

        let open = fold(SIMPLE, &[]);
        assert_eq!(shown(&open), [10, 12, 3, 11, 1]);
        assert!(open.folded.is_empty() && open.hidden_in.is_empty());
        assert!(open.graph.rows[0].is_merge);
        // Un commit ordinaire « replié » ne masque rien.
        assert_eq!(shown(&fold(SIMPLE, &[12, 3])), [10, 12, 3, 11, 1]);
    }

    // Fusions imbriquées : M0(B, M1) fusionne la branche F1 → F2 → M1, où M1(F2, S1) fusionne S1 (parti de F1).
    const NESTED: &[(u32, &[u32])] =
        &[(20, &[2, 15]), (15, &[12, 13]), (13, &[11]), (12, &[11]), (2, &[1]), (11, &[1]), (1, &[])];

    #[test]
    fn nested_merges_hide_their_own_branches() {
        let inner = fold(NESTED, &[15]);
        assert_eq!(shown(&inner), [20, 15, 12, 2, 11, 1], "M1 repliée : seule S1 disparaît");
        assert_eq!(count(&inner, 15), Some(1));
        assert_coherent(&inner);

        let outer = fold(NESTED, &[20]);
        assert_eq!(shown(&outer), [20, 2, 1], "M0 repliée : toute sa branche, fusion interne comprise");
        assert_eq!(count(&outer, 20), Some(4));
        assert_eq!(view_owner(&outer, 13), Some(20));
        assert_coherent(&outer);

        // Les deux repliées : comme M0 seule ; M1, masquée, n'a pas de compte.
        let both = fold(NESTED, &[20, 15]);
        assert_eq!(shown(&both), [20, 2, 1]);
        assert_eq!(both.folded.len(), 1);
        assert_eq!(count(&both, 20), Some(4));
        assert_eq!(view_owner(&both, 13), Some(20), "une fusion masquée ne reçoit pas la sélection");
    }

    #[test]
    fn head_on_a_merged_branch_stays_shown_while_other_labelled_tips_fold() {
        // HEAD sur F2, la pointe de la branche que M fusionne : F2 est une racine comme une extrémité, F1 suit (parent
        // d'un commit affiché) ; M ne perd que son lien de fusion.
        let view = fold_with_roots(SIMPLE, &[10], &[12]);
        assert_eq!(shown(&view), [10, 12, 3, 11, 1]);
        assert_eq!(count(&view, 10), Some(0));
        assert_coherent(&view);
        assert_eq!(view.graph.edges.len(), 4, "M → B → A, F2 → F1 → A : pas de lien de fusion");

        // Deux branches fusionnées dans main : X1 par M1, Y1 par M2. HEAD est sur Y1 (racine) ; X1 porte encore les
        // étiquettes `x` et `origin/x`, qui ne sont pas des racines : il est replié avec sa fusion.
        let list: &[(u32, &[u32])] = &[(7, &[5, 21]), (21, &[1]), (5, &[1, 11]), (11, &[1]), (1, &[])];
        let view = fold_with_roots(list, &[7, 5], &[21]);
        assert_eq!(shown(&view), [7, 21, 5, 1], "HEAD reste, la pointe étiquetée X1 est repliée");
        assert_eq!((count(&view, 7), count(&view, 5)), (Some(0), Some(1)));
        assert_eq!(view_owner(&view, 11), Some(5));
        assert_coherent(&view);
        assert_eq!(shown(&fold_with_roots(list, &[7, 5], &[])), [7, 5, 1], "sans racine, HEAD serait replié aussi");

        // HEAD au milieu de la branche fusionnée (F1 → F2 → F3, HEAD sur F2) : F2 et son ancêtre F1 restent, seul F3
        // (que seule la fusion atteint) est replié.
        let list: &[(u32, &[u32])] = &[(10, &[3, 13]), (13, &[12]), (3, &[1]), (12, &[11]), (11, &[1]), (1, &[])];
        let view = fold_with_roots(list, &[10], &[12]);
        assert_eq!(shown(&view), [10, 3, 12, 11, 1]);
        assert_eq!(count(&view, 10), Some(1));
        assert_eq!(view_owner(&view, 13), Some(10));
        assert_coherent(&view);
        // Une racine qui n'est pas dans la liste ne change rien.
        assert_eq!(shown(&fold_with_roots(SIMPLE, &[10], &[99])), [10, 3, 1]);
    }

    fn view_owner(view: &CollapsedLog, hidden: u32) -> Option<u32> {
        view.hidden_in.get(&oid(hidden)).map(|m| u32::from_be_bytes(m.0[..4].try_into().unwrap()))
    }

    // Branche refusionnée : F1 fusionnée par M1, puis F2 (suite de F1) fusionnée par M2.
    const REMERGED: &[(u32, &[u32])] = &[(7, &[6, 12]), (6, &[5]), (12, &[11]), (5, &[1, 11]), (11, &[1]), (1, &[])];

    #[test]
    fn a_remerged_branch_only_hides_what_each_merge_brings() {
        let last = fold(REMERGED, &[7]);
        assert_eq!(shown(&last), [7, 6, 5, 11, 1], "M2 repliée : F1, déjà fusionnée par M1, reste affichée");
        assert_eq!(count(&last, 7), Some(1));
        assert_coherent(&last);

        // M1 repliée seule : F1 reste atteinte par la branche que M2 fusionne.
        let first = fold(REMERGED, &[5]);
        assert_eq!(shown(&first), [7, 6, 12, 5, 11, 1]);
        assert_eq!(count(&first, 5), Some(0));
        assert_coherent(&first);

        let both = fold(REMERGED, &[7, 5]);
        assert_eq!(shown(&both), [7, 6, 5, 1]);
        assert_eq!((count(&both, 7), count(&both, 5)), (Some(2), Some(1)));
        assert_eq!(view_owner(&both, 11), Some(7), "première fusion repliée dans l'ordre de la liste");
        assert_coherent(&both);
    }

    #[test]
    fn merging_main_into_a_branch_never_hides_main() {
        // Mo(F1, X2) fusionne main dans la branche, Mp(X2, F2) fusionne la branche dans main.
        let list: &[(u32, &[u32])] = &[(20, &[2, 13]), (13, &[12]), (12, &[11, 2]), (2, &[1]), (11, &[1]), (1, &[])];
        let back = fold(list, &[12]);
        assert_eq!(shown(&back), [20, 13, 12, 2, 11, 1], "X2 est sur la ligne de main");
        assert_eq!(count(&back, 12), Some(0));

        let all = fold(list, &[20, 12]);
        assert_eq!(shown(&all), [20, 2, 1]);
        assert_eq!(count(&all, 20), Some(3));
        assert_coherent(&all);
    }

    #[test]
    fn a_branch_that_goes_on_after_the_merge_stays_shown() {
        // D2 continue la branche après sa fusion par M(A, D1).
        let list: &[(u32, &[u32])] = &[(12, &[11]), (5, &[1, 11]), (11, &[1]), (1, &[])];
        let view = fold(list, &[5]);
        assert_eq!(shown(&view), [12, 5, 11, 1]);
        assert_eq!(count(&view, 5), Some(0));
        assert_coherent(&view);
        // Le lien de fusion disparaît quand même : M → A, D2 → D1 → A.
        assert_eq!(view.graph.edges.len(), 3);
    }

    #[test]
    fn a_merge_whose_branch_is_not_loaded_yet_loses_only_its_open_link() {
        // Premier lot du journal : le 2e parent de M n'est pas encore chargé.
        let view = fold(&[(10, &[3, 99]), (3, &[])], &[10]);
        assert_eq!(shown(&view), [10, 3]);
        assert_eq!(count(&view, 10), Some(0));
        assert_coherent(&view);
    }

    /// Journal synthétique d'environ 10 000 commits (enfants d'abord) : 1 000 fusions sur main, chacune d'une
    /// branche de 8 commits ; une branche sur dix reçoit main avant d'être fusionnée. `long_lived` : chaque branche
    /// repart de la précédente (une même branche refusionnée 1 000 fois), sinon de main.
    fn synthetic(long_lived: bool) -> (Vec<Commit>, HashMap<Oid, usize>) {
        let mut next = 0u32;
        let mut new = |list: &mut Vec<Commit>, parents: &[u32]| {
            next += 1;
            list.push(commit(next, parents));
            next
        };
        let mut oldest_first = Vec::new();
        let root = new(&mut oldest_first, &[]);
        let (mut main, mut branch) = (root, root);
        for k in 0..1000 {
            let mut tip = if long_lived { branch } else { main };
            for _ in 0..8 {
                tip = new(&mut oldest_first, &[tip]);
            }
            let main_work = new(&mut oldest_first, &[main]);
            if k % 10 == 0 {
                tip = new(&mut oldest_first, &[tip, main_work]);
            }
            branch = tip;
            main = new(&mut oldest_first, &[main_work, tip]);
        }
        oldest_first.reverse();
        let index = oldest_first.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
        (oldest_first, index)
    }

    #[test]
    fn collapsing_every_merge_of_10_000_commits_takes_well_under_a_second() {
        for long_lived in [false, true] {
            let (commits, index) = synthetic(long_lived);
            assert!(commits.len() > 10_000);
            let start = Instant::now();
            let view = collapse_merges(&commits, &index, |_| true, &HashSet::new());
            let elapsed = start.elapsed();
            eprintln!(
                "repli total de {} commits (branche de longue durée : {long_lived}) : {elapsed:?}",
                commits.len()
            );
            assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
            // Reste la ligne de main : la racine, puis un commit et une fusion par tour.
            assert_eq!(view.commits.len(), 2001);
            assert_eq!(view.folded.len(), 1000);
            assert_coherent(&view);
        }
    }
}
