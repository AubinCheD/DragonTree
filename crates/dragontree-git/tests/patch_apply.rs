mod common;

use common::TestRepo;
use dragontree_git::FileDiff;
use dragontree_git::diff::parse_unified_diff;
use dragontree_git::patch::{HunkSelection, PatchMode, build_patch};

fn diff_of(repo: &TestRepo, cached: bool, path: &str) -> FileDiff {
    let mut args = vec!["diff", "--no-color", "--no-ext-diff", "--no-renames"];
    if cached {
        args.push("--cached");
    }
    args.extend(["--", path]);
    parse_unified_diff(&repo.run(&args)).remove(0)
}

fn apply(repo: &TestRepo, patch: &[u8], mode: PatchMode) {
    let mut args = vec!["apply", "--recount", "--whitespace=nowarn"];
    match mode {
        PatchMode::Stage => args.push("--cached"),
        PatchMode::Unstage => args.extend(["--cached", "--reverse"]),
        PatchMode::Discard => args.push("--reverse"),
    }
    args.push("-");
    repo.git
        .cmd(repo.path())
        .args(&args)
        .stdin(patch.to_vec())
        .run()
        .unwrap_or_else(|e| panic!("git apply a échoué : {e:?}\n--- patch ---\n{}", String::from_utf8_lossy(patch)));
}

/// Indices des lignes du premier hunk dont le texte vaut `text`.
fn line_idx(d: &FileDiff, text: &str) -> usize {
    d.hunks[0].lines.iter().position(|l| l.text == text).unwrap_or_else(|| panic!("ligne {text:?} absente"))
}

fn setup() -> TestRepo {
    let repo = TestRepo::new();
    repo.write("f.txt", "1\n2\n3\n4\n5\n");
    repo.commit_all("init");
    repo.write("f.txt", "1\nDEUX\n3\nquatre\n5\nsix\n");
    repo
}

#[test]
fn stage_selected_lines_only() {
    let repo = setup();
    let d = diff_of(&repo, false, "f.txt");
    let sel = vec![line_idx(&d, "2"), line_idx(&d, "DEUX")];
    let patch = build_patch(&d, &[HunkSelection { hunk: 0, lines: Some(sel) }], false).unwrap();
    apply(&repo, &patch, PatchMode::Stage);
    assert_eq!(repo.run(&["show", ":f.txt"]), "1\nDEUX\n3\n4\n5\n");
    assert_eq!(repo.read("f.txt"), "1\nDEUX\n3\nquatre\n5\nsix\n", "la copie de travail ne bouge pas");
}

#[test]
fn unstage_selected_lines_only() {
    let repo = setup();
    repo.run(&["add", "f.txt"]);
    let d = diff_of(&repo, true, "f.txt");
    let sel = vec![line_idx(&d, "4"), line_idx(&d, "quatre")];
    let patch = build_patch(&d, &[HunkSelection { hunk: 0, lines: Some(sel) }], true).unwrap();
    apply(&repo, &patch, PatchMode::Unstage);
    assert_eq!(repo.run(&["show", ":f.txt"]), "1\nDEUX\n3\n4\n5\nsix\n");
}

#[test]
fn discard_selected_lines_only() {
    let repo = setup();
    let d = diff_of(&repo, false, "f.txt");
    let sel = vec![line_idx(&d, "six")];
    let patch = build_patch(&d, &[HunkSelection { hunk: 0, lines: Some(sel) }], true).unwrap();
    apply(&repo, &patch, PatchMode::Discard);
    assert_eq!(repo.read("f.txt"), "1\nDEUX\n3\nquatre\n5\n");
}

#[test]
fn stage_lines_preserves_crlf() {
    let repo = TestRepo::new();
    repo.write("w.cs", "a\r\nb\r\nc\r\n");
    repo.commit_all("init");
    repo.write("w.cs", "a\r\nB\r\nc\r\nd\r\n");
    let d = diff_of(&repo, false, "w.cs");
    let sel = vec![line_idx(&d, "d\r")];
    let patch = build_patch(&d, &[HunkSelection { hunk: 0, lines: Some(sel) }], false).unwrap();
    apply(&repo, &patch, PatchMode::Stage);
    assert_eq!(repo.run(&["show", ":w.cs"]), "a\r\nb\r\nc\r\nd\r\n");
}

#[test]
fn stage_lines_no_newline_at_eof() {
    let repo = TestRepo::new();
    repo.write("n.txt", "a\nL");
    repo.commit_all("init");
    repo.write("n.txt", "a\nL2\nK");
    let d = diff_of(&repo, false, "n.txt");
    let sel = vec![line_idx(&d, "K")];
    let patch = build_patch(&d, &[HunkSelection { hunk: 0, lines: Some(sel) }], false).unwrap();
    apply(&repo, &patch, PatchMode::Stage);
    assert_eq!(repo.run(&["show", ":n.txt"]), "a\nL\nK");
}

#[test]
fn stage_lines_of_untracked_file_after_intent_to_add() {
    let repo = TestRepo::new();
    repo.write("base.txt", "x\n");
    repo.commit_all("init");
    repo.write("nouveau.txt", "un\ndeux\ntrois\n");
    repo.run(&["add", "-N", "nouveau.txt"]);
    let d = diff_of(&repo, false, "nouveau.txt");
    let sel = vec![line_idx(&d, "un"), line_idx(&d, "trois")];
    let patch = build_patch(&d, &[HunkSelection { hunk: 0, lines: Some(sel) }], false).unwrap();
    apply(&repo, &patch, PatchMode::Stage);
    assert_eq!(repo.run(&["show", ":nouveau.txt"]), "un\ntrois\n");
}

#[test]
fn stage_lines_preserves_latin1_bytes() {
    // Fichier Windows-1252 (« é » = 0xE9) : l'indexation partielle ne doit pas le réencoder.
    let repo = TestRepo::new();
    std::fs::write(repo.file("l.txt"), b"// ligne\n// V\xe9rifie\n").unwrap();
    repo.commit_all("init");
    std::fs::write(repo.file("l.txt"), b"// ligne\n// V\xe9rifie\n// \xe9t\xe9\n// autre\n").unwrap();
    let r = dragontree_git::Repo::open(&repo.git, repo.path()).unwrap();
    let d = r.diff_worktree_file("l.txt", false, false, &dragontree_git::DiffOptions::default()).unwrap();
    let idx = d.hunks[0].lines.iter().position(|l| l.text.contains("été")).expect("ligne décodée pour l'affichage");
    let patch = build_patch(&d, &[HunkSelection { hunk: 0, lines: Some(vec![idx]) }], false).unwrap();
    r.apply_patch(&patch, PatchMode::Stage).unwrap();
    assert_eq!(r.blob("", "l.txt").unwrap(), b"// ligne\n// V\xe9rifie\n// \xe9t\xe9\n");
}
