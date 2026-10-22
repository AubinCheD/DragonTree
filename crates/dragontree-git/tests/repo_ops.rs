mod common;

use common::TestRepo;
use dragontree_git::graph::GraphBuilder;
use dragontree_git::{
    CancelToken, ChangeKind, DiffOptions, FileState, GitError, HeadState, LineKind, LogOptions, MergeMode, Oid,
    OpState, Repo, ResetMode,
};

fn open(t: &TestRepo) -> Repo {
    Repo::open(&t.git, t.path()).unwrap()
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    let norm = |p: &std::path::Path| p.canonicalize().unwrap().to_string_lossy().to_lowercase();
    norm(a) == norm(b)
}

/// Contrainte 6 du plan : sans rien demander, un dépôt de test isole la configuration git globale ; git lit pour la
/// portée globale un fichier temporaire et lui seul, jamais le `~/.gitconfig` de l'utilisateur.
#[test]
fn test_repositories_never_read_the_users_global_git_config() {
    let t = TestRepo::new();
    // Vérifié avant toute écriture : en cas d'échec, rien n'est écrit dans la vraie configuration.
    let global = std::env::var_os("GIT_CONFIG_GLOBAL")
        .map(std::path::PathBuf::from)
        .filter(|f| f.starts_with(std::env::temp_dir()))
        .expect("GIT_CONFIG_GLOBAL désigne un fichier du dossier temporaire");
    assert_eq!(std::env::var_os("HOME").map(std::path::PathBuf::from).as_deref(), global.parent(), "HOME temporaire");

    t.run(&["config", "--global", "dragontree.isolation", "oui"]);
    let scopes = t.run(&["config", "--list", "--show-scope", "--show-origin"]);
    let global_lines: Vec<&str> = scopes.lines().filter(|l| l.starts_with("global\t")).collect();
    assert!(
        global_lines.iter().any(|l| l.ends_with("dragontree.isolation=oui")),
        "clé lue en portée globale : {scopes}"
    );
    assert!(
        global_lines.iter().all(|l| common::origin_is(l, &global)),
        "portée globale lue hors de {global:?} : {scopes}"
    );
}

#[test]
fn open_from_subdir_finds_toplevel() {
    let t = TestRepo::new();
    t.write("src/deep/a.txt", "a\n");
    let repo = Repo::open(&t.git, &t.file("src/deep")).unwrap();
    assert!(same_dir(&repo.workdir, t.path()));
    assert!(same_dir(&repo.git_dir, &t.file(".git")));
    assert!(same_dir(&repo.common_dir, &t.file(".git")));
    assert!(!repo.name().is_empty());
}

#[test]
fn a_linked_worktree_knows_the_root_of_the_main_worktree() {
    let t = TestRepo::new();
    t.write("a.txt", "a\n");
    t.commit_all("init");
    let main = open(&t);
    assert_eq!(main.main_workdir(), None, "copie de travail principale : rien à remonter");
    let dir = tempfile::tempdir().unwrap();
    let wt_path = dir.path().join("wt");
    t.run(&["worktree", "add", "-q", "-b", "wt", &wt_path.display().to_string()]);
    let linked = Repo::open(&t.git, &wt_path).unwrap();
    let root = linked.main_workdir().expect("racine de la copie principale");
    assert!(same_dir(&root, t.path()), "{}", root.display());
}

#[test]
fn open_non_repo_reports_not_a_repository() {
    let dir = tempfile::tempdir().unwrap();
    let git = common::git();
    match Repo::open(&git, dir.path()) {
        Err(GitError::NotARepository { .. }) => {}
        other => panic!("attendu NotARepository, obtenu {other:?}"),
    }
}

#[test]
fn stage_unstage_commit_roundtrip() {
    let t = TestRepo::new();
    let repo = open(&t);
    t.write("a.txt", "a\n");
    t.write("b.txt", "b\n");
    let s = repo.status().unwrap();
    assert_eq!(s.unstaged().len(), 2);

    repo.stage(&strings(&["a.txt", "b.txt"])).unwrap();
    assert_eq!(repo.status().unwrap().staged().len(), 2);

    // Avant le premier commit, désindexer passe par `git rm --cached`.
    repo.unstage(&strings(&["b.txt"])).unwrap();
    let s = repo.status().unwrap();
    assert_eq!(s.staged().len(), 1);
    assert_eq!(s.unstaged().len(), 1);

    repo.commit("Premier commit\n\nCorps du message avec #123 conservé", false).unwrap();
    assert_eq!(repo.last_commit_message().unwrap().trim(), "Premier commit\n\nCorps du message avec #123 conservé");
    let refs = repo.refs().unwrap();
    assert_eq!(refs.head, HeadState::Branch("main".into()));
    assert!(refs.head_oid.is_some());

    // Après le premier commit : `git restore --staged`.
    repo.stage(&strings(&["b.txt"])).unwrap();
    repo.unstage(&strings(&["b.txt"])).unwrap();
    let s = repo.status().unwrap();
    assert!(s.staged().is_empty());
    assert_eq!(s.unstaged()[0].worktree, FileState::Untracked);
}

