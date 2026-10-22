//! L2 : conflit dans un fichier Windows-1252 (« ANSI » du vieux code .NET français), résolu région par région sur
//! les octets : le texte est décodé pour l'affichage, et le fichier écrit reprend les octets d'origine (hors conflit
//! et côtés choisis), sans réencodage.

mod common;

use common::TestRepo;
use dragontree_git::conflict::{
    Resolution, decode_conflict_text, parse_conflicts, resolve_bytes, resolve_partial_bytes,
};

/// Octets Windows-1252 : é = E9, € = 80, œ = 9C, ’ = 92, à = E0 ; une ligne en CRLF.
fn cp1252(lines: &[&[u8]]) -> Vec<u8> {
    lines.concat()
}

const HEAD: &[u8] = b"// Fen\xeatre principale\r\n";
/// Assez de lignes communes entre les deux conflits pour que git ne les réunisse pas.
const MIDDLE: &[u8] = b"// L\x92\x9cuvre compl\xe8te\n// d\xe9but\n// suite\n// encore\n// fin du milieu\n";
const TAIL: &[u8] = b"// Fin du fichier \xe0 garder\n";

/// Dépôt en fusion arrêtée sur deux conflits dans `form.cs`, fichier Windows-1252 (non UTF-8).
fn conflicted() -> (TestRepo, Vec<u8>, Vec<u8>) {
    let t = TestRepo::new();
    let base = cp1252(&[HEAD, b"titre = \"R\xe9glages\"\n", MIDDLE, b"prix = \"10 \x80\"\n", TAIL]);
    std::fs::write(t.file("form.cs"), &base).unwrap();
    t.commit_all("base");
    t.run(&["checkout", "-q", "-b", "autre"]);
    let theirs = cp1252(&[HEAD, b"titre = \"Pr\xe9f\xe9rences\"\n", MIDDLE, b"prix = \"12 \x80\"\n", TAIL]);
    std::fs::write(t.file("form.cs"), &theirs).unwrap();
    t.commit_all("leur version");
    t.run(&["checkout", "-q", "main"]);
    let ours = cp1252(&[HEAD, b"titre = \"Param\xe8tres\"\n", MIDDLE, b"prix = \"11 \x80\"\n", TAIL]);
    std::fs::write(t.file("form.cs"), &ours).unwrap();
    t.commit_all("notre version");
    assert!(t.try_run(&["merge", "autre"]).is_err(), "la fusion doit s'arrêter sur les conflits");
    let conflicted = std::fs::read(t.file("form.cs")).unwrap();
    assert!(std::str::from_utf8(&conflicted).is_err(), "fichier de test non UTF-8");
    (t, ours, theirs)
}

#[test]
fn a_cp1252_conflict_is_resolved_region_by_region_keeping_the_original_bytes() {
    let (t, _, _) = conflicted();
    let bytes = std::fs::read(t.file("form.cs")).unwrap();

    // Affichage : décodé en Windows-1252, deux régions lisibles.
    let text = decode_conflict_text(&bytes);
    assert!(text.starts_with("// Fenêtre principale\r\n"), "{text}");
    assert!(text.contains("// L’œuvre complète\n"), "{text}");
    let regions = parse_conflicts(&text);
    assert_eq!(regions.len(), 2);
    assert_eq!(regions[0].ours, ["titre = \"Paramètres\""]);
    assert_eq!(regions[0].theirs, ["titre = \"Préférences\""]);
    assert_eq!(regions[1].ours, ["prix = \"11 €\""]);
    assert_eq!(regions[1].theirs, ["prix = \"12 €\""]);

    // Région par région : la première seule choisie garde les marqueurs de la seconde, octets d'origine compris.
    let partial = resolve_partial_bytes(&bytes, &[Some(Resolution::Ours), None]);
    assert!(partial.starts_with(&cp1252(&[HEAD, b"titre = \"Param\xe8tres\"\n", MIDDLE, b"<<<<<<< HEAD\n"])));
    let resolved = resolve_bytes(&bytes, &[Resolution::Ours, Resolution::Theirs]);
    let expected = cp1252(&[HEAD, b"titre = \"Param\xe8tres\"\n", MIDDLE, b"prix = \"12 \x80\"\n", TAIL]);
    assert_eq!(resolved, expected, "octets hors conflit et côtés choisis recopiés tels quels");

    // Les deux côtés, dans les deux ordres.
    let both = resolve_bytes(&bytes, &[Resolution::Both, Resolution::BothReversed]);
    let expected = cp1252(&[
        HEAD,
        b"titre = \"Param\xe8tres\"\ntitre = \"Pr\xe9f\xe9rences\"\n",
        MIDDLE,
        b"prix = \"12 \x80\"\nprix = \"11 \x80\"\n",
        TAIL,
    ]);
    assert_eq!(both, expected);

    // Le fichier écrit se valide : git voit le contenu choisi, octet pour octet.
    std::fs::write(t.file("form.cs"), &resolved).unwrap();
    t.run(&["add", "form.cs"]);
    t.run(&["commit", "-q", "--no-edit"]);
    let committed = t.git.cmd(t.path()).args(["show", "HEAD:form.cs"]).run().unwrap().stdout;
    assert_eq!(committed, resolved);
}

/// Tout notre côté, ou tout le leur : le fichier de chaque branche, à l'octet près.
#[test]
fn choosing_one_side_everywhere_gives_back_that_branch_file_byte_for_byte() {
    let (t, ours, theirs) = conflicted();
    let bytes = std::fs::read(t.file("form.cs")).unwrap();
    assert_eq!(resolve_bytes(&bytes, &[Resolution::Ours, Resolution::Ours]), ours);
    assert_eq!(resolve_bytes(&bytes, &[Resolution::Theirs, Resolution::Theirs]), theirs);
}
