//! Coloration syntaxique des diffs (syntect, chargé à la première utilisation), calculée hors du thread de
//! l'interface.
//!
//! Les deux côtés du diff sont reconstitués (ancien : contexte + supprimées ; nouveau : contexte + ajoutées) puis
//! colorés ligne à ligne, ce qui conserve l'état du parseur. Les plages portent sur le texte affiché, tabulations
//! développées (C6, `expand_tabs`) : la largeur de tabulation fait partie de la clé.
//!
//! Le calcul tourne dans une tâche de fond ; en attendant, le texte s'affiche sans couleur, puis une nouvelle image
//! est demandée à l'arrivée du résultat (une seule fois : rien ne relance le calcul tant que le diff, le thème et la
//! largeur de tabulation restent les mêmes). Un calcul devenu inutile (autre diff, vue fermée) est interrompu. Les
//! résultats vont dans un cache LRU partagé de `CACHE_ENTRIES` diffs, indexé par le contenu (empreinte du chemin et
//! de toutes les lignes, calculée une fois au chargement du diff : [`LoadedDiff::content_hash`]), le thème et la
//! largeur de tabulation : revenir à un diff déjà vu est immédiat, et une ligne modifiée est recolorée même si le
//! nombre de lignes ne change pas.
//!
//! Seuils : synchrone, la coloration s'arrêtait à 3 000 lignes, qui figeaient déjà l'interface environ une
//! demi-seconde. En arrière-plan, le seuil (`MAX_LINES`) passe à 20 000 lignes, la valeur du plan initial : environ
//! 3,5 s de calcul pour 20 000 lignes de code dense, pendant lesquelles le texte reste lisible et l'interface
//! fluide. S'y ajoutent une taille totale (`MAX_BYTES`) et une taille de ligne (`MAX_LINE_BYTES`) : syntect colore
//! une ligne d'un seul tenant, sans voir l'annulation, et une ligne de code minifié de quelques centaines de Kio
//! l'occupe des secondes. Une extension sans syntaxe ne lance aucune tâche (une fois les syntaxes chargées). Le cache
//! est aussi borné en lignes (`CACHE_LINES`) pour la mémoire.

use std::collections::VecDeque;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use dragontree_git::{FileDiff, LineKind};
use egui::Color32;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::diff_style::{LoadedDiff, expand_tabs};

/// Au-delà, pas de coloration (voir la documentation du module).
const MAX_LINES: usize = 20_000;
/// Au-delà de cette taille du texte d'un diff, en octets, pas de coloration (environ 4 à 6 s de calcul).
const MAX_BYTES: usize = 1024 * 1024;
/// Un diff dont une ligne dépasse cette taille, en octets, n'est pas coloré (code minifié, fichier généré) : syntect
/// colore une ligne d'un seul tenant, sans voir l'annulation, à environ 5 ms par Kio.
const MAX_LINE_BYTES: usize = 10_000;
/// Nombre de diffs gardés dans le cache.
const CACHE_ENTRIES: usize = 64;
/// Nombre total de lignes colorées gardées dans le cache (le diff le plus récent reste toujours).
const CACHE_LINES: usize = 100_000;

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();

/// Syntaxes de syntect, chargées au premier appel (une tâche de fond : le chargement prend du temps).
fn syntaxes() -> &'static SyntaxSet {
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Extension du fichier d'un diff, en minuscules : la syntaxe en dépend.
fn extension(path: &str) -> String {
    std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Syntaxe d'une extension ; quelques extensions courantes sous Windows renvoient à une syntaxe voisine.
fn syntax_for<'a>(ss: &'a SyntaxSet, ext: &str) -> Option<&'a SyntaxReference> {
    match ext {
        "csproj" | "props" | "targets" | "config" | "resx" | "xaml" | "nuspec" => ss.find_syntax_by_extension("xml"),
        "ts" | "tsx" | "mjs" | "cjs" => ss.find_syntax_by_extension("js"),
        "cshtml" | "razor" => ss.find_syntax_by_extension("html"),
        ext => ss.find_syntax_by_extension(ext),
    }
}

