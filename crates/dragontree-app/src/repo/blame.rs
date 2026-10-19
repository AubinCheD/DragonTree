//! Fenêtre « Historique du fichier » avec vue Blame.

use std::collections::HashMap;
use std::sync::Arc;

use dragontree_git::blame::{BlameCommit, BlameLine};
use dragontree_git::{ChangeKind, Commit, FileChange, FileDiff, GitError, LogOptions, LogScope, Oid, Repo};
use dragontree_i18n::{tr, trn};
use egui::{Align2, FontId, RichText, Sense, Ui};

use super::diff_view::{self, DiffMode, DiffViewState};
use super::restore::{RestoreFrom, RestoreTarget};
use super::{RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared, TabId};
use crate::icons;
use crate::theme::Palette;
use crate::widgets;

pub enum BlameEvent {
    History(u64, Result<Vec<Commit>, GitError>),
    Diff(u64, Result<FileDiff, GitError>),
    Blame(u64, Result<(Vec<BlameLine>, HashMap<Oid, BlameCommit>), GitError>),
}

pub struct FileHistoryWindow {
    tab: TabId,
    repo: Arc<Repo>,
    pub path: String,
    blame_mode: bool,
    commits: Vec<Commit>,
    history_loading: bool,
    selected: Option<usize>,
    diff: Option<FileDiff>,
    diff_state: DiffViewState,
    blame: Option<(Vec<BlameLine>, HashMap<Oid, BlameCommit>)>,
    blame_loading: bool,
    blame_rev: Option<Oid>,
    error: Option<String>,
    /// Version choisie dans le menu d'un commit, à remettre par l'onglet (`take_restore`).
    restore: Option<RestoreFrom>,
    generation: u64,
}

impl FileHistoryWindow {
    fn new(tab: TabId, repo: Arc<Repo>, path: String, blame_mode: bool, rev: Option<Oid>) -> FileHistoryWindow {
        FileHistoryWindow {
            tab,
            repo,
            path,
            blame_mode,
            commits: Vec::new(),
            history_loading: true,
            selected: None,
            diff: None,
            diff_state: DiffViewState::default(),
            blame: None,
            blame_loading: false,
            blame_rev: rev,
            error: None,
            restore: None,
            generation: 0,
        }
    }

    fn load_history(&mut self, shared: &Shared) {
        self.generation += 1;
        let (generation, tab, repo, path) = (self.generation, self.tab, self.repo.clone(), self.path.clone());
        shared.jobs.spawn("file-log", move || {
            let opts = LogOptions { scope: LogScope::Head, paths: vec![path], follow: true, ..LogOptions::default() };
            let res = repo.log(&opts, &dragontree_git::CancelToken::new());
            Some(AppEvent::Repo(tab, RepoEvent::Blame(BlameEvent::History(generation, res))))
        });
    }

    fn load_blame(&mut self, shared: &Shared) {
        self.blame_loading = true;
        let (generation, tab, repo, path, rev) =
            (self.generation, self.tab, self.repo.clone(), self.path.clone(), self.blame_rev);
        shared.jobs.spawn("blame", move || {
            Some(AppEvent::Repo(tab, RepoEvent::Blame(BlameEvent::Blame(generation, repo.blame(&path, rev)))))
        });
    }

    fn load_diff(&mut self, shared: &Shared, index: usize) {
        let Some(c) = self.commits.get(index) else { return };
        self.selected = Some(index);
        self.diff = None;
        self.diff_state = DiffViewState::default();
        let (generation, tab, repo, oid, parent) =
            (self.generation, self.tab, self.repo.clone(), c.id, c.parents.first().copied());
        let change = FileChange { path: self.path.clone(), old_path: None, kind: ChangeKind::Modified };
        let o = RepoTab::diff_options(shared);
        shared.jobs.spawn("file-diff", move || {
            Some(AppEvent::Repo(
                tab,
                RepoEvent::Blame(BlameEvent::Diff(generation, repo.diff_commit_file(oid, parent, &change, &o))),
            ))
        });
    }

