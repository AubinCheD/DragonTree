//! Indexation partielle avec l'option « Espaces » (L6) : sélection faite dans le diff affiché sans les espaces (`-w`),
//! patch construit sur le diff complet (`build_patch_ignoring_whitespace`). L'index et la copie de travail sont
//! comparés octet pour octet ; fichiers en CRLF.

mod common;

use common::TestRepo;
use dragontree_git::{
    DiffOptions, FileDiff, HunkSelection, LineKind, PatchMode, Repo, build_patch_ignoring_whitespace,
    whole_hunk_changes_whitespace,
};

const SHOWN: DiffOptions = DiffOptions { context: 3, ignore_whitespace: true };
const FULL: DiffOptions = DiffOptions { context: 3, ignore_whitespace: false };

fn open(t: &TestRepo) -> Repo {
    Repo::open(&t.git, t.path()).unwrap()
}

/// Lignes terminées par CRLF.
fn crlf(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("{l}\r\n")).collect()
}

/// Diff affiché (`-w`) et diff complet du fichier, côté indexé (`staged`) ou non.
fn diffs(repo: &Repo, path: &str, staged: bool) -> (FileDiff, FileDiff) {
    let shown = repo.diff_worktree_file(path, staged, false, &SHOWN).unwrap();
    let full = repo.diff_worktree_file(path, staged, false, &FULL).unwrap();
    (shown, full)
}

/// Applique une sélection faite dans le diff affiché.
fn apply(repo: &Repo, path: &str, staged: bool, mode: PatchMode, sel: &[HunkSelection]) {
    let (shown, full) = diffs(repo, path, staged);
    let patch = build_patch_ignoring_whitespace(&shown, &full, sel, mode.is_reverse()).unwrap().expect("patch");
    repo.apply_patch(&patch, mode).unwrap();
}

/// Octets du fichier dans l'index.
fn index_bytes(t: &TestRepo, path: &str) -> Vec<u8> {
    t.git.cmd(t.path()).args(["cat-file", "blob", &format!(":{path}")]).run().unwrap().stdout
}

/// Indices des lignes du hunk affiché qui sont des modifications de texte `text`.
fn changed_lines(diff: &FileDiff, hunk: usize, texts: &[&str]) -> Vec<usize> {
    let lines = &diff.hunks[hunk].lines;
    let found: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].kind != LineKind::Context && texts.contains(&lines[i].text.trim_end_matches('\r')))
        .collect();
    assert_eq!(found.len(), texts.len(), "{texts:?} dans {lines:#?}");
    found
}

/// Ancienne version de `form.cs` (CRLF).
const OLD: [&str; 17] = [
    "class Form",
    "{",
    "    void Load()",
    "    {",
    "        x = 1;",
    "        y = 2;",
    "    }",
    "",
    "    // un",
    "    // deux",
    "    // trois",
    "    // quatre",
    "    void Save()",
    "    {",
    "        z = 3;",
    "    }",
    "}",
];

