//! Outils externes de diff et de merge (Réglages › Outils externes) : préréglages détectés aux emplacements usuels,
//! commande personnalisée, et lancement de l'outil de diff.
//!
//! Le choix est « Configuration de git » (`git difftool` / `git mergetool` tels que configurés), un préréglage ou une
//! commande personnalisée. Préréglages et commande personnalisée sont des gabarits (`dragontree_git::ToolCommand`,
//! syntaxe documentée dans le moteur) : l'outil de diff est lancé directement, programme et arguments séparés, jamais
//! par `cmd.exe` ; l'outil de merge passe par `git mergetool --tool=… -c mergetool.….cmd=…` (voir
//! `Repo::run_mergetool_with`). Pour comparer deux versions, DragonTree les extrait dans des fichiers temporaires.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use dragontree_git::{GitError, Repo, ToolCommand, ToolFiles};
use dragontree_i18n::tr;
use serde::{Deserialize, Serialize};

use crate::editors::{EditorKind, SearchDirs, find_program};

/// Outil prédéfini.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolPreset {
    #[serde(rename = "vscode")]
    VsCode,
    #[serde(rename = "winmerge")]
    WinMerge,
    #[serde(rename = "beyondcompare")]
    BeyondCompare,
    #[serde(rename = "kdiff3")]
    KDiff3,
    #[serde(rename = "p4merge")]
    P4Merge,
}

impl ToolPreset {
    pub const ALL: [ToolPreset; 5] =
        [ToolPreset::VsCode, ToolPreset::WinMerge, ToolPreset::BeyondCompare, ToolPreset::KDiff3, ToolPreset::P4Merge];

    /// Identifiant stable ; nomme aussi l'outil pour git (`dragontree-<id>`).
    pub fn id(self) -> &'static str {
        match self {
            ToolPreset::VsCode => "vscode",
            ToolPreset::WinMerge => "winmerge",
            ToolPreset::BeyondCompare => "beyondcompare",
            ToolPreset::KDiff3 => "kdiff3",
            ToolPreset::P4Merge => "p4merge",
        }
    }

    /// Nom du produit (non traduit).
    pub fn label(self) -> &'static str {
        match self {
            ToolPreset::VsCode => "VS Code",
            ToolPreset::WinMerge => "WinMerge",
            ToolPreset::BeyondCompare => "Beyond Compare",
            ToolPreset::KDiff3 => "KDiff3",
            ToolPreset::P4Merge => "P4Merge",
        }
    }

    /// Arguments pour comparer `$LOCAL` (à gauche) et `$REMOTE` (à droite).
    fn diff_args(self) -> &'static str {
        match self {
            ToolPreset::VsCode => "--wait --diff $LOCAL $REMOTE",
            ToolPreset::WinMerge => "-e -u -dl Local -dr Remote $LOCAL $REMOTE",
            ToolPreset::BeyondCompare | ToolPreset::KDiff3 | ToolPreset::P4Merge => "$LOCAL $REMOTE",
        }
    }

    /// Arguments pour fusionner : `$LOCAL` (notre version), `$REMOTE` (la leur), `$BASE` (l'ancêtre commun),
    /// résultat dans `$MERGED`.
    fn merge_args(self) -> &'static str {
        match self {
            // VS Code : version entrante, version courante, base, résultat.
            ToolPreset::VsCode => "--wait --merge $REMOTE $LOCAL $BASE $MERGED",
            ToolPreset::WinMerge => "-u -e -dl Local -dr Remote $LOCAL $REMOTE $MERGED",
            ToolPreset::BeyondCompare => "$LOCAL $REMOTE $BASE -mergeoutput=$MERGED",
            ToolPreset::KDiff3 => "--auto -o $MERGED $BASE $LOCAL $REMOTE",
            ToolPreset::P4Merge => "$BASE $REMOTE $LOCAL $MERGED",
        }
    }

    /// Programme installé, cherché dans le PATH puis aux emplacements usuels.
    fn find(self, dirs: &SearchDirs) -> Option<PathBuf> {
        match self {
            ToolPreset::VsCode => find_program(EditorKind::VsCode, dirs),
            ToolPreset::WinMerge => dirs
                .in_path("WinMergeU.exe")
                .or_else(|| dirs.in_program_files("WinMerge/WinMergeU.exe"))
                .or_else(|| dirs.in_local_app_data("Programs/WinMerge/WinMergeU.exe")),
            ToolPreset::BeyondCompare => dirs.in_path("BComp.exe").or_else(|| {
                ["5", "4", "3"]
                    .iter()
                    .find_map(|v| dirs.in_program_files(&format!("Beyond Compare {v}/BComp.exe")))
                    .or_else(|| dirs.in_local_app_data("Programs/Beyond Compare 5/BComp.exe"))
            }),
            ToolPreset::KDiff3 => dirs.in_path("kdiff3.exe").or_else(|| dirs.in_program_files("KDiff3/kdiff3.exe")),
            ToolPreset::P4Merge => {
                dirs.in_path("p4merge.exe").or_else(|| dirs.in_program_files("Perforce/p4merge.exe"))
            }
        }
    }
}

