//! Bisect : bandeau d'état pendant une session (Bon, Mauvais, Passer, Terminer), étiquettes des commits marqués
//! et opérations. L'état vient du rafraîchissement de l'état du dépôt (`Repo::bisect_state`, tâche de fond).

use dragontree_git::{BisectMark, BisectState, Oid};
use dragontree_i18n::{tr, trn};
use egui::{Rect, RichText, Sense, Ui};

use super::{Refresh, RepoTab, RepoView};
use crate::app::Shared;
use crate::icons;
use crate::theme::Palette;

/// Étiquette d'un commit marqué par le bisect, ou `None`. Le premier mauvais commit (fin de recherche) reste
/// « mauvais ».
pub fn badge_text(state: &BisectState, oid: Oid) -> Option<String> {
    if state.bad == Some(oid) {
        Some(format!("{} {}", icons::BUG, tr!("bisect mauvais")))
    } else if state.good.contains(&oid) {
        Some(format!("{} {}", icons::CHECK, tr!("bisect bon")))
    } else {
        None
    }
}

/// Étiquette peinte dans le graphe, lisible par l'accessibilité (les pastilles peintes ne le sont pas) ; renvoie
/// sa largeur.
pub fn paint_badge(ui: &Ui, painter: &egui::Painter, pos: egui::Pos2, text: String, bad: bool, p: &Palette) -> f32 {
    let colors = if bad { p.badge_head } else { (p.hover, p.added_fg) };
    let width =
        super::history::paint_badge(ui, painter, pos, text.clone(), colors, egui::FontFamily::Proportional, !bad);
    let id = ui.scope_id().with(("bisect-badge", text.clone(), pos.x as i32, pos.y as i32));
    let info = ui.interact(Rect::from_min_size(pos, egui::vec2(width, 17.0)), id, Sense::hover());
    info.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Label, true, text.clone()));
    width
}

impl RepoTab {
    /// Une session est en cours.
    pub fn bisecting(&self) -> bool {
        self.bisect.is_some()
    }

    /// Une relecture des références ou de l'état est en route : HEAD et la session affichés peuvent dater d'avant la
    /// dernière opération (git a déjà extrait le commit suivant).
    fn state_refreshing(&self) -> bool {
        self.gens.refs_seen < self.gens.refs || self.gens.status_seen < self.gens.status
    }

    /// Les réponses Bon, Mauvais et Passer s'adressent au commit en cours d'examen : possibles quand git a un
    /// commit bon et un mauvais, et que la recherche n'est pas finie.
    fn bisect_testing(&self) -> bool {
        self.bisect.as_ref().is_some_and(|b| b.remaining.is_some() && b.found.is_none())
    }

    pub(super) fn op_bisect_start(&mut self, shared: &Shared) {
        self.run_simple(shared, tr!("Bisect"), Refresh::ALL, |r| {
            r.bisect_start()?;
            Ok(Some(tr!("Bisect démarré : HEAD est marqué mauvais, marquez un commit bon.").into()))
        });
    }

    /// Marque `rev` (le commit en cours d'examen si `None`) ; démarre la session au besoin.
    pub(crate) fn op_bisect_mark(&mut self, shared: &Shared, mark: BisectMark, rev: Option<Oid>) {
        self.run_simple(shared, tr!("Bisect"), Refresh::ALL, move |r| {
            r.bisect_mark(mark, rev.map(|o| o.to_hex()).as_deref())?;
            Ok(None)
        });
    }

    pub(super) fn op_bisect_reset(&mut self, shared: &Shared) {
        self.run_simple(shared, tr!("Bisect"), Refresh::ALL, |r| {
            r.bisect_reset()?;
            Ok(Some(tr!("Bisect terminé : retour à la branche de départ.").into()))
        });
    }

    /// Bandeau d'état : révisions restantes, boutons de réponse, ou premier mauvais commit en fin de recherche.
    pub(super) fn bisect_banner(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        let Some(state) = self.bisect.clone() else { return };
        let testing = self.bisect_testing();
        // Les réponses portent sur le commit affiché au moment du clic (pas sur HEAD à l'exécution de la tâche, qui
        // peut avoir bougé), et se bloquent tant qu'une opération tourne : un double clic ne marque pas le suivant.
        // Elles attendent aussi la relecture qui suit l'opération : sinon le commit affiché serait encore celui qu'on
        // vient de marquer, et la réponse suivante lui serait appliquée.
        let shown = self.refs.as_ref().and_then(|r| r.head_oid);
        let idle = self.running.is_empty() && !self.state_refreshing() && shown.is_some();
        ui.horizontal(|ui| {
            ui.label(RichText::new(icons::BUG).color(p.modified_fg).size(16.0));
            ui.label(RichText::new(tr!("Bisect en cours")).strong());
            if let Some(found) = state.found {
                ui.label(
                    RichText::new(tr!("— premier mauvais commit : {short}", short = found.short()))
                        .color(p.conflict_fg),
                );
                if ui.button(format!("{} {}", icons::CROSSHAIR, tr!("Voir le commit"))).clicked() {
                    self.view = RepoView::History;
                    self.select_commit(shared, found, true);
                }
            } else if let Some(n) = state.remaining {
                ui.label(
                    RichText::new(trn!(
                        n,
                        "— {n} révision à tester après celle-ci",
                        "— {n} révisions à tester après celle-ci"
                    ))
                    .color(p.weak),
                );
                if let Some(steps) = state.steps {
                    ui.label(RichText::new(trn!(steps, "(environ {n} étape)", "(environ {n} étapes)")).color(p.weak));
                }
            } else if state.good.is_empty() {
                ui.label(RichText::new(tr!("— marquez un commit bon (menu d'un commit)")).color(p.weak));
            } else {
                ui.label(RichText::new(tr!("— marquez un commit mauvais (menu d'un commit)")).color(p.weak));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(format!("{} {}", icons::X, tr!("Terminer"))).clicked() {
                    self.op_bisect_reset(shared);
                }
                if testing {
                    if ui
                        .add_enabled(idle, egui::Button::new(format!("{} {}", icons::SKIP_FORWARD, tr!("Passer"))))
                        .clicked()
                    {
                        self.op_bisect_mark(shared, BisectMark::Skip, shown);
                    }
                    if ui.add_enabled(idle, egui::Button::new(format!("{} {}", icons::BUG, tr!("Mauvais")))).clicked() {
                        self.op_bisect_mark(shared, BisectMark::Bad, shown);
                    }
                    if ui.add_enabled(idle, egui::Button::new(format!("{} {}", icons::CHECK, tr!("Bon")))).clicked() {
                        self.op_bisect_mark(shared, BisectMark::Good, shown);
                    }
                }
            });
        });
    }
}