/// Lignes de `form.cs`, avec les remplacements `(numéro de ligne, octets)` (sans fin de ligne : CRLF ajouté, sauf
/// pour une ligne qui finit déjà par `\n`).
fn form(changes: &[(usize, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, line) in OLD.iter().enumerate() {
        match changes.iter().find(|(n, _)| *n == i + 1) {
            Some((_, l)) if l.ends_with('\n') => out.extend_from_slice(l.as_bytes()),
            Some((_, l)) => out.extend_from_slice(format!("{l}\r\n").as_bytes()),
            None => out.extend_from_slice(format!("{line}\r\n").as_bytes()),
        }
    }
    out
}

/// Modifications de la copie de travail : ligne 5 changée (seule modification visible avec `-w`), et changements
/// d'espaces : ligne 4 passée en LF et ligne 6 réindentée (dans le hunk affiché), lignes 15 et 16 (hors du hunk).
const INSIDE: [(usize, &str); 3] = [(4, "    {\n"), (5, "        x = 10;"), (6, "\t\ty = 2;")];
const OUTSIDE: [(usize, &str); 2] = [(15, "\tz = 3;"), (16, "    }  ")];

/// Dépôt avec `form.cs` commité, puis modifié dans la copie de travail (`INSIDE` et `OUTSIDE`).
fn form_repo() -> (TestRepo, Vec<u8>) {
    let t = TestRepo::new();
    std::fs::write(t.file("form.cs"), form(&[])).unwrap();
    t.commit_all("init");
    let all: Vec<(usize, &str)> = INSIDE.iter().chain(OUTSIDE.iter()).copied().collect();
    let worktree = form(&all);
    std::fs::write(t.file("form.cs"), &worktree).unwrap();
    (t, worktree)
}

#[test]
fn a_whole_hunk_takes_the_whitespace_changes_inside_it_and_only_those() {
    let (t, worktree) = form_repo();
    let repo = open(&t);
    let (shown, full) = diffs(&repo, "form.cs", false);
    assert_eq!(shown.hunks.len(), 1, "seule la ligne 5 change sans les espaces");
    assert!(full.hunks.len() >= 2, "le diff complet a aussi les changements d'espaces hors du hunk");

    apply(&repo, "form.cs", false, PatchMode::Stage, &[HunkSelection { hunk: 0, lines: None }]);
    assert_eq!(index_bytes(&t, "form.cs"), form(&INSIDE));
    assert_eq!(std::fs::read(t.file("form.cs")).unwrap(), worktree, "copie de travail intacte");
}

#[test]
fn selected_lines_take_only_those_lines() {
    let (t, _) = form_repo();
    let repo = open(&t);
    let (shown, _) = diffs(&repo, "form.cs", false);
    let lines = changed_lines(&shown, 0, &["        x = 1;", "        x = 10;"]);

    apply(&repo, "form.cs", false, PatchMode::Stage, &[HunkSelection { hunk: 0, lines: Some(lines) }]);
    assert_eq!(index_bytes(&t, "form.cs"), form(&[(5, "        x = 10;")]), "lignes 4 et 6 inchangées");

    // Seulement l'ajout : la ligne 5 d'origine reste, la nouvelle s'ajoute après elle.
    t.run(&["reset", "-q"]);
    let lines = changed_lines(&shown, 0, &["        x = 10;"]);
    apply(&repo, "form.cs", false, PatchMode::Stage, &[HunkSelection { hunk: 0, lines: Some(lines) }]);
    assert_eq!(index_bytes(&t, "form.cs"), form(&[(5, "        x = 1;\r\n        x = 10;\r\n")]));
}

/// Bloc entouré d'un `if` : le diff complet supprime puis ajoute tout le bloc réindenté, le diff affiché ne montre que
/// les lignes ajoutées autour. Une ligne choisie va à la place où elle est affichée.
#[test]
fn a_selected_line_goes_where_it_is_shown_even_when_the_full_diff_orders_lines_otherwise() {
    let t = TestRepo::new();
    std::fs::write(t.file("w.cs"), crlf(&["a();", "b();", "c();"])).unwrap();
    t.commit_all("init");
    let new = crlf(&["if (ok)", "{", "    a();", "    b();", "}", "c();"]);
    std::fs::write(t.file("w.cs"), &new).unwrap();
    let repo = open(&t);
    let (shown, _) = diffs(&repo, "w.cs", false);

    let lines = changed_lines(&shown, 0, &["if (ok)", "{"]);
    apply(&repo, "w.cs", false, PatchMode::Stage, &[HunkSelection { hunk: 0, lines: Some(lines) }]);
    assert_eq!(index_bytes(&t, "w.cs"), crlf(&["if (ok)", "{", "a();", "b();", "c();"]).into_bytes());

    t.run(&["reset", "-q"]);
    apply(&repo, "w.cs", false, PatchMode::Stage, &[HunkSelection { hunk: 0, lines: None }]);
    assert_eq!(index_bytes(&t, "w.cs"), new.into_bytes());
}

/// Fin de fichier : `-w` ne voit pas le saut de ligne ajouté à la dernière ligne ; il suit le hunk entier, pas une
/// sélection de lignes.
#[test]
fn a_newline_added_at_the_end_of_the_file_follows_the_whole_hunk_only() {
    let t = TestRepo::new();
    std::fs::write(t.file("n.txt"), "un\r\ndeux").unwrap();
    t.commit_all("init");
    std::fs::write(t.file("n.txt"), "UN\r\ndeux\r\n").unwrap();
    let repo = open(&t);
    let (shown, _) = diffs(&repo, "n.txt", false);

    let lines = changed_lines(&shown, 0, &["un", "UN"]);
    apply(&repo, "n.txt", false, PatchMode::Stage, &[HunkSelection { hunk: 0, lines: Some(lines) }]);
    assert_eq!(index_bytes(&t, "n.txt"), b"UN\r\ndeux");

    t.run(&["reset", "-q"]);
    apply(&repo, "n.txt", false, PatchMode::Stage, &[HunkSelection { hunk: 0, lines: None }]);
    assert_eq!(index_bytes(&t, "n.txt"), b"UN\r\ndeux\r\n");
}

/// Désindexer et annuler suivent la même règle : le hunk entier emporte ses changements d'espaces, pas ceux d'ailleurs.
#[test]
fn unstaging_and_discarding_a_whole_hunk_follow_the_same_rule() {
    let (t, worktree) = form_repo();
    let repo = open(&t);
    t.run(&["add", "form.cs"]);
    apply(&repo, "form.cs", true, PatchMode::Unstage, &[HunkSelection { hunk: 0, lines: None }]);
    assert_eq!(index_bytes(&t, "form.cs"), form(&OUTSIDE), "changements hors du hunk toujours indexés");
    assert_eq!(std::fs::read(t.file("form.cs")).unwrap(), worktree);

    t.run(&["reset", "-q"]);
    apply(&repo, "form.cs", false, PatchMode::Discard, &[HunkSelection { hunk: 0, lines: None }]);
    assert_eq!(std::fs::read(t.file("form.cs")).unwrap(), form(&OUTSIDE), "hors du hunk : rien d'annulé");
    assert_eq!(index_bytes(&t, "form.cs"), form(&[]));
}

/// Revue finale (C3) : un hunk entier n'emporte des changements d'espaces que si l'une de ses lignes de contexte en a ;
/// le dialogue « Annuler des lignes » ne le dit que dans ce cas.
#[test]
fn a_whole_hunk_changes_whitespace_only_when_one_of_its_context_lines_does() {
    let (t, _) = form_repo();
    let repo = open(&t);
    let (shown, full) = diffs(&repo, "form.cs", false);
    assert!(whole_hunk_changes_whitespace(&shown, &full, 0), "lignes 4 et 6 dans le hunk");

    // Seule la ligne 5 change, puis s'y ajoutent des changements d'espaces hors du hunk.
    for outside in [&[][..], &OUTSIDE[..]] {
        let changes: Vec<(usize, &str)> = [(5, "        x = 10;")].iter().chain(outside).copied().collect();
        std::fs::write(t.file("form.cs"), form(&changes)).unwrap();
        let (shown, full) = diffs(&repo, "form.cs", false);
        assert_eq!(shown.hunks.len(), 1);
        assert!(!whole_hunk_changes_whitespace(&shown, &full, 0), "aucun changement d'espaces dans le hunk");
        assert!(!whole_hunk_changes_whitespace(&shown, &full, 1), "hunk absent");
    }
}
