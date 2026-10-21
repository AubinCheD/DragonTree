//! Fenêtre « Comparaison A ↔ B » : fichiers qui diffèrent entre deux révisions (commits, branches), ou entre une
//! révision et la copie de travail, et diff du fichier choisi avec la vue de diff de l'historique.

use std::sync::Arc;

use dragontree_git::{FileChange, GitError, Oid, Repo};
use dragontree_i18n::{tr, trn};
use egui::{RichText, Ui};

use super::diff_view::{self, DiffMode, DiffViewState};
use super::{RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared, TabId};
use crate::diff_style::LoadedDiff;
use crate::icons;
use crate::theme::Palette;
use crate::toasts::Toast;

/// Un côté de la comparaison : la révision que git lit et le nom affiché.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareSide {
    pub rev: String,
    pub label: String,
}

impl CompareSide {
    pub fn commit(oid: Oid) -> CompareSide {
        CompareSide { rev: oid.to_hex(), label: oid.short() }
    }

    pub fn head() -> CompareSide {
        CompareSide { rev: "HEAD".into(), label: "HEAD".into() }
    }

    /// Branche (locale `main`, distante `origin/main`) ou tout nom de référence.
    pub fn named(name: &str) -> CompareSide {
        CompareSide { rev: name.to_string(), label: name.to_string() }
    }
}

pub enum CompareEvent {
    Files(u64, Result<Vec<FileChange>, GitError>),
    Diff(u64, Result<LoadedDiff, GitError>),
}

pub struct CompareWindow {
    tab: TabId,
    repo: Arc<Repo>,
    a: CompareSide,
    /// `None` : la copie de travail.
    b: Option<CompareSide>,
    files: Vec<FileChange>,
    files_loading: bool,
    selected: Option<usize>,
    diff: Option<LoadedDiff>,
    diff_loading: bool,
    diff_state: DiffViewState,
    highlighter: crate::highlight::Highlighter,
    error: Option<String>,
    /// Numéro de la dernière demande (liste ou diff) : les résultats plus anciens sont ignorés.
    generation: u64,
}

impl CompareWindow {
    fn new(tab: TabId, repo: Arc<Repo>, a: CompareSide, b: Option<CompareSide>, generation: u64) -> CompareWindow {
        CompareWindow {
            tab,
            repo,
            a,
            b,
            files: Vec::new(),
            files_loading: true,
            selected: None,
            diff: None,
            diff_loading: false,
            diff_state: DiffViewState::default(),
            highlighter: crate::highlight::Highlighter::default(),
            error: None,
            generation,
        }
    }

    fn b_rev(&self) -> Option<String> {
        self.b.as_ref().map(|b| b.rev.clone())
    }

    fn b_label(&self) -> String {
        self.b.as_ref().map_or_else(|| tr!("copie de travail").to_string(), |b| b.label.clone())
    }

    fn load_files(&mut self, shared: &Shared) {
        self.generation += 1;
        self.files_loading = true;
        self.selected = None;
        self.diff = None;
        self.diff_loading = false;
        self.error = None;
        let (generation, tab, repo) = (self.generation, self.tab, self.repo.clone());
        let (a, b) = (self.a.rev.clone(), self.b_rev());
        shared.jobs.spawn("compare-files", move || {
            let res = repo.compare_files(&a, b.as_deref());
            Some(AppEvent::Repo(tab, RepoEvent::Compare(CompareEvent::Files(generation, res))))
        });
    }

    fn load_diff(&mut self, shared: &Shared, index: usize) {
        let Some(change) = self.files.get(index).cloned() else { return };
        self.generation += 1;
        self.selected = Some(index);
        self.diff = None;
        self.diff_loading = true;
        self.diff_state = DiffViewState::default();
        let (generation, tab, repo) = (self.generation, self.tab, self.repo.clone());
        let (a, b) = (self.a.rev.clone(), self.b_rev());
        let o = RepoTab::diff_options(shared);
        shared.jobs.spawn("compare-diff", move || {
            let res = repo.compare_file_diff(&a, b.as_deref(), &change, &o).map(LoadedDiff::new);
            Some(AppEvent::Repo(tab, RepoEvent::Compare(CompareEvent::Diff(generation, res))))
        });
    }

