//! Outils externes : « Ouvrir dans l'outil de diff externe » (détails d'un commit) avec une commande personnalisée,
//! et la section « Outils externes » des réglages. Le faux outil est un script `.cmd` qui copie le contenu des deux
//! versions dans un fichier de contrôle.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::external_tools::ToolChoice;
use dragontree_app::settings::Settings;
use dragontree_app::{StartOptions, icons};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type H = Harness<'static, dragontree_app::DragonTreeApp>;

fn repo(base: &Path) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, "f.txt", "version 1\n", "Première version");
    commit_file(&dir, "f.txt", "version 2\n", "Deuxième version");
    dir
}

/// Faux outil de diff : écrit le contenu de `$LOCAL` puis de `$REMOTE` dans `vu.txt` ; renvoie ce fichier.
fn seed_fake_tool(base: &Path) -> PathBuf {
    let out = base.join("vu.txt");
    let script = base.join("faux outil de diff.cmd");
    let body = format!("@echo off\r\ntype \"%~1\" > \"{out}\"\r\ntype \"%~2\" >> \"{out}\"\r\n", out = out.display());
    std::fs::write(&script, body).unwrap();
    let settings = Settings {
        diff_tool: ToolChoice::Custom,
        custom_diff_tool: format!("\"{}\" $LOCAL $REMOTE", script.display()),
        ..Settings::default()
    };
    settings.save_to(&base.join("profil").join("settings.json")).unwrap();
    out
}

fn open(base: &Path, repo: &Path, ready: &str) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label(ready).is_some());
    h
}

#[test]
fn a_commit_file_opens_in_the_custom_diff_tool_with_both_versions() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let out = seed_fake_tool(tmp.path());
    let mut h = open(tmp.path(), &repo, "Deuxième version");

    h.get_by_label("Deuxième version").click();
    wait_until(&mut h, "fichier du commit", |h| h.query_by_label("f.txt").is_some());
    h.run_steps(3);
    h.get_by_label("f.txt").click_secondary();
    let entry = format!("{} Ouvrir dans l'outil de diff externe", icons::GIT_DIFF);
    wait_until(&mut h, "menu du fichier", |h| h.query_by_label(&entry).is_some());
    h.run_steps(3);
    h.get_by_label(&entry).click();
    h.run_steps(3);
    wait_git(&mut h, "outil de diff lancé sur les deux versions", || {
        std::fs::read_to_string(&out).is_ok_and(|s| s.replace("\r\n", "\n") == "version 1\nversion 2\n")
    });
}

#[test]
fn the_settings_offer_the_external_tools_section() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo, "Deuxième version");
    open_menu(&mut h, "Configuration");
    click_menu_item(&mut h, "Réglages… Ctrl+,");
    wait_until(&mut h, "section des outils externes", |h| h.query_by_label("Outils externes").is_some());
    assert!(h.query_by_label("Outil de diff").is_some());
    assert!(h.query_by_label("Outil de merge").is_some());
}
