//! Git Flow natif : initialisation (clés `gitflow.*`), lecture d'une configuration existante, feature, release et
//! hotfix avec leurs fusions `--no-ff` et leurs tags, arrêt sur conflit.

mod common;

use common::TestRepo;
use dragontree_git::{FinishOutcome, FlowKind, FlowStep, GitError, GitFlowConfig, OpState, Repo};

/// Dépôt `main` avec un premier commit, et la configuration par défaut de git-flow sur `main`/`develop`.
fn flow_repo() -> (TestRepo, Repo, GitFlowConfig) {
    let t = TestRepo::new();
    t.write("a.txt", "1\n");
    t.commit_all("Premier commit");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    let cfg = GitFlowConfig::with_production("main");
    repo.gitflow_init(&cfg).unwrap();
    (t, repo, cfg)
}

fn branches(t: &TestRepo) -> Vec<String> {
    t.run(&["branch", "--format=%(refname:short)"]).lines().map(str::to_string).collect()
}

fn current(t: &TestRepo) -> String {
    t.run(&["branch", "--show-current"]).trim().to_string()
}

/// La branche contient le commit `rev`.
fn contains(t: &TestRepo, branch: &str, rev: &str) -> bool {
    t.try_run(&["merge-base", "--is-ancestor", rev, branch]).is_ok()
}

#[test]
fn a_repository_without_gitflow_keys_is_not_initialized() {
    let t = TestRepo::new();
    t.write("a.txt", "1\n");
    t.commit_all("Premier commit");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    assert_eq!(repo.gitflow_config().unwrap(), None);
}

#[test]
fn init_writes_the_standard_gitflow_keys_and_creates_develop() {
    let (t, repo, _) = flow_repo();
    assert!(branches(&t).contains(&"develop".to_string()));
    for (key, value) in [
        ("gitflow.branch.master", "main"),
        ("gitflow.branch.develop", "develop"),
        ("gitflow.prefix.feature", "feature/"),
        ("gitflow.prefix.release", "release/"),
        ("gitflow.prefix.hotfix", "hotfix/"),
        ("gitflow.prefix.support", "support/"),
    ] {
        assert_eq!(t.run(&["config", "--local", "--get", key]).trim(), value, "{key}");
    }
    // La clé du préfixe de tag existe, même vide : git-flow la lit.
    assert!(t.try_run(&["config", "--local", "--get", "gitflow.prefix.versiontag"]).is_ok());
    assert_eq!(repo.gitflow_config().unwrap(), Some(GitFlowConfig::with_production("main")));
}

#[test]
fn an_existing_gitflow_avh_configuration_is_recognized() {
    let t = TestRepo::new();
    t.write("a.txt", "1\n");
    t.commit_all("Premier commit");
    t.run(&["branch", "dev"]);
    // Clés telles que `git flow init` les écrit (préfixes personnalisés comme le préfixe de tag).
    for (key, value) in [
        ("gitflow.branch.master", "main"),
        ("gitflow.branch.develop", "dev"),
        ("gitflow.prefix.feature", "feat-"),
        ("gitflow.prefix.release", "rel-"),
        ("gitflow.prefix.hotfix", "fix-"),
        ("gitflow.prefix.support", "sup-"),
        ("gitflow.prefix.versiontag", "v"),
    ] {
        t.run(&["config", "--local", key, value]);
    }
    let repo = Repo::open(&t.git, t.path()).unwrap();
    let cfg = repo.gitflow_config().unwrap().expect("configuration reconnue");
    assert_eq!((cfg.production.as_str(), cfg.development.as_str()), ("main", "dev"));
    assert_eq!(cfg.branch(FlowKind::Feature, "x"), "feat-x");
    assert_eq!(cfg.version_tag, "v");
    assert_eq!(cfg.classify("fix-1.0.1"), Some((FlowKind::Hotfix, "1.0.1".to_string())));
    assert_eq!(cfg.classify("dev"), None);
}

#[test]
fn init_refuses_a_missing_production_branch_and_identical_branches() {
    let t = TestRepo::new();
    let repo = Repo::open(&t.git, t.path()).unwrap();
    // Aucun commit : la branche de production n'existe pas.
    let err = repo.gitflow_init(&GitFlowConfig::with_production("main")).unwrap_err();
    assert!(matches!(err, GitError::Refused(_)), "{err:?}");
    t.write("a.txt", "1\n");
    t.commit_all("Premier commit");
    let mut same = GitFlowConfig::with_production("main");
    same.development = "main".into();
    assert!(matches!(repo.gitflow_init(&same), Err(GitError::Refused(_))));
    let mut no_prefix = GitFlowConfig::with_production("main");
    no_prefix.feature = String::new();
    assert!(matches!(repo.gitflow_init(&no_prefix), Err(GitError::Refused(_))));
    assert_eq!(repo.gitflow_config().unwrap(), None, "rien n'est écrit quand la validation échoue");
}

