//! Fenêtre « Statistiques du dépôt » : comptes calculés en arrière-plan (annulables), contributeurs classés et
//! histogramme des commits par mois, dessiné avec le peintre d'egui.

use std::sync::Arc;

use dragontree_git::{CancelToken, GitError, MonthCount, Repo, RepoStats};
use dragontree_i18n::{tr, trn};
use egui::{Align2, CornerRadius, FontId, RichText, Sense, Ui};

use super::{RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared, TabId};
use crate::icons;
use crate::theme::Palette;
use crate::widgets;

/// Largeur d'une barre de l'histogramme et espace entre deux barres.
const BAR_WIDTH: f32 = 16.0;
const BAR_GAP: f32 = 4.0;
const CHART_HEIGHT: f32 = 150.0;
/// Contributeurs listés au plus (la fin de la liste n'apporte rien à l'écran).
const MAX_CONTRIBUTORS: usize = 200;

enum State {
    Loading,
    Done(Box<RepoStats>),
    Failed(String),
    Cancelled,
}

pub struct StatsWindow {
    tab: TabId,
    repo: Arc<Repo>,
    state: State,
    /// Numéro du dernier calcul demandé : les résultats plus anciens sont ignorés.
    generation: u64,
    cancel: CancelToken,
}

impl StatsWindow {
    fn new(tab: TabId, repo: Arc<Repo>, generation: u64) -> StatsWindow {
        StatsWindow { tab, repo, state: State::Loading, generation, cancel: CancelToken::new() }
    }

    /// Lance (ou relance) le calcul ; un calcul encore en cours est annulé.
    fn start(&mut self, shared: &Shared) {
        self.cancel.cancel();
        self.cancel = CancelToken::new();
        self.generation += 1;
        self.state = State::Loading;
        let (generation, tab, repo, cancel) = (self.generation, self.tab, self.repo.clone(), self.cancel.clone());
        shared.jobs.spawn("repo-stats", move || {
            Some(AppEvent::Repo(tab, RepoEvent::Stats(generation, repo.stats(&cancel).map(Box::new))))
        });
    }

    fn handle(&mut self, generation: u64, result: Result<Box<RepoStats>, GitError>) {
        if generation != self.generation {
            return;
        }
        self.state = match result {
            Ok(stats) => State::Done(stats),
            Err(GitError::Cancelled) => State::Cancelled,
            Err(e) => State::Failed(e.to_string()),
        };
    }
}

/// « 2024-03 » : le mois d'une barre de l'histogramme (neutre vis-à-vis de la langue).
fn month_label(m: &MonthCount) -> String {
    format!("{}-{:02}", m.year, m.month)
}

impl RepoTab {
    /// Ouvre la fenêtre et lance le calcul.
    pub(super) fn open_stats(&mut self, shared: &Shared) {
        let Some(repo) = self.repo.clone() else { return };
        // Les numéros continuent ceux de la fenêtre remplacée (voir `open_compare`).
        let generation = self.stats.as_ref().map_or(self.gens.stats, |w| w.generation);
        if let Some(old) = &self.stats {
            old.cancel.cancel();
        }
        let mut window = StatsWindow::new(self.id, repo, generation);
        window.start(shared);
        self.gens.stats = window.generation;
        self.stats = Some(window);
    }

    pub(super) fn on_stats(&mut self, generation: u64, result: Result<Box<RepoStats>, GitError>) {
        if let Some(w) = &mut self.stats {
            w.handle(generation, result);
        }
    }

