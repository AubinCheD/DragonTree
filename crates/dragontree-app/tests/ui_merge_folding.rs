//! Replier et déplier les fusions dans l'historique : triangle de la ligne de fusion, ← / →, menu contextuel,
//! Affichage › Replier / Déplier toutes les fusions.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

const MERGE: &str = "Fusion de feature";
const COLLAPSE: &str = "Replier : Fusion de feature";
const EXPAND: &str = "Déplier : Fusion de feature";

/// main : « Base », « Suite de main », puis « Fusion de feature » (sans avance rapide) de la branche feature
/// (« Travail 1 », « Travail 2 », partie de « Base »), supprimée ensuite.
fn merged_repo(base: &Path) -> PathBuf {
    let repo = base.join("northwind");
    init_repo(&repo);
    commit_file(&repo, "base.txt", "base\n", "Base");
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    commit_file(&repo, "f1.txt", "1\n", "Travail 1");
    commit_file(&repo, "f2.txt", "2\n", "Travail 2");
    git(&repo, &["checkout", "-q", "main"]);
    commit_file(&repo, "main.txt", "m\n", "Suite de main");
    git(&repo, &["merge", "-q", "--no-ff", "-m", MERGE, "feature"]);
    git(&repo, &["branch", "-q", "-d", "feature"]);
    repo
}

fn open(base: &Path) -> Harness<'static, DragonTreeApp> {
    let repo = merged_repo(base);
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "historique", |h| h.query_by_label("Travail 2").is_some());
    h
}

/// Ligne du graphe d'un commit : le panneau de détails d'un commit sélectionné répète son message, mais vient avant.
fn row<'h>(h: &'h Harness<'static, DragonTreeApp>, summary: &'h str) -> egui_kittest::Node<'h> {
    h.query_all_by_label(summary).last().unwrap_or_else(|| panic!("ligne « {summary} » absente"))
}

/// Le texte est affiché (ligne, ou aussi panneau de détails si son commit est sélectionné).
fn shown(h: &Harness<'static, DragonTreeApp>, text: &str) -> bool {
    h.query_all_by_label(text).next().is_some()
}

fn branch_shown(h: &Harness<'static, DragonTreeApp>) -> bool {
    shown(h, "Travail 1") && shown(h, "Travail 2")
}

fn branch_hidden(h: &Harness<'static, DragonTreeApp>) -> bool {
    !shown(h, "Travail 1") && !shown(h, "Travail 2")
}

#[test]
fn the_triangle_of_a_merge_hides_its_branch_and_shows_it_again() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open(tmp.path());
    assert!(h.query_by_label("2 commits repliés").is_none());

    click_label(&mut h, COLLAPSE);
    wait_until(&mut h, "branche repliée", branch_hidden);
    wait_until(&mut h, "nombre de commits repliés", |h| h.query_by_label("2 commits repliés").is_some());
    for summary in [MERGE, "Suite de main", "Base"] {
        assert!(shown(&h, summary), "« {summary} » reste affiché");
    }

    click_label(&mut h, EXPAND);
    wait_until(&mut h, "branche dépliée", branch_shown);
    assert!(h.query_by_label("2 commits repliés").is_none());

    // Le menu contextuel de la ligne de fusion replie aussi.
    let merge = row(&h, MERGE).rect().center();
    click_at(&mut h, merge, egui::PointerButton::Secondary);
    click_label_containing(&mut h, "Replier la fusion");
    wait_until(&mut h, "branche repliée par le menu", branch_hidden);
}

#[test]
fn left_and_right_collapse_and_expand_the_selected_merge_but_not_in_a_text_field() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open(tmp.path());
    row(&h, MERGE).click();
    h.run_steps(3);

    h.key_press(Key::ArrowLeft);
    wait_until(&mut h, "← : branche repliée", |h| branch_hidden(h) && h.query_by_label(EXPAND).is_some());
    h.key_press(Key::ArrowRight);
    wait_until(&mut h, "→ : branche dépliée", |h| branch_shown(h) && h.query_by_label(COLLAPSE).is_some());

    // Dans le champ de recherche, ← déplace le curseur : la fusion reste dépliée.
    let search = search_field(&h);
    search.focus();
    h.run_steps(2);
    h.key_press(Key::ArrowLeft);
    h.run_steps(3);
    assert!(h.query_by_label(COLLAPSE).is_some(), "← dans un champ de saisie ne replie pas la fusion");
    assert!(branch_shown(&h));
}

