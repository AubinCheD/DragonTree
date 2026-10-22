//! Construction de patchs partiels pour indexer, désindexer ou annuler des lignes choisies.
//!
//! Le patch est appliqué par `git apply` : en sens direct sur l'index (Stage), en sens inverse
//! sur l'index (Unstage) ou sur la copie de travail (Discard).
//!
//! Avec l'option « Espaces », la sélection est faite dans un diff sans les espaces (`-w`) et le patch est construit
//! sur le diff complet : voir [`build_patch_ignoring_whitespace`] (règle de correspondance, refus).

use std::collections::HashMap;

use dragontree_i18n::tr;

use crate::diff::{DiffLine, FileDiff, Hunk, LineKind};
use crate::error::GitError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchMode {
    /// Diff index → copie de travail appliqué à l'index.
    Stage,
    /// Diff HEAD → index appliqué à l'envers à l'index.
    Unstage,
    /// Diff index → copie de travail appliqué à l'envers à la copie de travail.
    Discard,
}

impl PatchMode {
    pub fn is_reverse(&self) -> bool {
        !matches!(self, PatchMode::Stage)
    }
}

/// Sélection dans un hunk : `None` pour le hunk entier, sinon les indices dans `hunk.lines`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkSelection {
    pub hunk: usize,
    pub lines: Option<Vec<usize>>,
}