/// Outil choisi dans les réglages pour le diff ou pour le merge.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    /// `git difftool` / `git mergetool` tels que configurés dans git.
    #[default]
    Git,
    Preset(ToolPreset),
    /// La commande saisie dans les réglages.
    Custom,
}

/// Préréglage trouvé sur la machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedTool {
    pub preset: ToolPreset,
    pub program: PathBuf,
    /// VS Code : le script sh `code` voisin de `code.cmd`. Lancé par le `sh` de git, il n'passe pas par cmd.exe : le
    /// merge s'en sert, ce qui lève le refus des noms de fichiers à blancs ou à `&`.
    pub script: Option<PathBuf>,
}

/// Préréglages installés dans `dirs` (lit le disque : hors du thread de l'interface).
pub fn detect(dirs: &SearchDirs) -> Vec<DetectedTool> {
    ToolPreset::ALL
        .into_iter()
        .filter_map(|p| {
            p.find(dirs).map(|program| {
                let sibling = program.with_file_name("code");
                let script = (p == ToolPreset::VsCode && sibling.is_file()).then_some(sibling);
                DetectedTool { preset: p, program, script }
            })
        })
        .collect()
}

pub fn detect_system() -> Vec<DetectedTool> {
    detect(&SearchDirs::system())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Diff,
    Merge,
}

/// Outil prêt à lancer : le gabarit analysé et le nom que git lui donne (`--tool=`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTool {
    pub name: String,
    pub label: String,
    pub command: ToolCommand,
    /// Le code de sortie de l'outil dit si le merge a réussi (`mergetool.<nom>.trustExitCode`). Seulement pour KDiff3 :
    /// git tient les codes de WinMerge, P4Merge et Beyond Compare pour peu fiables, et un outil fermé sans enregistrer
    /// ferait indexer un fichier qui garde ses marqueurs de conflit. Une commande personnalisée le demande dans les
    /// réglages.
    pub trust_exit_code: bool,
}

/// Pourquoi un outil ne peut pas être lancé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    NotDetected(ToolPreset),
    /// Commande personnalisée vide ou illisible.
    Invalid(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::NotDetected(preset) => {
                write!(f, "{}", tr!("{tool} est introuvable (Réglages › Outils externes).", tool = preset.label()))
            }
            ToolError::Invalid(why) => write!(f, "{why}"),
        }
    }
}

/// [`resolve_with_trust`] sans confiance dans le code de sortie d'une commande personnalisée (le diff l'ignore).
pub fn resolve(
    kind: ToolKind,
    choice: &ToolChoice,
    custom: &str,
    detected: &[DetectedTool],
) -> Result<Option<ResolvedTool>, ToolError> {
    resolve_with_trust(kind, choice, custom, false, detected)
}