/// Le diff se colore : ni vide, ni trop gros (`MAX_LINES`, `MAX_BYTES`, `MAX_LINE_BYTES`), ni d'une extension sans
/// syntaxe. Tant que les syntaxes ne sont pas chargées (elles le sont par la première coloration, en arrière-plan),
/// l'extension n'est pas vérifiée ici : la tâche de fond, qui les charge, le fait.
fn colorable(diff: &LoadedDiff) -> bool {
    let size = (1..=MAX_LINES).contains(&diff.line_count())
        && diff.byte_count() <= MAX_BYTES
        && diff.longest_line() <= MAX_LINE_BYTES;
    size && SYNTAXES.get().is_none_or(|ss| syntax_for(ss, &extension(diff.display_path())).is_some())
}

fn theme(dark: bool) -> &'static Theme {
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    let set = THEMES.get_or_init(ThemeSet::load_defaults);
    let name = if dark { "base16-ocean.dark" } else { "InspiredGitHub" };
    set.themes.get(name).or_else(|| set.themes.values().next()).expect("thèmes syntect par défaut")
}

/// Couleur d'une plage d'octets du texte affiché d'une ligne.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub color: Color32,
}

impl Span {
    pub fn range(&self) -> Range<usize> {
        self.start as usize..self.end as usize
    }
}

/// Couleurs d'un diff : les plages de chaque ligne, hunk après hunk (`starts` : première ligne de chaque hunk).
#[derive(Debug, Default)]
struct Colors {
    starts: Vec<usize>,
    lines: Vec<Box<[Span]>>,
}

impl Colors {
    fn line(&self, h: usize, l: usize) -> Option<&[Span]> {
        let spans = self.lines.get(self.starts.get(h)? + l)?;
        (!spans.is_empty()).then_some(&**spans)
    }
}

/// Ce qui détermine les couleurs d'un diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    content: u64,
    dark: bool,
    tab_width: usize,
}

/// Cache LRU des couleurs calculées, le plus récemment utilisé en tête : au plus `CACHE_ENTRIES` diffs et
/// `CACHE_LINES` lignes en tout.
#[derive(Default)]
struct Lru {
    entries: VecDeque<(Key, Arc<Colors>)>,
}

impl Lru {
    fn get(&mut self, key: &Key) -> Option<Arc<Colors>> {
        let i = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(i)?;
        let colors = entry.1.clone();
        self.entries.push_front(entry);
        Some(colors)
    }

    fn insert(&mut self, key: Key, colors: Arc<Colors>) {
        self.entries.retain(|(k, _)| *k != key);
        self.entries.push_front((key, colors));
        let mut lines: usize = self.entries.iter().map(|(_, c)| c.lines.len()).sum();
        while self.entries.len() > CACHE_ENTRIES || (self.entries.len() > 1 && lines > CACHE_LINES) {
            if let Some((_, evicted)) = self.entries.pop_back() {
                lines -= evicted.lines.len();
            }
        }
    }
}

/// Cache partagé par toutes les vues de diff.
fn cache() -> MutexGuard<'static, Lru> {
    static CACHE: OnceLock<Mutex<Lru>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default).lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
thread_local! {
    /// Colorations calculées sur le thread courant (tests : jamais sur celui qui affiche).
    static COMPUTED_HERE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Colore les lignes de `diff` ; `None` si `cancel` l'a interrompu. Une extension inconnue ne donne aucune couleur.
fn compute(diff: &FileDiff, tab_width: usize, dark: bool, cancel: &AtomicBool) -> Option<Colors> {
    #[cfg(test)]
    COMPUTED_HERE.with(|n| n.set(n.get() + 1));
    let mut starts = Vec::with_capacity(diff.hunks.len());
    let mut lines = Vec::new();
    for hunk in &diff.hunks {
        starts.push(lines.len());
        lines.extend(&hunk.lines);
    }
    let mut colors = Colors { starts, lines: vec![Box::default(); lines.len()] };
    let ss = syntaxes();
    let Some(syntax) = syntax_for(ss, &extension(diff.display_path())) else { return Some(colors) };
    let theme = theme(dark);
    for side_old in [true, false] {
        let mut h = HighlightLines::new(syntax, theme);
        for (i, line) in lines.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let kind = line.kind;
            let on_side = match kind {
                LineKind::Context => !side_old,
                LineKind::Removed => side_old,
                LineKind::Added => !side_old,
            };
            // Le contexte fait avancer l'état du parseur des deux côtés.
            if !on_side && !(kind == LineKind::Context && side_old) {
                continue;
            }
            let text = expand_tabs(&line.text, tab_width);
            let with_nl = format!("{text}\n");
            let Ok(ranges) = h.highlight_line(&with_nl, ss) else { continue };
            if !on_side {
                continue;
            }
            let mut spans: Vec<Span> = Vec::with_capacity(ranges.len());
            let mut offset = 0;
            for (style, piece) in ranges {
                let end = (offset + piece.len()).min(text.len());
                if offset < end {
                    let c = style.foreground;
                    let color = Color32::from_rgb(c.r, c.g, c.b);
                    // Plages voisines de même couleur réunies : moins de mémoire, mêmes couleurs.
                    match spans.last_mut() {
                        Some(last) if last.color == color && last.end as usize == offset => last.end = end as u32,
                        _ => spans.push(Span { start: offset as u32, end: end as u32, color }),
                    }
                }
                offset += piece.len();
            }
            colors.lines[i] = spans.into_boxed_slice();
        }
    }
    Some(colors)
}

