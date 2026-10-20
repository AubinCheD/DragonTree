//! Outils externes : `git mergetool` avec un outil décrit par un gabarit (faux outil `.cmd` qui écrit la résolution
//! dans `$MERGED`) et `git difftool` avec l'outil configuré dans git.

mod common;

use std::path::{Path, PathBuf};

use common::TestRepo;
use dragontree_git::{GitError, MergeMode, Repo, ToolCommand};

/// Dossier d'outils hors du dépôt (avec des espaces dans son nom : le programme est cité) et faux outil de merge :
/// copie `$REMOTE` (leur version) dans `$MERGED`. Les chemins de git ont des `/` : ils sont convertis pour `copy`.
fn fake_merge_tool(dir: &Path) -> PathBuf {
    let tools = dir.join("outils externes");
    std::fs::create_dir_all(&tools).unwrap();
    let script = tools.join("prend leur version.cmd");
    let lines = [
        "@echo off",
        r#"set "SRC=%~2""#,
        r#"set "DST=%~4""#,
        r#"set "SRC=%SRC:/=\%""#,
        r#"set "DST=%DST:/=\%""#,
        r#"copy /Y "%SRC%" "%DST%" >nul"#,
    ];
    std::fs::write(&script, lines.join("\r\n") + "\r\n").unwrap();
    script
}

/// Faux outil de merge en script sh (pas de cmd.exe) : copie `$REMOTE` dans `$MERGED`.
fn fake_sh_tool(dir: &Path) -> PathBuf {
    let script = dir.join("prend leur version.sh");
    std::fs::write(&script, "cp \"$2\" \"$4\"\n").unwrap();
    script
}

/// Conflit sur `file` : ours / theirs.
fn conflicted(file: &str) -> (TestRepo, Repo) {
    let t = TestRepo::new();
    t.write(file, "base\n");
    t.commit_all("base");
    t.run(&["checkout", "-q", "-b", "other"]);
    t.write(file, "theirs\n");
    t.commit_all("theirs");
    t.run(&["checkout", "-q", "main"]);
    t.write(file, "ours\n");
    t.commit_all("ours");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    assert!(repo.merge("other", MergeMode::Default).is_err());
    (t, repo)
}

fn template_for(script: &Path) -> ToolCommand {
    ToolCommand::parse(&format!("\"{}\" $LOCAL $REMOTE $BASE $MERGED", script.display())).unwrap()
}

#[test]
fn mergetool_with_a_template_resolves_the_conflict_with_the_fake_tool() {
    let (t, repo) = conflicted("f");
    let tools = tempfile::tempdir().unwrap();
    let tool = template_for(&fake_merge_tool(tools.path()));
    // Aucune configuration de git n'est lue ni écrite : tout passe par `-c`.
    repo.run_mergetool_with("dragontree-test", &tool, "f", true).unwrap();
    assert_eq!(t.read("f"), "theirs\n");
    assert!(repo.status().unwrap().conflicted().is_empty());
    assert_eq!(t.try_run(&["config", "--get", "mergetool.dragontree-test.cmd"]).unwrap_or_default(), "");
}

#[test]
fn mergetool_paths_with_shell_metacharacters_are_not_interpreted() {
    // Le fichier de conflit porte un nom que cmd.exe et sh interpréteraient ; le programme est un script sh.
    let name = "a b&c $(x) 'q' `y`.txt";
    let (t, repo) = conflicted(name);
    let tools = tempfile::tempdir().unwrap();
    let script = fake_sh_tool(tools.path()).display().to_string().replace('\\', "/");
    let tool = ToolCommand::parse(&format!("sh \"{script}\" $LOCAL $REMOTE $BASE $MERGED")).unwrap();
    repo.run_mergetool_with("dragontree-test", &tool, name, true).unwrap();
    assert_eq!(t.read(name), "theirs\n");
    assert!(repo.status().unwrap().conflicted().is_empty());
    assert!(!t.file("x").exists() && !t.file("calc").exists());
}

#[test]
fn a_cmd_script_tool_is_refused_on_a_file_name_cmd_would_interpret_but_runs_on_other_names() {
    let tools = tempfile::tempdir().unwrap();
    let tool = template_for(&fake_merge_tool(tools.path()));

    let (t, repo) = conflicted("a&calc.txt");
    let err = repo.run_mergetool_with("dragontree-test", &tool, "a&calc.txt", true).unwrap_err();
    assert!(matches!(err, GitError::Refused(_)), "{err:?}");
    assert!(t.read("a&calc.txt").contains("<<<<<<<"), "git n'a pas été lancé");
    // Un blanc fait perdre ses guillemets à `cmd /c` quand le programme en a aussi ; `%`, `!`, `^` et les
    // parenthèses sont interprétés par cmd.exe : refusés de la même façon.
    for name in ["a b.txt", "a%PATH%b.txt", "a!b.txt", "a^b.txt", "a(b).txt"] {
        let (t, repo) = conflicted(name);
        let err = repo.run_mergetool_with("dragontree-test", &tool, name, true);
        assert!(matches!(err, Err(GitError::Refused(_))), "{name} : {err:?}");
        assert!(t.read(name).contains("<<<<<<<"), "{name} : git n'a pas été lancé");
    }

    // Apostrophes et `$` restent sans danger pour cmd.exe : l'outil s'exécute.
    let name = "a'q'$x.txt";
    let (t, repo) = conflicted(name);
    repo.run_mergetool_with("dragontree-test", &tool, name, true).unwrap();
    assert_eq!(t.read(name), "theirs\n");
}

