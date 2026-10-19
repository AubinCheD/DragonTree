//! Commandes personnalisées : lancées depuis Dépôt › Commandes personnalisées et depuis les menus contextuels des
//! commits, branches et fichiers ; script exécuté dans le dépôt ou URL ouverte, confirmation, fenêtre de sortie,
//! ajout dans Réglages › Commandes.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::custom_commands::{CommandKind, CommandTarget, CustomCommand};
use dragontree_app::settings::Settings;
use dragontree_app::{DragonTreeApp, StartOptions, icons};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type H = Harness<'static, DragonTreeApp>;

fn settings_path(base: &Path) -> PathBuf {
    base.join("profil").join("settings.json")
}

/// Profil de test dont les réglages contiennent ces commandes.
fn seed(base: &Path, commands: Vec<CustomCommand>) {
    let settings = Settings { custom_commands: commands, ..Settings::default() };
    settings.save_to(&settings_path(base)).unwrap();
}

fn command(name: &str, target: CommandTarget, kind: CommandKind, line: &str) -> CustomCommand {
    CustomCommand { name: name.into(), target, kind, command: line.into(), ..Default::default() }
}

fn repo(base: &Path) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    commit_file(&dir, "a.txt", "2\n", "Second commit");
    dir
}

fn open(base: &Path, repo: &Path, view: &str) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some(view.into()), ..start(base) });
    wait_until(&mut h, "branche lue", |h| h.query_all_by_label("main").next().is_some());
    h
}

/// Ouvre le sous-menu « Commandes personnalisées » du menu contextuel affiché.
fn open_context_submenu(h: &mut H) {
    click(h, &format!("{} Commandes personnalisées", icons::TERMINAL_WINDOW));
}

/// Clique l'élément `label` dès qu'il est utilisable, en une image : un menu ou un dialogue qui vient de s'ouvrir
/// peut encore se replacer.
fn click(h: &mut H, label: &str) {
    click_label(h, label);
    h.run_steps(3);
}

/// Champ de saisie dont le texte indicatif est `hint` : on y tape `text`.
fn type_in(h: &mut H, hint: &str, text: &str) {
    wait_until(h, &format!("champ « {hint} »"), |h| h.query_by(|n| n.placeholder() == Some(hint)).is_some());
    let field = h.get_by(|n| n.placeholder() == Some(hint));
    field.focus();
    field.type_text(text);
    h.run_steps(2);
}

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[test]
fn a_script_command_on_a_commit_writes_its_sha_in_the_repository() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    seed(
        tmp.path(),
        vec![
            command("Écrire le SHA", CommandTarget::Commit, CommandKind::Script, "echo ${sha} > out.txt"),
            command("Sur un fichier", CommandTarget::File, CommandKind::Script, "echo ${file} > fichier.txt"),
        ],
    );
    let first = git(&repo, &["rev-parse", "HEAD~1"]).trim().to_string();
    let mut h = open(tmp.path(), &repo, "history");
    wait_until(&mut h, "commit affiché", |h| h.query_by_label("Premier commit").is_some());
    right_click_label(&mut h, "Premier commit");
    open_context_submenu(&mut h);
    wait_until(&mut h, "commande du commit", |h| h.query_by_label("Écrire le SHA").is_some());
    assert!(h.query_by_label("Sur un fichier").is_none(), "une commande de fichier n'est pas proposée sur un commit");
    click(&mut h, "Écrire le SHA");
    wait_git(&mut h, "SHA écrit dans le dépôt", || read(&repo.join("out.txt")).is_some_and(|s| s.trim() == first));
}

#[test]
fn a_script_command_on_a_changed_file_receives_its_path() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src").join("b c.txt"), "neuf\n").unwrap();
    seed(
        tmp.path(),
        vec![command("Nom du fichier", CommandTarget::File, CommandKind::Script, "echo ${file}> nom.txt")],
    );
    let mut h = open(tmp.path(), &repo, "changes");
    // La ligne de la liste (un bouton), pas le titre du diff qui porte le même nom une fois le fichier sélectionné.
    let row = |h: &H| {
        h.query_by(|n| n.label().as_deref() == Some("src/b c.txt") && n.role() == egui::accesskit::Role::Button)
            .is_some()
    };
    wait_until(&mut h, "fichier non suivi", row);
    click_by(&mut h, "ligne du fichier", egui::PointerButton::Secondary, |n| {
        n.label().as_deref() == Some("src/b c.txt") && n.role() == egui::accesskit::Role::Button
    });
    open_context_submenu(&mut h);
    click(&mut h, "Nom du fichier");
    // Chemin Windows, cité pour cmd (il contient une espace).
    wait_git(&mut h, "chemin écrit", || read(&repo.join("nom.txt")).is_some_and(|s| s.trim() == r#""src\b c.txt""#));
}

#[test]
fn a_url_command_on_a_branch_opens_the_url_with_the_branch_encoded() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    git(&repo, &["branch", "essai&co"]);
    seed(
        tmp.path(),
        vec![command(
            "Voir sur la forge",
            CommandTarget::Branch,
            CommandKind::Url,
            "https://forge.contoso.test/tree/${branch}",
        )],
    );
    let mut h = open(tmp.path(), &repo, "history");
    wait_until(&mut h, "branche listée", |h| h.query_by_label("essai&co").is_some());
    right_click_label(&mut h, "essai&co");
    open_context_submenu(&mut h);
    // L'URL n'est demandée que pendant une image : on la guette dès celle du clic.
    click_label(&mut h, "Voir sur la forge");
    let url = |h: &H| {
        h.output().platform_output.commands.iter().find_map(|c| match c {
            egui::OutputCommand::OpenUrl(u) => Some(u.url.clone()),
            _ => None,
        })
    };
    let mut opened = url(&h);
    if opened.is_none() {
        wait_until(&mut h, "URL ouverte", |h| {
            opened = url(h);
            opened.is_some()
        });
    }
    assert_eq!(opened.as_deref(), Some("https://forge.contoso.test/tree/essai%26co"));
}

