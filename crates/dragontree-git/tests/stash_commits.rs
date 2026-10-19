//! Lecture des commits de stashs sans parcourir leur historique.

mod common;

use common::TestRepo;
use dragontree_git::{CancelToken, Repo, fold_stashes};

#[test]
fn stash_commits_are_read_alone_and_fold_above_their_base() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    t.commit_all("base");
    t.write("f", "1\n");
    t.write("neuf", "x\n");
    t.run(&["stash", "push", "-q", "-u", "-m", "premier"]);
    t.write("f", "2\n");
    t.run(&["stash", "push", "-q", "-m", "second"]);
    t.write("f", "3\n");
    t.commit_all("suite");

    let repo = Repo::open(&t.git, t.path()).unwrap();
    let oids: Vec<_> = repo.stashes().unwrap().iter().map(|s| s.oid).collect();
    let stashes = repo.commits_of(&oids, &CancelToken::new()).unwrap();
    // Le plus récent d'abord, sans « base » ni commits d'index : seulement les commits demandés.
    assert_eq!(stashes.iter().map(|c| &*c.summary).collect::<Vec<_>>(), ["On main: second", "On main: premier"]);
    assert!(repo.commits_of(&[], &CancelToken::new()).unwrap().is_empty());

    let log = repo.log(&Default::default(), &CancelToken::new()).unwrap();
    assert_eq!(log.len(), 2, "le journal des branches ne contient aucun stash");
    let folded = fold_stashes(log, &stashes);
    assert_eq!(
        folded.iter().map(|c| &*c.summary).collect::<Vec<_>>(),
        ["suite", "On main: second", "On main: premier", "base"],
        "chaque stash juste au-dessus de sa base (« base »), sans index ni non suivis"
    );
}
