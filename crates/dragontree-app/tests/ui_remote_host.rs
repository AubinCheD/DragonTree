//! Ce qu'un hôte qui pilote DragonTree à distance (le téléphone de la suite) lit de lui : ses dépôts, tels qu'il les
//! connaît en mémoire, leur état rapide, les lectures ratées d'un onglet ; DragonTree masqué chez lui, qui vit quand
//! même ; et ce qu'il y fait : un dépôt existant ajouté en fond, ses onglets de fond refermés, un dépôt cloné sans
//! fenêtre.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::{
    SilentServer, commit_file, git, harness, init_repo, repo_with_silent_remote, seed_accounts, start, wait_until,
};
use dragontree_app::DragonTreeApp;
use dragontree_app::clone::CloneRequest;
use dragontree_app::repo::{Refresh, RemoteOp};
use dragontree_app::settings::{Settings, long_path_key, path_key};
use dragontree_app::workspaces::TabColor;
use dragontree_git::{GitError, HeadState};
use egui_kittest::Harness;

fn repo_with_commit(dir: &Path) {
    init_repo(dir);
    commit_file(dir, "a.txt", "un\n", "premier");
}

/// Un hôte où DragonTree est masqué : sa logique tourne à chaque image, suivie de `background_tick` ; jamais `ui`.
struct Hidden(DragonTreeApp);

impl eframe::App for Hidden {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        eframe::App::logic(&mut self.0, ctx, frame);
        self.0.background_tick();
    }

    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {}
}

type HiddenHarness = Harness<'static, Hidden>;

fn hidden(base: &Path) -> HiddenHarness {
    let start = start(base);
    Harness::builder().with_size(egui::vec2(900.0, 700.0)).build_eframe(move |cc| Hidden(DragonTreeApp::new(cc, start)))
}

/// Fait tourner l'hôte jusqu'à ce que `cond` soit vraie.
fn run_until(h: &mut HiddenHarness, what: &str, mut cond: impl FnMut(&DragonTreeApp) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline {
        h.step();
        if cond(&h.state().0) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("délai dépassé en attendant : {what}");
}

/// Fait tourner l'hôte pendant `duration`.
fn run_for(h: &mut HiddenHarness, duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        h.step();
        std::thread::sleep(Duration::from_millis(25));
    }
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

/// Masqué chez son hôte (pas de `ui`), DragonTree vit : ses réglages modifiés sont enregistrés, et l'onglet que l'hôte
/// regarde à distance relit un changement sur le disque, comme l'onglet affiché ; un onglet de fond que personne ne
/// regarde attend de l'être.
#[test]
fn hidden_dragontree_keeps_the_watched_tab_fresh_and_saves_its_settings() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("depot");
    repo_with_commit(&repo);
    let mut h = hidden(tmp.path());
    h.step();
    h.state_mut().0.open_repo_tab(&repo);
    run_until(&mut h, "dépôt lu", |a| a.repo_tab(&repo).is_some_and(|t| t.status.is_some() && t.refs.is_some()));
    assert!(h.state().0.active_repo_dir().is_none(), "en fond");
    let file = tmp.path().join("profil").join("settings.json");
    let saved = || serde_json::from_slice::<Settings>(&std::fs::read(&file).unwrap_or_default()).ok();
    assert!(
        saved().is_some_and(|s| s.open_tabs.iter().any(|p| path_key(p) == path_key(&repo))),
        "l'onglet ouvert est enregistré sans `ui`"
    );

    let sees = |a: &DragonTreeApp, name: &str| {
        let status = a.repo_tab(&repo).and_then(|t| t.status.clone());
        status.is_some_and(|s| s.entries.iter().any(|e| e.path == name))
    };
    std::fs::write(repo.join("b.txt"), "b\n").unwrap();
    run_for(&mut h, Duration::from_secs(2));
    assert!(!sees(&h.state().0, "b.txt"), "onglet de fond que personne ne regarde : pas relu");
    // Regardé à distance : ce qui attendait est relu, puis chaque changement.
    h.state_mut().0.set_remote_focus(Some(&repo));
    run_until(&mut h, "b.txt relu", |a| sees(a, "b.txt"));
    std::fs::write(repo.join("c.txt"), "c\n").unwrap();
    run_until(&mut h, "c.txt relu", |a| sees(a, "c.txt"));
    // Plus regardé : il attend de nouveau.
    h.state_mut().0.set_remote_focus(None);
    std::fs::write(repo.join("d.txt"), "d\n").unwrap();
    run_for(&mut h, Duration::from_secs(2));
    assert!(!sees(&h.state().0, "d.txt"), "plus regardé : pas relu");
}