#[test]
fn starting_a_feature_branches_from_develop_and_checks_it_out() {
    let (t, repo, cfg) = flow_repo();
    t.run(&["checkout", "-q", "develop"]);
    t.write("d.txt", "d\n");
    let develop_tip = t.commit_all("Sur develop");
    t.run(&["checkout", "-q", "main"]);
    assert_eq!(repo.gitflow_start(&cfg, FlowKind::Feature, "x").unwrap(), "feature/x");
    assert_eq!(current(&t), "feature/x");
    assert_eq!(t.run(&["rev-parse", "HEAD"]).trim(), develop_tip);
    // Un nom déjà pris ou invalide est refusé.
    assert!(matches!(repo.gitflow_start(&cfg, FlowKind::Feature, "x"), Err(GitError::Refused(_))));
    assert!(matches!(repo.gitflow_start(&cfg, FlowKind::Feature, "a b"), Err(GitError::Refused(_))));
}

#[test]
fn finishing_a_feature_merges_it_into_develop_with_a_merge_commit() {
    let (t, repo, cfg) = flow_repo();
    repo.gitflow_start(&cfg, FlowKind::Feature, "x").unwrap();
    t.write("f.txt", "f\n");
    let feature_tip = t.commit_all("Travail de la feature");
    let outcome = repo.gitflow_finish(&cfg, FlowKind::Feature, "x", false).unwrap();
    assert_eq!(outcome, FinishOutcome::Done);
    assert_eq!(current(&t), "develop");
    assert!(contains(&t, "develop", &feature_tip));
    // --no-ff : même sans divergence, un commit de fusion à deux parents.
    assert_eq!(t.run(&["rev-list", "--parents", "-n", "1", "develop"]).split_whitespace().count(), 3);
    assert!(branches(&t).contains(&"feature/x".to_string()), "la branche est gardée sans demande de suppression");
}

#[test]
fn finishing_a_feature_can_delete_the_branch() {
    let (t, repo, cfg) = flow_repo();
    repo.gitflow_start(&cfg, FlowKind::Feature, "x").unwrap();
    t.write("f.txt", "f\n");
    t.commit_all("Travail");
    repo.gitflow_finish(&cfg, FlowKind::Feature, "x", true).unwrap();
    assert!(!branches(&t).contains(&"feature/x".to_string()));
}

#[test]
fn finishing_a_release_tags_master_and_both_branches_contain_it() {
    let (t, repo, cfg) = flow_repo();
    t.run(&["checkout", "-q", "develop"]);
    t.write("d.txt", "d\n");
    t.commit_all("Fonction prête");
    assert_eq!(repo.gitflow_start(&cfg, FlowKind::Release, "1.0").unwrap(), "release/1.0");
    t.write("v.txt", "1.0\n");
    let release_tip = t.commit_all("Version 1.0");
    assert_eq!(repo.gitflow_finish(&cfg, FlowKind::Release, "1.0", true).unwrap(), FinishOutcome::Done);
    assert_eq!(current(&t), "develop");
    assert!(contains(&t, "main", &release_tip));
    assert!(contains(&t, "develop", &release_tip));
    // Tag annoté sur la production, message « Version 1.0 ».
    assert_eq!(t.run(&["cat-file", "-t", "1.0"]).trim(), "tag");
    assert_eq!(t.run(&["tag", "-l", "--format=%(contents:subject)", "1.0"]).trim(), "Version 1.0");
    assert_eq!(t.run(&["rev-parse", "1.0^{commit}"]).trim(), t.run(&["rev-parse", "main"]).trim());
    assert!(!branches(&t).contains(&"release/1.0".to_string()));
}

#[test]
fn the_version_tag_prefix_is_applied() {
    let (t, repo, mut cfg) = flow_repo();
    cfg.version_tag = "v".into();
    repo.gitflow_init(&cfg).unwrap();
    repo.gitflow_start(&cfg, FlowKind::Release, "2.0").unwrap();
    t.write("v.txt", "2\n");
    t.commit_all("Version 2.0");
    repo.gitflow_finish(&cfg, FlowKind::Release, "2.0", false).unwrap();
    assert_eq!(t.run(&["tag", "-l"]).trim(), "v2.0");
}

#[test]
fn a_hotfix_starts_from_master_and_is_merged_into_both_branches() {
    let (t, repo, cfg) = flow_repo();
    t.run(&["checkout", "-q", "develop"]);
    t.write("d.txt", "d\n");
    t.commit_all("Travail en cours sur develop");
    assert_eq!(repo.gitflow_start(&cfg, FlowKind::Hotfix, "1.0.1").unwrap(), "hotfix/1.0.1");
    assert_eq!(t.run(&["rev-parse", "HEAD"]).trim(), t.run(&["rev-parse", "main"]).trim());
    t.write("fix.txt", "fix\n");
    let fix = t.commit_all("Correctif");
    assert_eq!(repo.gitflow_finish(&cfg, FlowKind::Hotfix, "1.0.1", false).unwrap(), FinishOutcome::Done);
    assert!(contains(&t, "main", &fix));
    assert!(contains(&t, "develop", &fix));
    assert_eq!(t.run(&["tag", "-l"]).trim(), "1.0.1");
    assert_eq!(current(&t), "develop");
}

