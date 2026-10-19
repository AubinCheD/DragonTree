//! Sous-modules : ajout (avec branche), déplacement, changement d'URL (git ≥ 2.25 et repli), suppression
//! complète. Les sous-modules viennent d'un dépôt local : git refuse `file://` sans `protocol.file.allow`
//! (git ≥ 2.38.1), autorisé ici pour le processus de test seulement.

mod common;

use common::TestRepo;
use dragontree_git::{NetOptions, Repo};

/// git refuse `file://` pour un sous-module (le clone est lancé hors de la configuration du dépôt) : on
/// l'autorise pour ce processus de test seulement, par l'environnement (git ≥ 2.31).
fn allow_file_protocol() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY : appelé avant tout lancement de git par ce fichier de tests, toujours avec les mêmes valeurs.
        unsafe {
            std::env::set_var("GIT_CONFIG_COUNT", "1");
            std::env::set_var("GIT_CONFIG_KEY_0", "protocol.file.allow");
            std::env::set_var("GIT_CONFIG_VALUE_0", "always");
        }
    });
}

/// Dépôt « bibliothèque » avec un commit sur main et une branche `stable`.
fn library() -> TestRepo {
    allow_file_protocol();
    let t = TestRepo::new();
    t.write("lib.txt", "1\n");
    t.commit_all("bibliothèque");
    t.run(&["branch", "stable"]);
    t.write("lib.txt", "2\n");
    t.commit_all("suite");
    t
}

/// Dépôt contenant un commit.
fn host() -> (TestRepo, Repo) {
    allow_file_protocol();
    let t = TestRepo::new();
    t.write("a.txt", "1\n");
    t.commit_all("premier");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    (t, repo)
}

fn add(repo: &Repo, lib: &TestRepo, path: &str, branch: Option<&str>) {
    repo.add_submodule(&lib.url(), path, branch, &NetOptions::default(), &mut |_| {}).unwrap();
}

fn gitmodules(t: &TestRepo) -> String {
    std::fs::read_to_string(t.file(".gitmodules")).unwrap_or_default()
}

fn tracked(t: &TestRepo) -> String {
    t.run(&["ls-files", "--stage"])
}

#[test]
fn adding_a_submodule_lists_it_with_its_name_and_url() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "vendor/lib", None);
    let subs = repo.submodules().unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].path, "vendor/lib");
    assert_eq!(subs[0].name, "vendor/lib");
    assert_eq!(subs[0].url, lib.url());
    assert!(t.file("vendor/lib/lib.txt").exists());
    assert!(gitmodules(&t).contains("vendor/lib"));
}

#[test]
fn adding_with_a_branch_records_it_and_checks_it_out() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "lib", Some("stable"));
    assert!(gitmodules(&t).contains("branch = stable"));
    // Le clone applique les fins de ligne de l'utilisateur (CRLF possible) : seul le contenu compte.
    assert_eq!(t.read("lib/lib.txt").trim(), "1");
}

#[test]
fn adding_a_submodule_from_an_unreachable_url_reports_git_s_message() {
    let (t, repo) = host();
    let missing = t.path().join("n-existe-pas");
    let url = format!("file:///{}", missing.display().to_string().replace('\\', "/"));
    let err = repo.add_submodule(&url, "lib", None, &NetOptions::default(), &mut |_| {}).unwrap_err();
    assert!(err.stderr_summary().contains("n-existe-pas"), "{}", err.stderr_summary());
    assert!(repo.submodules().unwrap().is_empty());
}

#[test]
fn moving_a_submodule_updates_gitmodules_and_the_working_tree() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "lib", None);
    t.run(&["commit", "-q", "-m", "ajoute lib"]);
    repo.move_submodule("lib", "deps/lib").unwrap();
    let subs = repo.submodules().unwrap();
    assert_eq!(subs.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(), ["deps/lib"]);
    assert!(t.file("deps/lib/lib.txt").exists());
    assert!(!t.file("lib").exists());
    assert!(gitmodules(&t).contains("path = deps/lib"));
}

#[test]
fn changing_the_url_updates_gitmodules_and_the_local_config() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "lib", None);
    repo.set_submodule_url("lib", "https://git.fabrikam.test/lib.git").unwrap();
    assert_eq!(repo.submodules().unwrap()[0].url, "https://git.fabrikam.test/lib.git");
    assert_eq!(t.run(&["config", "submodule.lib.url"]).trim(), "https://git.fabrikam.test/lib.git");
}

#[test]
fn the_fallback_for_old_git_changes_the_url_the_same_way() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "lib", None);
    repo.set_submodule_url_via_config("lib", "https://git.fabrikam.test/autre.git").unwrap();
    assert_eq!(repo.submodules().unwrap()[0].url, "https://git.fabrikam.test/autre.git");
    assert_eq!(t.run(&["config", "submodule.lib.url"]).trim(), "https://git.fabrikam.test/autre.git");
}

