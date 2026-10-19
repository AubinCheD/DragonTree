use std::collections::{HashMap, HashSet};

use crate::log::Commit;
use crate::oid::Oid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    pub index: usize,
    pub oid: Oid,
    pub message: String,
    pub time: i64,
}

pub const STASH_FORMAT: &str = "%H%x1f%gd%x1f%ct%x1f%gs";

/// Parse `git stash list -z --format=STASH_FORMAT`.
pub fn parse_stash_list(out: &str) -> Vec<StashEntry> {
    out.split('\0')
        .map(|r| r.trim_start_matches('\n'))
        .filter(|r| !r.trim().is_empty())
        .filter_map(|r| {
            let f: Vec<&str> = r.splitn(4, '\x1f').collect();
            if f.len() != 4 {
                return None;
            }
            let index = f[1].rsplit_once("@{")?.1.strip_suffix('}')?.parse().ok()?;
            Some(StashEntry {
                index,
                oid: Oid::from_hex(f[0].trim())?,
                message: f[3].trim_end().to_string(),
                time: f[2].parse().unwrap_or(0),
            })
        })
        .collect()
}

/// Insère les stashs dans la liste de commits d'un journal, chacun juste au-dessus de sa base. `stashes` : les commits
/// des stashs (du plus récent au plus ancien) ; un stash est un commit de fusion technique (parent 1 : la base,
/// parent 2 : l'index, parent 3 éventuel : les fichiers non suivis). Seul le premier parent est gardé, et les commits
/// d'index et de non suivis ne sont jamais ajoutés. Un stash dont la base n'est pas dans la liste (branche masquée,
/// filtre, lot pas encore chargé) n'apparaît pas : il ne doit pas ramener d'historique que la vue exclut.
pub fn fold_stashes(commits: Vec<Commit>, stashes: &[Commit]) -> Vec<Commit> {
    if stashes.is_empty() {
        return commits;
    }
    let listed: HashSet<Oid> = commits.iter().map(|c| c.id).collect();
    let mut above: HashMap<Oid, Vec<Commit>> = HashMap::new();
    for stash in stashes {
        let Some(&base) = stash.parents.first() else { continue };
        if listed.contains(&base) && !listed.contains(&stash.id) {
            let mut row = stash.clone();
            row.parents = [base].into();
            above.entry(base).or_default().push(row);
        }
    }
    let mut out = Vec::with_capacity(commits.len() + above.len());
    for commit in commits {
        if let Some(rows) = above.remove(&commit.id) {
            out.extend(rows);
        }
        out.push(commit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stash_list_parses_entries() {
        let a = "1111111111111111111111111111111111111111";
        let b = "2222222222222222222222222222222222222222";
        let out = format!(
            "{a}\x1fstash@{{0}}\x1f1700000000\x1fOn main: mon message\0{b}\x1fstash@{{1}}\x1f1690000000\x1fWIP on feature/x: abc1234 sujet\0"
        );
        let v = parse_stash_list(&out);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].index, 0);
        assert_eq!(v[0].message, "On main: mon message");
        assert_eq!(v[0].time, 1_700_000_000);
        assert_eq!(v[1].index, 1);
        assert_eq!(v[1].oid, Oid::from_hex(b).unwrap());
    }

    fn oid(n: u8) -> Oid {
        Oid::from_hex(&format!("{n:x}").repeat(40)).unwrap()
    }

    fn commit(id: Oid, parents: &[Oid], summary: &str) -> Commit {
        Commit {
            id,
            parents: parents.into(),
            author: "a".into(),
            author_email: "a@x.fr".into(),
            author_time: 0,
            committer: "a".into(),
            committer_time: 0,
            summary: summary.into(),
        }
    }

    #[test]
    fn fold_puts_the_stash_just_above_its_base_with_its_first_parent_only() {
        let (stash, index, untracked, tip, base, root) = (oid(1), oid(2), oid(3), oid(6), oid(4), oid(5));
        let commits = vec![commit(tip, &[base], "pointe"), commit(base, &[root], "base"), commit(root, &[], "racine")];
        let stashes = [commit(stash, &[base, index, untracked], "On main: mon stash")];
        let folded = fold_stashes(commits, &stashes);
        let summaries: Vec<&str> = folded.iter().map(|c| &*c.summary).collect();
        assert_eq!(summaries, ["pointe", "On main: mon stash", "base", "racine"]);
        assert_eq!(&*folded[1].parents, &[base], "premier parent seulement");
        assert_eq!(&*folded[2].parents, &[root], "les autres commits ne changent pas");
    }

    #[test]
    fn fold_keeps_the_stash_order_on_a_shared_base_and_handles_several_bases() {
        let (s0, s1, s2, a, b) = (oid(1), oid(2), oid(3), oid(4), oid(5));
        let commits = vec![commit(a, &[b], "a"), commit(b, &[], "b")];
        let stashes = [commit(s0, &[b, oid(7)], "stash 0"), commit(s1, &[a], "stash 1"), commit(s2, &[b], "stash 2")];
        let folded = fold_stashes(commits, &stashes);
        assert_eq!(folded.iter().map(|c| c.id).collect::<Vec<_>>(), [s1, a, s0, s2, b]);
    }

    #[test]
    fn fold_leaves_out_a_stash_whose_base_is_not_in_the_list() {
        // Base masquée (V4/V5), filtrée ou pas encore chargée : le stash n'ajoute rien et rien ne revient.
        let (a, b) = (oid(1), oid(2));
        let commits = vec![commit(a, &[b], "fusion"), commit(b, &[], "racine")];
        let stashes = [commit(oid(8), &[oid(9)], "stash d'une branche masquée")];
        assert_eq!(fold_stashes(commits.clone(), &stashes), commits);
        assert_eq!(fold_stashes(commits.clone(), &[]), commits);
    }
}