#[test]
fn stage_paths_with_spaces_unicode_and_glob_chars() {
    let t = TestRepo::new();
    let repo = open(&t);
    t.write("Peppol - Copie/é tè.cs", "x\n");
    t.write("[ab].txt", "literal\n");
    t.write("a.txt", "not staged\n");
    repo.stage(&strings(&["Peppol - Copie/é tè.cs", "[ab].txt"])).unwrap();
    let s = repo.status().unwrap();
    let staged: Vec<&str> = s.staged().iter().map(|e| e.path.as_str()).collect();
    assert_eq!(staged.len(), 2, "{staged:?}");
    assert!(staged.contains(&"Peppol - Copie/é tè.cs"));
    assert!(staged.contains(&"[ab].txt"));
}

#[test]
fn amend_changes_message() {
    let t = TestRepo::new();
    t.write("a", "1\n");
    t.commit_all("v1");
    let repo = open(&t);
    repo.commit("v1 corrigé", true).unwrap();
    assert_eq!(repo.last_commit_message().unwrap().trim(), "v1 corrigé");
    assert_eq!(t.run(&["rev-list", "--count", "HEAD"]).trim(), "1");
}

#[test]
fn branch_create_checkout_rename_delete() {
    let t = TestRepo::new();
    t.write("a", "1\n");
    t.commit_all("init");
    let repo = open(&t);
    assert!(repo.is_valid_branch_name("feature/ok"));
    assert!(!repo.is_valid_branch_name("pas valide"));
    repo.create_branch("feature/x", "main", true).unwrap();
    assert_eq!(repo.refs().unwrap().head, HeadState::Branch("feature/x".into()));
    repo.rename_branch("feature/x", "feature/y").unwrap();
    repo.checkout("main").unwrap();
    repo.delete_branch("feature/y", false).unwrap();
    let refs = repo.refs().unwrap();
    assert_eq!(refs.locals.len(), 1);
    assert_eq!(refs.head, HeadState::Branch("main".into()));
}

#[test]
fn checkout_detached_commit() {
    let t = TestRepo::new();
    t.write("a", "1\n");
    let first = t.commit_all("1");
    t.write("a", "2\n");
    t.commit_all("2");
    let repo = open(&t);
    repo.checkout(&first).unwrap();
    assert_eq!(repo.refs().unwrap().head, HeadState::Detached(Oid::from_hex(&first).unwrap()));
}

#[test]
fn merge_conflict_sets_operation_state() {
    let t = TestRepo::new();
    t.write("f", "base\n");
    t.commit_all("base");
    t.run(&["checkout", "-q", "-b", "other"]);
    t.write("f", "theirs\n");
    t.commit_all("theirs");
    t.run(&["checkout", "-q", "main"]);
    t.write("f", "ours\n");
    t.commit_all("ours");

    let repo = open(&t);
    let err = repo.merge("other", MergeMode::Default).unwrap_err();
    assert!(matches!(err, GitError::Failed { .. }), "{err:?}");
    assert_eq!(repo.operation_state(), OpState::Merge);
    let s = repo.status().unwrap();
    assert_eq!(s.conflicted().len(), 1);
    assert_eq!(s.conflicted()[0].path, "f");

    repo.abort_op(OpState::Merge).unwrap();
    assert_eq!(repo.operation_state(), OpState::None);
    assert!(repo.status().unwrap().is_clean());
}

#[test]
fn merge_no_ff_creates_merge_commit() {
    let t = TestRepo::new();
    t.write("f", "base\n");
    t.commit_all("base");
    t.run(&["checkout", "-q", "-b", "feat"]);
    t.write("g", "g\n");
    t.commit_all("feat");
    t.run(&["checkout", "-q", "main"]);
    let repo = open(&t);
    repo.merge("feat", MergeMode::NoFf).unwrap();
    let head = repo.refs().unwrap().head_oid.unwrap();
    assert_eq!(repo.commit_info(head).unwrap().parents.len(), 2);
}

