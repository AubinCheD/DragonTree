//! Git Flow sans l'extension git-flow : des commandes git ordinaires, avec les clés de configuration de git-flow
//! (`gitflow.branch.master`, `gitflow.branch.develop`, `gitflow.prefix.*`) pour qu'un dépôt déjà initialisé par
//! git-flow AVH soit reconnu, et réciproquement.
//!
//! Terminer une branche est une suite de commandes (`checkout`, `merge --no-ff`, `tag -a`, `branch -d`) lancées
//! l'une après l'autre. Si une fusion s'arrête sur des conflits, la suite s'arrête là : le dépôt reste dans l'état
//! de fusion de git (à continuer ou abandonner), et le résultat nomme les étapes qui restent à faire à la main.

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::ops::MergeMode;
use crate::repo::{OpState, Repo};

/// Nature d'une branche de travail de Git Flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlowKind {
    Feature,
    Release,
    Hotfix,
}

/// Configuration de Git Flow : branches permanentes et préfixes (clés `gitflow.*` de la configuration locale).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFlowConfig {
    /// Branche de production (`gitflow.branch.master`).
    pub production: String,
    /// Branche de développement (`gitflow.branch.develop`).
    pub development: String,
    pub feature: String,
    pub release: String,
    pub hotfix: String,
    pub support: String,
    /// Préfixe des tags de version (`gitflow.prefix.versiontag`), vide par défaut.
    pub version_tag: String,
}

/// Une étape de la fin d'une branche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowStep {
    /// Se placer sur la branche puis y fusionner la branche terminée (`--no-ff`).
    MergeInto(String),
    /// Tag annoté (nom complet, préfixe de version compris) sur la branche de production.
    Tag(String),
    /// Supprimer la branche terminée (`branch -d`, jamais forcé).
    Delete(String),
}

/// Résultat de la fin d'une branche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishOutcome {
    Done,
    /// Une fusion dans `into` s'est arrêtée sur des conflits : le dépôt est en cours de fusion, `remaining` sont
    /// les étapes suivantes, non exécutées.
    Stopped {
        into: String,
        remaining: Vec<FlowStep>,
    },
    /// Une étape a échoué sans conflit alors que des étapes précédentes étaient faites (le dépôt reste où elle l'a
    /// laissé) : `remaining` commence par l'étape en échec, `error` est celle de git.
    Failed {
        error: GitError,
        remaining: Vec<FlowStep>,
    },
}

impl GitFlowConfig {
    /// Réglages usuels de git-flow, avec `production` comme branche de production.
    pub fn with_production(production: &str) -> GitFlowConfig {
        GitFlowConfig {
            production: production.to_string(),
            development: "develop".into(),
            feature: "feature/".into(),
            release: "release/".into(),
            hotfix: "hotfix/".into(),
            support: "support/".into(),
            version_tag: String::new(),
        }
    }

    fn prefix(&self, kind: FlowKind) -> &str {
        match kind {
            FlowKind::Feature => &self.feature,
            FlowKind::Release => &self.release,
            FlowKind::Hotfix => &self.hotfix,
        }
    }

    /// Nom complet de la branche `name` de cette nature (`feature/x`).
    pub fn branch(&self, kind: FlowKind, name: &str) -> String {
        format!("{}{name}", self.prefix(kind))
    }

    /// Nom du tag de version pour `name`.
    pub fn tag(&self, name: &str) -> String {
        format!("{}{name}", self.version_tag)
    }

    /// Nature et nom court d'une branche de travail, `None` pour une autre branche.
    pub fn classify(&self, branch: &str) -> Option<(FlowKind, String)> {
        [FlowKind::Feature, FlowKind::Release, FlowKind::Hotfix].into_iter().find_map(|kind| {
            let prefix = self.prefix(kind);
            let name = branch.strip_prefix(prefix).filter(|n| !prefix.is_empty() && !n.is_empty())?;
            Some((kind, name.to_string()))
        })
    }

    /// Branche de départ d'une branche de cette nature : la production pour un hotfix, sinon le développement.
    pub fn base(&self, kind: FlowKind) -> &str {
        if kind == FlowKind::Hotfix { &self.production } else { &self.development }
    }
}

