//! Git Flow : dialogues d'initialisation, de démarrage et de fin d'une branche de travail (feature, release,
//! hotfix) et leurs opérations. La logique est dans le moteur (`dragontree_git::gitflow`) ; les entrées de menu
//! sont déclarées dans le registre de commandes.

use dragontree_git::{FinishOutcome, FlowKind, FlowStep, GitError, GitFlowConfig};
use dragontree_i18n::tr;
use egui::{RichText, Ui};

use super::dialogs::{valid_ref_name, warning};
use super::{Dialog, Refresh, RepoTab};
use crate::app::Shared;
use crate::theme::Palette;

/// Titre et libellé du bouton du dialogue de démarrage, et exemple de nom, pour chaque nature de branche.
pub fn start_texts(kind: FlowKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        FlowKind::Feature => (tr!("Démarrer une feature"), tr!("Démarrer la feature"), tr!("nom-de-la-feature")),
        FlowKind::Release => (tr!("Démarrer une release"), tr!("Démarrer la release"), "1.0.0"),
        FlowKind::Hotfix => (tr!("Démarrer un hotfix"), tr!("Démarrer le hotfix"), "1.0.1"),
    }
}

/// Étapes restantes d'une fin de branche interrompue, en une phrase.
fn remaining_text(branch: &str, name: &str, remaining: &[FlowStep]) -> String {
    if remaining.is_empty() {
        return tr!("Rien d'autre ne reste à faire.").into();
    }
    let steps: Vec<String> = remaining
        .iter()
        .map(|step| match step {
            FlowStep::MergeInto(into) => tr!("fusionner « {branch} » dans « {into} » (--no-ff)", branch, into),
            FlowStep::Tag(tag) => {
                tr!("créer le tag annoté « {tag} » (message « Version {name} ») sur la production", tag, name)
            }
            FlowStep::Delete(name) => tr!("supprimer la branche « {name} »", name),
        })
        .collect();
    tr!("Il restera à faire à la main : {steps}.", steps = steps.join(" ; "))
}

impl RepoTab {
    /// Nom de la branche de production proposé à l'initialisation : `main`, sinon `master`, sinon la branche
    /// courante.
    fn default_production(&self) -> String {
        let Some(refs) = &self.refs else { return "main".into() };
        ["main", "master"]
            .into_iter()
            .find(|name| refs.local(name).is_some())
            .map(str::to_string)
            .or_else(|| refs.current_branch().map(str::to_string))
            .unwrap_or_else(|| "main".into())
    }

    /// Dépôt › Git Flow › Initialiser : la configuration actuelle (git-flow ou DragonTree) préremplit les champs.
    pub(super) fn open_gitflow_init(&mut self) {
        let cfg = self.gitflow.clone().unwrap_or_else(|| GitFlowConfig::with_production(&self.default_production()));
        self.dialog = Some(Dialog::GitFlowInit(Box::new(cfg)));
    }

    pub(super) fn open_gitflow_start(&mut self, kind: FlowKind) {
        self.dialog = Some(Dialog::GitFlowStart { kind, name: String::new() });
    }

    /// Dépôt › Git Flow › Terminer : la branche courante, si c'est une feature, une release ou un hotfix.
    pub(super) fn open_gitflow_finish(&mut self) {
        let Some(cfg) = &self.gitflow else { return };
        let current = self.refs.as_ref().and_then(|r| r.current_branch().map(str::to_string));
        if let Some((kind, name)) = current.and_then(|b| cfg.classify(&b)) {
            self.dialog = Some(Dialog::GitFlowFinish { kind, name, delete_branch: false });
        }
    }

    /// Nature et nom court de la branche courante si c'est une branche de travail de Git Flow.
    pub(super) fn current_flow_branch(&self) -> Option<(FlowKind, String)> {
        let current = self.refs.as_ref()?.current_branch()?;
        self.gitflow.as_ref()?.classify(current)
    }

    fn local_branch_exists(&self, name: &str) -> bool {
        self.refs.as_ref().is_some_and(|r| r.local(name).is_some())
    }

    /// Des modifications suivies (indexées ou non) ou des conflits : la fin d'une branche les refuse.
    fn has_tracked_changes(&self) -> bool {
        use dragontree_git::FileState;
        self.status.as_ref().is_some_and(|s| {
            s.entries.iter().any(|e| {
                !matches!(e.index, FileState::Ignored)
                    && (e.conflict.is_some()
                        || !matches!(e.index, FileState::Unmodified | FileState::Untracked)
                        || !matches!(e.worktree, FileState::Unmodified | FileState::Untracked | FileState::Ignored))
            })
        })
    }