#[test]
fn a_repository_command_asks_for_confirmation_then_shows_its_output() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut hello = command("Dire bonjour", CommandTarget::Repo, CommandKind::Script, "echo bonjour depuis ${branch}");
    hello.confirm = true;
    hello.show_output = true;
    seed(tmp.path(), vec![hello]);
    let mut h = open(tmp.path(), &repo, "history");
    open_menu(&mut h, "Dépôt");
    open_menu(&mut h, "Commandes personnalisées");
    click_menu_item(&mut h, "Dire bonjour");
    // La confirmation montre la ligne complète, variables remplacées ; rien n'est lancé avant le clic.
    wait_until(&mut h, "confirmation", |h| h.query_by_label("echo bonjour depuis main").is_some());
    assert!(h.query_by_label("bonjour depuis main").is_none());
    click(&mut h, "Exécuter");
    wait_until(&mut h, "fenêtre de sortie", |h| {
        h.query_by_label("bonjour depuis main").is_some() && h.query_by_label("Code de sortie : 0").is_some()
    });
}

#[test]
fn a_command_added_in_the_settings_is_saved_and_offered_in_the_repository_menu() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo, "history");
    // Sans commande, le sous-menu le dit et mène aux réglages, directement sur l'onglet des commandes.
    open_menu(&mut h, "Dépôt");
    open_menu(&mut h, "Commandes personnalisées");
    assert!(menu_item_disabled(&h, "Aucune commande"));
    click_menu_item(&mut h, "Gérer les commandes…");
    click(&mut h, &format!("{} Ajouter une commande", icons::PLUS));
    type_in(&mut h, "Nom dans les menus", "Lister les fichiers");
    type_in(&mut h, "Ligne de commande, ex. notepad ${file}", "dir /b > liste.txt");
    click(&mut h, "Enregistrer");
    let path = settings_path(tmp.path());
    wait_until(&mut h, "commande enregistrée", |_| !Settings::load_from(&path).custom_commands.is_empty());
    let saved = Settings::load_from(&path).custom_commands;
    assert_eq!(saved, [command("Lister les fichiers", CommandTarget::Repo, CommandKind::Script, "dir /b > liste.txt")]);

    open_menu(&mut h, "Dépôt");
    open_menu(&mut h, "Commandes personnalisées");
    click_menu_item(&mut h, "Lister les fichiers");
    wait_git(&mut h, "commande exécutée", || read(&repo.join("liste.txt")).is_some_and(|s| s.contains("a.txt")));
}

#[test]
fn a_line_that_reads_a_dt_variable_is_refused_and_flagged_in_the_editor() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    seed(tmp.path(), vec![command("Lire DT_SHA", CommandTarget::Repo, CommandKind::Script, "echo %dt_sha%> lu.txt")]);
    let mut h = open(tmp.path(), &repo, "history");
    open_menu(&mut h, "Dépôt");
    open_menu(&mut h, "Commandes personnalisées");
    click_menu_item(&mut h, "Lire DT_SHA");
    // cmd développerait `%DT_SHA%` avant d'analyser la ligne : la commande n'est pas lancée.
    wait_until(&mut h, "refus signalé", |h| h.query_by_label("« Lire DT_SHA » n'a pas été lancée").is_some());
    assert!(!becomes_true(&mut h, std::time::Duration::from_millis(500), || repo.join("lu.txt").exists()));
    // L'éditeur le signale aussi.
    open_menu(&mut h, "Dépôt");
    open_menu(&mut h, "Commandes personnalisées");
    click_menu_item(&mut h, "Gérer les commandes…");
    wait_until(&mut h, "avertissement de l'éditeur", |h| h.query_by_label_contains("Ne lisez pas %DT_").is_some());
}

#[test]
fn a_line_that_reads_cmd_s_own_command_line_is_refused_and_flagged_in_the_editor() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let line = "echo %CMDCMDLINE% ${repo}> lu.txt";
    seed(tmp.path(), vec![command("Ligne de cmd", CommandTarget::Repo, CommandKind::Script, line)]);
    let mut h = open(tmp.path(), &repo, "history");
    open_menu(&mut h, "Dépôt");
    open_menu(&mut h, "Commandes personnalisées");
    click_menu_item(&mut h, "Ligne de cmd");
    // La ligne brute de cmd contient les valeurs citées : ses guillemets décaleraient ceux de la ligne.
    wait_until(&mut h, "refus signalé", |h| h.query_by_label("« Ligne de cmd » n'a pas été lancée").is_some());
    assert!(!becomes_true(&mut h, std::time::Duration::from_millis(500), || repo.join("lu.txt").exists()));
    open_menu(&mut h, "Dépôt");
    open_menu(&mut h, "Commandes personnalisées");
    click_menu_item(&mut h, "Gérer les commandes…");
    wait_until(&mut h, "avertissement de l'éditeur", |h| {
        h.query_by_label_contains("Ne lisez pas CMDCMDLINE").is_some()
    });
}