#[test]
fn collapse_all_moves_a_hidden_selection_to_its_merge_and_keeps_the_search_coherent() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("northwind");
    let mut h = open(tmp.path());
    let merge_sha = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
    let work_sha = git(&repo, &["rev-parse", "HEAD^2~1"]).trim().to_string();

    // Un commit de la branche est sélectionné : replier toutes les fusions le masque, la fusion prend la sélection.
    row(&h, "Travail 1").click();
    wait_until(&mut h, "détails de « Travail 1 »", |h| h.query_by_label(&work_sha).is_some());
    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Replier toutes les fusions");
    wait_until(&mut h, "toutes les fusions repliées", branch_hidden);
    wait_until(&mut h, "fusion sélectionnée", |h| h.query_by_label(&merge_sha).is_some());

    open_menu(&mut h, "Affichage");
    assert!(menu_item_disabled(&h, "Replier toutes les fusions"), "tout est déjà replié");
    assert!(!menu_item_disabled(&h, "Déplier toutes les fusions"));
    h.key_press(Key::Escape);
    h.run_steps(2);

    // La recherche porte sur la liste affichée ; déplier toutes les fusions la relance sur la liste complète.
    let search = search_field(&h);
    search.focus();
    search.type_text("Travail");
    wait_until(&mut h, "aucun résultat", |h| h.query_by_label("Aucun commit ne correspond à la recherche.").is_some());
    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Déplier toutes les fusions");
    wait_until(&mut h, "résultats de la branche dépliée", branch_shown);
    assert!(!shown(&h, "Suite de main"), "la recherche reste appliquée");
    open_menu(&mut h, "Affichage");
    assert!(menu_item_disabled(&h, "Déplier toutes les fusions"), "plus rien n'est replié");
}

/// Champ de recherche de l'historique (son texte indicatif commence par une loupe).
fn search_field<'h>(h: &'h Harness<'static, DragonTreeApp>) -> egui_kittest::Node<'h> {
    h.get_by(|n| n.placeholder().is_some_and(|p| p.ends_with("Rechercher : message, auteur, SHA")))
}

/// HEAD sur `feature`, que main a fusionnée par « Fusion de feature » (comme après le fetch d'une demande de fusion
/// acceptée sur le serveur) ; `ancienne`, fusionnée par « Fusion d'ancienne », existe encore : son commit porte
/// l'étiquette de la branche.
fn head_on_a_merged_branch(base: &Path) -> PathBuf {
    let repo = base.join("contoso");
    init_repo(&repo);
    commit_file(&repo, "base.txt", "base\n", "Base");
    git(&repo, &["checkout", "-q", "-b", "ancienne"]);
    commit_file(&repo, "a.txt", "a\n", "Ancien travail");
    git(&repo, &["checkout", "-q", "main"]);
    git(&repo, &["merge", "-q", "--no-ff", "-m", "Fusion d'ancienne", "ancienne"]);
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    commit_file(&repo, "f1.txt", "1\n", "Travail 1");
    commit_file(&repo, "f2.txt", "2\n", "Travail 2");
    git(&repo, &["checkout", "-q", "main"]);
    commit_file(&repo, "main.txt", "m\n", "Suite de main");
    git(&repo, &["merge", "-q", "--no-ff", "-m", MERGE, "feature"]);
    git(&repo, &["checkout", "-q", "feature"]);
    repo
}

#[test]
fn collapse_all_keeps_head_on_a_merged_branch_but_hides_other_labelled_tips() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = head_on_a_merged_branch(tmp.path());
    let head = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });
    wait_until(&mut h, "historique", |h| shown(h, "Travail 2") && shown(h, "Ancien travail"));
    // HEAD connu (références lues : la branche courante dans la barre latérale) : c'est lui que le repli épargne.
    wait_until(&mut h, "références lues", |h| shown(h, "feature"));

    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Replier toutes les fusions");
    // Seul HEAD reste affiché derrière une fusion repliée : l'étiquette `ancienne` est repliée avec sa fusion.
    wait_until(&mut h, "branche étiquetée « ancienne » repliée", |h| !shown(h, "Ancien travail"));
    // HEAD pointe encore sur la branche fusionnée : elle reste affichée, sa fusion seule est repliée.
    assert!(branch_shown(&h), "HEAD et sa branche restent affichés");
    assert!(h.query_by_label(EXPAND).is_some());

    h.key_press_modifiers(Modifiers::COMMAND, Key::Num0);
    wait_until(&mut h, "HEAD révélé", |h| h.query_by_label(&head).is_some());
}

/// L'ordre qui a révélé le bogue : tout replier dès que l'historique est affiché, sans attendre les références (le test
/// précédent les attend). Arrivées après, elles font refaire le repli : HEAD et sa branche finissent affichés.
#[test]
fn collapse_all_before_the_refs_are_read_still_spares_head_once_they_arrive() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = head_on_a_merged_branch(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });
    wait_until(&mut h, "historique", |h| shown(h, "Travail 2") && shown(h, "Ancien travail"));
    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Replier toutes les fusions");
    wait_until(&mut h, "références lues", |h| shown(h, "feature"));
    wait_until(&mut h, "HEAD et sa branche affichés, « ancienne » repliée", |h| {
        branch_shown(h) && !shown(h, "Ancien travail")
    });
}