/// Calcul lancé : son résultat arrive sur `rx` ; abandonné (drapeau `cancel`) quand il n'a plus d'objet.
struct Pending {
    rx: Receiver<Arc<Colors>>,
    cancel: Arc<AtomicBool>,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Lance la coloration de `diff` en arrière-plan (partagé : rien n'est copié sur le thread de l'interface) :
/// le résultat entre dans le cache, puis une image est demandée.
fn spawn(ctx: &egui::Context, key: Key, diff: LoadedDiff) -> Pending {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let (flag, ctx) = (cancel.clone(), ctx.clone());
    // Échec du lancement : canal fermé, le diff reste sans couleur.
    let _ = std::thread::Builder::new().name("dragontree-highlight".into()).spawn(move || {
        let Some(colors) = compute(&diff, key.tab_width, key.dark, &flag) else { return };
        let colors = Arc::new(colors);
        cache().insert(key, colors.clone());
        if tx.send(colors).is_ok() {
            ctx.request_repaint();
        }
    });
    Pending { rx, cancel }
}

/// Coloration d'une vue de diff : couleurs du diff affiché, ou calcul en cours.
#[derive(Default)]
pub struct Highlighter {
    dark: bool,
    key: Option<Key>,
    colors: Option<Arc<Colors>>,
    pending: Option<Pending>,
}

impl Highlighter {
    pub fn new(dark: bool) -> Highlighter {
        Highlighter { dark, ..Default::default() }
    }

    pub fn is_dark(&self) -> bool {
        self.dark
    }

    /// À chaque image, avant `colors` : reprend les couleurs du cache, ou lance leur calcul en arrière-plan, si le
    /// diff (contenu), le thème ou la largeur de tabulation ont changé ; récupère le résultat d'un calcul terminé.
    /// L'empreinte du contenu vient du chargement du diff ([`LoadedDiff`]) : rien n'est parcouru ici tant que rien ne
    /// change.
    pub fn prepare(&mut self, ctx: &egui::Context, diff: &LoadedDiff, tab_width: usize) {
        let key = Key { content: diff.content_hash(), dark: self.dark, tab_width };
        if self.key != Some(key) {
            self.key = Some(key);
            (self.colors, self.pending) = (None, None);
            if colorable(diff) {
                self.colors = cache().get(&key);
                if self.colors.is_none() {
                    self.pending = Some(spawn(ctx, key, diff.clone()));
                }
            }
        }
        if let Some(pending) = &self.pending {
            match pending.rx.try_recv() {
                Ok(colors) => (self.colors, self.pending) = (Some(colors), None),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.pending = None,
            }
        }
    }