    /// Champs du dialogue d'initialisation ; renvoie vrai si la saisie est valide.
    pub(super) fn gitflow_init_body(&self, ui: &mut Ui, cfg: &mut GitFlowConfig, p: &Palette) -> bool {
        ui.label(
            RichText::new(tr!(
                "Les réglages sont écrits dans la configuration du dépôt (clés gitflow.*), comme le fait git-flow."
            ))
            .color(p.weak),
        );
        ui.add_space(4.0);
        egui::Grid::new("gitflow-init-grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            let field = |ui: &mut Ui, label: &str, value: &mut String| {
                ui.label(label);
                ui.add(egui::TextEdit::singleline(value).desired_width(260.0));
                ui.end_row();
            };
            field(ui, tr!("Branche de production"), &mut cfg.production);
            field(ui, tr!("Branche de développement"), &mut cfg.development);
            field(ui, tr!("Préfixe des features"), &mut cfg.feature);
            field(ui, tr!("Préfixe des releases"), &mut cfg.release);
            field(ui, tr!("Préfixe des hotfixes"), &mut cfg.hotfix);
            field(ui, tr!("Préfixe des branches de support"), &mut cfg.support);
            field(ui, tr!("Préfixe des tags de version"), &mut cfg.version_tag);
        });
        // Les champs sont lus sans espaces autour : ils ne peuvent pas en contenir de toute façon.
        for value in [
            &mut cfg.production,
            &mut cfg.development,
            &mut cfg.feature,
            &mut cfg.release,
            &mut cfg.hotfix,
            &mut cfg.support,
            &mut cfg.version_tag,
        ] {
            *value = value.trim().to_string();
        }
        let prefix_ok = |prefix: &str| valid_ref_name(&format!("{prefix}x"));
        let mut valid = true;
        if !valid_ref_name(&cfg.production) || !valid_ref_name(&cfg.development) {
            warning(ui, tr!("Nom de branche invalide (pas d'espace ni de caractères ~^:?*[\\)."), p);
            valid = false;
        } else if cfg.production == cfg.development {
            warning(ui, tr!("Les branches de production et de développement doivent être différentes."), p);
            valid = false;
        }
        if ![&cfg.feature, &cfg.release, &cfg.hotfix, &cfg.support].into_iter().all(|v| prefix_ok(v)) {
            warning(ui, tr!("Préfixe invalide : les quatre préfixes de branche sont obligatoires."), p);
            valid = false;
        }
        if !(cfg.version_tag.is_empty() || valid_ref_name(&format!("{}1", cfg.version_tag))) {
            warning(ui, tr!("Préfixe de tag invalide (pas d'espace ni de caractères ~^:?*[\\)."), p);
            valid = false;
        }
        if !self.local_branch_exists(&cfg.production) {
            warning(ui, tr!("La branche de production n'existe pas : faites d'abord un commit dessus."), p);
            valid = false;
        } else if valid && !self.local_branch_exists(&cfg.development) {
            ui.label(
                RichText::new(tr!(
                    "La branche « {name} » sera créée à partir de « {production} ».",
                    name = &cfg.development,
                    production = &cfg.production
                ))
                .color(p.weak),
            );
        }
        valid
    }

    /// Champs du dialogue de démarrage ; renvoie vrai si la saisie est valide.
    pub(super) fn gitflow_start_body(&self, ui: &mut Ui, kind: FlowKind, name: &mut String, p: &Palette) -> bool {
        let Some(cfg) = &self.gitflow else { return false };
        let branch = cfg.branch(kind, name.trim());
        ui.label(RichText::new(tr!("À partir de : {start_label}", start_label = cfg.base(kind))).color(p.weak));
        ui.add(egui::TextEdit::singleline(name).hint_text(start_texts(kind).2).desired_width(f32::INFINITY))
            .request_focus();
        if name.trim().is_empty() {
            return false;
        }
        ui.label(RichText::new(tr!("Branche créée : {branch}", branch = &branch)).color(p.weak));
        if !valid_ref_name(&branch) {
            warning(ui, tr!("Nom de branche invalide (pas d'espace ni de caractères ~^:?*[\\)."), p);
            false
        } else if self.local_branch_exists(&branch) {
            warning(ui, tr!("Une branche porte déjà ce nom."), p);
            false
        } else {
            true
        }
    }

    /// Champs du dialogue de fin ; renvoie vrai si la fin peut être lancée.
    pub(super) fn gitflow_finish_body(
        &self,
        ui: &mut Ui,
        kind: FlowKind,
        name: &str,
        delete_branch: &mut bool,
        p: &Palette,
    ) -> bool {
        let Some(cfg) = &self.gitflow else { return false };
        let branch = cfg.branch(kind, name);
        let development = &cfg.development;
        match kind {
            FlowKind::Feature => ui.label(tr!(
                "« {branch} » sera fusionnée dans « {development} » (toujours avec un commit de fusion).",
                branch = &branch,
                development
            )),
            FlowKind::Release | FlowKind::Hotfix => ui.label(tr!(
                "« {branch} » sera fusionnée dans « {production} » (commit de fusion), taguée « {tag} » (message « Version {name} »), puis fusionnée dans « {development} ».",
                branch = &branch,
                production = &cfg.production,
                tag = cfg.tag(name),
                name,
                development
            )),
        };
        ui.add_space(4.0);
        ui.checkbox(delete_branch, tr!("Supprimer la branche « {branch} » ensuite", branch = &branch)).on_hover_text(
            tr!("Suppression sûre : git refuse de supprimer une branche qui ne serait pas entièrement fusionnée."),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(tr!(
                "En cas de conflit, la suite s'arrête : résolvez-les, validez la fusion (bandeau en haut), puis faites à la main les étapes qui restent."
            ))
            .color(p.weak)
            .size(12.0),
        );
        if self.has_tracked_changes() {
            warning(
                ui,
                tr!("Des modifications ne sont pas commitées : commitez-les ou mettez-les de côté d'abord."),
                p,
            );
            return false;
        }
        true
    }

    pub(super) fn op_gitflow_init(&mut self, shared: &Shared, cfg: GitFlowConfig) {
        self.run_simple(shared, "Git Flow", Refresh::ALL, move |r| {
            r.gitflow_init(&cfg)?;
            Ok(Some(tr!(
                "Git Flow initialisé : {production} et {development}",
                production = &cfg.production,
                development = &cfg.development
            )))
        });
    }

    pub(super) fn op_gitflow_start(&mut self, shared: &Shared, kind: FlowKind, name: String) {
        let Some(cfg) = self.gitflow.clone() else { return };
        self.run_simple(shared, "Git Flow", Refresh::ALL, move |r| {
            let branch = r.gitflow_start(&cfg, kind, &name)?;
            Ok(Some(tr!("Branche {branch} créée", branch)))
        });
    }

    pub(super) fn op_gitflow_finish(&mut self, shared: &Shared, kind: FlowKind, name: String, delete_branch: bool) {
        let Some(cfg) = self.gitflow.clone() else { return };
        self.run_simple(shared, "Git Flow", Refresh::ALL, move |r| {
            let branch = cfg.branch(kind, &name);
            match r.gitflow_finish(&cfg, kind, &name, delete_branch)? {
                FinishOutcome::Done => Ok(Some(tr!("« {branch} » terminée", branch))),
                // Les conflits ne sont pas un succès : le dépôt reste en cours de fusion (bandeau de l'onglet) et
                // le message dit ce qu'il faudra faire ensuite.
                FinishOutcome::Stopped { into, remaining } => Err(GitError::Refused(tr!(
                    "Conflits en fusionnant « {branch} » dans « {into} » : résolvez-les, puis validez la fusion (ou abandonnez-la). {rest}",
                    branch = &branch,
                    into,
                    rest = remaining_text(&branch, &name, &remaining)
                ))),
                // Échec sans conflit après des étapes faites (branche tenue par un worktree, crochet, fichier non suivi
                // gênant…) : la suite dit ce qui reste, en partant de l'étape en échec.
                FinishOutcome::Failed { error, remaining } => Err(GitError::Refused(tr!(
                    "La fin de « {branch} » s'est arrêtée : {error}. Le dépôt reste dans l'état où elle l'a laissé. {rest}",
                    branch = &branch,
                    error = error.stderr_summary(),
                    rest = remaining_text(&branch, &name, &remaining)
                ))),
            }
        });
    }
}