/// Ce que sait l'onglet de `repo` : son état est lu, ses branches aussi ; ses lectures ratées.
fn reads(app: &DragonTreeApp, repo: &Path) -> (bool, bool, Option<String>, Option<String>) {
    let t = app.repo_tab(repo).expect("onglet ouvert");
    let error = |e: &Option<GitError>| e.as_ref().map(ToString::to_string);
    (t.status.is_some(), t.refs.is_some(), error(&t.status_error), error(&t.refs_error))
}

/// Une lecture ratée de l'état ou des branches est gardée sur l'onglet (et notifiée, comme avant), avec ce qu'il avait
/// lu avant ; la lecture réussie suivante l'efface. Un hôte peut dire pourquoi au lieu de « lecture… ».
#[test]
fn a_failed_read_is_kept_on_the_tab_until_the_next_good_one() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("depot");
    repo_with_commit(&repo);
    let mut h = harness(start(tmp.path()));
    h.state_mut().open_repo_tab(&repo);
    let all_read = (true, true, None, None);
    wait_until(&mut h, "dépôt lu", |h| reads(h.state(), &repo) == all_read);

    // HEAD et index illisibles : les deux lectures échouent.
    let (head, index) = (repo.join(".git").join("HEAD"), repo.join(".git").join("index"));
    let (head_was, index_was) = (std::fs::read(&head).unwrap(), std::fs::read(&index).unwrap());
    std::fs::write(&head, "illisible\n").unwrap();
    std::fs::write(&index, "illisible").unwrap();
    h.state_mut().refresh_repo(&repo, Refresh::ALL);
    wait_until(&mut h, "lectures ratées", |h| matches!(reads(h.state(), &repo), (_, _, Some(_), Some(_))));
    let (status, refs, _, refs_error) = reads(h.state(), &repo);
    assert!(status && refs, "les lectures d'avant restent");
    assert!(refs_error.as_ref().is_some_and(|e| e.contains("HEAD")), "{refs_error:?}");

    // Réparé : la lecture suivante efface les erreurs.
    std::fs::write(&head, head_was).unwrap();
    std::fs::write(&index, index_was).unwrap();
    h.state_mut().refresh_repo(&repo, Refresh::ALL);
    wait_until(&mut h, "relu", |h| reads(h.state(), &repo) == all_read);
}

