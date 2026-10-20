//! Recherche de commits par contenu des modifications (`-G`) et par chemin, sur de vrais dépôts.

mod common;

use std::collections::HashSet;

use common::TestRepo;
use dragontree_git::{CancelToken, CommitSearch, GitError, LogOptions, LogScope, Oid, RefSelection, Repo};

fn oid(hex: &str) -> Oid {
    Oid::from_hex(hex.trim()).unwrap()
}

fn search(t: &TestRepo, opts: &LogOptions, search: &CommitSearch) -> Result<HashSet<Oid>, GitError> {
    Repo::open(&t.git, t.path()).unwrap().search_commits(opts, search, &CancelToken::new())
}

/// Trois commits : le premier crée `src/a.rs`, le deuxième y ajoute `needle`, le troisième crée `docs/b.md`.
fn sample() -> (TestRepo, [Oid; 3]) {
    let t = TestRepo::new();
    t.write("src/a.rs", "fn a() {}\n");
    let first = oid(&t.commit_all("Premier"));
    t.write("src/a.rs", "fn a() {}\nlet needle = 1;\n");
    let second = oid(&t.commit_all("Deuxième"));
    t.write("docs/b.md", "texte\n");
    let third = oid(&t.commit_all("Troisième"));
    (t, [first, second, third])
}

#[test]
fn content_search_finds_the_commit_that_changed_the_text() {
    let (t, [_, second, _]) = sample();
    let found = search(&t, &LogOptions::default(), &CommitSearch::Content("needle".into())).unwrap();
    assert_eq!(found, HashSet::from([second]));
    // Une expression régulière : `-G` n'est pas un texte littéral.
    let found = search(&t, &LogOptions::default(), &CommitSearch::Content("nee+dle = [0-9]".into())).unwrap();
    assert_eq!(found, HashSet::from([second]));
    let none = search(&t, &LogOptions::default(), &CommitSearch::Content("introuvable".into())).unwrap();
    assert!(none.is_empty());
}

#[test]
fn invalid_regex_is_reported_with_the_message_of_git() {
    let (t, _) = sample();
    let err = search(&t, &LogOptions::default(), &CommitSearch::Content("(".into())).unwrap_err();
    assert!(matches!(err, GitError::Failed { .. }), "{err:?}");
    assert!(err.to_string().contains("Unmatched"), "{err}");
}

#[test]
fn path_search_takes_paths_and_globs() {
    let (t, [first, second, third]) = sample();
    let opts = LogOptions::default();
    let by_dir = search(&t, &opts, &CommitSearch::Paths(vec!["src".into()])).unwrap();
    assert_eq!(by_dir, HashSet::from([first, second]));
    let by_glob = search(&t, &opts, &CommitSearch::Paths(vec!["*.md".into()])).unwrap();
    assert_eq!(by_glob, HashSet::from([third]));
    let several = search(&t, &opts, &CommitSearch::Paths(vec!["docs".into(), "src/a.rs".into()])).unwrap();
    assert_eq!(several.len(), 3);
}

#[test]
fn a_path_that_looks_like_an_option_is_only_a_path() {
    let (t, _) = sample();
    let found = search(&t, &LogOptions::default(), &CommitSearch::Paths(vec!["--all".into()])).unwrap();
    assert!(found.is_empty());
}

#[test]
fn search_stays_inside_the_scope() {
    let (t, [_, second, _]) = sample();
    t.run(&["switch", "-q", "-c", "autre"]);
    t.write("src/c.rs", "let needle = 2;\n");
    let on_other = oid(&t.commit_all("Autre"));
    t.run(&["switch", "-q", "-"]);
    let head = LogOptions { scope: LogScope::Head, ..LogOptions::default() };
    let found = search(&t, &head, &CommitSearch::Content("needle".into())).unwrap();
    assert_eq!(found, HashSet::from([second]));
    let all = search(&t, &LogOptions::default(), &CommitSearch::Content("needle".into())).unwrap();
    assert_eq!(all, HashSet::from([second, on_other]));
    // Une révision supplémentaire par l'entrée standard, comme les commits perdus du graphe.
    let extra = RefSelection { head: true, extra: vec![on_other.to_hex()], ..RefSelection::default() };
    let with_extra = LogOptions {
        scope: LogScope::Refs(RefSelection {
            tags: false,
            remotes: false,
            only: vec!["refs/heads/main".into()],
            ..extra
        }),
        ..LogOptions::default()
    };
    let found = search(&t, &with_extra, &CommitSearch::Content("needle".into())).unwrap();
    assert!(found.contains(&on_other) && found.contains(&second));
}

#[test]
fn search_in_a_repository_without_commit_is_empty() {
    let t = TestRepo::new();
    let found = search(&t, &LogOptions::default(), &CommitSearch::Content("x".into())).unwrap();
    assert!(found.is_empty());
}