#[test]
fn rebase_and_cherry_pick_and_revert() {
    let t = TestRepo::new();
    t.write("a", "a\n");
    t.commit_all("base");
    t.run(&["checkout", "-q", "-b", "feat"]);
    t.write("b", "b\n");
    let feat = t.commit_all("feat b");
    t.run(&["checkout", "-q", "main"]);
    t.write("c", "c\n");
    t.commit_all("main c");
    let repo = open(&t);

    repo.cherry_pick(&[Oid::from_hex(&feat).unwrap()]).unwrap();
    assert!(t.file("b").exists());
    let picked = repo.refs().unwrap().head_oid.unwrap();
    repo.revert(picked).unwrap();
    assert!(!t.file("b").exists());

    repo.checkout("feat").unwrap();
    repo.rebase("main").unwrap();
    assert!(t.file("c").exists(), "feat rebasée sur main contient c");
    assert_eq!(repo.operation_state(), OpState::None);
}

#[test]
fn stage_after_stash_still_takes_literal_paths() {
    // Garde-fou : GIT_LITERAL_PATHSPECS ne doit s'appliquer qu'aux commandes à chemins.
    let t = TestRepo::new();
    t.write("a", "1\n");
    t.commit_all("init");
    t.write("[x].txt", "x\n");
    t.write("x.txt", "y\n");
    let repo = open(&t);
    repo.stage(&strings(&["[x].txt"])).unwrap();
    let staged: Vec<String> = repo.status().unwrap().staged().iter().map(|e| e.path.clone()).collect();
    assert_eq!(staged, vec!["[x].txt".to_string()]);
}

#[test]
fn stash_save_list_pop() {
    let t = TestRepo::new();
    t.write("a", "1\n");
    t.commit_all("init");
    t.write("a", "2\n");
    t.write("nouveau", "n\n");
    let repo = open(&t);
    repo.stash_save(Some("mon stash"), true, false).unwrap();
    assert!(repo.status().unwrap().is_clean());
    let stashes = repo.stashes().unwrap();
    assert_eq!(stashes.len(), 1);
    assert!(stashes[0].message.contains("mon stash"), "{:?}", stashes[0].message);
    repo.stash_pop(0).unwrap();
    assert_eq!(t.read("a"), "2\n");
    assert!(t.file("nouveau").exists());
    assert!(repo.stashes().unwrap().is_empty());
}

#[test]
fn stash_apply_and_drop() {
    let t = TestRepo::new();
    t.write("a", "1\n");
    t.commit_all("init");
    t.write("a", "2\n");
    let repo = open(&t);
    repo.stash_save(None, false, false).unwrap();
    repo.stash_apply(0).unwrap();
    assert_eq!(t.read("a"), "2\n");
    assert_eq!(repo.stashes().unwrap().len(), 1);
    repo.stash_drop(0).unwrap();
    assert!(repo.stashes().unwrap().is_empty());
}

#[test]
fn tag_annotated_and_delete() {
    let t = TestRepo::new();
    t.write("a", "1\n");
    let c = t.commit_all("init");
    let repo = open(&t);
    repo.create_tag("v1.0", "HEAD", Some("Version 1.0")).unwrap();
    repo.create_tag("leger", "HEAD", None).unwrap();
    let refs = repo.refs().unwrap();
    let v1 = refs.tags.iter().find(|x| x.name == "v1.0").unwrap();
    assert!(v1.annotated);
    assert_eq!(v1.oid, Oid::from_hex(&c).unwrap());
    assert!(!refs.tags.iter().find(|x| x.name == "leger").unwrap().annotated);
    repo.delete_tag("v1.0").unwrap();
    assert_eq!(repo.refs().unwrap().tags.len(), 1);
}

#[test]
fn reset_hard_moves_head() {
    let t = TestRepo::new();
    t.write("a", "1\n");
    let first = t.commit_all("1");
    t.write("a", "2\n");
    t.commit_all("2");
    t.write("a", "dirty\n");
    let repo = open(&t);
    repo.reset(&first, ResetMode::Hard).unwrap();
    assert_eq!(t.read("a"), "1\n");
    assert_eq!(repo.refs().unwrap().head_oid, Oid::from_hex(&first));
}

