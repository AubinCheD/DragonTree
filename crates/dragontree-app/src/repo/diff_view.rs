//! Affichage d'un diff (unifié ou côte à côte) : numéros de ligne, surlignage intra-ligne,
//! sélection de lignes et actions sur les hunks (indexer, désindexer, annuler).

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::time::Duration;

use dragontree_git::{FileDiff, HunkSelection, LineKind, PatchMode};
use dragontree_i18n::{tr, trn};
use egui::text::LayoutJob;
use egui::{Align2, FontId, Rect, RichText, Sense, TextFormat, Ui};

use crate::diff_style::{DiffStyle, copy_text, expand_tabs, lines_to_copy, whitespace_markers};
use crate::highlight::Highlighter;
use crate::icons;
use crate::theme::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffMode {
    ReadOnly,
    Unstaged,
    Staged,
}

/// Action demandée par l'utilisateur dans le diff.
pub struct DiffAction {
    pub mode: PatchMode,
    pub selection: Vec<HunkSelection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    Header(usize),
    Line(usize, usize),
    Pair(Option<(usize, usize)>, Option<(usize, usize)>),
}

struct RowCache {
    key: u64,
    split: bool,
    tab_width: usize,
    rows: Vec<Row>,
    highlights: HashMap<(usize, usize), Vec<Range<usize>>>,
    max_chars: usize,
}

#[derive(Default)]
pub struct DiffViewState {
    pub selected: BTreeSet<(usize, usize)>,
    anchor: Option<(usize, usize)>,
    drag_origin: Option<(usize, usize)>,
    cache: Option<RowCache>,
}

impl DiffViewState {
    /// Retire de la sélection les lignes qui n'existent plus (nouveau diff du même fichier).
    pub fn prune_selection(&mut self, diff: Option<&FileDiff>) {
        let Some(diff) = diff else {
            self.selected.clear();
            return;
        };
        self.selected.retain(|&(h, l)| {
            diff.hunks.get(h).and_then(|hk| hk.lines.get(l)).is_some_and(|line| line.kind != LineKind::Context)
        });
        self.anchor = None;
    }
}

fn fingerprint(diff: &FileDiff) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    diff.display_path().hash(&mut h);
    (diff.is_binary, diff.truncated, diff.hunks.len()).hash(&mut h);
    for hunk in &diff.hunks {
        hunk.header.hash(&mut h);
        hunk.lines.len().hash(&mut h);
        for l in &hunk.lines {
            (l.kind as u8, l.text.len()).hash(&mut h);
        }
    }
    h.finish()
}

fn build_rows(diff: &FileDiff, split: bool, tab_width: usize) -> (Vec<Row>, usize) {
    let mut rows = Vec::new();
    let mut max_chars = 0;
    for (h, hunk) in diff.hunks.iter().enumerate() {
        rows.push(Row::Header(h));
        for l in &hunk.lines {
            max_chars = max_chars.max(expand_tabs(&l.text, tab_width).chars().count());
        }
        if !split {
            rows.extend((0..hunk.lines.len()).map(|l| Row::Line(h, l)));
            continue;
        }
        let mut i = 0;
        let lines = &hunk.lines;
        while i < lines.len() {
            if lines[i].kind == LineKind::Context {
                rows.push(Row::Pair(Some((h, i)), Some((h, i))));
                i += 1;
                continue;
            }
            let mut removed = Vec::new();
            while i < lines.len() && lines[i].kind == LineKind::Removed {
                removed.push(i);
                i += 1;
            }
            let mut added = Vec::new();
            while i < lines.len() && lines[i].kind == LineKind::Added {
                added.push(i);
                i += 1;
            }
            for k in 0..removed.len().max(added.len()) {
                rows.push(Row::Pair(removed.get(k).map(|&l| (h, l)), added.get(k).map(|&l| (h, l))));
            }
        }
    }
    (rows, max_chars)
}