/// L'outil à lancer pour ce choix : `None` pour « Configuration de git » (git s'en charge). Ne touche pas au disque :
/// `detected` vient de la détection faite au démarrage. `trust_custom` : le code de sortie d'une commande
/// personnalisée dit si le merge a réussi.
pub fn resolve_with_trust(
    kind: ToolKind,
    choice: &ToolChoice,
    custom: &str,
    trust_custom: bool,
    detected: &[DetectedTool],
) -> Result<Option<ResolvedTool>, ToolError> {
    let parse = |template: &str| ToolCommand::parse(template).map_err(|e| ToolError::Invalid(e.to_string()));
    Ok(match choice {
        ToolChoice::Git => None,
        ToolChoice::Custom => Some(ResolvedTool {
            name: "dragontree-custom".into(),
            label: tr!("commande personnalisée").into(),
            command: parse(custom)?,
            trust_exit_code: trust_custom,
        }),
        ToolChoice::Preset(preset) => {
            let found = detected.iter().find(|d| d.preset == *preset).ok_or(ToolError::NotDetected(*preset))?;
            // Merge avec VS Code : le script sh voisin plutôt que code.cmd (git l'exécute avec son sh, sans cmd.exe).
            let chosen = match (kind, &found.script) {
                (ToolKind::Merge, Some(script)) => script,
                _ => &found.program,
            };
            let program = chosen.to_string_lossy();
            if program.contains('"') {
                return Err(ToolError::Invalid(tr!("Chemin d'outil invalide : {path}", path = program)));
            }
            let args = if kind == ToolKind::Diff { preset.diff_args() } else { preset.merge_args() };
            Some(ResolvedTool {
                name: format!("dragontree-{}", preset.id()),
                label: preset.label().into(),
                command: parse(&format!("\"{program}\" {args}"))?,
                trust_exit_code: *preset == ToolPreset::KDiff3,
            })
        }
    })
}

