//! Le « + » de la liste des dépôts : ouvrir un dossier, cloner un dépôt distant ou ajouter un dossier à scanner ; et,
//! pour un clonage par URL, le compte qui l'authentifie.

mod common;

use common::*;
use egui_kittest::kittest::{NodeT, Queryable};

fn plus_menu(h: &mut egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>) {
    wait_until(h, "« + » de la liste des dépôts", |h| h.query_by_label("Ajouter un dépôt…").is_some());
    h.get_by_label("Ajouter un dépôt…").click();
    h.run_steps(3);
}

#[test]
fn the_plus_of_repositories_offers_folder_and_clone() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));
    // L'écran d'accueil propose aussi d'ouvrir un dépôt ou d'ajouter un dossier : le menu en ajoute un de chaque.
    h.run_steps(3);
    let items = ["Ouvrir un dépôt…", "Cloner un dépôt distant…", "Ajouter un dossier à scanner…"];
    let before: Vec<usize> = items.iter().map(|i| h.query_all_by_label_contains(i).count()).collect();
    plus_menu(&mut h);
    for (item, n) in items.iter().zip(before) {
        assert_eq!(h.query_all_by_label_contains(item).count(), n + 1, "{item}");
    }
    click_label_containing(&mut h, "Cloner un dépôt distant…");
    wait_until(&mut h, "fenêtre Cloner", |h| h.query_by_label_contains("Depuis une URL").is_some());
}

#[test]
fn cloning_by_url_preselects_the_matching_account() {
    let tmp = tempfile::tempdir().unwrap();
    let ada = dragontree_forges::Account {
        id: "aaaaaaaaaaaaaaaa".into(),
        kind: dragontree_forges::ForgeKind::GitHub,
        host: "github.com".into(),
        username: "ada".into(),
        display_name: String::new(),
        orgs: Vec::new(),
        avatar_url: None,
    };
    seed_accounts(tmp.path(), &[ada]);
    let mut h = harness(start(tmp.path()));
    plus_menu(&mut h);
    click_label_containing(&mut h, "Cloner un dépôt distant…");
    wait_until(&mut h, "fenêtre Cloner", |h| h.query_by_label_contains("Depuis une URL").is_some());
    click_label_containing(&mut h, "Depuis une URL");
    h.run_steps(3);
    // Le champ de l'URL : le premier champ de texte sous « URL du dépôt ».
    fn url_field<'h>(
        h: &'h egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>,
    ) -> Option<egui_kittest::Node<'h>> {
        let below = h.query_by_label_contains("URL du dépôt")?.rect().bottom() - 1.0;
        h.query_all_by_role(egui::accesskit::Role::TextInput)
            .filter(|n| n.rect().top() >= below)
            .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
    }
    wait_until(&mut h, "champ de l'URL", |h| url_field(h).is_some());
    url_field(&h).unwrap().focus();
    h.run_steps(2);
    url_field(&h).unwrap().type_text("https://github.com/ada/depot.git");
    h.run_steps(3);
    let account = |h: &egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>| {
        h.query_by_role_and_label(egui::accesskit::Role::ComboBox, "Compte").and_then(|n| n.accesskit_node().value())
    };
    wait_until(&mut h, "compte présélectionné", |h| account(h).is_some_and(|v| v.contains("ada")));
}