/// Plages de caractères modifiés pour chaque paire de lignes supprimée/ajoutée d'un même bloc.
fn build_highlights(diff: &FileDiff, tab_width: usize) -> HashMap<(usize, usize), Vec<Range<usize>>> {
    let mut out = HashMap::new();
    let total: usize = diff.hunks.iter().map(|h| h.lines.len()).sum();
    if total > 20_000 {
        return out;
    }
    for (h, hunk) in diff.hunks.iter().enumerate() {
        let lines = &hunk.lines;
        let mut i = 0;
        while i < lines.len() {
            if lines[i].kind != LineKind::Removed {
                i += 1;
                continue;
            }
            let start = i;
            while i < lines.len() && lines[i].kind == LineKind::Removed {
                i += 1;
            }
            let removed: Vec<usize> = (start..i).collect();
            let astart = i;
            while i < lines.len() && lines[i].kind == LineKind::Added {
                i += 1;
            }
            let added: Vec<usize> = (astart..i).collect();
            if removed.len() > 100 || added.len() > 100 {
                continue;
            }
            for (&r, &a) in removed.iter().zip(added.iter()) {
                let (old, new) = (expand_tabs(&lines[r].text, tab_width), expand_tabs(&lines[a].text, tab_width));
                if old.len() > 2000 || new.len() > 2000 {
                    continue;
                }
                let (ro, rn) = char_diff(&old, &new);
                // Ligne entièrement réécrite : pas de surlignage supplémentaire.
                let changed_old: usize = ro.iter().map(|r| r.len()).sum();
                let changed_new: usize = rn.iter().map(|r| r.len()).sum();
                if changed_old * 10 > old.len().max(1) * 7 && changed_new * 10 > new.len().max(1) * 7 {
                    continue;
                }
                out.insert((h, r), ro);
                out.insert((h, a), rn);
            }
        }
    }
    out
}

/// Différence caractère par caractère ; plages en octets de chaque côté.
fn char_diff(old: &str, new: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let diff = similar::TextDiff::configure().timeout(Duration::from_millis(20)).diff_chars(old, new);
    let old_offsets: Vec<usize> = old.char_indices().map(|(i, _)| i).chain(std::iter::once(old.len())).collect();
    let new_offsets: Vec<usize> = new.char_indices().map(|(i, _)| i).chain(std::iter::once(new.len())).collect();
    let (mut ro, mut rn) = (Vec::new(), Vec::new());
    for op in diff.ops() {
        let (tag, o, n) = op.as_tag_tuple();
        if tag == similar::DiffTag::Equal {
            continue;
        }
        if !o.is_empty() {
            ro.push(old_offsets[o.start]..old_offsets[o.end]);
        }
        if !n.is_empty() {
            rn.push(new_offsets[n.start]..new_offsets[n.end]);
        }
    }
    (merge_close(ro), merge_close(rn))
}