#[test]
fn a_failing_tool_leaves_the_conflict_in_place() {
    let (t, repo) = conflicted("f");
    // `/c` ne doit pas être converti en `C:/` par le sh de git.
    let tool = ToolCommand::parse("cmd.exe /c exit 1").unwrap();
    assert!(repo.run_mergetool_with("dragontree-test", &tool, "f", true).is_err());
    assert_eq!(repo.status().unwrap().conflicted().len(), 1);
    assert!(t.read("f").contains("<<<<<<<"));
}

#[test]
fn an_invalid_tool_name_is_refused_before_running_git() {
    let (_t, repo) = conflicted("f");
    let tool = ToolCommand::parse("prog").unwrap();
    assert!(matches!(repo.run_mergetool_with("a b", &tool, "f", true), Err(GitError::Refused(_))));
}

#[test]
fn difftool_runs_the_tool_configured_in_git_on_revisions_and_on_the_working_copy() {
    let t = TestRepo::new();
    t.write("f", "v1\n");
    t.commit_all("v1");
    t.write("f", "v2\n");
    t.commit_all("v2");
    t.run(&["config", "diff.tool", "fake"]);
    t.run(&["config", "difftool.fake.cmd", "cat \"$LOCAL\" \"$REMOTE\" > vu.txt"]);
    let repo = Repo::open(&t.git, t.path()).unwrap();

    repo.run_difftool(&["HEAD~1", "HEAD"], false, "f").unwrap();
    assert_eq!(t.read("vu.txt"), "v1\nv2\n", "entre deux révisions : ancienne puis récente");

    // Copie de travail : l'index à gauche, le fichier à droite.
    t.write("f", "v3\n");
    repo.run_difftool(&[], false, "f").unwrap();
    assert_eq!(t.read("vu.txt"), "v2\nv3\n");

    // Version indexée : HEAD à gauche, l'index à droite.
    t.run(&["add", "f"]);
    repo.run_difftool(&[], true, "f").unwrap();
    assert_eq!(t.read("vu.txt"), "v2\nv3\n");
}

/// Outil sh qui ne touche pas à `$MERGED` et note le chemin de `$LOCAL` (`vu.txt`) : il « réussit » (code 0) sans
/// avoir résolu le conflit.
fn lazy_sh_tool(dir: &Path) -> ToolCommand {
    let script = dir.join("paresseux.sh");
    std::fs::write(&script, "echo \"$1\" > vu.txt\nexit 0\n").unwrap();
    let script = script.display().to_string().replace('\\', "/");
    ToolCommand::parse(&format!("sh \"{script}\" $LOCAL $REMOTE $BASE $MERGED")).unwrap()
}

#[test]
fn an_unchanged_file_is_not_taken_for_resolved_unless_the_exit_code_is_trusted() {
    let tools = tempfile::tempdir().unwrap();
    let tool = lazy_sh_tool(tools.path());

    // Code de sortie 0 mais fichier inchangé : sans confiance, git ne le tient pas pour résolu.
    let (t, repo) = conflicted("f");
    assert!(repo.run_mergetool_with("dragontree-test", &tool, "f", false).is_err());
    assert_eq!(repo.status().unwrap().conflicted().len(), 1);
    assert!(t.read("f").contains("<<<<<<<"));

    // Avec la confiance, le code 0 suffit : le fichier est indexé tel quel, marqueurs compris. C'est pourquoi la
    // confiance n'est donnée qu'aux outils dont le code de sortie est fiable.
    let (t, repo) = conflicted("f");
    repo.run_mergetool_with("dragontree-test", &tool, "f", true).unwrap();
    assert!(repo.status().unwrap().conflicted().is_empty());
    assert!(t.read("f").contains("<<<<<<<"));
}

#[test]
fn the_tool_files_are_never_written_to_a_temporary_folder_even_if_the_user_configured_it() {
    let tools = tempfile::tempdir().unwrap();
    let tool = lazy_sh_tool(tools.path());
    let (t, repo) = conflicted("f");
    t.run(&["config", "mergetool.writeToTemp", "true"]);
    let _ = repo.run_mergetool_with("dragontree-test", &tool, "f", true);
    let seen = t.read("vu.txt");
    assert!(seen.trim().starts_with("./f_LOCAL_"), "{seen}");
}

#[test]
fn a_template_whose_text_has_runs_of_blanks_is_refused_for_the_merge() {
    let (t, repo) = conflicted("f");
    for template in ["prog \"a  b\" $MERGED", "prog \"a\tb\" $MERGED"] {
        let tool = ToolCommand::parse(template).unwrap();
        let err = repo.run_mergetool_with("dragontree-test", &tool, "f", true);
        assert!(matches!(err, Err(GitError::Refused(_))), "{template} : {err:?}");
    }
    assert!(t.read("f").contains("<<<<<<<"));
}

#[test]
fn file_versions_are_read_with_their_filters_and_a_missing_file_is_none() {
    let t = TestRepo::new();
    t.run(&["config", "core.autocrlf", "true"]);
    t.write(".gitattributes", "*.txt text\n");
    t.write("f.txt", "a\nb\n");
    t.commit_all("v1");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    // Dans la base, les fins de ligne sont LF ; la version « copie de travail » est en CRLF (autocrlf).
    let head = repo.file_version("HEAD", "f.txt").unwrap().unwrap();
    assert_eq!(String::from_utf8(head).unwrap(), "a\r\nb\r\n");
    assert_eq!(repo.file_version("", "f.txt").unwrap().map(|b| b.len()), Some(6));
    assert_eq!(repo.file_version("HEAD", "absent.txt").unwrap(), None);
    assert_eq!(repo.file_version("", "absent.txt").unwrap(), None);
    // Une révision inconnue est une vraie erreur, pas un côté vide.
    assert!(repo.file_version("0123456789abcdef0123456789abcdef01234567", "f.txt").is_err());
}