#[test]
fn reset_soft_keeps_changes_staged() {
    let t = TestRepo::new();
    t.write("a", "1\n");
    let first = t.commit_all("1");
    t.write("a", "2\n");
    t.commit_all("2");
    let repo = open(&t);
    repo.reset(&first, ResetMode::Soft).unwrap();
    assert_eq!(repo.status().unwrap().staged().len(), 1);
}

#[test]
fn discard_restores_file_and_removes_untracked() {
    let t = TestRepo::new();
    t.write("a", "1\n");
    t.commit_all("init");
    t.write("a", "modifié\n");
    t.write("dossier/nouveau.txt", "n\n");
    let repo = open(&t);
    repo.discard(&strings(&["a"]), &strings(&["dossier/nouveau.txt"])).unwrap();
    assert_eq!(t.read("a"), "1\n");
    assert!(!t.file("dossier/nouveau.txt").exists());
    assert!(repo.status().unwrap().is_clean());
}

#[test]
fn log_and_graph_on_real_repo() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    t.commit_all("c0");
    t.run(&["checkout", "-q", "-b", "feat"]);
    t.write("g", "1\n");
    t.commit_all("feat 1");
    t.run(&["checkout", "-q", "main"]);
    t.write("h", "2\n");
    t.commit_all("main 2");
    t.run(&["merge", "-q", "--no-ff", "-m", "Merge feat", "feat"]);
    let repo = open(&t);
    let commits = repo.log(&LogOptions::default(), &CancelToken::new()).unwrap();
    assert_eq!(commits.len(), 4);
    assert_eq!(&*commits[0].summary, "Merge feat");
    assert_eq!(commits[0].parents.len(), 2);
    let mut g = GraphBuilder::new();
    for c in &commits {
        g.push(c.id, &c.parents);
    }
    let layout = g.finish();
    assert_eq!(layout.max_lane, 1);
    assert_eq!(layout.rows.len(), 4);
}

#[test]
fn log_empty_repo_returns_empty() {
    let t = TestRepo::new();
    let repo = open(&t);
    assert!(repo.log(&LogOptions::default(), &CancelToken::new()).unwrap().is_empty());
    let refs = repo.refs().unwrap();
    assert_eq!(refs.head, HeadState::Unborn("main".into()));
}

#[test]
fn log_excludes_stash_refs() {
    let t = TestRepo::new();
    t.write("a", "1\n");
    t.commit_all("init");
    t.write("a", "2\n");
    t.run(&["stash", "push", "-q", "-m", "s"]);
    let repo = open(&t);
    assert_eq!(repo.log(&LogOptions::default(), &CancelToken::new()).unwrap().len(), 1);
}

#[test]
fn commit_info_and_files() {
    let t = TestRepo::new();
    // Contenu assez long pour que git reconnaisse le renommage (similarité > 50 %).
    let body: String = (1..=10).map(|i| format!("ligne {i}\n")).collect();
    t.write("a.txt", &body);
    let root = t.commit_all("racine");
    t.write("a.txt", &body.replace("ligne 10\n", "a2\n"));
    t.write("b.txt", "b\n");
    t.run(&["mv", "a.txt", "c.txt"]);
    let second = t.commit_all("Sujet\n\nCorps");
    let repo = open(&t);
    let root = Oid::from_hex(&root).unwrap();
    let second = Oid::from_hex(&second).unwrap();

    let info = repo.commit_info(second).unwrap();
    assert_eq!(info.parents, vec![root]);
    assert_eq!(info.author, "Testeur");
    assert_eq!(info.author_email, "test@example.com");
    assert_eq!(info.message.trim_end(), "Sujet\n\nCorps");

    let root_files = repo.commit_files(root, None).unwrap();
    assert_eq!(root_files.len(), 1);
    assert_eq!(root_files[0].kind, ChangeKind::Added);

    let files = repo.commit_files(second, Some(root)).unwrap();
    let kinds: Vec<(String, ChangeKind)> = files.iter().map(|f| (f.path.clone(), f.kind)).collect();
    assert!(kinds.contains(&("b.txt".into(), ChangeKind::Added)), "{kinds:?}");
    assert!(kinds.contains(&("c.txt".into(), ChangeKind::Renamed)), "{kinds:?}");

    let renamed = files.iter().find(|f| f.kind == ChangeKind::Renamed).unwrap();
    let d = repo.diff_commit_file(second, Some(root), renamed, &DiffOptions::default()).unwrap();
    assert_eq!(d.old_path.as_deref(), Some("a.txt"));
    assert_eq!(d.new_path.as_deref(), Some("c.txt"));
    assert!(d.hunks[0].lines.iter().any(|l| l.kind == LineKind::Added && l.text == "a2"));

    let root_diff = repo.diff_commit_file(root, None, &root_files[0], &DiffOptions::default()).unwrap();
    assert!(root_diff.is_new);
}

