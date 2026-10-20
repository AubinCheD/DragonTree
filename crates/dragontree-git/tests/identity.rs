//! Identité git globale (`git config --global`, avec `GIT_CONFIG_GLOBAL` temporaire) et commit sans identité.

mod common;

use common::TestRepo;
use dragontree_git::identity::{self, Identity};
use dragontree_git::{GitError, Repo};

/// `GIT_CONFIG_GLOBAL` est un état du processus : un seul test de ce fichier le positionne (jamais ~/.gitconfig).
/// `useConfigOnly` empêche git de deviner l'identité à partir du nom de la machine : sans configuration, le commit
/// échoue de la même façon partout.
#[test]
fn global_identity_round_trips_and_a_commit_without_identity_is_recognised() {
    // Dossier temporaire pour le fichier global ET pour HOME / USERPROFILE : GIT_CONFIG_GLOBAL n'existe qu'à partir de
    // git 2.32 (2.31 est pris en charge), une version plus ancienne écrirait sinon dans le vrai ~/.gitconfig.
    let home = tempfile::tempdir().unwrap();
    let global = home.path().join("gitconfig-global");
    std::fs::write(&global, "[user]\n\tuseConfigOnly = true\n").unwrap();
    // SAFETY : positionné avant toute commande git de ce fichier, qui ne contient que ce test.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &global);
        std::env::set_var("HOME", home.path());
        std::env::set_var("USERPROFILE", home.path());
    }
    let t = TestRepo::new();
    let cwd = std::env::temp_dir();

    // Aucune identité globale : lecture vide.
    assert_eq!(identity::global_identity(&t.git, &cwd).unwrap(), Identity::default());

    // Écriture puis relecture (accents et espaces conservés) dans le fichier global de test.
    let id = Identity { name: "Zoé Northwind".into(), email: "zoe@northwind.test".into() };
    identity::set_global_identity(&t.git, &cwd, &id).unwrap();
    assert_eq!(identity::global_identity(&t.git, &cwd).unwrap(), id);
    let file = std::fs::read_to_string(&global).unwrap();
    assert!(file.contains("zoe@northwind.test"), "{file}");

    // Une valeur vide retire la clé ; retirer une clé absente n'est pas une erreur.
    identity::set_global_identity(&t.git, &cwd, &Identity { name: String::new(), email: id.email.clone() }).unwrap();
    identity::set_global_identity(&t.git, &cwd, &Identity { name: String::new(), email: id.email.clone() }).unwrap();
    assert_eq!(
        identity::global_identity(&t.git, &cwd).unwrap(),
        Identity { name: String::new(), email: "zoe@northwind.test".into() }
    );
    identity::set_global_identity(&t.git, &cwd, &Identity::default()).unwrap();

    // Sans identité ni locale ni globale, le commit échoue : l'erreur est reconnue d'après la configuration.
    t.run(&["config", "--unset-all", "user.name"]);
    t.run(&["config", "--unset-all", "user.email"]);
    t.write("a.txt", "1\n");
    t.run(&["add", "-A"]);
    let repo = Repo::open(&t.git, t.path()).unwrap();
    assert_eq!(repo.effective_identity().unwrap(), Identity::default());
    assert!(matches!(repo.commit("Premier", false), Err(GitError::IdentityMissing(_))));

    // Une identité globale suffit ensuite (le dépôt n'a pas de valeur locale).
    identity::set_global_identity(&t.git, &cwd, &id).unwrap();
    assert_eq!(repo.effective_identity().unwrap(), id);
    repo.commit("Premier", false).unwrap();

    // Une identité donnée par author.* / committer.* (sans user.*) suffit à git : pas un défaut d'identité.
    identity::set_global_identity(&t.git, &cwd, &Identity::default()).unwrap();
    assert!(!repo.identity_usable());
    for (key, value) in [
        ("author.name", "Auteur"),
        ("author.email", "a@northwind.test"),
        ("committer.name", "Committer"),
        ("committer.email", "c@northwind.test"),
    ] {
        t.run(&["config", key, value]);
    }
    assert!(repo.effective_identity().unwrap().is_incomplete());
    assert!(repo.identity_usable());
    // Un autre échec de commit (rien à valider) n'est pas pris pour un défaut d'identité.
    assert!(matches!(repo.commit("Vide", false), Err(GitError::Failed { .. })));
}