/// Un dépôt existant ajouté par l'hôte : git doit l'ouvrir ; il l'est dans un onglet de fond (l'onglet affiché ne
/// change pas), en tête des récents, et les réglages sont enregistrés aussitôt. Un dossier que git n'ouvre pas est
/// refusé sans rien changer ; un dépôt déjà ouvert garde son onglet, sous la forme où DragonTree le connaît.
#[test]
fn a_repo_added_by_the_host_opens_in_the_background_and_is_remembered() {
    let tmp = tempfile::tempdir().unwrap();
    let (shown, added) = (tmp.path().join("affiche"), tmp.path().join("ajoute"));
    repo_with_commit(&shown);
    repo_with_commit(&added);
    let plain = tmp.path().join("pas-un-depot");
    std::fs::create_dir_all(&plain).unwrap();
    let mut h = harness(start(tmp.path()));
    h.state_mut().show_repo(&shown);
    h.step();
    let file = tmp.path().join("profil").join("settings.json");
    let saved = || Settings::load_from(&file);

    for bad in [plain.clone(), tmp.path().join("absent")] {
        assert!(h.state_mut().add_repo(&bad).is_err(), "{}", bad.display());
        let d = h.state().repo_directory();
        assert!(d.tabs == [shown.clone()] && !d.recent.contains(&bad), "rien de changé : {:?}", d.recent);
    }

    assert_eq!(h.state_mut().add_repo(&added), Ok(added.clone()));
    let d = h.state().repo_directory();
    assert_eq!(d.active.as_deref(), Some(shown.as_path()), "l'onglet affiché ne change pas");
    assert_eq!(d.tabs, [shown.clone(), added.clone()]);
    assert_eq!(d.recent.first(), Some(&added));
    let s = saved();
    assert_eq!(s.recent_repos.first(), Some(&added), "enregistré aussitôt");
    assert_eq!(s.open_tabs, [shown.clone(), added.clone()]);

    let other_form = PathBuf::from(added.display().to_string().to_uppercase());
    assert_eq!(h.state_mut().add_repo(&other_form), Ok(added.clone()), "déjà ouvert : son onglet");
    assert_eq!(h.state().repo_directory().tabs.len(), 2);
    wait_until(&mut h, "dépôt ajouté lu", |h| h.state().repo_tab(&added).is_some_and(|t| t.status.is_some()));
    assert_eq!(h.state().active_repo_dir(), Some(shown.as_path()));
}

/// L'hôte referme un onglet qu'il avait ouvert en fond : jamais l'onglet affiché, ni un onglet occupé (dépôt qui
/// s'ouvre, opération en cours) ; l'onglet refermé n'est pas proposé par « Rouvrir l'onglet fermé » (Ctrl+Maj+T),
/// l'utilisateur ne l'a pas fermé, et il quitte les onglets enregistrés.
#[test]
fn the_host_closes_its_background_tab_unless_shown_or_busy() {
    let tmp = tempfile::tempdir().unwrap();
    let server = SilentServer::start();
    let background = repo_with_silent_remote(tmp.path(), &server);
    let shown = tmp.path().join("affiche");
    repo_with_commit(&shown);
    let mut h = harness(start(tmp.path()));
    h.state_mut().show_repo(&shown);
    h.state_mut().open_repo_tab(&background);
    assert!(!h.state_mut().close_repo_tab(&background), "il s'ouvre encore");
    assert!(!h.state_mut().close_repo_tab(&tmp.path().join("inconnu")));
    let opened =
        |h: &Harness<'static, DragonTreeApp>, p: &Path| h.state().repo_tab(p).is_some_and(|t| t.repo.is_some());
    wait_until(&mut h, "dépôts ouverts", |h| opened(h, &shown) && opened(h, &background));
    assert!(!h.state_mut().close_repo_tab(&shown), "affiché");

    // Un fetch qui ne répond pas : occupé tant qu'il tourne.
    let id = h.state_mut().queue_operation(&background, RemoteOp::Fetch).unwrap();
    wait_until(&mut h, "fetch parti", |_| server.connections() > 0);
    assert!(!h.state_mut().close_repo_tab(&background), "occupé");
    assert!(h.state().cancel_operation(&background, id));
    wait_until(&mut h, "fetch arrêté", |h| h.state().repo_tab(&background).is_some_and(|t| !t.is_busy()));

    assert!(h.state_mut().close_repo_tab(&background));
    assert!(h.state().repo_tab(&background).is_none());
    assert_eq!(h.state().active_repo_dir(), Some(shown.as_path()), "l'onglet affiché reste");
    h.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::T);
    h.run_steps(5);
    assert!(h.state().repo_tab(&background).is_none(), "pas rouvert par Ctrl+Maj+T");
    let file = tmp.path().join("profil").join("settings.json");
    wait_until(&mut h, "onglets enregistrés", |_| Settings::load_from(&file).open_tabs == [shown.clone()]);
    server.release();
}