    /// Échange A et B (impossible contre la copie de travail, qui ne peut être que B).
    fn swap(&mut self, shared: &Shared) {
        let Some(b) = self.b.take() else { return };
        self.b = Some(std::mem::replace(&mut self.a, b));
        self.load_files(shared);
    }

    pub fn handle(&mut self, shared: &Shared, ev: CompareEvent) {
        match ev {
            CompareEvent::Files(g, res) if g == self.generation => {
                self.files_loading = false;
                match res {
                    Ok(files) => {
                        self.files = files;
                        if !self.files.is_empty() {
                            self.load_diff(shared, 0);
                        }
                    }
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            CompareEvent::Diff(g, res) if g == self.generation => {
                self.diff_loading = false;
                match res {
                    Ok(d) => self.diff = Some(d),
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            _ => {}
        }
    }

    /// Affiche la fenêtre ; renvoie vrai tant qu'elle reste ouverte.
    pub fn ui(&mut self, ctx: &egui::Context, shared: &mut Shared, p: &Palette) -> bool {
        let mut open = true;
        let names = tr!("Comparaison {a} ↔ {b}", a = self.a.label, b = self.b_label());
        crate::widgets::centered_window(ctx, format!("{} {names}", icons::GIT_DIFF))
            .id(egui::Id::unique(("compare", self.tab)))
            .open(&mut open)
            .default_size([1100.0, 680.0])
            .show(ctx, |ui| self.body(ui, shared, p));
        open
    }

    fn body(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("A : {}", self.a.label)).strong());
            let swap = ui
                .add_enabled(self.b.is_some(), egui::Button::new(icons::SWAP))
                .on_hover_text(tr!("Inverser A et B"))
                .on_disabled_hover_text(tr!("La copie de travail ne peut être que B."));
            if swap.clicked() {
                self.swap(shared);
            }
            ui.label(RichText::new(format!("B : {}", self.b_label())).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let split = shared.settings.split_diff;
                if ui.selectable_label(split, icons::COLUMNS).on_hover_text(tr!("Côte à côte")).clicked() {
                    shared.settings.split_diff = !split;
                    shared.settings_dirty = true;
                }
            });
        });
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(p.removed_fg));
        }
        ui.separator();
        egui::Panel::left(("compare-files", self.tab)).resizable(true).default_size(340.0).show(ui, |ui| {
            if self.files_loading {
                ui.spinner();
                return;
            }
            let count = trn!(self.files.len(), "{n} fichier différent", "{n} fichiers différents");
            ui.label(RichText::new(count).strong());
            let mut clicked = None;
            egui::ScrollArea::vertical().id_salt(("compare-scroll", self.tab)).auto_shrink([false, false]).show(
                ui,
                |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, f) in self.files.iter().enumerate() {
                        let (letter, color, label) = super::details::change_badge(f.kind, p);
                        let selected = self.selected == Some(i);
                        let old = f.old_path.as_deref();
                        let id = ("compare-file", &f.path);
                        let resp = super::details::file_row(ui, id, letter, color, &f.path, old, selected, p);
                        if resp.on_hover_text(label).clicked() && !selected {
                            clicked = Some(i);
                        }
                    }
                },
            );
            if let Some(i) = clicked {
                self.load_diff(shared, i);
            }
        });
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(diff) = self.diff.clone() {
                ui.label(RichText::new(format!("+{}  -{}", diff.added_count(), diff.removed_count())).color(p.weak));
                if self.highlighter.is_dark() != p.dark {
                    self.highlighter = crate::highlight::Highlighter::new(p.dark);
                }
                let hl = shared.settings.syntax_highlight.then_some(&mut self.highlighter);
                let split = shared.settings.split_diff;
                let salt = ("compare-diff", self.tab);
                let style = crate::diff_style::DiffStyle::from_settings(&shared.settings);
                diff_view::show(ui, &diff, DiffMode::ReadOnly, &mut self.diff_state, split, &style, hl, p, salt);
            } else if self.diff_loading {
                ui.centered_and_justified(|ui| ui.spinner());
            } else if !self.files_loading && self.files.is_empty() && self.error.is_none() {
                ui.centered_and_justified(|ui| ui.label(RichText::new(tr!("Aucune différence.")).color(p.weak)));
            }
        });
    }
}