/// Chemin du programme d'une commande : tel quel s'il a un dossier ; sinon cherché dans `path_var`. Un nom avec
/// extension est cherché tel quel ; sans extension, avec celles que Windows exécute, dans l'ordre de PATHEXT
/// (`.com`, `.exe`, `.bat`, `.cmd`) : `code` trouve `code.cmd`, jamais le script sh `code` qui est à côté et que
/// Windows ne sait pas lancer.
pub fn resolve_program(program: &OsStr, path_var: Option<&OsStr>) -> PathBuf {
    let given = Path::new(program);
    if given.components().count() > 1 {
        return given.to_path_buf();
    }
    let Some(path_var) = path_var else { return given.to_path_buf() };
    let extensions: &[&str] = if given.extension().is_some() { &[""] } else { &[".com", ".exe", ".bat", ".cmd"] };
    for dir in std::env::split_paths(path_var) {
        for extension in extensions {
            let mut name = OsString::from(program);
            name.push(extension);
            let candidate = dir.join(name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    given.to_path_buf()
}

/// Commande qui compare `local` et `remote` : programme et arguments séparés, sans interpréteur.
pub fn diff_command(tool: &ResolvedTool, local: &Path, remote: &Path, path_var: Option<&OsStr>) -> Command {
    let files = ToolFiles { local: local.to_path_buf(), remote: remote.to_path_buf(), ..ToolFiles::default() };
    let mut argv = tool.command.argv(&files).into_iter();
    let program = argv.next().unwrap_or_default();
    let mut cmd = Command::new(resolve_program(&program, path_var));
    cmd.args(argv);
    cmd
}

/// Ce que contient un côté de la comparaison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Aucun contenu (fichier ajouté ou supprimé de ce côté).
    Empty,
    /// Version à une révision (`rev` vide : l'index).
    Blob { rev: String, path: String },
    /// Le fichier de la copie de travail, passé tel quel à l'outil (il peut y enregistrer).
    WorkFile(String),
}

/// Fichier à comparer : dans la vue Changements ou dans les détails d'un commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalDiff {
    /// `old_path` : le nom d'origine d'un renommage indexé (la version de HEAD s'y lit).
    Working {
        path: String,
        staged: bool,
        old_path: Option<String>,
    },
    Commit {
        id: String,
        has_parent: bool,
        path: String,
        old_path: Option<String>,
    },
}

impl ExternalDiff {
    pub fn path(&self) -> &str {
        match self {
            ExternalDiff::Working { path, .. } | ExternalDiff::Commit { path, .. } => path,
        }
    }

    /// Les deux côtés, ancien à gauche (`$LOCAL`) et récent à droite (`$REMOTE`), avec le nom de leur dossier
    /// temporaire (qui dit à l'utilisateur de quelle version il s'agit).
    pub fn sides(&self) -> ((String, Source), (String, Source)) {
        match self {
            ExternalDiff::Working { path, staged: false, .. } => (
                ("index".into(), Source::Blob { rev: String::new(), path: path.clone() }),
                ("copie-de-travail".into(), Source::WorkFile(path.clone())),
            ),
            ExternalDiff::Working { path, staged: true, old_path } => (
                (
                    "HEAD".into(),
                    Source::Blob { rev: "HEAD".into(), path: old_path.clone().unwrap_or_else(|| path.clone()) },
                ),
                ("index".into(), Source::Blob { rev: String::new(), path: path.clone() }),
            ),
            ExternalDiff::Commit { id, has_parent, path, old_path } => {
                let short = &id[..id.len().min(7)];
                let left = if *has_parent {
                    Source::Blob { rev: format!("{id}^"), path: old_path.clone().unwrap_or_else(|| path.clone()) }
                } else {
                    Source::Empty
                };
                (
                    (format!("{short}-parent"), left),
                    (short.to_string(), Source::Blob { rev: id.clone(), path: path.clone() }),
                )
            }
        }
    }
}

/// Nom de fichier sans rien que `cmd.exe` ou un outil interpréterait : lettres, chiffres, `.`, `-`, `_`, le reste
/// devient `_`. Les noms des fichiers temporaires viennent du dépôt : ils sont assainis.
pub fn sanitize_file_name(name: &str) -> String {
    let clean: String = name.chars().map(|c| if c.is_alphanumeric() || "._-".contains(c) { c } else { '_' }).collect();
    let clean = clean.trim_start_matches('.').to_string();
    if clean.is_empty() { "fichier".to_string() } else { clean }
}

/// Dossier racine des comparaisons temporaires.
pub fn temp_root() -> PathBuf {
    std::env::temp_dir().join("dragontree-diff")
}

/// Supprime les comparaisons de `root` plus vieilles que `max_age` (un outil qui rend la main tout de suite laisse
/// ses fichiers : ils sont retirés à la comparaison suivante).
pub fn purge_old(root: &Path, max_age: Duration) {
    let now = SystemTime::now();
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|modified| now.duration_since(modified).is_ok_and(|age| age >= max_age));
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Une comparaison en cours de préparation : un dossier propre à elle sous `root`.
fn new_session(root: &Path) -> std::io::Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = root.join(format!("{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Écrit un côté dans `<session>/<label>/<nom>` et renvoie son chemin ; la copie de travail est passée telle quelle.
fn materialize(
    repo: &Repo,
    session: &Path,
    label: &str,
    file_name: &str,
    source: &Source,
) -> Result<PathBuf, GitError> {
    if let Source::WorkFile(path) = source {
        let full = repo.workdir.join(path);
        if full.is_file() {
            return Ok(full);
        }
    }
    let bytes = match source {
        // Fichier absent de ce côté (ajouté, supprimé, non suivi) : côté vide.
        Source::Empty | Source::WorkFile(_) => Vec::new(),
        // Contenu filtré comme dans la copie de travail (fins de ligne, LFS) ; absent à cette révision : côté vide.
        // Toute autre erreur de lecture est signalée, jamais un côté vide qui ferait croire à un fichier supprimé.
        Source::Blob { rev, path } => repo.file_version(rev, path)?.unwrap_or_default(),
    };
    let dir = session.join(sanitize_file_name(label));
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(sanitize_file_name(file_name));
    std::fs::write(&file, bytes)?;
    Ok(file)
}

/// Temps au-delà duquel un outil qui s'est arrêté est supposé avoir attendu l'utilisateur : ses fichiers temporaires
/// sont alors supprimés tout de suite (sinon ils restent pour `purge_old`).
const BLOCKING_TOOL: Duration = Duration::from_secs(3);

/// Compare le fichier avec l'outil : extrait les deux versions sous `root`, lance l'outil et attend sa fin (à
/// appeler en tâche de fond). Le code de sortie de l'outil est ignoré : beaucoup en renvoient un quand les fichiers
/// diffèrent.
pub fn run_external_diff(
    repo: &Repo,
    tool: &ResolvedTool,
    diff: &ExternalDiff,
    root: &Path,
    path_var: Option<&OsStr>,
) -> Result<(), GitError> {
    purge_old(root, Duration::from_secs(24 * 3600));
    let session = new_session(root)?;
    let file_name = Path::new(diff.path()).file_name().map_or_else(|| "fichier".into(), |n| n.to_string_lossy());
    let ((left_label, left), (right_label, right)) = diff.sides();
    let local = materialize(repo, &session, &left_label, &file_name, &left)?;
    let remote = materialize(repo, &session, &right_label, &file_name, &right)?;
    let mut cmd = diff_command(tool, &local, &remote, path_var);
    cmd.current_dir(&repo.workdir);
    let started = Instant::now();
    let mut child = crate::util::detached(&mut cmd)
        .spawn()
        .map_err(|e| GitError::Spawn(tr!("{tool} n'a pas pu être lancé : {error}", tool = &tool.label, error = e)))?;
    let _ = child.wait();
    if started.elapsed() >= BLOCKING_TOOL {
        let _ = std::fs::remove_dir_all(&session);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    fn detected(preset: ToolPreset) -> Vec<DetectedTool> {
        vec![DetectedTool {
            preset,
            program: PathBuf::from(format!(r"C:\Outils sûrs\{}\prog.exe", preset.id())),
            script: None,
        }]
    }

    fn argv(tool: &ResolvedTool, files: &ToolFiles) -> Vec<String> {
        tool.command.argv(files).into_iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    fn files() -> ToolFiles {
        ToolFiles { local: "L".into(), remote: "R".into(), base: "B".into(), merged: "M".into() }
    }

    #[test]
    fn each_preset_builds_its_diff_and_merge_command() {
        let expected: [(ToolPreset, &[&str], &[&str]); 5] = [
            (ToolPreset::VsCode, &["--wait", "--diff", "L", "R"], &["--wait", "--merge", "R", "L", "B", "M"]),
            (
                ToolPreset::WinMerge,
                &["-e", "-u", "-dl", "Local", "-dr", "Remote", "L", "R"],
                &["-u", "-e", "-dl", "Local", "-dr", "Remote", "L", "R", "M"],
            ),
            (ToolPreset::BeyondCompare, &["L", "R"], &["L", "R", "B", "-mergeoutput=M"]),
            (ToolPreset::KDiff3, &["L", "R"], &["--auto", "-o", "M", "B", "L", "R"]),
            (ToolPreset::P4Merge, &["L", "R"], &["B", "R", "L", "M"]),
        ];
        for (preset, diff, merge) in expected {
            let found = detected(preset);
            let program = found[0].program.to_string_lossy().into_owned();
            let with_program = |args: &[&str]| -> Vec<String> {
                std::iter::once(program.as_str()).chain(args.iter().copied()).map(str::to_string).collect()
            };
            let tool = resolve(ToolKind::Diff, &ToolChoice::Preset(preset), "", &found).unwrap().unwrap();
            assert_eq!(argv(&tool, &files()), with_program(diff), "diff {preset:?}");
            assert_eq!(tool.name, format!("dragontree-{}", preset.id()));
            let tool = resolve(ToolKind::Merge, &ToolChoice::Preset(preset), "", &found).unwrap().unwrap();
            assert_eq!(argv(&tool, &files()), with_program(merge), "merge {preset:?}");
        }
    }

    #[test]
    fn only_kdiff3_trusts_its_exit_code_and_a_custom_command_must_opt_in() {
        for preset in ToolPreset::ALL {
            let found = detected(preset);
            let tool = resolve(ToolKind::Merge, &ToolChoice::Preset(preset), "", &found).unwrap().unwrap();
            assert_eq!(tool.trust_exit_code, preset == ToolPreset::KDiff3, "{preset:?}");
        }
        let custom = |trust| resolve_with_trust(ToolKind::Merge, &ToolChoice::Custom, "meld $MERGED", trust, &[]);
        assert!(!custom(false).unwrap().unwrap().trust_exit_code);
        assert!(custom(true).unwrap().unwrap().trust_exit_code);
        // Sans autre précision (diff, ou valeur par défaut des réglages) : pas de confiance.
        let plain = resolve(ToolKind::Merge, &ToolChoice::Custom, "meld $MERGED", &[]).unwrap().unwrap();
        assert!(!plain.trust_exit_code);
    }

    #[test]
    fn vs_code_merges_with_its_sh_script_and_diffs_with_code_cmd() {
        let found = vec![DetectedTool {
            preset: ToolPreset::VsCode,
            program: PathBuf::from(r"C:\VS Code\bin\code.cmd"),
            script: Some(PathBuf::from(r"C:\VS Code\bin\code")),
        }];
        let choice = ToolChoice::Preset(ToolPreset::VsCode);
        let merge = resolve(ToolKind::Merge, &choice, "", &found).unwrap().unwrap();
        assert_eq!(merge.command.program(), r"C:\VS Code\bin\code");
        let diff = resolve(ToolKind::Diff, &choice, "", &found).unwrap().unwrap();
        assert_eq!(diff.command.program(), r"C:\VS Code\bin\code.cmd");
        // Sans script voisin, le merge garde code.cmd.
        let found = vec![DetectedTool { script: None, ..found[0].clone() }];
        let merge = resolve(ToolKind::Merge, &choice, "", &found).unwrap().unwrap();
        assert_eq!(merge.command.program(), r"C:\VS Code\bin\code.cmd");
    }

    #[test]
    fn a_staged_rename_reads_the_head_side_at_its_origin_path() {
        let renamed =
            ExternalDiff::Working { path: "neuf.rs".into(), staged: true, old_path: Some("ancien.rs".into()) };
        let ((_, left), (_, right)) = renamed.sides();
        assert_eq!(left, Source::Blob { rev: "HEAD".into(), path: "ancien.rs".into() });
        assert_eq!(right, Source::Blob { rev: String::new(), path: "neuf.rs".into() });
    }

    #[test]
    fn the_sh_line_of_a_preset_merge_quotes_the_program_and_the_variables() {
        let found = detected(ToolPreset::KDiff3);
        let tool = resolve(ToolKind::Merge, &ToolChoice::Preset(ToolPreset::KDiff3), "", &found).unwrap().unwrap();
        assert_eq!(
            tool.command.sh_command(),
            r#"'C:/Outils sûrs/kdiff3/prog.exe' '--auto' '-o' "$MERGED" "$BASE" "$LOCAL" "$REMOTE""#
        );
    }

    #[test]
    fn git_means_no_tool_and_a_missing_preset_is_reported() {
        assert_eq!(resolve(ToolKind::Diff, &ToolChoice::Git, "ignoré", &[]), Ok(None));
        let missing =
            resolve(ToolKind::Merge, &ToolChoice::Preset(ToolPreset::P4Merge), "", &detected(ToolPreset::KDiff3));
        assert_eq!(missing, Err(ToolError::NotDetected(ToolPreset::P4Merge)));
    }

    #[test]
    fn a_custom_command_is_split_into_arguments_with_quotes_and_variables() {
        let custom = r#""C:\Program Files\Meld\meld.exe" --label "Avant" $LOCAL $REMOTE"#;
        let tool = resolve(ToolKind::Diff, &ToolChoice::Custom, custom, &[]).unwrap().unwrap();
        assert_eq!(tool.name, "dragontree-custom");
        let files = ToolFiles { local: r"C:\Temp\a b\x&y.txt".into(), remote: "R".into(), ..ToolFiles::default() };
        assert_eq!(
            argv(&tool, &files),
            [r"C:\Program Files\Meld\meld.exe", "--label", "Avant", r"C:\Temp\a b\x&y.txt", "R"]
        );
        assert!(matches!(resolve(ToolKind::Diff, &ToolChoice::Custom, "  ", &[]), Err(ToolError::Invalid(_))));
        assert!(matches!(resolve(ToolKind::Merge, &ToolChoice::Custom, "prog \"x", &[]), Err(ToolError::Invalid(_))));
    }

    #[test]
    fn the_diff_command_passes_paths_as_separate_arguments_without_a_shell() {
        let tool = resolve(ToolKind::Diff, &ToolChoice::Custom, "meld $LOCAL $REMOTE", &[]).unwrap().unwrap();
        let (local, remote) = (Path::new(r"C:\a b\x&calc %PATH% ^y.txt"), Path::new(r"C:\c\d.txt"));
        let cmd = diff_command(&tool, local, remote, None);
        assert_eq!(cmd.get_program(), "meld", "sans PATH, le nom est gardé tel quel");
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), [local.as_os_str(), remote.as_os_str()]);
    }

    #[test]
    fn a_bare_program_name_is_searched_in_the_path_with_windows_extensions() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        touch(&bin.join("outil.cmd"));
        touch(&bin.join("autre.exe"));
        let path_var = std::env::join_paths([dir.path().join("vide"), bin.clone()]).unwrap();
        assert_eq!(resolve_program(OsStr::new("outil"), Some(&path_var)), bin.join("outil.cmd"));
        // Comme le dossier bin de VS Code : un script sh `code` sans extension à côté de `code.cmd`. Jamais lui.
        touch(&bin.join("code"));
        touch(&bin.join("code.cmd"));
        assert_eq!(resolve_program(OsStr::new("code"), Some(&path_var)), bin.join("code.cmd"));
        // Ordre de PATHEXT : .com, .exe, .bat, .cmd.
        touch(&bin.join("multi.cmd"));
        touch(&bin.join("multi.bat"));
        assert_eq!(resolve_program(OsStr::new("multi"), Some(&path_var)), bin.join("multi.bat"));
        // Un nom avec extension est pris tel quel, même s'il existe aussi sans.
        assert_eq!(resolve_program(OsStr::new("code.cmd"), Some(&path_var)), bin.join("code.cmd"));
        // Seul le fichier sans extension existe : il n'est pas un exécutable Windows, le nom est gardé tel quel.
        touch(&bin.join("script"));
        assert_eq!(resolve_program(OsStr::new("script"), Some(&path_var)), PathBuf::from("script"));
        assert_eq!(resolve_program(OsStr::new("autre"), Some(&path_var)), bin.join("autre.exe"));
        assert_eq!(resolve_program(OsStr::new("absent"), Some(&path_var)), PathBuf::from("absent"));
        // Un chemin avec dossier n'est pas cherché.
        assert_eq!(resolve_program(OsStr::new(r"C:\x\y.exe"), Some(&path_var)), PathBuf::from(r"C:\x\y.exe"));
    }

    #[test]
    fn the_sides_of_each_comparison_name_the_version_shown() {
        let work = ExternalDiff::Working { path: "src/a.rs".into(), staged: false, old_path: None };
        let ((l, left), (r, right)) = work.sides();
        assert_eq!((l.as_str(), r.as_str()), ("index", "copie-de-travail"));
        assert_eq!(left, Source::Blob { rev: String::new(), path: "src/a.rs".into() });
        assert_eq!(right, Source::WorkFile("src/a.rs".into()));

        let staged = ExternalDiff::Working { path: "a.rs".into(), staged: true, old_path: None };
        let ((_, left), (_, right)) = staged.sides();
        assert_eq!(left, Source::Blob { rev: "HEAD".into(), path: "a.rs".into() });
        assert_eq!(right, Source::Blob { rev: String::new(), path: "a.rs".into() });

        let id = "0123456789abcdef0123456789abcdef01234567".to_string();
        let renamed = ExternalDiff::Commit {
            id: id.clone(),
            has_parent: true,
            path: "neuf.rs".into(),
            old_path: Some("ancien.rs".into()),
        };
        let ((l, left), (r, right)) = renamed.sides();
        assert_eq!((l.as_str(), r.as_str()), ("0123456-parent", "0123456"));
        assert_eq!(left, Source::Blob { rev: format!("{id}^"), path: "ancien.rs".into() });
        assert_eq!(right, Source::Blob { rev: id.clone(), path: "neuf.rs".into() });

        let root = ExternalDiff::Commit { id, has_parent: false, path: "a.rs".into(), old_path: None };
        assert_eq!(root.sides().0.1, Source::Empty, "commit racine : rien à gauche");
    }

    #[test]
    fn temporary_file_names_are_sanitized() {
        assert_eq!(sanitize_file_name("Rapport final (v2).docx"), "Rapport_final__v2_.docx");
        assert_eq!(sanitize_file_name("a&b|c<d>e^f%g!h\"i.txt"), "a_b_c_d_e_f_g_h_i.txt");
        // Pas de remontée de dossier, jamais de nom vide.
        assert_eq!(sanitize_file_name("../x"), "_x");
        assert_eq!(sanitize_file_name("..."), "fichier");
        assert_eq!(sanitize_file_name("é.rs"), "é.rs");
    }

    #[test]
    fn old_comparisons_are_purged_and_recent_ones_kept() {
        let root = tempfile::tempdir().unwrap();
        touch(&root.path().join("ancienne/a/f.txt"));
        purge_old(root.path(), Duration::from_secs(3600));
        assert!(root.path().join("ancienne").exists(), "récente : gardée");
        purge_old(root.path(), Duration::ZERO);
        assert!(!root.path().join("ancienne").exists());
        purge_old(&root.path().join("absent"), Duration::ZERO);
    }

    #[test]
    fn an_external_diff_extracts_both_versions_and_runs_the_fake_tool_on_them() {
        use dragontree_git::Git;
        let dir = tempfile::tempdir().unwrap();
        let git = Git::discover().unwrap();
        let repo_dir = dir.path().join("depot");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let run = |args: &[&str]| git.cmd(&repo_dir).args(args).run().unwrap().stdout_str();
        run(&["init", "-q", "-b", "main"]);
        for (k, v) in [("user.name", "Testeur"), ("user.email", "t@example.com"), ("core.autocrlf", "false")] {
            run(&["config", k, v]);
        }
        let file = "Rapport final (v2).txt";
        let commit = |content: &str| {
            std::fs::write(repo_dir.join(file), content).unwrap();
            run(&["add", "-A"]);
            run(&["commit", "-q", "-m", "c"]);
            run(&["rev-parse", "HEAD"]).trim().to_string()
        };
        commit("avant\n");
        let id = commit("après\n");
        let repo = Repo::open(&git, &repo_dir).unwrap();

        // Faux outil : écrit le contenu de $LOCAL puis de $REMOTE dans un fichier.
        let out = dir.path().join("vu.txt");
        let script = dir.path().join("faux outil.cmd");
        let body =
            format!("@echo off\r\ntype \"%~1\" > \"{out}\"\r\ntype \"%~2\" >> \"{out}\"\r\n", out = out.display());
        std::fs::write(&script, body).unwrap();
        let template = format!("\"{}\" $LOCAL $REMOTE", script.display());
        let tool = resolve(ToolKind::Diff, &ToolChoice::Custom, &template, &[]).unwrap().unwrap();
        let seen = || std::fs::read_to_string(&out).unwrap().replace("\r\n", "\n");

        let diff = ExternalDiff::Commit { id: id.clone(), has_parent: true, path: file.into(), old_path: None };
        let root = dir.path().join("temp");
        run_external_diff(&repo, &tool, &diff, &root, None).unwrap();
        assert_eq!(seen(), "avant\naprès\n");
        // Les fichiers temporaires portent un nom assaini, dans un dossier par version.
        let short = &id[..7];
        let session = std::fs::read_dir(&root).unwrap().next().unwrap().unwrap().path();
        assert!(session.join(format!("{short}-parent")).join("Rapport_final__v2_.txt").is_file());
        assert!(session.join(short).join("Rapport_final__v2_.txt").is_file());

        // Copie de travail : l'outil reçoit le vrai fichier, modifiable.
        std::fs::write(repo_dir.join(file), "local\n").unwrap();
        let work = ExternalDiff::Working { path: file.into(), staged: false, old_path: None };
        run_external_diff(&repo, &tool, &work, &root, None).unwrap();
        assert_eq!(seen(), "après\nlocal\n");

        // Un outil introuvable est signalé, sans panique.
        let missing = "C:/introuvable/outil.exe $LOCAL $REMOTE";
        let missing = resolve(ToolKind::Diff, &ToolChoice::Custom, missing, &[]).unwrap().unwrap();
        assert!(matches!(run_external_diff(&repo, &missing, &work, &root, None), Err(GitError::Spawn(_))));
    }
}