/// Construit un patch ne contenant que les lignes sélectionnées.
///
/// Sens direct : `+`/`-` sélectionnés gardés, `-` non sélectionné → contexte, `+` non sélectionné
/// → supprimé. Sens inverse : `+` non sélectionné → contexte, `-` non sélectionné → supprimé.
/// Renvoie `None` si la sélection ne contient aucune modification.
pub fn build_patch(diff: &FileDiff, sel: &[HunkSelection], reverse: bool) -> Option<Vec<u8>> {
    let mut sel: Vec<&HunkSelection> = sel.iter().filter(|s| s.hunk < diff.hunks.len()).collect();
    sel.sort_by_key(|s| s.hunk);
    sel.dedup_by_key(|s| s.hunk);

    let mut body: Vec<u8> = Vec::new();
    let mut delta: i64 = 0;
    let mut covers_everything = sel.len() == diff.hunks.len();

    for s in &sel {
        let hunk = &diff.hunks[s.hunk];
        let selected = |i: usize| s.lines.as_ref().is_none_or(|v| v.contains(&i));

        let mut out: Vec<PLine> = Vec::with_capacity(hunk.lines.len());
        for (i, l) in hunk.lines.iter().enumerate() {
            let tag = match (l.kind, selected(i)) {
                (LineKind::Context, _) => ' ',
                (LineKind::Added, true) => '+',
                (LineKind::Removed, true) => '-',
                (LineKind::Removed, false) if !reverse => ' ',
                (LineKind::Added, false) if reverse => ' ',
                _ => {
                    covers_everything = false;
                    continue;
                }
            };
            if tag == ' ' && l.kind != LineKind::Context {
                covers_everything = false;
            }
            out.push(PLine { tag, text: l.bytes(), no_nl: l.no_newline });
        }
        if !out.iter().any(|l| l.tag != ' ') {
            continue;
        }
        fix_missing_newlines(&mut out);

        let old_count = out.iter().filter(|l| l.tag != '+').count() as i64;
        let new_count = out.iter().filter(|l| l.tag != '-').count() as i64;
        // Le côté de référence (ancien en sens direct, nouveau en sens inverse) est inchangé :
        // sa position d'origine est exacte ; l'autre s'en déduit avec le décalage cumulé.
        let (old_start, new_start) = if reverse {
            let ns = i64::from(hunk.new_start);
            (other_start(ns, new_count, old_count, -delta), ns)
        } else {
            let os = i64::from(hunk.old_start);
            (os, other_start(os, old_count, new_count, delta))
        };
        delta += new_count - old_count;

        let suffix = hunk.header.splitn(3, "@@").nth(2).unwrap_or("");
        body.extend_from_slice(format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@{suffix}\n").as_bytes());
        for l in &out {
            body.push(l.tag as u8);
            body.extend_from_slice(l.text);
            body.push(b'\n');
            if l.no_nl {
                body.extend_from_slice(b"\\ No newline at end of file\n");
            }
        }
    }

    if body.is_empty() {
        return None;
    }
    let mut patch: Vec<u8> = Vec::new();
    if covers_everything {
        for h in &diff.header_lines {
            patch.extend_from_slice(h.as_bytes());
            patch.push(b'\n');
        }
    } else {
        // Sélection partielle : simple modification (pas de création/suppression de fichier).
        let old = diff.old_path.as_deref().or(diff.new_path.as_deref()).unwrap_or("");
        let new = diff.new_path.as_deref().or(diff.old_path.as_deref()).unwrap_or("");
        patch.extend_from_slice(format!("diff --git a/{old} b/{new}\n--- a/{old}\n+++ b/{new}\n").as_bytes());
    }
    patch.extend_from_slice(&body);
    Some(patch)
}

/// Patch d'une sélection faite dans un diff affiché sans les espaces (`-w`, option « Espaces », L6).
///
/// `shown` est le diff affiché, `full` le diff complet du même fichier (sans `-w`), lus ensemble ; `sel` désigne des
/// hunks et des lignes de `shown`. Un patch tiré de `shown` ne s'appliquerait pas : ses lignes de contexte sont celles
/// de la nouvelle version, qui peuvent différer de l'ancienne par des espaces. Le patch est donc construit sur les
/// lignes du diff complet (octets exacts, `\r` et fin de fichier sans saut de ligne compris), dans l'ordre du hunk
/// affiché : chaque ligne de contexte affichée dont les deux versions diffèrent (par des espaces seulement, puisque
/// `-w` les a égalées) y devient une suppression suivie d'un ajout. L'ordre est celui que l'utilisateur voit, même là
/// où le diff complet range les lignes autrement (un bloc réindenté y est tout supprimé puis tout ajouté).
///
/// Règle de correspondance :
/// - hunk entier (« Indexer le hunk », « Désindexer le hunk », « Annuler le hunk ») : toutes les modifications du hunk
///   affiché, y compris les changements d'espaces de ses lignes de contexte, c'est-à-dire des lignes des deux versions
///   qu'il couvre ; jamais ceux d'en dehors des hunks affichés ;
/// - lignes choisies : ces lignes seulement, à leur place dans le hunk affiché ; les changements d'espaces des lignes
///   de contexte ne sont pas pris (ces lignes gardent leur version de départ).
///
/// Refus (`GitError::Refused`, rien n'est appliqué) plutôt qu'un patch douteux : ligne de contexte affichée dont une
/// version ne se retrouve pas dans le diff complet (les deux diffs n'alignent pas les lignes de la même façon), ligne
/// dont le texte diffère d'un diff à l'autre ou deux versions qui diffèrent autrement que par des espaces (fichier
/// modifié entre les deux lectures), diff complet tronqué ou binaire.
pub fn build_patch_ignoring_whitespace(
    shown: &FileDiff,
    full: &FileDiff,
    sel: &[HunkSelection],
    reverse: bool,
) -> Result<Option<Vec<u8>>, GitError> {
    if full.truncated || full.is_binary {
        return Err(GitError::Refused(
            tr!(
                "« Espaces » : le diff complet du fichier n'a pas pu être lu en entier (fichier trop gros ou binaire). Rien n'a été modifié ; désactivez « Espaces » pour choisir des lignes."
            )
            .into(),
        ));
    }
    let full_lines = FullLines::new(full);
    let mut sel: Vec<&HunkSelection> = sel.iter().filter(|s| s.hunk < shown.hunks.len()).collect();
    sel.sort_by_key(|s| s.hunk);
    sel.dedup_by_key(|s| s.hunk);

    let mut exact = FileDiff {
        old_path: full.old_path.clone(),
        new_path: full.new_path.clone(),
        header_lines: exact_header(full),
        is_new: full.is_new,
        is_deleted: full.is_deleted,
        ..FileDiff::default()
    };
    let mut exact_sel = Vec::with_capacity(sel.len());
    for s in sel {
        let hunk = &shown.hunks[s.hunk];
        let (exact_hunk, map) = exact_hunk(hunk, &full_lines)?;
        // Lignes choisies : seules les modifications affichées comptent (un contexte n'est pas sélectionnable).
        let chosen = s.lines.as_ref().map(|chosen| {
            chosen
                .iter()
                .filter(|&&i| hunk.lines.get(i).is_some_and(|l| l.kind != LineKind::Context))
                .flat_map(|&i| map[i].iter().copied())
                .collect()
        });
        exact_sel.push(HunkSelection { hunk: exact.hunks.len(), lines: chosen });
        exact.hunks.push(exact_hunk);
    }
    Ok(build_patch(&exact, &exact_sel, reverse))
}

/// En-tête du diff exact : celui du diff complet pour une création ou une suppression de fichier (un seul hunk, le
/// même avec ou sans `-w`), sinon une simple modification, comme une sélection partielle (`build_patch`) : les hunks
/// exacts ne couvrent pas forcément tout le diff complet (changements d'espaces hors des hunks affichés), et un
/// changement de mode n'est pas affiché.
fn exact_header(full: &FileDiff) -> Vec<String> {
    if full.is_new || full.is_deleted {
        return full.header_lines.clone();
    }
    let old = full.old_path.as_deref().or(full.new_path.as_deref()).unwrap_or("");
    let new = full.new_path.as_deref().or(full.old_path.as_deref()).unwrap_or("");
    vec![format!("diff --git a/{old} b/{new}"), format!("--- a/{old}"), format!("+++ b/{new}")]
}

/// Lignes du diff complet, par numéro dans l'ancienne et dans la nouvelle version.
struct FullLines<'a> {
    old: HashMap<u32, &'a DiffLine>,
    new: HashMap<u32, &'a DiffLine>,
    hunks: &'a [Hunk],
}