/// Lit la sortie de `git config --get-regexp ^gitflow\.` : `None` si les branches de production et de
/// développement ne sont pas toutes deux définies (dépôt non initialisé). Les préfixes absents prennent la valeur
/// usuelle de git-flow.
fn parse_config(text: &str) -> Option<GitFlowConfig> {
    let value = |key: &str| {
        text.lines().find_map(|line| {
            let (k, v) = line.split_once(' ').unwrap_or((line, ""));
            k.trim().eq_ignore_ascii_case(key).then(|| v.trim().to_string())
        })
    };
    let production = value("gitflow.branch.master").filter(|v| !v.is_empty())?;
    let development = value("gitflow.branch.develop").filter(|v| !v.is_empty())?;
    let defaults = GitFlowConfig::with_production(&production);
    let prefix = |key: &str, default: String| value(key).filter(|v| !v.is_empty()).unwrap_or(default);
    Some(GitFlowConfig {
        production,
        development,
        feature: prefix("gitflow.prefix.feature", defaults.feature),
        release: prefix("gitflow.prefix.release", defaults.release),
        hotfix: prefix("gitflow.prefix.hotfix", defaults.hotfix),
        support: prefix("gitflow.prefix.support", defaults.support),
        version_tag: value("gitflow.prefix.versiontag").unwrap_or_default(),
    })
}

impl Repo {
    /// Configuration de Git Flow du dépôt, `None` s'il n'est pas initialisé (écrite par DragonTree ou par
    /// git-flow). Une commande de lecture : à lancer depuis une tâche de fond.
    pub fn gitflow_config(&self) -> Result<Option<GitFlowConfig>, GitError> {
        let out = self.read().args(["config", "--local", "--get-regexp", r"^gitflow\."]).allow_codes(&[1]).run()?;
        Ok(parse_config(&out.stdout_str()))
    }

    fn branch_exists(&self, name: &str) -> bool {
        self.ref_exists(&format!("refs/heads/{name}"))
    }

    fn ref_exists(&self, full_name: &str) -> bool {
        self.read()
            .args(["rev-parse", "--verify", "--quiet", full_name])
            .allow_codes(&[1])
            .run()
            .is_ok_and(|o| o.code == 0)
    }

    /// Valide la configuration d'initialisation ; rien n'est écrit si elle est refusée.
    fn check_flow_config(&self, cfg: &GitFlowConfig) -> Result<(), GitError> {
        let refuse = |text: String| Err(GitError::Refused(text));
        if !self.is_valid_branch_name(&cfg.production) {
            return refuse(tr!("Nom invalide pour la branche de production : « {name} »", name = cfg.production));
        }
        if !self.is_valid_branch_name(&cfg.development) {
            return refuse(tr!("Nom invalide pour la branche de développement : « {name} »", name = cfg.development));
        }
        if cfg.production == cfg.development {
            return refuse(tr!("Les branches de production et de développement doivent être différentes").into());
        }
        for prefix in [&cfg.feature, &cfg.release, &cfg.hotfix, &cfg.support] {
            if prefix.is_empty() || !self.is_valid_branch_name(&format!("{prefix}x")) {
                return refuse(tr!("Préfixe de branche invalide : « {prefix} »", prefix = prefix));
            }
        }
        if !cfg.version_tag.is_empty() && !self.is_valid_branch_name(&format!("{}1", cfg.version_tag)) {
            return refuse(tr!("Préfixe de tag invalide : « {prefix} »", prefix = cfg.version_tag));
        }
        if !self.branch_exists(&cfg.production) {
            return refuse(tr!(
                "La branche de production « {name} » n'existe pas : faites d'abord un commit dessus",
                name = cfg.production
            ));
        }
        Ok(())
    }

