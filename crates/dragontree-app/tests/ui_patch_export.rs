//! Copier comme patch : un commit, un stash, des fichiers de la vue Changements.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::StartOptions;
use dragontree_app::icons;
use egui_kittest::kittest::Queryable;

type H = egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>;

fn copy_patch_label() -> String {
    format!("{} Copier comme patch", icons::COPY)
}

fn open(repo: &Path, base: &Path, view: &str) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some(view.into()), ..start(base) });
    wait_until(&mut h, "branche lue", |h| h.query_all_by_label("main").next().is_some());
    h
}

/// Ligne de la liste des fichiers modifiés : un bouton, pas le titre du diff qui porte le même nom de fichier.
fn file_row<'h>(h: &'h H, name: &'h str) -> Option<egui_kittest::Node<'h>> {
    h.query_by(move |n| n.label().as_deref() == Some(name) && n.role() == egui::accesskit::Role::Button)
}

/// Fait tourner l'interface jusqu'à ce qu'un texte soit copié dans le presse-papiers (dès l'image du clic).
fn wait_copied(h: &mut H) -> String {
    let copied = |h: &H| {
        h.output().platform_output.commands.iter().find_map(|cmd| match cmd {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
    };
    let mut text = copied(h);
    if text.is_none() {
        wait_until(h, "texte copié dans le presse-papiers", |h| {
            text = copied(h);
            text.is_some()
        });
    }
    text.unwrap()
}

/// Le patch s'applique (`git apply --check`) dans `repo` tel qu'il est.
fn applies(repo: &Path, patch: &str, tmp: &Path) -> bool {
    let file: PathBuf = tmp.join("copie.patch");
    std::fs::write(&file, patch).unwrap();
    try_git(repo, &["apply", "--check", file.to_str().unwrap()]).is_some()
}

#[test]
fn copy_as_patch_on_a_commit_puts_a_valid_patch_in_the_clipboard() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("fabrikam");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "1\n", "Commit de base");
    commit_file(&repo, "a.txt", "1\n2\n", "Ajoute une ligne");
    let mut h = open(&repo, tmp.path(), "history");
    wait_until(&mut h, "commit affiché", |h| h.query_by_label("Ajoute une ligne").is_some());
    right_click_label(&mut h, "Ajoute une ligne");
    click_label(&mut h, &copy_patch_label());
    let patch = wait_copied(&mut h);
    assert!(patch.starts_with("From "), "{patch}");
    assert!(patch.contains("Subject: [PATCH] Ajoute une ligne") && patch.contains("+2"), "{patch}");
    // Le patch s'applique sur la base du commit.
    git(&repo, &["checkout", "-q", "--detach", "HEAD~1"]);
    assert!(applies(&repo, &patch, tmp.path()));
}

#[test]
fn copy_as_patch_on_a_stash_includes_its_untracked_files() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("northwind");
    init_repo(&repo);
    commit_file(&repo, "f.txt", "base\n", "Commit de base");
    std::fs::write(repo.join("f.txt"), "modifié\n").unwrap();
    std::fs::write(repo.join("neuf.txt"), "non suivi\n").unwrap();
    git(&repo, &["stash", "push", "-q", "-u", "-m", "mon stash"]);
    let mut h = open(&repo, tmp.path(), "history");
    let row = "stash@{0}  On main: mon stash";
    wait_until(&mut h, "stash listé", |h| h.query_by_label(row).is_some());
    right_click_label(&mut h, row);
    click_label(&mut h, &copy_patch_label());
    let patch = wait_copied(&mut h);
    assert!(patch.contains("+modifié") && patch.contains("+non suivi"), "{patch}");
    assert!(applies(&repo, &patch, tmp.path()));
}

#[test]
fn copy_as_patch_on_changed_files_copies_only_the_selected_ones() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("contoso");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "a\n", "Commit de base");
    commit_file(&repo, "b.txt", "b\n", "Second commit");
    std::fs::write(repo.join("a.txt"), "a2\n").unwrap();
    std::fs::write(repo.join("b.txt"), "b2\n").unwrap();
    let mut h = open(&repo, tmp.path(), "changes");
    wait_until(&mut h, "fichiers modifiés", |h| file_row(h, "a.txt").is_some() && file_row(h, "b.txt").is_some());
    let row = file_row(&h, "a.txt").unwrap().rect().center();
    click_at(&mut h, row, egui::PointerButton::Secondary);
    click_label(&mut h, &copy_patch_label());
    let patch = wait_copied(&mut h);
    assert!(patch.contains("+a2") && !patch.contains("b2"), "{patch}");
    git(&repo, &["checkout", "--", "a.txt"]);
    assert!(applies(&repo, &patch, tmp.path()));
}