impl<'a> FullLines<'a> {
    fn new(full: &'a FileDiff) -> FullLines<'a> {
        let (mut old, mut new) = (HashMap::new(), HashMap::new());
        for line in full.hunks.iter().flat_map(|h| &h.lines) {
            if let Some(k) = line.old_no {
                old.insert(k, line);
            }
            if let Some(j) = line.new_no {
                new.insert(j, line);
            }
        }
        FullLines { old, new, hunks: &full.hunks }
    }

    /// Ligne de la nouvelle version identique à la ligne `k` de l'ancienne, quand `k` est hors des hunks du diff
    /// complet : décalée des lignes ajoutées et supprimées par les hunks qui la précèdent (un hunk sans ancienne ligne
    /// se place après sa ligne `old_start`).
    fn unchanged_partner(&self, k: u32) -> i64 {
        let shift: i64 = self
            .hunks
            .iter()
            .filter(|h| h.old_start + h.old_count.max(1) <= k)
            .map(|h| i64::from(h.new_count) - i64::from(h.old_count))
            .sum();
        i64::from(k) + shift
    }
}

/// Hunk exact du hunk affiché `shown` : mêmes lignes, même ordre, octets du diff complet ; une ligne de contexte dont
/// les deux versions diffèrent y devient une suppression suivie d'un ajout. Renvoie aussi, pour chaque ligne affichée,
/// ses indices dans le hunk exact.
fn exact_hunk(shown: &Hunk, full: &FullLines<'_>) -> Result<(Hunk, Vec<Vec<usize>>), GitError> {
    let mut lines: Vec<DiffLine> = Vec::with_capacity(shown.lines.len());
    let mut map = Vec::with_capacity(shown.lines.len());
    for l in &shown.lines {
        let first = lines.len();
        match (l.kind, l.old_no, l.new_no) {
            (LineKind::Removed, Some(k), _) => lines.push(same_line(l, full.old.get(&k).copied(), k)?),
            (LineKind::Added, _, Some(j)) => lines.push(same_line(l, full.new.get(&j).copied(), j)?),
            (LineKind::Context, Some(k), Some(j)) => match (full.old.get(&k), full.new.get(&j)) {
                (Some(old), Some(new)) if old.bytes() == new.bytes() && old.no_newline == new.no_newline => {
                    lines.push(with_kind(old, LineKind::Context, Some(k), Some(j)));
                }
                (Some(old), Some(new)) if equal_ignoring_whitespace(old.bytes(), new.bytes()) => {
                    lines.push(with_kind(old, LineKind::Removed, Some(k), None));
                    lines.push(with_kind(new, LineKind::Added, None, Some(j)));
                }
                (Some(_), Some(_)) => return Err(changed_meanwhile(j)),
                // Hors des hunks du diff complet : ligne inchangée, si le diff complet l'aligne sur la même.
                (None, None) if full.unchanged_partner(k) == i64::from(j) => {
                    lines.push(with_kind(l, LineKind::Context, Some(k), Some(j)));
                }
                _ => return Err(not_aligned(j)),
            },
            _ => return Err(not_aligned(l.new_no.or(l.old_no).unwrap_or(0))),
        }
        map.push((first..lines.len()).collect());
    }
    let hunk = Hunk {
        old_start: shown.old_start,
        old_count: shown.old_count,
        new_start: shown.new_start,
        new_count: shown.new_count,
        header: shown.header.clone(),
        lines,
    };
    Ok((hunk, map))
}

/// Ligne supprimée ou ajoutée affichée, avec les octets du diff complet quand il la contient (même texte exigé) :
/// une ligne supprimée ou ajoutée de `-w` est déjà celle du fichier (seuls les contextes viennent de l'autre version).
fn same_line(shown: &DiffLine, full: Option<&DiffLine>, line: u32) -> Result<DiffLine, GitError> {
    match full {
        Some(f) if f.bytes() == shown.bytes() => Ok(with_kind(f, shown.kind, shown.old_no, shown.new_no)),
        Some(_) => Err(changed_meanwhile(line)),
        None => Ok(shown.clone()),
    }
}

fn with_kind(line: &DiffLine, kind: LineKind, old_no: Option<u32>, new_no: Option<u32>) -> DiffLine {
    DiffLine { kind, old_no, new_no, ..line.clone() }
}

/// Égalité de `git diff -w` : les blancs (espace, tabulation, `\r`, saut de page…) ne comptent pas.
fn equal_ignoring_whitespace(a: &[u8], b: &[u8]) -> bool {
    let blank = |c: &&u8| matches!(**c, b' ' | b'\t' | b'\n' | b'\x0B' | b'\x0C' | b'\r');
    a.iter().filter(|c| !blank(c)).eq(b.iter().filter(|c| !blank(c)))
}

fn not_aligned(line: u32) -> GitError {
    GitError::Refused(tr!(
        "« Espaces » : la ligne {line} du hunk affiché ne se retrouve pas dans le diff complet (les deux diffs n'alignent pas les lignes de la même façon). Rien n'a été modifié ; désactivez « Espaces » pour choisir ces lignes.",
        line
    ))
}

fn changed_meanwhile(line: u32) -> GitError {
    GitError::Refused(tr!(
        "« Espaces » : la ligne {line} diffère entre le diff affiché et le diff complet (fichier modifié entre-temps ?). Rien n'a été modifié ; recommencez une fois le diff rechargé.",
        line
    ))
}

struct PLine<'a> {
    tag: char,
    text: &'a [u8],
    no_nl: bool,
}