    /// Le fichier de la fenêtre et la version à lui remettre, si l'utilisateur en a choisi une dans le menu d'un commit.
    pub fn take_restore(&mut self) -> Option<(String, RestoreFrom)> {
        self.restore.take().map(|from| (self.path.clone(), from))
    }

    pub fn handle(&mut self, ev: BlameEvent) {
        match ev {
            BlameEvent::History(g, res) if g == self.generation => {
                self.history_loading = false;
                match res {
                    Ok(c) => self.commits = c,
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            BlameEvent::Diff(g, res) if g == self.generation => match res {
                Ok(d) => self.diff = Some(d),
                Err(e) => self.error = Some(e.to_string()),
            },
            BlameEvent::Blame(g, res) if g == self.generation => {
                self.blame_loading = false;
                match res {
                    Ok(b) => self.blame = Some(b),
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            _ => {}
        }
    }

    /// Affiche la fenêtre ; renvoie (ouverte, commit à sélectionner dans l'historique).
    pub fn ui(&mut self, ctx: &egui::Context, shared: &mut Shared, p: &Palette) -> (bool, Option<Oid>) {
        let mut open = true;
        let mut select = None;
        let title = format!("{} {}", icons::CLOCK_COUNTER_CLOCKWISE, self.path);
        crate::widgets::centered_window(ctx, title)
            .id(egui::Id::unique(("file-history", self.tab)))
            .open(&mut open)
            .default_size([1100.0, 680.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(!self.blame_mode, format!("{} {}", icons::LIST, tr!("Historique"))).clicked()
                    {
                        self.blame_mode = false;
                    }
                    if ui.selectable_label(self.blame_mode, format!("{} Blame", icons::USER)).clicked() {
                        self.blame_mode = true;
                        if self.blame.is_none() && !self.blame_loading {
                            self.load_blame(shared);
                        }
                    }
                    if let Some(rev) = self.blame_rev.filter(|_| self.blame_mode) {
                        ui.label(RichText::new(tr!("à la révision {rev}", rev = rev.short())).color(p.weak));
                    }
                });
                if let Some(e) = &self.error {
                    ui.label(RichText::new(e).color(p.removed_fg));
                }
                ui.separator();
                if self.blame_mode {
                    select = self.blame_ui(ui, p);
                } else {
                    self.history_ui(ui, shared, p);
                }
            });
        (open, select)
    }

    fn history_ui(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        egui::Panel::left(("fh-list", self.tab)).resizable(true).default_size(380.0).show(ui, |ui| {
            if self.history_loading {
                ui.spinner();
                return;
            }
            ui.label(RichText::new(trn!(self.commits.len(), "{n} commit", "{n} commits")).color(p.weak));
            let mut clicked = None;
            let mut restore = None;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(
                ui,
                40.0,
                self.commits.len(),
                |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let now = widgets::now_ts();
                    for i in range {
                        let c = &self.commits[i];
                        let (rect, resp) = widgets::row_response(
                            ui,
                            ("file-commit", c.id),
                            40.0,
                            self.selected == Some(i),
                            Sense::click(),
                        );
                        let painter = ui.painter_at(rect);
                        painter.text(
                            rect.left_top() + egui::vec2(8.0, 11.0),
                            Align2::LEFT_CENTER,
                            c.summary.to_string(),
                            FontId::proportional(13.0),
                            p.text,
                        );
                        painter.text(
                            rect.left_top() + egui::vec2(8.0, 29.0),
                            Align2::LEFT_CENTER,
                            format!("{} · {} · {}", c.id.short(), c.author, widgets::format_time(c.author_time, now)),
                            FontId::proportional(11.5),
                            p.weak,
                        );
                        if resp.clicked() {
                            clicked = Some(i);
                        }
                        let target = RestoreTarget { commit: c.id, kind: None, has_parent: !c.parents.is_empty() };
                        resp.context_menu(|ui| {
                            if let Some(from) = RepoTab::restore_menu_entries(ui, target) {
                                restore = Some(from);
                            }
                        });
                    }
                },
            );
            if let Some(i) = clicked {
                self.load_diff(shared, i);
            }
            if restore.is_some() {
                self.restore = restore;
            }
        });
        egui::CentralPanel::default().show(ui, |ui| match &self.diff {
            Some(d) => {
                let d = d.clone();
                diff_view::show(
                    ui,
                    &d,
                    DiffMode::ReadOnly,
                    &mut self.diff_state,
                    shared.settings.split_diff,
                    None,
                    p,
                    ("fh-diff", self.tab),
                );
            }
            None if self.selected.is_some() => {
                ui.centered_and_justified(|ui| ui.spinner());
            }
            None => {
                ui.centered_and_justified(|ui| ui.label(RichText::new(tr!("Sélectionnez un commit")).color(p.weak)));
            }
        });
    }

    fn blame_ui(&mut self, ui: &mut Ui, p: &Palette) -> Option<Oid> {
        let Some((lines, commits)) = &self.blame else {
            ui.centered_and_justified(|ui| ui.spinner());
            return None;
        };
        let mut select = None;
        let font = FontId::monospace(12.5);
        let row_h = 18.0;
        let gutter_w = 300.0;
        let now = widgets::now_ts();
        egui::ScrollArea::both().auto_shrink([false, false]).show_rows(ui, row_h, lines.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let top = ui.max_rect().top();
            let left = ui.max_rect().left();
            for (k, i) in range.enumerate() {
                let l = &lines[i];
                let rect = egui::Rect::from_min_size(
                    egui::pos2(left, top + k as f32 * row_h),
                    egui::vec2(ui.available_width().max(1200.0), row_h),
                );
                ui.advance_cursor_after_rect(rect);
                let resp = ui.interact(rect, ui.scope_id().with(("blame-line", i)), Sense::click());
                let first_of_run = i == 0 || lines[i - 1].commit != l.commit;
                let color = p.lane(l.commit.0[0] % 10);
                let gutter = egui::Rect::from_min_size(rect.min, egui::vec2(gutter_w, row_h));
                ui.painter().rect_filled(gutter, 0.0, if resp.hovered() { p.hover } else { p.gutter });
                ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(3.0, row_h)), 0.0, color);
                if first_of_run && let Some(c) = commits.get(&l.commit) {
                    let text = format!(
                        "{}  {}  {}",
                        l.commit.short(),
                        widgets::ellipsize_middle(&c.author, 16),
                        widgets::format_time(c.author_time, now)
                    );
                    ui.painter().with_clip_rect(gutter).text(
                        gutter.left_center() + egui::vec2(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        text,
                        FontId::proportional(11.5),
                        p.weak,
                    );
                }
                ui.painter().text(
                    egui::pos2(rect.left() + gutter_w + 6.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                    l.line_no.to_string(),
                    FontId::monospace(11.0),
                    p.weak,
                );
                ui.painter().text(
                    egui::pos2(rect.left() + gutter_w + 46.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    diff_view::display_text(&l.text),
                    font.clone(),
                    p.text,
                );
                let resp = resp.on_hover_text(
                    commits
                        .get(&l.commit)
                        .map(|c| format!("{}\n{} — {}", c.summary, c.author, widgets::format_date(c.author_time)))
                        .unwrap_or_default(),
                );
                if resp.double_clicked() {
                    select = Some(l.commit);
                }
            }
        });
        select
    }
}

impl RepoTab {
    pub(super) fn open_file_history(&mut self, shared: &Shared, path: String, blame: bool, rev: Option<Oid>) {
        let Some(repo) = self.repo.clone() else { return };
        let mut w = FileHistoryWindow::new(self.id, repo, path, blame, if blame { rev } else { None });
        w.load_history(shared);
        if blame {
            w.load_blame(shared);
        }
        self.file_history = Some(w);
    }
}