/// L'hôte clone sans fenêtre, comme « Cloner » depuis une URL : ce que propose la fenêtre (dossier parent, comptes, le
/// compte de l'URL) lui est donné ; une demande incomplète ou une destination occupée est refusée sans rien lancer.
/// Le clonage est suivi (titre, puis ce qu'il a donné) ; réussi, le compte choisi reste celui du dépôt, son dossier
/// parent devient le dossier de clonage proposé, et le dépôt est ouvert dans un onglet de fond, en tête des récents,
/// même DragonTree masqué. Annulé, son dossier partiel est supprimé.
#[test]
fn the_host_clones_without_a_window_and_the_clone_opens_in_the_background() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    repo_with_commit(&source);
    let (root, parent, shown) = (tmp.path().join("racine"), tmp.path().join("clones"), tmp.path().join("affiche"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&parent).unwrap();
    repo_with_commit(&shown);
    let profile = tmp.path().join("profil");
    std::fs::create_dir_all(&profile).unwrap();
    let file = profile.join("settings.json");
    let settings = Settings { scan_roots: vec![root.clone()], ..Default::default() };
    std::fs::write(&file, serde_json::to_vec(&settings).unwrap()).unwrap();
    let ada = dragontree_forges::Account {
        id: "aaaaaaaaaaaaaaaa".into(),
        kind: dragontree_forges::ForgeKind::GitHub,
        host: "github.com".into(),
        username: "ada".into(),
        display_name: String::new(),
        orgs: Vec::new(),
        avatar_url: None,
    };
    seed_accounts(tmp.path(), std::slice::from_ref(&ada));
    let mut h = hidden(tmp.path());
    h.state_mut().0.show_repo(&shown);
    run_until(&mut h, "comptes lus", |a| !a.clone_defaults().accounts.is_empty());

    let defaults = h.state().0.clone_defaults();
    assert_eq!(defaults.parent, root, "sans clonage passé : le premier dossier analysé");
    assert_eq!(defaults.accounts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), [ada.id.as_str()]);
    assert_eq!(h.state().0.account_for_url(" https://github.com/ada/depot.git "), Some(ada.id.clone()));
    assert_eq!(h.state().0.account_for_url("https://gitlab.com/ada/depot.git"), None);

    let request = CloneRequest {
        url: source.display().to_string(),
        parent: parent.clone(),
        folder: "copie".into(),
        branch: Some("main".into()),
        account: Some(ada.id.clone()),
        recurse: false,
    };
    let occupied = parent.join("occupe");
    std::fs::create_dir_all(&occupied).unwrap();
    std::fs::write(occupied.join("x.txt"), "x").unwrap();
    for bad in [
        CloneRequest { url: " ".into(), ..request.clone() },
        CloneRequest { url: "--upload-pack=x".into(), ..request.clone() },
        CloneRequest { folder: "a/b".into(), ..request.clone() },
        CloneRequest { folder: "..".into(), ..request.clone() },
        CloneRequest { folder: String::new(), ..request.clone() },
        CloneRequest { parent: PathBuf::from("relatif"), ..request.clone() },
        CloneRequest { parent: tmp.path().join("absent"), ..request.clone() },
        CloneRequest { folder: "occupe".into(), ..request.clone() },
        CloneRequest { branch: Some("-x".into()), ..request.clone() },
        CloneRequest { account: Some("inconnu".into()), ..request.clone() },
    ] {
        assert!(h.state_mut().0.clone_remote(bad.clone()).is_err(), "{bad:?}");
    }
    // Ni secrets ni configuration, comme pour le pont d'Eyrie : un dossier de clés ou de jetons où qu'il soit, sans
    // casse, un dossier dont le nom commence par un point (`.config`, `.claude`…), qu'un outil lirait ; une
    // destination qui existe déjà est jugée résolue : une jonction (ou un nom court, `AWS~1`) ne cache pas un tel
    // dossier, vide.
    let keys = tmp.path().join(".AWS");
    std::fs::create_dir_all(&keys).unwrap();
    let link = parent.join("lien");
    let linked = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&keys)
        .output()
        .is_ok_and(|o| o.status.success());
    assert!(linked, "jonction {} → {}", link.display(), keys.display());
    for bad in [
        CloneRequest { folder: ".ssh".into(), ..request.clone() },
        CloneRequest { folder: ".GnuPG".into(), ..request.clone() },
        CloneRequest { folder: ".config".into(), ..request.clone() },
        CloneRequest { folder: ".claude".into(), ..request.clone() },
        CloneRequest { folder: "System Volume Information".into(), ..request.clone() },
        CloneRequest { parent: keys.clone(), ..request.clone() },
        CloneRequest { folder: "lien".into(), ..request.clone() },
    ] {
        let e = h.state_mut().0.clone_remote(bad.clone()).unwrap_err();
        assert!(e.contains("protégé"), "{bad:?} : {e}");
    }
    assert!(h.state().0.host_jobs().is_empty(), "rien de lancé");

    let id = h.state_mut().0.clone_remote(request.clone()).unwrap();
    let job = &h.state().0.host_jobs()[0];
    assert_eq!((job.id, job.title.as_str(), job.result.is_none()), (id, "Clonage de copie", true));
    let dest = parent.join("copie");
    run_until(&mut h, "clonage fini", |a| a.host_jobs()[0].result.is_some());
    assert_eq!(h.state().0.host_jobs()[0].result, Some(Ok(())));
    assert!(h.state().0.repo_tab(&dest).is_some(), "ouvert dans un onglet");
    let d = h.state().0.repo_directory();
    assert_eq!(d.active.as_deref(), Some(shown.as_path()), "l'onglet affiché ne change pas");
    assert_eq!(d.recent.first(), Some(&dest), "en tête des récents");
    assert_eq!(h.state().0.clone_defaults().parent, parent, "le dossier de ce clonage est proposé ensuite");
    let account_of = |file: &Path| Settings::load_from(file).repo_accounts.get(&long_path_key(&dest)).cloned();
    run_until(&mut h, "compte du dépôt enregistré", |_| account_of(&file) == Some(ada.id.clone()));
    assert!(h.state_mut().0.clone_remote(request.clone()).is_err(), "destination désormais occupée");

    // Un serveur qui ne répond pas : le clonage tourne jusqu'à son annulation, un second vers la même destination
    // attend qu'il soit fini.
    let server = SilentServer::start();
    let hanging = CloneRequest {
        url: format!("http://127.0.0.1:{}/depot.git", server.port),
        folder: "muet".into(),
        branch: None,
        account: None,
        ..request
    };
    let id = h.state_mut().0.clone_remote(hanging.clone()).unwrap();
    run_until(&mut h, "clonage parti", |_| server.connections() > 0);
    assert!(h.state_mut().0.clone_remote(hanging).is_err(), "déjà en cours vers ce dossier");
    assert!(h.state().0.cancel_host_job(id));
    run_until(&mut h, "clonage annulé", |a| a.host_jobs().iter().any(|j| j.id == id && j.result.is_some()));
    let job = h.state().0.host_jobs().iter().find(|j| j.id == id).cloned().unwrap();
    assert_eq!(job.result, Some(Err("Clonage annulé.".into())));
    assert!(!parent.join("muet").exists(), "dossier partiel supprimé");
    assert!(!h.state().0.cancel_host_job(id), "déjà fini");
    assert!(h.state().0.repo_tab(&parent.join("muet")).is_none());
    server.release();
}