    pub(super) fn stats_window(&mut self, ctx: &egui::Context, shared: &mut Shared, p: &Palette) {
        let Some(window) = &mut self.stats else { return };
        let mut open = true;
        let mut restart = false;
        let title = format!("{} {}", icons::CHART_BAR, tr!("Statistiques du dépôt"));
        widgets::centered_window(ctx, title)
            .id(egui::Id::unique(("repo-stats", window.tab)))
            .open(&mut open)
            .default_size([720.0, 640.0])
            .show(ctx, |ui| match &window.state {
                State::Loading => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr!("Calcul des statistiques…"));
                        if ui.button(tr!("Annuler")).clicked() {
                            window.cancel.cancel();
                        }
                    });
                }
                State::Cancelled => {
                    ui.label(RichText::new(tr!("Calcul annulé.")).color(p.weak));
                    restart = ui.button(format!("{} {}", icons::ARROWS_CLOCKWISE, tr!("Recalculer"))).clicked();
                }
                State::Failed(error) => {
                    ui.label(RichText::new(tr!("Calcul impossible : {error}", error)).color(p.removed_fg));
                    restart = ui.button(format!("{} {}", icons::ARROWS_CLOCKWISE, tr!("Recalculer"))).clicked();
                }
                State::Done(stats) => {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        stats_body(ui, stats, p);
                        ui.add_space(6.0);
                        restart = ui.button(format!("{} {}", icons::ARROWS_CLOCKWISE, tr!("Actualiser"))).clicked();
                    });
                }
            });
        if !open {
            // Fermer la fenêtre arrête le calcul en cours.
            window.cancel.cancel();
            self.stats = None;
        } else if restart {
            window.start(shared);
        }
    }
}

fn stats_body(ui: &mut Ui, stats: &RepoStats, p: &Palette) {
    let line = |ui: &mut Ui, key: String, value: String| {
        ui.label(format!("{key} : {value}"));
    };
    ui.label(RichText::new(tr!("Commits")).strong());
    line(ui, tr!("Commits (branche courante)").into(), stats.commits_head.to_string());
    line(ui, tr!("Commits (toutes les branches)").into(), stats.commits_all.to_string());
    let date = |t: Option<i64>| t.map_or_else(|| "—".to_string(), widgets::format_date);
    line(ui, tr!("Premier commit").into(), date(stats.first_commit));
    line(ui, tr!("Dernier commit").into(), date(stats.last_commit));
    ui.add_space(8.0);
    ui.label(RichText::new(tr!("Dépôt")).strong());
    line(ui, tr!("Fichiers suivis").into(), stats.tracked_files.to_string());
    let objects = &stats.objects;
    let size = tr!(
        "{pack} en pack ({packed} objets), {loose} en vrac ({count} objets)",
        pack = &objects.pack_size,
        packed = objects.packed_objects,
        loose = &objects.loose_size,
        count = objects.loose_objects
    );
    line(ui, tr!("Taille de la base d'objets").into(), size);
    line(
        ui,
        tr!("Branches et tags").into(),
        tr!(
            "{local} locales, {remote} distantes, {tags} tags",
            local = stats.local_branches,
            remote = stats.remote_branches,
            tags = stats.tags
        ),
    );
    ui.add_space(8.0);
    ui.label(RichText::new(tr!("Commits par mois (toutes les branches)")).strong());
    histogram(ui, &stats.monthly, p);
    ui.add_space(8.0);
    ui.label(RichText::new(trn!(stats.contributors.len(), "{n} contributeur", "{n} contributeurs")).strong());
    contributors(ui, stats, p);
}