/// Position de départ du côté dérivé : une plage vide pointe sur la ligne qui précède.
fn other_start(ref_start: i64, ref_count: i64, count: i64, delta: i64) -> i64 {
    let first_line = if ref_count == 0 { ref_start + 1 } else { ref_start };
    let start = first_line + delta;
    if count == 0 { (start - 1).max(0) } else { start.max(1) }
}

/// Un marqueur « pas de fin de ligne » n'est valide que sur la dernière ligne de son côté.
/// Une ligne de contexte marquée mais suivie de lignes d'un seul côté est scindée en `-`/`+`.
fn fix_missing_newlines(out: &mut Vec<PLine<'_>>) {
    let mut i = 0;
    while i < out.len() {
        if out[i].no_nl {
            let later_old = out[i + 1..].iter().any(|l| l.tag != '+');
            let later_new = out[i + 1..].iter().any(|l| l.tag != '-');
            match out[i].tag {
                ' ' if later_old || later_new => {
                    let text = out[i].text;
                    out[i] = PLine { tag: '-', text, no_nl: !later_old };
                    out.insert(i + 1, PLine { tag: '+', text, no_nl: !later_new });
                    i += 1;
                }
                '-' if later_old => out[i].no_nl = false,
                '+' if later_new => out[i].no_nl = false,
                _ => {}
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse_unified_diff;

    fn diff(lines: &[&str]) -> FileDiff {
        let mut text = lines.join("\n");
        text.push('\n');
        parse_unified_diff(&text).remove(0)
    }

    fn sample() -> FileDiff {
        diff(&[
            "diff --git a/f.txt b/f.txt",
            "index 1111111..2222222 100644",
            "--- a/f.txt",
            "+++ b/f.txt",
            "@@ -1,3 +1,4 @@ ctx",
            " un",
            "-deux",
            "+DEUX",
            "+deux bis",
            " trois",
        ])
    }

    fn s(p: Vec<u8>) -> String {
        String::from_utf8(p).unwrap()
    }

    fn hunk_body(patch: &str) -> Vec<&str> {
        let start = patch.find("@@").unwrap();
        patch[start..].lines().collect()
    }

    #[test]
    fn patch_whole_hunk() {
        let p = s(build_patch(&sample(), &[HunkSelection { hunk: 0, lines: None }], false).unwrap());
        assert!(p.starts_with("diff --git a/f.txt b/f.txt\n"));
        assert!(p.contains("\n--- a/f.txt\n+++ b/f.txt\n"));
        assert_eq!(hunk_body(&p), vec!["@@ -1,3 +1,4 @@ ctx", " un", "-deux", "+DEUX", "+deux bis", " trois"]);
        assert!(p.ends_with('\n'));
    }

    #[test]
    fn patch_forward_drops_unselected_adds() {
        // Indices : 0 " un", 1 "-deux", 2 "+DEUX", 3 "+deux bis", 4 " trois"
        let p = s(build_patch(&sample(), &[HunkSelection { hunk: 0, lines: Some(vec![3]) }], false).unwrap());
        assert_eq!(hunk_body(&p), vec!["@@ -1,3 +1,4 @@ ctx", " un", " deux", "+deux bis", " trois"]);
    }

    #[test]
    fn patch_reverse_keeps_unselected_adds_as_context() {
        let p = s(build_patch(&sample(), &[HunkSelection { hunk: 0, lines: Some(vec![2]) }], true).unwrap());
        assert_eq!(hunk_body(&p), vec!["@@ -1,3 +1,4 @@ ctx", " un", "+DEUX", " deux bis", " trois"]);
    }

    #[test]
    fn patch_header_counts() {
        let p = s(build_patch(&sample(), &[HunkSelection { hunk: 0, lines: Some(vec![1]) }], false).unwrap());
        assert_eq!(hunk_body(&p), vec!["@@ -1,3 +1,2 @@ ctx", " un", "-deux", " trois"]);
    }

    #[test]
    fn patch_none_when_empty() {
        assert!(build_patch(&sample(), &[HunkSelection { hunk: 0, lines: Some(vec![0, 4]) }], false).is_none());
        assert!(build_patch(&sample(), &[], false).is_none());
    }

    #[test]
    fn patch_second_hunk_start_follows_delta() {
        let d = diff(&[
            "diff --git a/g b/g",
            "--- a/g",
            "+++ b/g",
            "@@ -1,2 +1,3 @@",
            " a",
            "+b",
            " c",
            "@@ -10,2 +11,2 @@",
            " x",
            "-y",
            "+z",
        ]);
        // Sens direct : on n'indexe que la 2e modification ; l'ancien côté (index) ne bouge pas.
        let p = s(build_patch(&d, &[HunkSelection { hunk: 1, lines: None }], false).unwrap());
        assert!(p.contains("@@ -10,2 +10,2 @@\n x\n-y\n+z\n"), "{p}");
        // Sens inverse avec les deux hunks : le nouveau côté est la référence.
        let both = [HunkSelection { hunk: 0, lines: None }, HunkSelection { hunk: 1, lines: None }];
        let p = s(build_patch(&d, &both, true).unwrap());
        assert!(p.contains("@@ -1,2 +1,3 @@\n"), "{p}");
        assert!(p.contains("@@ -10,2 +11,2 @@\n"), "{p}");
    }

    #[test]
    fn patch_no_newline_marker_kept_on_whole_hunk() {
        let d = diff(&[
            "diff --git a/n b/n",
            "--- a/n",
            "+++ b/n",
            "@@ -1,2 +1,2 @@",
            " a",
            "-L",
            "\\ No newline at end of file",
            "+M",
            "\\ No newline at end of file",
        ]);
        let p = s(build_patch(&d, &[HunkSelection { hunk: 0, lines: None }], false).unwrap());
        assert_eq!(
            hunk_body(&p),
            vec!["@@ -1,2 +1,2 @@", " a", "-L", "\\ No newline at end of file", "+M", "\\ No newline at end of file"]
        );
    }

    #[test]
    fn patch_no_newline_split_when_context_line_is_followed_by_additions() {
        // index : "a\nL" (sans \n final) ; copie de travail : "a\nL2\nK" (sans \n final).
        let d = diff(&[
            "diff --git a/n b/n",
            "--- a/n",
            "+++ b/n",
            "@@ -1,2 +1,3 @@",
            " a",
            "-L",
            "\\ No newline at end of file",
            "+L2",
            "+K",
            "\\ No newline at end of file",
        ]);
        // On n'indexe que « +K » : L reste, K s'ajoute après lui → L doit gagner un \n.
        let p = s(build_patch(&d, &[HunkSelection { hunk: 0, lines: Some(vec![3]) }], false).unwrap());
        assert_eq!(
            hunk_body(&p),
            vec![
                "@@ -1,2 +1,3 @@",
                " a",
                "-L",
                "\\ No newline at end of file",
                "+L",
                "+K",
                "\\ No newline at end of file"
            ]
        );
    }

    #[test]
    fn patch_keeps_carriage_returns() {
        let d = diff(&["diff --git a/w b/w", "--- a/w", "+++ b/w", "@@ -1 +1,2 @@", " a\r", "+b\r"]);
        let p = s(build_patch(&d, &[HunkSelection { hunk: 0, lines: None }], false).unwrap());
        assert!(p.contains("\n a\r\n+b\r\n"), "{p:?}");
    }

    #[test]
    fn patch_partial_new_file_uses_modification_header() {
        let d = diff(&[
            "diff --git a/n.txt b/n.txt",
            "new file mode 100644",
            "index 0000000..1111111",
            "--- /dev/null",
            "+++ b/n.txt",
            "@@ -0,0 +1,2 @@",
            "+one",
            "+two",
        ]);
        let full = s(build_patch(&d, &[HunkSelection { hunk: 0, lines: None }], false).unwrap());
        assert!(full.contains("new file mode 100644"));
        assert!(full.contains("--- /dev/null"));
        let partial = s(build_patch(&d, &[HunkSelection { hunk: 0, lines: Some(vec![0]) }], false).unwrap());
        assert!(!partial.contains("new file mode"));
        assert!(partial.contains("--- a/n.txt\n+++ b/n.txt\n"));
        assert_eq!(hunk_body(&partial), vec!["@@ -0,0 +1,1 @@", "+one"]);
    }

    // --- « Espaces » (L6) : diff affiché sans les espaces, patch sur le diff complet -----------------------------

    /// Lignes du patch à partir du premier hunk, `\r` compris (`str::lines` les retirerait).
    fn raw_body(patch: &str) -> Vec<&str> {
        let start = patch.find("@@").unwrap();
        patch[start..].split_terminator('\n').collect()
    }

    fn refused(r: Result<Option<Vec<u8>>, GitError>) -> String {
        match r {
            Err(GitError::Refused(msg)) => msg,
            other => panic!("attendu un refus, obtenu {other:?}"),
        }
    }

    /// Diff affiché (`-w`) : le contexte « b » vient de la nouvelle version (sans `\r`), « x » est changé.
    fn shown_ws() -> FileDiff {
        diff(&["diff --git a/f b/f", "--- a/f", "+++ b/f", "@@ -1,3 +1,3 @@", " a\r", " b", "-x\r", "+X\r"])
    }

    /// Diff complet correspondant : la ligne 2 est aussi passée de CRLF à LF.
    fn full_ws() -> FileDiff {
        diff(&["diff --git a/f b/f", "--- a/f", "+++ b/f", "@@ -1,3 +1,3 @@", " a\r", "-b\r", "-x\r", "+b", "+X\r"])
    }

    #[test]
    fn whitespace_patch_keeps_the_shown_order_with_the_full_diff_bytes() {
        let whole = [HunkSelection { hunk: 0, lines: None }];
        let p = s(build_patch_ignoring_whitespace(&shown_ws(), &full_ws(), &whole, false).unwrap().unwrap());
        assert!(p.starts_with("diff --git a/f b/f\n--- a/f\n+++ b/f\n"), "{p:?}");
        assert_eq!(raw_body(&p), vec!["@@ -1,3 +1,3 @@", " a\r", "-b\r", "+b", "-x\r", "+X\r"]);

        // Lignes choisies (« -x », « +X ») : la ligne 2 garde son CRLF d'origine.
        let lines = [HunkSelection { hunk: 0, lines: Some(vec![2, 3]) }];
        let p = s(build_patch_ignoring_whitespace(&shown_ws(), &full_ws(), &lines, false).unwrap().unwrap());
        assert_eq!(raw_body(&p), vec!["@@ -1,3 +1,3 @@", " a\r", " b\r", "-x\r", "+X\r"]);

        // Un contexte sélectionné n'emporte rien ; en sens inverse (désindexer, annuler), même hunk exact.
        let context_only = [HunkSelection { hunk: 0, lines: Some(vec![1]) }];
        assert_eq!(build_patch_ignoring_whitespace(&shown_ws(), &full_ws(), &context_only, false).unwrap(), None);
        let p = s(build_patch_ignoring_whitespace(&shown_ws(), &full_ws(), &whole, true).unwrap().unwrap());
        assert_eq!(raw_body(&p), vec!["@@ -1,3 +1,3 @@", " a\r", "-b\r", "+b", "-x\r", "+X\r"]);
    }

    #[test]
    fn whitespace_patch_refuses_a_context_line_the_full_diff_aligns_elsewhere() {
        // Diff complet : une ligne ajoutée en tête, le reste décalé d'une ligne.
        let full = diff(&["diff --git a/f b/f", "--- a/f", "+++ b/f", "@@ -1 +1,2 @@", "+z", " a"]);
        // Diff affiché qui aligne la ligne 4 de l'ancienne version sur la ligne 4 de la nouvelle.
        let shown = diff(&["diff --git a/f b/f", "--- a/f", "+++ b/f", "@@ -4,2 +4,2 @@", " d", "-e", "+E"]);
        let whole = [HunkSelection { hunk: 0, lines: None }];
        let msg = refused(build_patch_ignoring_whitespace(&shown, &full, &whole, false));
        assert!(msg.contains("ligne 4") && msg.contains("Rien n'a été modifié"), "{msg}");
    }

    #[test]
    fn whitespace_patch_refuses_lines_that_differ_between_the_two_diffs() {
        let whole = [HunkSelection { hunk: 0, lines: None }];
        // Ligne supprimée dont le texte n'est pas celui du diff complet.
        let shown = diff(&["diff --git a/f b/f", "--- a/f", "+++ b/f", "@@ -1,2 +1,2 @@", " a", "-autre", "+B"]);
        let full = diff(&["diff --git a/f b/f", "--- a/f", "+++ b/f", "@@ -1,2 +1,2 @@", " a", "-b", "+B"]);
        assert!(refused(build_patch_ignoring_whitespace(&shown, &full, &whole, false)).contains("ligne 2"));
        // Contexte affiché dont les deux versions diffèrent autrement que par des espaces.
        let shown = diff(&["diff --git a/f b/f", "--- a/f", "+++ b/f", "@@ -1,3 +1,3 @@", " a", "-x", "+X", " b"]);
        let full = diff(&["diff --git a/f b/f", "--- a/f", "+++ b/f", "@@ -1,3 +1,3 @@", " a", "-x", "-b", "+X", "+c"]);
        assert!(refused(build_patch_ignoring_whitespace(&shown, &full, &whole, false)).contains("ligne 3"));
        // Diff complet tronqué : rien à quoi se fier.
        let truncated = FileDiff { truncated: true, hunks: Vec::new(), ..full_ws() };
        refused(build_patch_ignoring_whitespace(&shown_ws(), &truncated, &whole, false));
    }
}