    /// Un calcul est en cours : le diff s'affiche sans couleur en attendant.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Couleurs de la ligne `l` du hunk `h` (texte affiché, tabulations développées) ; `None` sans coloration.
    pub fn colors(&self, h: usize, l: usize) -> Option<&[Span]> {
        self.colors.as_ref()?.line(h, l)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use dragontree_git::diff::parse_unified_diff;

    fn rust_diff(name: &str, removed: &str, added: &str) -> LoadedDiff {
        let text = [
            &format!("diff --git a/{name} b/{name}"),
            &format!("--- a/{name}"),
            &format!("+++ b/{name}"),
            "@@ -1 +1 @@",
            &format!("-{removed}"),
            &format!("+{added}"),
        ]
        .map(|s| s.to_string())
        .join("\n")
            + "\n";
        LoadedDiff::new(parse_unified_diff(&text).remove(0))
    }

    /// Nouveau fichier `name` fait des lignes `lines`.
    fn added_lines(name: &str, lines: &[String]) -> LoadedDiff {
        let mut text =
            format!("diff --git a/{name} b/{name}\n--- /dev/null\n+++ b/{name}\n@@ -0,0 +1,{} @@\n", lines.len());
        for line in lines {
            text.push('+');
            text.push_str(line);
            text.push('\n');
        }
        LoadedDiff::new(parse_unified_diff(&text).remove(0))
    }

    /// Fait tourner `prepare` (comme les images successives) jusqu'à la fin du calcul (au plus 20 s).
    fn until_ready(h: &mut Highlighter, ctx: &egui::Context, diff: &LoadedDiff) {
        let deadline = Instant::now() + Duration::from_secs(20);
        h.prepare(ctx, diff, 4);
        while h.is_pending() {
            assert!(Instant::now() < deadline, "délai dépassé en attendant la coloration");
            std::thread::sleep(Duration::from_millis(5));
            h.prepare(ctx, diff, 4);
        }
    }

    fn key(n: u64) -> Key {
        Key { content: n, dark: false, tab_width: 4 }
    }

    fn colors_of(lines: usize) -> Arc<Colors> {
        Arc::new(Colors { starts: vec![0], lines: vec![Box::default(); lines] })
    }

    #[test]
    fn rust_code_gets_several_colors() {
        let diff = rust_diff("several.rs", "fn main() {}", "fn main() { let x = 1; }");
        let (mut h, ctx) = (Highlighter::new(false), egui::Context::default());
        until_ready(&mut h, &ctx, &diff);
        let spans = h.colors(0, 1).expect("ligne ajoutée colorée");
        let distinct: std::collections::HashSet<_> = spans.iter().map(|s| s.color).collect();
        assert!(distinct.len() >= 2, "{spans:?}");
        assert_eq!(spans.last().unwrap().end as usize, "fn main() { let x = 1; }".len());
    }

    #[test]
    fn a_line_edited_without_changing_the_line_count_is_recolored() {
        // Même chemin, même nombre de lignes, même longueur : seule la ligne ajoutée change.
        let code = rust_diff("recolored.rs", "fn a() {}", "fn main() {}");
        let comment = rust_diff("recolored.rs", "fn a() {}", "// main() {}");
        let (mut h, ctx) = (Highlighter::new(false), egui::Context::default());
        until_ready(&mut h, &ctx, &code);
        let before = h.colors(0, 1).expect("code coloré").to_vec();
        until_ready(&mut h, &ctx, &comment);
        let after = h.colors(0, 1).expect("commentaire coloré").to_vec();
        assert_ne!(before, after, "couleurs recalculées pour le nouveau texte");
        assert_eq!(after.len(), 1, "un commentaire d'une seule couleur : {after:?}");

        // Revenir au premier contenu : couleurs reprises du cache, sans calcul.
        h.prepare(&ctx, &code, 4);
        assert!(!h.is_pending());
        assert_eq!(h.colors(0, 1).unwrap(), before.as_slice());
    }

    /// Structure : `prepare` ne colore jamais sur le thread qui affiche ; le texte reste sans couleur en attendant.
    #[test]
    fn no_highlighting_work_runs_on_the_interface_thread() {
        let diff = rust_diff("interface_thread.rs", "fn old() {}", "fn interface_thread() { let y = 2; }");
        let (mut h, ctx) = (Highlighter::new(false), egui::Context::default());
        h.prepare(&ctx, &diff, 4);
        assert!(h.is_pending() && h.colors(0, 1).is_none(), "sans couleur en attendant le calcul");
        until_ready(&mut h, &ctx, &diff);
        assert!(h.colors(0, 1).is_some());
        // Une autre vue du même diff reprend le cache, sans calcul.
        let mut other = Highlighter::new(false);
        other.prepare(&ctx, &diff, 4);
        assert!(!other.is_pending() && other.colors(0, 1).is_some());
        assert_eq!(COMPUTED_HERE.with(std::cell::Cell::get), 0, "aucune coloration sur ce thread");
    }

    #[test]
    fn the_tab_width_and_the_theme_are_part_of_the_key() {
        let diff = rust_diff("tabs.rs", "fn a() {}", "\tlet tabs = 1;");
        let ctx = egui::Context::default();
        let mut h = Highlighter::new(false);
        until_ready(&mut h, &ctx, &diff);
        let four = h.colors(0, 1).unwrap().to_vec();
        assert_eq!(four.last().unwrap().end as usize, "    let tabs = 1;".len(), "plages du texte développé");
        h.prepare(&ctx, &diff, 8);
        assert!(h.is_pending(), "autre largeur de tabulation : autre calcul");
        let mut dark = Highlighter::new(true);
        dark.prepare(&ctx, &diff, 4);
        assert!(dark.is_pending(), "autre thème : autre calcul");
    }

    /// Une extension sans syntaxe : pas de couleur, et aucune tâche lancée une fois les syntaxes chargées (avant,
    /// c'est la tâche de fond qui les charge, puis le diff reste sans couleur).
    #[test]
    fn an_unknown_extension_is_not_colored_and_starts_no_thread() {
        let _ = syntaxes();
        let diff = rust_diff("x.zzz", "a", "b");
        let (mut h, ctx) = (Highlighter::new(false), egui::Context::default());
        h.prepare(&ctx, &diff, 4);
        assert!(!h.is_pending(), "aucune tâche pour une extension inconnue");
        assert!(h.colors(0, 1).is_none());
    }

    /// Une ligne très longue (code minifié) ou un diff trop gros ne sont pas colorés, et aucune tâche n'est lancée :
    /// colorer une seule ligne de plusieurs centaines de Kio prend des secondes, sans voir l'annulation.
    #[test]
    fn a_very_long_line_or_a_very_large_diff_is_not_colored() {
        let ctx = egui::Context::default();
        let minified = added_lines("minified.js", &[format!("var a={};", "1+".repeat(MAX_LINE_BYTES / 2))]);
        let mut h = Highlighter::new(false);
        h.prepare(&ctx, &minified, 4);
        assert!(!h.is_pending() && h.colors(0, 0).is_none(), "ligne trop longue");

        // Lignes courtes, moins de `MAX_LINES`, mais plus de `MAX_BYTES` en tout.
        let many: Vec<String> = (0..MAX_BYTES / 100 + 1).map(|i| format!("{i:0>100}")).collect();
        let large = added_lines("large.rs", &many);
        assert!(large.line_count() <= MAX_LINES && large.byte_count() > MAX_BYTES);
        h.prepare(&ctx, &large, 4);
        assert!(!h.is_pending() && h.colors(0, 0).is_none(), "diff trop gros");

        // À la limite : coloré.
        let edge = added_lines("edge.js", &["1".repeat(MAX_LINE_BYTES)]);
        h.prepare(&ctx, &edge, 4);
        assert!(h.is_pending(), "ligne à la limite : colorée");
    }

    #[test]
    fn the_cache_evicts_the_least_recently_used_diff() {
        let mut lru = Lru::default();
        for n in 0..CACHE_ENTRIES as u64 {
            lru.insert(key(n), colors_of(1));
        }
        // Le premier vient de servir : c'est le suivant, le moins récemment utilisé, qui part.
        assert!(lru.get(&key(0)).is_some());
        lru.insert(key(1000), colors_of(1));
        assert_eq!(lru.entries.len(), CACHE_ENTRIES);
        assert!(lru.get(&key(1)).is_none(), "évincé");
        assert!(lru.get(&key(0)).is_some() && lru.get(&key(2)).is_some() && lru.get(&key(1000)).is_some());

        // Borné aussi en lignes : un gros diff chasse les plus anciens, mais reste lui-même.
        lru.insert(key(2000), colors_of(CACHE_LINES - 10));
        let lines: usize = lru.entries.iter().map(|(_, c)| c.lines.len()).sum();
        assert!(lines <= CACHE_LINES, "{lines}");
        assert!(lru.get(&key(2000)).is_some());
        lru.insert(key(3000), colors_of(CACHE_LINES + 1));
        assert_eq!(lru.entries.len(), 1, "le plus récent reste, même trop gros");
    }
}