/// Fusionne les plages séparées par un ou deux caractères (plus lisible).
fn merge_close(ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for r in ranges {
        match out.last_mut() {
            Some(last) if r.start <= last.end + 2 => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

/// Affiche le diff ; renvoie l'action demandée (indexation…) le cas échéant.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    diff: &FileDiff,
    mode: DiffMode,
    state: &mut DiffViewState,
    split: bool,
    style: &DiffStyle,
    highlighter: Option<&mut Highlighter>,
    p: &Palette,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
) -> Option<DiffAction> {
    let message = if diff.truncated {
        Some(format!("{} {}", icons::WARNING, tr!("Diff trop volumineux pour être affiché.")))
    } else if diff.is_binary {
        Some(format!("{} {}", icons::FILE, tr!("Fichier binaire.")))
    } else if diff.hunks.is_empty() {
        Some(if diff.old_path.is_some() && diff.new_path.is_some() && diff.old_path != diff.new_path {
            format!("{} {}", icons::ARROW_ELBOW_DOWN_RIGHT, tr!("Fichier renommé sans modification de contenu."))
        } else {
            tr!("Aucune différence de contenu (changement de mode ou fichier vide).").to_string()
        })
    } else {
        None
    };
    if let Some(m) = message {
        ui.add_space(16.0);
        ui.vertical_centered(|ui| ui.label(RichText::new(m).color(p.weak)));
        return None;
    }

    let key = fingerprint(diff);
    let tab_width = style.tab_width;
    if state.cache.as_ref().is_none_or(|c| c.key != key || c.split != split || c.tab_width != tab_width) {
        let (rows, max_chars) = build_rows(diff, split, tab_width);
        let highlights = build_highlights(diff, tab_width);
        state.cache = Some(RowCache { key, split, tab_width, rows, highlights, max_chars });
    }
    let cache = state.cache.take().expect("cache construit");

    let font = style.font_id();
    let char_w = ui.painter().layout_no_wrap("M".into(), font.clone(), p.text).size().x;
    let row_h = (ui.painter().layout_no_wrap("Mg".into(), font.clone(), p.text).size().y + 4.0).round();
    let max_no =
        diff.hunks.iter().map(|h| (h.old_start + h.old_count).max(h.new_start + h.new_count)).max().unwrap_or(1);
    let no_w = (max_no.to_string().len().max(3) as f32) * char_w + 10.0;
    let text_w = cache.max_chars as f32 * char_w + 24.0;
    let mut action = None;
    let mut highlighter = highlighter;
    if let Some(hl) = highlighter.as_deref_mut() {
        hl.prepare(diff, tab_width);
    }

    let interactive = mode != DiffMode::ReadOnly;
    let pointer_down = ui.ctx().input(|i| i.pointer.primary_down());
    let modifiers = ui.ctx().input(|i| i.modifiers);
    if !pointer_down {
        state.drag_origin = None;
    }

    ui.spacing_mut().item_spacing.y = 0.0;
    let content_w = if split {
        (ui.available_width()).max(2.0 * (no_w + text_w))
    } else {
        (2.0 * no_w + 18.0 + text_w).max(ui.available_width())
    };
    egui::ScrollArea::both().id_salt(("diff", id_salt)).auto_shrink([false, false]).show_rows(
        ui,
        row_h,
        cache.rows.len(),
        |ui, range| {
            let top = ui.max_rect().top();
            let left = ui.max_rect().left();
            ui.set_min_width(content_w);
            for (k, ri) in range.clone().enumerate() {
                let rect = Rect::from_min_size(egui::pos2(left, top + k as f32 * row_h), egui::vec2(content_w, row_h));
                match cache.rows[ri] {
                    Row::Header(h) => {
                        let hunk = &diff.hunks[h];
                        ui.painter().rect_filled(rect, 0.0, p.hunk_bg);
                        ui.painter().text(
                            egui::pos2(rect.left() + 8.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            &hunk.header,
                            style.small_font_id(1.0),
                            p.weak,
                        );
                        if interactive {
                            let vis = ui.clip_rect();
                            let mut x = vis.right().min(rect.right()) - 6.0;
                            let selected_here: Vec<usize> =
                                state.selected.iter().filter(|(hh, _)| *hh == h).map(|(_, l)| *l).collect();
                            let mut button = |ui: &mut Ui, text: String, tip: &str| -> bool {
                                let galley =
                                    ui.painter().layout_no_wrap(text.clone(), FontId::proportional(12.0), p.text);
                                let w = galley.size().x + 14.0;
                                let r = Rect::from_min_max(
                                    egui::pos2(x - w, rect.top() + 2.0),
                                    egui::pos2(x, rect.bottom() - 2.0),
                                );
                                x -= w + 4.0;
                                ui.put(r, egui::Button::new(RichText::new(text).size(12.0)).small())
                                    .on_hover_text(tip)
                                    .clicked()
                            };
                            let whole = vec![HunkSelection { hunk: h, lines: None }];
                            let lines = vec![HunkSelection { hunk: h, lines: Some(selected_here.clone()) }];
                            match mode {
                                DiffMode::Unstaged => {
                                    if button(
                                        ui,
                                        format!("{} {}", icons::ARROW_COUNTER_CLOCKWISE, tr!("Annuler le hunk")),
                                        tr!("Annuler ces modifications dans la copie de travail"),
                                    ) {
                                        action =
                                            Some(DiffAction { mode: PatchMode::Discard, selection: whole.clone() });
                                    }
                                    if button(
                                        ui,
                                        format!("{} {}", icons::PLUS, tr!("Indexer le hunk")),
                                        tr!("Indexer tout le hunk"),
                                    ) {
                                        action = Some(DiffAction { mode: PatchMode::Stage, selection: whole });
                                    }
                                    if !selected_here.is_empty() {
                                        let n = selected_here.len();
                                        if button(
                                            ui,
                                            trn!(n, "Annuler {n} ligne", "Annuler {n} lignes"),
                                            tr!("Annuler les lignes sélectionnées"),
                                        ) {
                                            action =
                                                Some(DiffAction { mode: PatchMode::Discard, selection: lines.clone() });
                                        }
                                        if button(
                                            ui,
                                            trn!(n, "Indexer {n} ligne", "Indexer {n} lignes"),
                                            tr!("Indexer les lignes sélectionnées"),
                                        ) {
                                            action = Some(DiffAction { mode: PatchMode::Stage, selection: lines });
                                        }
                                    }
                                }
                                DiffMode::Staged => {
                                    if button(
                                        ui,
                                        format!("{} {}", icons::MINUS, tr!("Désindexer le hunk")),
                                        tr!("Retirer tout le hunk de l'index"),
                                    ) {
                                        action = Some(DiffAction { mode: PatchMode::Unstage, selection: whole });
                                    }
                                    if !selected_here.is_empty() {
                                        let n = selected_here.len();
                                        if button(
                                            ui,
                                            trn!(n, "Désindexer {n} ligne", "Désindexer {n} lignes"),
                                            tr!("Retirer les lignes sélectionnées de l'index"),
                                        ) {
                                            action = Some(DiffAction { mode: PatchMode::Unstage, selection: lines });
                                        }
                                    }
                                }
                                DiffMode::ReadOnly => {}
                            }
                        }
                    }
                    Row::Line(h, l) => {
                        let resp =
                            ui.allocate_rect(rect, if interactive { Sense::click_and_drag() } else { Sense::click() });
                        paint_unified_line(
                            ui,
                            rect,
                            diff,
                            (h, l),
                            &cache,
                            state,
                            no_w,
                            char_w,
                            style,
                            highlighter.as_deref_mut(),
                            p,
                        );
                        if interactive {
                            handle_selection(state, diff, (h, l), &resp, modifiers, ui);
                        }
                        copy_menu(&resp, diff, state, (h, l));
                    }
                    Row::Pair(left_line, right_line) => {
                        let half = rect.width() / 2.0;
                        let lrect = Rect::from_min_size(rect.min, egui::vec2(half, row_h));
                        let rrect =
                            Rect::from_min_size(egui::pos2(rect.left() + half, rect.top()), egui::vec2(half, row_h));
                        for (side_rect, line, old_side) in [(lrect, left_line, true), (rrect, right_line, false)] {
                            let resp = ui.allocate_rect(
                                side_rect,
                                if interactive { Sense::click_and_drag() } else { Sense::click() },
                            );
                            match line {
                                Some(hl) => {
                                    paint_split_line(
                                        ui,
                                        side_rect,
                                        diff,
                                        hl,
                                        old_side,
                                        &cache,
                                        state,
                                        no_w,
                                        style,
                                        highlighter.as_deref_mut(),
                                        p,
                                    );
                                    if interactive {
                                        handle_selection(state, diff, hl, &resp, modifiers, ui);
                                    }
                                    copy_menu(&resp, diff, state, hl);
                                }
                                None => {
                                    ui.painter().rect_filled(side_rect, 0.0, p.gutter);
                                }
                            }
                        }
                        ui.painter().vline(rect.left() + half, rect.y_range(), egui::Stroke::new(1.0, p.border));
                    }
                }
            }
        },
    );
    state.cache = Some(cache);
    action
}

/// Menu contextuel d'une ligne : copie les lignes sélectionnées (ou la ligne cliquée) telles qu'elles sont dans le
/// fichier, sans marqueur de diff ni espaces visibles.
fn copy_menu(resp: &egui::Response, diff: &FileDiff, state: &DiffViewState, key: (usize, usize)) {
    resp.context_menu(|ui| {
        let lines = lines_to_copy(&state.selected, key);
        let label = trn!(lines.len(), "Copier {n} ligne", "Copier {n} lignes");
        if ui.button(label).clicked() {
            ui.ctx().copy_text(copy_text(diff, &lines));
            ui.close();
        }
    });
}

fn handle_selection(
    state: &mut DiffViewState,
    diff: &FileDiff,
    key: (usize, usize),
    resp: &egui::Response,
    modifiers: egui::Modifiers,
    ui: &Ui,
) {
    let selectable = |k: (usize, usize)| {
        diff.hunks.get(k.0).and_then(|h| h.lines.get(k.1)).is_some_and(|l| l.kind != LineKind::Context)
    };
    // Seul le bouton principal sélectionne : un clic droit (menu « Copier ») garde la sélection.
    let primary_down = ui.ctx().input(|i| i.pointer.button_down(egui::PointerButton::Primary));
    if resp.drag_started() || (resp.is_pointer_button_down_on() && primary_down && state.drag_origin.is_none()) {
        state.drag_origin = Some(key);
        if !modifiers.ctrl && !modifiers.shift {
            state.selected.clear();
            if selectable(key) {
                state.selected.insert(key);
            }
            state.anchor = Some(key);
        }
    }
    // Glisser : sélection continue dans le même hunk.
    if let Some(origin) = state.drag_origin
        && ui.ctx().input(|i| i.pointer.primary_down())
        && resp.hovered()
        && origin.0 == key.0
        && origin != key
        && !modifiers.ctrl
    {
        let (a, b) = if origin.1 <= key.1 { (origin.1, key.1) } else { (key.1, origin.1) };
        state.selected.retain(|k| k.0 != key.0);
        for l in a..=b {
            if selectable((key.0, l)) {
                state.selected.insert((key.0, l));
            }
        }
    }
    if resp.clicked() {
        if modifiers.ctrl {
            if !state.selected.remove(&key) && selectable(key) {
                state.selected.insert(key);
            }
            state.anchor = Some(key);
        } else if modifiers.shift
            && let Some(anchor) = state.anchor.filter(|a| a.0 == key.0)
        {
            let (a, b) = if anchor.1 <= key.1 { (anchor.1, key.1) } else { (key.1, anchor.1) };
            for l in a..=b {
                if selectable((key.0, l)) {
                    state.selected.insert((key.0, l));
                }
            }
        }
    }
}

fn line_colors(kind: LineKind, p: &Palette) -> (egui::Color32, egui::Color32, &'static str) {
    match kind {
        LineKind::Added => (p.added_bg, p.added_hl, "+"),
        LineKind::Removed => (p.removed_bg, p.removed_hl, "-"),
        LineKind::Context => (egui::Color32::TRANSPARENT, egui::Color32::TRANSPARENT, " "),
    }
}

#[allow(clippy::too_many_arguments)]
fn line_job(
    text: &str,
    kind: LineKind,
    ranges: Option<&Vec<Range<usize>>>,
    font: &FontId,
    colors: Option<Vec<(Range<usize>, egui::Color32)>>,
    marked: Option<&str>,
    p: &Palette,
) -> LayoutJob {
    let (_, hl, _) = line_colors(kind, p);
    let mut job = LayoutJob::default();
    // Découpe aux bornes des surlignages intra-ligne et des couleurs syntaxiques.
    let mut cuts: Vec<usize> = vec![0, text.len()];
    if let Some(r) = ranges {
        for x in r {
            cuts.push(x.start.min(text.len()));
            cuts.push(x.end.min(text.len()));
        }
    }
    if let Some(c) = &colors {
        for (r, _) in c {
            cuts.push(r.start.min(text.len()));
            cuts.push(r.end.min(text.len()));
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a >= b || !text.is_char_boundary(a) || !text.is_char_boundary(b) {
            continue;
        }
        let highlighted = ranges.is_some_and(|r| r.iter().any(|x| x.start <= a && b <= x.end));
        let color = colors
            .as_ref()
            .and_then(|c| c.iter().find(|(r, _)| r.start <= a && b <= r.end).map(|(_, col)| *col))
            .unwrap_or(p.text);
        // Espaces visibles : mêmes colonnes que le texte, les coupures ci-dessus restent valables.
        let piece: std::borrow::Cow<str> = match marked {
            Some(m) => m.chars().skip(text[..a].chars().count()).take(text[a..b].chars().count()).collect(),
            None => text[a..b].into(),
        };
        job.append(
            &piece,
            0.0,
            TextFormat {
                font_id: font.clone(),
                color,
                background: if highlighted { hl } else { egui::Color32::TRANSPARENT },
                ..Default::default()
            },
        );
    }
    job
}

#[allow(clippy::too_many_arguments)]
fn paint_unified_line(
    ui: &Ui,
    rect: Rect,
    diff: &FileDiff,
    (h, l): (usize, usize),
    cache: &RowCache,
    state: &DiffViewState,
    no_w: f32,
    char_w: f32,
    style: &DiffStyle,
    highlighter: Option<&mut Highlighter>,
    p: &Palette,
) {
    let font = &style.font_id();
    let line = &diff.hunks[h].lines[l];
    let (bg, _, marker) = line_colors(line.kind, p);
    let selected = state.selected.contains(&(h, l));
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, if selected { p.selection } else { bg });
    let gutter = Rect::from_min_size(rect.min, egui::vec2(2.0 * no_w, rect.height()));
    painter.rect_filled(gutter, 0.0, if selected { p.selection } else { p.gutter });
    let no = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
    painter.text(
        egui::pos2(rect.left() + no_w - 4.0, rect.center().y),
        Align2::RIGHT_CENTER,
        no(line.old_no),
        style.small_font_id(1.5),
        p.weak,
    );
    painter.text(
        egui::pos2(rect.left() + 2.0 * no_w - 4.0, rect.center().y),
        Align2::RIGHT_CENTER,
        no(line.new_no),
        style.small_font_id(1.5),
        p.weak,
    );
    let marker_color = match line.kind {
        LineKind::Added => p.added_fg,
        LineKind::Removed => p.removed_fg,
        LineKind::Context => p.weak,
    };
    painter.text(
        egui::pos2(rect.left() + 2.0 * no_w + 8.0, rect.center().y),
        Align2::CENTER_CENTER,
        marker,
        font.clone(),
        marker_color,
    );
    let text = expand_tabs(&line.text, style.tab_width);
    let marked = style.show_whitespace.then(|| whitespace_markers(&line.text, style.tab_width));
    let colors = highlighter.and_then(|hl| hl.colors(diff, h, l, &text, p.dark));
    let job = line_job(&text, line.kind, cache.highlights.get(&(h, l)), font, colors, marked.as_deref(), p);
    let galley = painter.layout_job(job);
    painter.galley(
        egui::pos2(rect.left() + 2.0 * no_w + 18.0, rect.center().y - galley.size().y / 2.0),
        galley,
        p.text,
    );
    if line.no_newline {
        let x = rect.left() + 2.0 * no_w + 22.0 + text.chars().count() as f32 * char_w;
        painter.text(
            egui::pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            format!("{} {}", icons::PROHIBIT, tr!("pas de saut de ligne final")),
            FontId::proportional(10.5),
            p.weak,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_split_line(
    ui: &Ui,
    rect: Rect,
    diff: &FileDiff,
    (h, l): (usize, usize),
    old_side: bool,
    cache: &RowCache,
    state: &DiffViewState,
    no_w: f32,
    style: &DiffStyle,
    highlighter: Option<&mut Highlighter>,
    p: &Palette,
) {
    let font = &style.font_id();
    let line = &diff.hunks[h].lines[l];
    let (bg, _, _) = line_colors(line.kind, p);
    let selected = state.selected.contains(&(h, l)) && line.kind != LineKind::Context;
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 0.0, if selected { p.selection } else { bg });
    let gutter = Rect::from_min_size(rect.min, egui::vec2(no_w, rect.height()));
    painter.rect_filled(gutter, 0.0, if selected { p.selection } else { p.gutter });
    let n = if old_side { line.old_no } else { line.new_no };
    painter.text(
        egui::pos2(rect.left() + no_w - 4.0, rect.center().y),
        Align2::RIGHT_CENTER,
        n.map(|n| n.to_string()).unwrap_or_default(),
        style.small_font_id(1.5),
        p.weak,
    );
    let text = expand_tabs(&line.text, style.tab_width);
    let marked = style.show_whitespace.then(|| whitespace_markers(&line.text, style.tab_width));
    let colors = highlighter.and_then(|hl| hl.colors(diff, h, l, &text, p.dark));
    let job = line_job(&text, line.kind, cache.highlights.get(&(h, l)), font, colors, marked.as_deref(), p);
    let galley = painter.layout_job(job);
    painter.galley(egui::pos2(rect.left() + no_w + 6.0, rect.center().y - galley.size().y / 2.0), galley, p.text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use dragontree_git::diff::parse_unified_diff;

    fn sample() -> FileDiff {
        let text =
            ["diff --git a/f b/f", "--- a/f", "+++ b/f", "@@ -1,3 +1,3 @@", " a", "-let x = 1;", "+let x = 42;", " c"]
                .join("\n")
                + "\n";
        parse_unified_diff(&text).remove(0)
    }

    #[test]
    fn split_rows_pair_removed_and_added() {
        let (rows, max) = build_rows(&sample(), true, 4);
        assert_eq!(rows[0], Row::Header(0));
        assert_eq!(rows[1], Row::Pair(Some((0, 0)), Some((0, 0))));
        assert_eq!(rows[2], Row::Pair(Some((0, 1)), Some((0, 2))));
        assert_eq!(rows.len(), 4);
        assert_eq!(max, "let x = 42;".len());
        let (unified, _) = build_rows(&sample(), false, 4);
        assert_eq!(unified.len(), 5);
    }

    #[test]
    fn intra_line_highlight_marks_only_changed_chars() {
        let hl = build_highlights(&sample(), 4);
        let old = &hl[&(0, 1)];
        let new = &hl[&(0, 2)];
        assert_eq!(&"let x = 1;"[old[0].clone()], "1");
        assert_eq!(&"let x = 42;"[new[0].clone()], "42");
    }

    #[test]
    fn prune_drops_vanished_lines() {
        let mut st = DiffViewState::default();
        st.selected.insert((0, 1));
        st.selected.insert((0, 9));
        st.selected.insert((0, 0)); // contexte
        st.prune_selection(Some(&sample()));
        assert_eq!(st.selected.iter().copied().collect::<Vec<_>>(), vec![(0, 1)]);
    }
}