#[test]
fn a_conflict_stops_the_sequence_and_names_what_remains() {
    let (t, repo, cfg) = flow_repo();
    repo.gitflow_start(&cfg, FlowKind::Release, "1.0").unwrap();
    t.write("a.txt", "release\n");
    t.commit_all("Release");
    // La production a changé la même ligne pendant ce temps.
    t.run(&["checkout", "-q", "main"]);
    t.write("a.txt", "production\n");
    t.commit_all("Correctif direct");
    t.run(&["checkout", "-q", "release/1.0"]);
    let outcome = repo.gitflow_finish(&cfg, FlowKind::Release, "1.0", true).unwrap();
    assert_eq!(
        outcome,
        FinishOutcome::Stopped {
            into: "main".into(),
            remaining: vec![
                FlowStep::Tag("1.0".into()),
                FlowStep::MergeInto("develop".into()),
                FlowStep::Delete("release/1.0".into()),
            ],
        }
    );
    // Le dépôt est dans l'état de fusion de git, rien n'a été tagué ni supprimé.
    assert_eq!(repo.operation_state(), OpState::Merge);
    assert_eq!(current(&t), "main");
    assert!(t.run(&["tag", "-l"]).trim().is_empty());
    assert!(branches(&t).contains(&"release/1.0".to_string()));
}

#[test]
fn finishing_is_refused_before_any_change_when_it_cannot_complete() {
    let (t, repo, cfg) = flow_repo();
    // Branche inexistante.
    assert!(matches!(repo.gitflow_finish(&cfg, FlowKind::Feature, "nope", false), Err(GitError::Refused(_))));
    // Tag déjà pris : refus avant la première fusion.
    repo.gitflow_start(&cfg, FlowKind::Release, "1.0").unwrap();
    t.write("v.txt", "1\n");
    t.commit_all("Release");
    t.run(&["tag", "1.0", "main"]);
    let main_before = t.run(&["rev-parse", "main"]);
    assert!(matches!(repo.gitflow_finish(&cfg, FlowKind::Release, "1.0", false), Err(GitError::Refused(_))));
    assert_eq!(t.run(&["rev-parse", "main"]), main_before);
    assert_eq!(current(&t), "release/1.0");
    // Modifications suivies non commitées : refus (git-flow fait de même).
    t.run(&["tag", "-d", "1.0"]);
    t.write("v.txt", "modifié\n");
    assert!(matches!(repo.gitflow_finish(&cfg, FlowKind::Release, "1.0", false), Err(GitError::Refused(_))));
}

#[test]
fn a_permanent_branch_held_by_another_worktree_is_refused_before_any_command() {
    let (t, repo, cfg) = flow_repo();
    repo.gitflow_start(&cfg, FlowKind::Release, "1.0").unwrap();
    t.write(
        "v.txt", "1
",
    );
    t.commit_all("Release");
    // develop est sortie dans un autre worktree : la deuxième fusion échouerait après la production et le tag.
    let other = t.path().join("..").join(format!("{}-other", t.path().file_name().unwrap().to_string_lossy()));
    t.run(&["worktree", "add", other.to_str().unwrap(), "develop"]);
    let (main_before, branches_before) = (t.run(&["rev-parse", "main"]), branches(&t));
    let err = repo.gitflow_finish(&cfg, FlowKind::Release, "1.0", true).unwrap_err();
    assert!(matches!(err, GitError::Refused(_)), "{err:?}");
    assert_eq!(t.run(&["rev-parse", "main"]), main_before, "la production n'a pas bougé");
    assert!(t.run(&["tag", "-l"]).trim().is_empty());
    assert_eq!(branches_before, branches(&t));
    assert_eq!(current(&t), "release/1.0");
    t.run(&["worktree", "remove", "--force", other.to_str().unwrap()]);
}

#[test]
fn a_failure_after_the_first_step_names_the_remaining_steps() {
    let (t, repo, cfg) = flow_repo();
    repo.gitflow_start(&cfg, FlowKind::Release, "1.0").unwrap();
    t.write(
        "v.txt", "1
",
    );
    t.commit_all("Release");
    // Un crochet refuse la fusion dans develop seulement : la production et le tag sont déjà faits.
    let hook = t.file(".git/hooks/pre-merge-commit");
    std::fs::write(
        &hook,
        "#!/bin/sh
[ \"$(git symbolic-ref --short HEAD)\" = develop ] && exit 1
exit 0
",
    )
    .unwrap();
    let outcome = repo.gitflow_finish(&cfg, FlowKind::Release, "1.0", true).unwrap();
    let FinishOutcome::Failed { remaining, .. } = outcome else { panic!("échec attendu : {outcome:?}") };
    assert_eq!(
        remaining,
        vec![FlowStep::MergeInto("develop".into()), FlowStep::Delete("release/1.0".into())],
        "l'étape en échec est la première des restantes"
    );
    assert_eq!(t.run(&["tag", "-l"]).trim(), "1.0", "la production et le tag étaient faits");
    assert!(branches(&t).contains(&"release/1.0".to_string()), "rien n'est supprimé");
    assert_eq!(
        repo.operation_state(),
        OpState::None,
        "la fusion refusée est abandonnée : les étapes restantes la comptent"
    );
}