#[test]
fn diff_worktree_staged_unstaged_untracked() {
    let t = TestRepo::new();
    t.write("a", "1\n2\n");
    t.commit_all("init");
    t.write("a", "1\nDEUX\n");
    t.write("u.txt", "nouveau\n");
    let repo = open(&t);
    let o = DiffOptions::default();

    let unstaged = repo.diff_worktree_file("a", false, false, &o).unwrap();
    assert_eq!(unstaged.added_count(), 1);
    assert_eq!(unstaged.removed_count(), 1);

    repo.stage(&strings(&["a"])).unwrap();
    let staged = repo.diff_worktree_file("a", true, false, &o).unwrap();
    assert_eq!(staged.added_count(), 1);
    assert!(repo.diff_worktree_file("a", false, false, &o).unwrap().hunks.is_empty());

    let untracked = repo.diff_worktree_file("u.txt", false, true, &o).unwrap();
    assert!(untracked.is_new);
    assert_eq!(untracked.added_count(), 1);
    assert_eq!(untracked.new_path.as_deref(), Some("u.txt"));
}

#[test]
fn diff_ignore_whitespace_option() {
    let t = TestRepo::new();
    t.write("a", "x y\n");
    t.commit_all("init");
    t.write("a", "x    y\n");
    let repo = open(&t);
    let o = DiffOptions { context: 3, ignore_whitespace: true };
    assert!(repo.diff_worktree_file("a", false, false, &o).unwrap().hunks.is_empty());
}

#[test]
fn apply_patch_through_repo() {
    let t = TestRepo::new();
    t.write("f", "1\n2\n");
    t.commit_all("init");
    t.write("f", "1\n2\n3\n");
    let repo = open(&t);
    let d = repo.diff_worktree_file("f", false, false, &DiffOptions::default()).unwrap();
    let patch =
        dragontree_git::build_patch(&d, &[dragontree_git::HunkSelection { hunk: 0, lines: None }], false).unwrap();
    repo.apply_patch(&patch, dragontree_git::PatchMode::Stage).unwrap();
    assert_eq!(t.run(&["show", ":f"]), "1\n2\n3\n");
}

#[test]
fn intent_to_add_makes_untracked_diffable() {
    let t = TestRepo::new();
    t.write("base", "b\n");
    t.commit_all("init");
    t.write("n.txt", "x\n");
    let repo = open(&t);
    repo.intent_to_add("n.txt").unwrap();
    let d = repo.diff_worktree_file("n.txt", false, false, &DiffOptions::default()).unwrap();
    assert_eq!(d.added_count(), 1);
}

#[test]
fn remotes_and_config_and_blob() {
    let t = TestRepo::new();
    t.write("a", "contenu\n");
    t.commit_all("init");
    let repo = open(&t);
    repo.add_remote("origin", "https://example.com/r.git").unwrap();
    let remotes = repo.remotes().unwrap();
    assert_eq!(
        remotes,
        vec![dragontree_git::Remote {
            name: "origin".into(),
            fetch_url: "https://example.com/r.git".into(),
            push_url: "https://example.com/r.git".into(),
        }]
    );
    assert_eq!(repo.config("user.name").as_deref(), Some("Testeur"));
    assert_eq!(repo.config("nope.absent"), None);
    assert_eq!(repo.blob("HEAD", "a").unwrap(), b"contenu\n");
    repo.remove_remote("origin").unwrap();
    assert!(repo.remotes().unwrap().is_empty());
}

#[test]
fn submodules_empty_without_gitmodules() {
    let t = TestRepo::new();
    let repo = open(&t);
    assert!(repo.submodules().unwrap().is_empty());
}

