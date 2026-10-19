//! Construction de patchs partiels pour indexer, désindexer ou annuler des lignes choisies.
//!
//! Le patch est appliqué par `git apply` : en sens direct sur l'index (Stage), en sens inverse
//! sur l'index (Unstage) ou sur la copie de travail (Discard).

use crate::diff::{FileDiff, LineKind};

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
}