    /// Initialise (ou reconfigure) Git Flow : écrit les clés `gitflow.*` dans la configuration locale et crée la
    /// branche de développement à partir de celle de production si elle n'existe pas.
    pub fn gitflow_init(&self, cfg: &GitFlowConfig) -> Result<(), GitError> {
        self.check_flow_config(cfg)?;
        if !self.branch_exists(&cfg.development) {
            self.create_branch(&cfg.development, &cfg.production, false)?;
        }
        for (key, value) in [
            ("gitflow.branch.master", &cfg.production),
            ("gitflow.branch.develop", &cfg.development),
            ("gitflow.prefix.feature", &cfg.feature),
            ("gitflow.prefix.release", &cfg.release),
            ("gitflow.prefix.hotfix", &cfg.hotfix),
            ("gitflow.prefix.support", &cfg.support),
            ("gitflow.prefix.versiontag", &cfg.version_tag),
        ] {
            self.set_config_local(key, value)?;
        }
        Ok(())
    }

    /// Démarre une branche de travail : depuis le développement (feature, release) ou la production (hotfix), puis
    /// la sort. Renvoie son nom complet.
    pub fn gitflow_start(&self, cfg: &GitFlowConfig, kind: FlowKind, name: &str) -> Result<String, GitError> {
        let name = name.trim();
        let branch = cfg.branch(kind, name);
        if name.is_empty() || !self.is_valid_branch_name(&branch) {
            return Err(GitError::Refused(tr!("Nom de branche invalide : « {name} »", name = branch)));
        }
        if self.branch_exists(&branch) {
            return Err(GitError::Refused(tr!("La branche « {name} » existe déjà", name = branch)));
        }
        let base = cfg.base(kind);
        if !self.branch_exists(base) {
            return Err(GitError::Refused(tr!("La branche « {name} » n'existe pas", name = base)));
        }
        self.create_branch(&branch, base, true)?;
        Ok(branch)
    }

    /// Les étapes de la fin d'une branche de travail, dans l'ordre.
    fn finish_plan(cfg: &GitFlowConfig, kind: FlowKind, name: &str, branch: &str, delete: bool) -> Vec<FlowStep> {
        let mut plan = match kind {
            FlowKind::Feature => vec![FlowStep::MergeInto(cfg.development.clone())],
            FlowKind::Release | FlowKind::Hotfix => vec![
                FlowStep::MergeInto(cfg.production.clone()),
                FlowStep::Tag(cfg.tag(name)),
                FlowStep::MergeInto(cfg.development.clone()),
            ],
        };
        if delete {
            plan.push(FlowStep::Delete(branch.to_string()));
        }
        plan
    }