/// Mesure sur un vrai dépôt (lecture seule) : `DRAGONTREE_REAL_REPO=<chemin> cargo test -- --ignored real_repo`.
#[test]
#[ignore]
fn real_repo_smoke_timings() {
    let Ok(path) = std::env::var("DRAGONTREE_REAL_REPO") else { return };
    let git = common::git();
    let t0 = std::time::Instant::now();
    let repo = Repo::open(&git, std::path::Path::new(&path)).unwrap();
    let t_open = t0.elapsed();
    let refs = repo.refs().unwrap();
    let t_refs = t0.elapsed();
    let status = repo.status().unwrap();
    let t_status = t0.elapsed();
    let first = repo.log(&LogOptions { max_count: Some(2000), ..LogOptions::default() }, &CancelToken::new()).unwrap();
    let t_first = t0.elapsed();
    let commits = repo.log(&LogOptions::default(), &CancelToken::new()).unwrap();
    let t_log = t0.elapsed();
    let mut g = GraphBuilder::new();
    for c in &commits {
        g.push(c.id, &c.parents);
    }
    let layout = g.finish();
    let t_graph = t0.elapsed();
    println!(
        "open {:?} | refs {:?} ({} locales, {} distantes, {} tags) | status {:?} ({} entrées) | log 2000 {:?} ({}) | log complet {:?} ({} commits) | graphe {:?} ({} arêtes, max_lane {})",
        t_open,
        t_refs - t_open,
        refs.locals.len(),
        refs.remotes.len(),
        refs.tags.len(),
        t_status - t_refs,
        status.entries.len(),
        t_first - t_status,
        first.len(),
        t_log - t_first,
        commits.len(),
        t_graph - t_log,
        layout.edges.len(),
        layout.max_lane
    );
}

#[test]
fn discard_refuses_nested_repository() {
    let t = TestRepo::new();
    t.write("a", "1\n");
    t.commit_all("init");
    let inner = t.file("inner");
    std::fs::create_dir_all(&inner).unwrap();
    for args in [&["init", "-q"][..], &["config", "user.name", "x"], &["config", "user.email", "x@x"]] {
        t.git.cmd(&inner).args(args).run().unwrap();
    }
    std::fs::write(inner.join("f"), "f\n").unwrap();
    t.git.cmd(&inner).args(["add", "-A"]).run().unwrap();
    t.git.cmd(&inner).args(["commit", "-q", "-m", "historique précieux"]).run().unwrap();
    t.write("x.txt", "x\n");
    let repo = open(&t);
    let untracked: Vec<String> = repo.status().unwrap().unstaged().iter().map(|e| e.path.clone()).collect();
    assert!(untracked.contains(&"inner/".to_string()), "{untracked:?}");
    let err = repo.discard(&[], &strings(&["inner/", "x.txt"])).unwrap_err();
    assert!(err.to_string().contains("inner"), "{err}");
    assert!(inner.join(".git").exists(), "le dépôt imbriqué n'est jamais supprimé");
    assert!(t.file("x.txt").exists(), "rien n'est supprimé quand la sélection contient un dépôt imbriqué");
}

#[test]
fn add_to_gitignore_preserves_existing_bytes() {
    let t = TestRepo::new();
    std::fs::write(t.file(".gitignore"), b"# fichiers g\xe9n\xe9r\xe9s\nbin/").unwrap();
    open(&t).add_to_gitignore("*.log").unwrap();
    assert_eq!(std::fs::read(t.file(".gitignore")).unwrap(), b"# fichiers g\xe9n\xe9r\xe9s\nbin/\n*.log\n");
    let t2 = TestRepo::new();
    open(&t2).add_to_gitignore("cache/").unwrap();
    assert_eq!(std::fs::read(t2.file(".gitignore")).unwrap(), b"cache/\n");
}

#[test]
fn stash_by_oid_survives_index_shift() {
    let t = TestRepo::new();
    t.write("a", "1\n");
    t.commit_all("init");
    let repo = open(&t);
    t.write("a", "un\n");
    repo.stash_save(Some("un"), false, false).unwrap();
    let un = repo.stashes().unwrap()[0].oid;
    t.write("a", "deux\n");
    repo.stash_save(Some("deux"), false, false).unwrap();
    repo.stash_drop_oid(un).unwrap();
    let left: Vec<String> = repo.stashes().unwrap().into_iter().map(|s| s.message).collect();
    assert_eq!(left.len(), 1);
    assert!(left[0].contains("deux"), "{left:?}");
    assert!(repo.stash_drop_oid(un).is_err(), "un stash disparu n'est jamais confondu avec un autre");
    let deux = repo.stashes().unwrap()[0].oid;
    repo.stash_pop_oid(deux).unwrap();
    assert_eq!(t.read("a"), "deux\n");
}
