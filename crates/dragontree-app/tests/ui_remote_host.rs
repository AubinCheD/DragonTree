//! Ce qu'un hôte qui pilote DragonTree à distance (le téléphone de la suite) lit de lui : ses dépôts, tels qu'il les
//! connaît en mémoire, et leur état rapide.

mod common;

use std::path::Path;

use common::{commit_file, git, harness, init_repo, start, wait_until};
use dragontree_app::settings::{Settings, path_key};
use dragontree_app::workspaces::TabColor;
use dragontree_git::HeadState;

fn repo_with_commit(dir: &Path) {
    init_repo(dir);
    commit_file(dir, "a.txt", "un\n", "premier");
}

/// Ses onglets, favoris, récents, masqués, dossiers racines et couleurs d'onglets, et les dépôts trouvés sous les
/// racines avec leur état et leur branche : lus en mémoire, ce qui vient de changer y est avant que ses réglages
/// soient écrits. L'état rapide d'un dépôt est celui de son onglet une fois lu, sinon celui de la liste.
#[test]
fn the_repo_directory_is_what_dragontree_knows_now() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("racine");
    let (alpha, beta) = (root.join("alpha"), root.join("sous").join("beta"));
    repo_with_commit(&alpha);
    repo_with_commit(&beta);
    std::fs::write(beta.join("a.txt"), "deux\n").unwrap();
    let elsewhere = tmp.path().join("ailleurs");
    repo_with_commit(&elsewhere);
    git(&elsewhere, &["checkout", "-q", "-b", "feature/x"]);
    let profile = tmp.path().join("profil");
    std::fs::create_dir_all(&profile).unwrap();
    let mut settings = Settings {
        scan_roots: vec![root.clone()],
        // Hors des racines : un favori trouvé dessous a deux lignes dans la liste de droite (même identifiant).
        favorites: vec![elsewhere.clone()],
        hidden_repos: vec![alpha.clone()],
        recent_repos: vec![elsewhere.clone()],
        ..Default::default()
    };
    settings.set_tab_color(&alpha, Some(TabColor::Red));
    let file = profile.join("settings.json");
    std::fs::write(&file, serde_json::to_vec(&settings).unwrap()).unwrap();

    let mut h = harness(start(tmp.path()));
    wait_until(&mut h, "scan et relevé de l'état finis", |h| {
        let d = h.state().repo_directory();
        !d.scanning && !d.sweeping && d.found.len() == 2 && d.found.iter().all(|f| f.status.is_some())
    });
    let d = h.state().repo_directory();
    let mut found: Vec<(String, u32, Option<HeadState>)> =
        d.found.iter().map(|f| (f.repo.name.clone(), f.status.as_ref().unwrap().dirty, f.head.clone())).collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    let main = Some(HeadState::Branch("main".into()));
    assert_eq!(found, [("alpha".into(), 0, main.clone()), ("beta".into(), 1, main)]);
    assert_eq!((d.roots, d.favorites, d.hidden), (vec![root.clone()], vec![elsewhere.clone()], vec![alpha.clone()]));
    assert_eq!(d.recent, vec![elsewhere.clone()]);
    assert_eq!(d.tab_colors.get(&path_key(&alpha)), Some(&TabColor::Red));
    assert!(d.tabs.is_empty() && d.active.is_none(), "l'accueil seul");

    // Affiché, puis un autre ouvert en fond : dans le répertoire tout de suite, avant que les réglages soient écrits.
    h.state_mut().show_repo(&elsewhere);
    h.state_mut().open_repo_tab(&alpha);
    let d = h.state().repo_directory();
    assert_eq!(d.tabs, [elsewhere.clone(), alpha.clone()]);
    assert_eq!(d.active.as_deref(), Some(elsewhere.as_path()));
    let saved: Settings = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert!(saved.open_tabs.is_empty(), "pas encore écrit : {:?}", saved.open_tabs);

    // Hors de la liste, l'état d'un dépôt est celui que son onglet a lu.
    assert_eq!(h.state().repo_quick_status(&beta).map(|(s, _)| s.dirty), Some(1), "relevé par la liste");
    assert!(h.state().repo_quick_status(&tmp.path().join("inconnu")).is_none());
    assert!(h.state().repo_quick_status(&elsewhere).is_none(), "ni lu ni dans la liste");
    wait_until(&mut h, "état de l'onglet lu", |h| {
        h.state()
            .repo_quick_status(&elsewhere)
            .is_some_and(|(s, head)| s.dirty == 0 && head == Some(HeadState::Branch("feature/x".into())))
    });
}