/// Histogramme : une barre par mois, hauteur proportionnelle au maximum ; défile horizontalement s'il est large.
fn histogram(ui: &mut Ui, months: &[MonthCount], p: &Palette) {
    if months.is_empty() {
        ui.label(RichText::new(tr!("Aucun commit.")).color(p.weak));
        return;
    }
    let peak = months.iter().map(|m| m.commits).max().unwrap_or(1).max(1);
    let axis = 30.0;
    let width = months.len() as f32 * (BAR_WIDTH + BAR_GAP) + BAR_GAP;
    egui::ScrollArea::horizontal().id_salt("stats-months").auto_shrink([false, true]).show(ui, |ui| {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(width.max(ui.available_width()), CHART_HEIGHT + axis), Sense::hover());
        let painter = ui.painter_at(rect);
        let base = rect.top() + CHART_HEIGHT;
        painter.line_segment([egui::pos2(rect.left(), base), egui::pos2(rect.right(), base)], (1.0, p.weak));
        for (i, m) in months.iter().enumerate() {
            let x = rect.left() + BAR_GAP + i as f32 * (BAR_WIDTH + BAR_GAP);
            let height =
                (m.commits as f32 / peak as f32 * (CHART_HEIGHT - 16.0)).max(if m.commits > 0 { 2.0 } else { 0.0 });
            let bar = egui::Rect::from_min_max(egui::pos2(x, base - height), egui::pos2(x + BAR_WIDTH, base));
            let hover = ui.interact(
                egui::Rect::from_min_max(egui::pos2(x, rect.top()), egui::pos2(x + BAR_WIDTH, base + axis)),
                ui.scope_id().with(("stats-bar", i)),
                Sense::hover(),
            );
            let fill = if hover.hovered() { p.accent.gamma_multiply(0.75) } else { p.accent };
            painter.rect_filled(bar, CornerRadius::same(2), fill);
            if m.commits > 0 {
                painter.text(
                    egui::pos2(bar.center().x, bar.top() - 2.0),
                    Align2::CENTER_BOTTOM,
                    m.commits.to_string(),
                    FontId::proportional(9.5),
                    p.weak,
                );
            }
            // Le mois sous chaque barre, l'année au premier mois et à chaque janvier.
            painter.text(
                egui::pos2(bar.center().x, base + 4.0),
                Align2::CENTER_TOP,
                format!("{:02}", m.month),
                FontId::proportional(10.0),
                p.weak,
            );
            if i == 0 || m.month == 1 {
                painter.text(
                    egui::pos2(bar.center().x, base + 16.0),
                    Align2::CENTER_TOP,
                    m.year.to_string(),
                    FontId::proportional(10.0),
                    p.text,
                );
            }
            hover.on_hover_text(format!("{} : {}", month_label(m), trn!(m.commits, "{n} commit", "{n} commits")));
        }
    });
}

fn contributors(ui: &mut Ui, stats: &RepoStats, p: &Palette) {
    let top = stats.contributors.first().map_or(1, |c| c.commits).max(1);
    for c in stats.contributors.iter().take(MAX_CONTRIBUTORS) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(120.0, 12.0), Sense::hover());
            let filled = egui::Rect::from_min_size(
                rect.min,
                egui::vec2(rect.width() * c.commits as f32 / top as f32, rect.height()),
            );
            ui.painter().rect_filled(rect, CornerRadius::same(2), p.hover);
            ui.painter().rect_filled(filled, CornerRadius::same(2), p.accent);
            ui.label(RichText::new(c.commits.to_string()).strong());
            let who = if c.email.is_empty() { c.name.clone() } else { format!("{} <{}>", c.name, c.email) };
            let resp = ui.add(egui::Label::new(who).truncate());
            if !c.aliases.is_empty() {
                resp.on_hover_text(tr!("Aussi connu sous : {names}", names = c.aliases.join(", ")));
            }
        });
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::settings::SettingsFile;

    #[test]
    fn a_result_from_a_replaced_calculation_is_ignored_and_a_cancelled_one_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let git = crate::test_git();
        git.cmd(dir.path()).args(["init", "-q"]).run().unwrap();
        let (store, settings, _) = SettingsFile::load(&dir.path().join("settings.json"));
        let (shared, _events) = Shared::new(egui::Context::default(), settings, store, None, None);
        let repo = Arc::new(Repo::open(&git, dir.path()).unwrap());
        let mut window = StatsWindow::new(1, repo, 0);
        window.start(&shared);
        let first = window.generation;
        window.start(&shared);
        assert!(window.generation > first);
        assert!(!window.cancel.is_cancelled(), "le nouveau calcul n'est pas annulé");

        window.handle(first, Ok(Box::default()));
        assert!(matches!(window.state, State::Loading), "résultat du calcul remplacé : ignoré");
        window.handle(window.generation, Err(GitError::Cancelled));
        assert!(matches!(window.state, State::Cancelled));
        window.handle(window.generation, Ok(Box::default()));
        assert!(matches!(window.state, State::Done(_)));
    }
}