impl RepoTab {
    /// Ouvre la comparaison de `a` avec `b` (`None` : la copie de travail).
    pub(super) fn open_compare(&mut self, shared: &Shared, a: CompareSide, b: Option<CompareSide>) {
        let Some(repo) = self.repo.clone() else { return };
        // Les numéros continuent ceux de la fenêtre remplacée : ses tâches encore en cours ne peuvent pas être
        // prises pour celles de la nouvelle.
        if let Some(old) = &self.compare {
            self.gens.compare = self.gens.compare.max(old.generation);
        }
        let mut w = CompareWindow::new(self.id, repo, a, b, self.gens.compare);
        w.load_files(shared);
        self.gens.compare = w.generation;
        self.compare = Some(w);
    }

    pub(super) fn on_compare(&mut self, shared: &mut Shared, ev: CompareEvent) {
        if let Some(w) = &mut self.compare {
            w.handle(shared, ev);
        } else if let CompareEvent::Files(_, Err(e)) = ev {
            shared.toast(Toast::error(tr!("Comparaison impossible"), e.to_string()));
        }
    }

    /// Entrées de menu « Comparer » d'un commit : avec HEAD, la copie de travail et le commit marqué.
    pub(super) fn compare_commit_entries(&mut self, ui: &mut Ui, shared: &mut Shared, oid: Oid) {
        let a = CompareSide::commit(oid);
        if ui.button(format!("{} {}", icons::GIT_DIFF, tr!("Comparer avec HEAD"))).clicked() {
            self.open_compare(shared, a.clone(), Some(CompareSide::head()));
            ui.close();
        }
        if ui.button(format!("{} {}", icons::GIT_DIFF, tr!("Comparer avec les modifications locales"))).clicked() {
            self.open_compare(shared, a.clone(), None);
            ui.close();
        }
        if ui.button(format!("{} {}", icons::GIT_DIFF, tr!("Marquer pour comparaison"))).clicked() {
            self.compare_mark = Some(oid);
            ui.close();
        }
        if let Some(mark) = self.compare_mark.filter(|m| *m != oid) {
            let label = tr!("Comparer avec {short} (marqué)", short = mark.short());
            if ui.button(format!("{} {label}", icons::GIT_DIFF)).clicked() {
                self.open_compare(shared, a, Some(CompareSide::commit(mark)));
                ui.close();
            }
        }
    }

    /// Entrée de menu d'une branche : la comparer avec la branche courante.
    pub(super) fn compare_branch_entry(&mut self, ui: &mut Ui, shared: &mut Shared, name: &str, current: Option<&str>) {
        let Some(cur) = current.filter(|c| *c != name) else { return };
        if ui.button(format!("{} {}", icons::GIT_DIFF, tr!("Comparer avec la branche courante"))).clicked() {
            self.open_compare(shared, CompareSide::named(name), Some(CompareSide::named(cur)));
            ui.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use dragontree_git::{ChangeKind, Git};

    use super::*;
    use crate::settings::SettingsFile;

    #[test]
    fn a_result_from_a_replaced_window_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::discover().unwrap();
        git.cmd(dir.path()).args(["init", "-q"]).run().unwrap();
        let (store, settings, _) = SettingsFile::load(&dir.path().join("settings.json"));
        let (mut shared, _events) = Shared::new(egui::Context::default(), settings, store, None, None);
        let mut tab = RepoTab::new(1, dir.path().to_path_buf(), &mut shared);
        tab.repo = Some(Arc::new(Repo::open(&git, dir.path()).unwrap()));

        tab.open_compare(&shared, CompareSide::named("HEAD"), None);
        let first = tab.compare.as_ref().unwrap().generation;
        tab.open_compare(&shared, CompareSide::named("main"), None);
        assert!(tab.compare.as_ref().unwrap().generation > first, "les numéros ne repartent pas de zéro");

        // La liste de la première comparaison arrive après son remplacement : elle n'est pas affichée.
        let stale = FileChange { path: "ancien.txt".into(), old_path: None, kind: ChangeKind::Added };
        tab.on_compare(&mut shared, CompareEvent::Files(first, Ok(vec![stale])));
        let w = tab.compare.as_ref().unwrap();
        assert!(w.files.is_empty() && w.files_loading);
    }
}