    /// Termine la branche de travail `name` : fusions `--no-ff` (feature : dans le développement ; release et
    /// hotfix : dans la production, tag annoté « Version `name` », puis dans le développement) ; la branche n'est
    /// supprimée que si `delete_branch` (toujours avec `-d` : une branche non fusionnée est conservée).
    ///
    /// Tout ce qui peut être vérifié l'est avant la première commande (branche et tag, copie de travail propre,
    /// aucune opération en cours). Une fusion qui s'arrête sur des conflits interrompt la suite sans erreur :
    /// voir [`FinishOutcome::Stopped`]. Les branches tenues par un autre worktree sont refusées d'emblée. Une autre
    /// erreur à la première étape est renvoyée telle quelle (rien n'est fait) ; plus loin, [`FinishOutcome::Failed`]
    /// porte l'erreur et les étapes restantes, le dépôt restant où l'échec l'a laissé.
    pub fn gitflow_finish(
        &self,
        cfg: &GitFlowConfig,
        kind: FlowKind,
        name: &str,
        delete_branch: bool,
    ) -> Result<FinishOutcome, GitError> {
        let branch = cfg.branch(kind, name);
        let refuse = |text: String| Err(GitError::Refused(text));
        if !self.branch_exists(&branch) {
            return refuse(tr!("La branche « {name} » n'existe pas", name = branch));
        }
        for permanent in [&cfg.production, &cfg.development] {
            if !self.branch_exists(permanent) {
                return refuse(tr!("La branche « {name} » n'existe pas", name = permanent));
            }
        }
        if kind != FlowKind::Feature && self.ref_exists(&format!("refs/tags/{}", cfg.tag(name))) {
            return refuse(tr!("Le tag « {name} » existe déjà", name = cfg.tag(name)));
        }
        if self.operation_state() != OpState::None {
            return refuse(tr!("Une opération git est en cours : terminez-la ou abandonnez-la d'abord").into());
        }
        let dirty = self.read().args(["status", "--porcelain", "-uno"]).run()?;
        if !dirty.stdout.is_empty() {
            return refuse(
                tr!("Des modifications suivies ne sont pas commitées : commitez-les ou mettez-les de côté d'abord")
                    .into(),
            );
        }

        let plan = Self::finish_plan(cfg, kind, name, &branch, delete_branch);
        // Une branche sortie dans un autre worktree ferait échouer une étape en cours de route : refus d'emblée.
        for step in &plan {
            if let FlowStep::MergeInto(held) | FlowStep::Delete(held) = step
                && let Some(wt) = self.worktree_holding(held)
            {
                return refuse(tr!(
                    "La branche « {name} » est déjà sortie dans le worktree {path} : changez-y de branche d'abord",
                    name = held,
                    path = wt.path.display()
                ));
            }
        }
        for (i, step) in plan.iter().enumerate() {
            let result = match step {
                FlowStep::MergeInto(target) => {
                    self.checkout(target).and_then(|()| self.merge(&branch, MergeMode::NoFf))
                }
                FlowStep::Tag(tag) => self.create_tag(tag, "HEAD", Some(&format!("Version {name}"))),
                FlowStep::Delete(branch) => self.delete_branch(branch, false),
            };
            let Err(error) = result else { continue };
            // Conflits : git garde l'état de fusion, la suite attend leur résolution.
            if let FlowStep::MergeInto(target) = step
                && self.operation_state() == OpState::Merge
                && self.has_unmerged_paths()
            {
                return Ok(FinishOutcome::Stopped { into: target.clone(), remaining: plan[i + 1..].to_vec() });
            }
            // Une fusion refusée sans conflit (crochet) laisse git en attente de validation : on l'abandonne pour que
            // les étapes restantes (dont cette fusion) restent exactes.
            if self.operation_state() == OpState::Merge {
                let _ = self.abort_op(OpState::Merge);
            }
            // Rien n'est encore fait : l'erreur seule suffit. Sinon la suite dit ce qui reste.
            return if i == 0 {
                Err(error)
            } else {
                Ok(FinishOutcome::Failed { error, remaining: plan[i..].to_vec() })
            };
        }
        Ok(FinishOutcome::Done)
    }

    /// Des fichiers en conflit existent : la fusion en cours attend sa résolution.
    fn has_unmerged_paths(&self) -> bool {
        self.read().args(["diff", "--name-only", "--diff-filter=U"]).run().is_ok_and(|o| !o.stdout.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configuration_needs_both_permanent_branches() {
        assert_eq!(parse_config(""), None);
        assert_eq!(parse_config("gitflow.branch.master main\n"), None);
        assert_eq!(parse_config("gitflow.branch.develop develop\n"), None);
    }

    #[test]
    fn missing_prefixes_take_the_usual_values() {
        let cfg = parse_config("gitflow.branch.master main\ngitflow.branch.develop dev\n").unwrap();
        assert_eq!(cfg, GitFlowConfig { development: "dev".into(), ..GitFlowConfig::with_production("main") });
    }

    #[test]
    fn an_empty_version_tag_line_without_value_is_read_as_empty() {
        let text = "gitflow.branch.master master\ngitflow.branch.develop develop\ngitflow.prefix.versiontag\n";
        assert_eq!(parse_config(text).unwrap().version_tag, "");
    }

    #[test]
    fn branches_are_classified_by_prefix() {
        let cfg = GitFlowConfig::with_production("main");
        assert_eq!(cfg.classify("feature/a/b"), Some((FlowKind::Feature, "a/b".to_string())));
        assert_eq!(cfg.classify("release/1.0"), Some((FlowKind::Release, "1.0".to_string())));
        assert_eq!(cfg.classify("feature/"), None);
        assert_eq!(cfg.classify("main"), None);
        assert_eq!(cfg.classify("support/1.x"), None, "les branches de support ne se terminent pas");
    }
}