#[test]
fn removing_a_submodule_leaves_no_trace_in_gitmodules_the_index_or_git_modules() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "lib", None);
    t.run(&["commit", "-q", "-m", "ajoute lib"]);
    assert!(t.path().join(".git/modules/lib").exists());
    repo.remove_submodule("lib").unwrap();
    assert!(repo.submodules().unwrap().is_empty());
    assert!(!gitmodules(&t).contains("lib"), "{}", gitmodules(&t));
    assert!(!tracked(&t).contains("lib\n"), "{}", tracked(&t));
    assert!(!t.file("lib").exists());
    assert!(!t.path().join(".git/modules/lib").exists());
    assert!(t.try_run(&["config", "submodule.lib.url"]).is_err());
}

#[test]
fn removing_a_submodule_with_local_changes_works_since_the_caller_confirmed() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "lib", None);
    t.write("lib/lib.txt", "modifié\n");
    t.write("lib/neuf.txt", "x\n");
    repo.remove_submodule("lib").unwrap();
    assert!(repo.submodules().unwrap().is_empty());
    assert!(!t.file("lib").exists());
}

#[test]
fn an_unknown_submodule_is_refused_and_nothing_is_deleted() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "lib", None);
    assert!(repo.remove_submodule("inconnu").is_err());
    assert!(repo.remove_submodule("../a.txt").is_err());
    assert!(t.file("a.txt").exists());
    assert_eq!(repo.submodules().unwrap().len(), 1);
}

#[test]
fn git_version_is_parsed_for_the_set_url_choice() {
    use dragontree_git::submodule::parse_git_version;
    assert_eq!(parse_git_version("git version 2.44.0.windows.1"), Some((2, 44)));
    assert_eq!(parse_git_version("git version 2.25.0"), Some((2, 25)));
    assert_eq!(parse_git_version("n'importe quoi"), None);
}

#[test]
fn set_url_then_remove_removes_everything() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "lib", None);
    t.run(&["commit", "-q", "-m", "ajoute lib"]);
    repo.set_submodule_url("lib", "https://git.fabrikam.test/lib.git").unwrap();
    // .gitmodules est indexé par le changement d'URL : git rm ne s'arrête plus.
    assert!(t.run(&["diff", "--name-only"]).trim().is_empty(), "rien de non indexé");
    repo.remove_submodule("lib").unwrap();
    assert!(repo.submodules().unwrap().is_empty());
    assert!(!gitmodules(&t).contains("lib"), "{}", gitmodules(&t));
    assert!(!tracked(&t).contains("lib\n"));
    assert!(!t.path().join(".git/modules/lib").exists());
}

#[test]
fn set_url_then_move_works() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "lib", None);
    t.run(&["commit", "-q", "-m", "ajoute lib"]);
    repo.set_submodule_url_via_config("lib", "https://git.fabrikam.test/lib.git").unwrap();
    repo.move_submodule("lib", "deps/lib").unwrap();
    let subs = repo.submodules().unwrap();
    assert_eq!((subs[0].path.as_str(), subs[0].url.as_str()), ("deps/lib", "https://git.fabrikam.test/lib.git"));
}

#[test]
fn an_unstaged_gitmodules_is_refused_before_anything_is_touched() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "lib", None);
    t.run(&["commit", "-q", "-m", "ajoute lib"]);
    // Modification faite en dehors de l'application : non indexée.
    t.run(&["config", "-f", ".gitmodules", "submodule.lib.url", "https://git.fabrikam.test/x.git"]);
    let err = repo.remove_submodule("lib").unwrap_err();
    assert!(matches!(err, dragontree_git::GitError::Refused(_)), "{err:?}");
    assert!(t.file("lib/lib.txt").exists(), "le sous-module n'a pas été vidé");
    assert!(repo.move_submodule("lib", "deps/lib").is_err());
    assert!(!t.file("deps").exists(), "aucun dossier parent créé");
    assert!(t.file("lib/lib.txt").exists());
}

#[test]
fn a_failed_move_does_not_leave_the_created_parent_folder() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "lib", None);
    t.run(&["commit", "-q", "-m", "ajoute lib"]);
    // Destination dans le sous-module lui-même : git refuse, après la création du dossier parent.
    assert!(repo.move_submodule("lib", "lib/profond/x").is_err());
    assert!(!t.file("lib/profond").exists());
}

#[test]
fn a_moved_submodule_keeps_its_name_and_is_removed_completely() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "lib", None);
    t.run(&["commit", "-q", "-m", "ajoute lib"]);
    repo.move_submodule("lib", "deps/lib").unwrap();
    assert_eq!(repo.submodules().unwrap()[0].name, "lib");
    assert!(t.path().join(".git/modules/lib").exists());
    repo.remove_submodule("deps/lib").unwrap();
    assert!(repo.submodules().unwrap().is_empty());
    assert!(!t.path().join(".git/modules/lib").exists());
    assert!(!t.file("deps/lib").exists());
    assert!(!tracked(&t).contains("lib\n"));
}

#[test]
fn removing_a_slash_named_submodule_leaves_no_empty_parent_in_git_modules() {
    let lib = library();
    let (t, repo) = host();
    add(&repo, &lib, "vendor/lib", None);
    t.run(&["commit", "-q", "-m", "ajoute lib"]);
    assert!(t.path().join(".git/modules/vendor/lib").exists());
    repo.remove_submodule("vendor/lib").unwrap();
    assert!(!t.path().join(".git/modules/vendor").exists());
}
