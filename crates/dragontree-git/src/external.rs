//! Outils externes de diff et de merge : commande d'outil décrite par un gabarit (`"programme" $LOCAL $REMOTE`), lancée
//! par DragonTree (diff) ou par `git mergetool` (merge).
//!
//! # Syntaxe du gabarit
//!
//! Un gabarit est une suite d'arguments séparés par des blancs ; le premier est le programme. Des guillemets doubles
//! (`"C:\Program Files\x.exe"`) regroupent un argument qui contient des blancs. Il n'y a **aucun autre échappement** :
//! pas de `\`, pas de `^`, pas de `%` spécial, et un argument ne peut pas contenir de guillemet. Les variables
//! `$LOCAL`, `$REMOTE`, `$BASE` et `$MERGED` (en majuscules) sont remplacées par un chemin, n'importe où dans un
//! argument (`-mergeoutput=$MERGED`) ; elles ne sont pas permises dans le nom du programme. Une valeur reste toujours
//! **un seul argument** : un chemin avec des blancs, `&`, `%` ou `^` n'est jamais redécoupé ni interprété.
//!
//! Aucun interpréteur (`cmd.exe`) n'est utilisé pour lancer un outil de diff : le programme et ses arguments sont
//! passés séparément au système. Pour `git mergetool`, git exécute la commande avec son `sh` : [`ToolCommand::sh_command`]
//! cite chaque morceau de texte entre apostrophes et chaque variable entre guillemets (`"$MERGED"`), si bien que les
//! chemins que git donne à l'outil (venus du dépôt) ne sont jamais interprétés non plus.

use std::ffi::OsString;
use std::path::PathBuf;

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::repo::Repo;

/// Variable d'un gabarit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolVar {
    Local,
    Remote,
    Base,
    Merged,
}

impl ToolVar {
    const ALL: [ToolVar; 4] = [ToolVar::Local, ToolVar::Remote, ToolVar::Base, ToolVar::Merged];

    /// Nom dans un gabarit, sans le `$` (aussi le nom de la variable de `git mergetool`).
    pub fn name(self) -> &'static str {
        match self {
            ToolVar::Local => "LOCAL",
            ToolVar::Remote => "REMOTE",
            ToolVar::Base => "BASE",
            ToolVar::Merged => "MERGED",
        }
    }
}

/// Valeurs des variables d'un gabarit ; une variable sans valeur est remplacée par un argument vide.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolFiles {
    pub local: PathBuf,
    pub remote: PathBuf,
    pub base: PathBuf,
    pub merged: PathBuf,
}

impl ToolFiles {
    fn get(&self, var: ToolVar) -> &PathBuf {
        match var {
            ToolVar::Local => &self.local,
            ToolVar::Remote => &self.remote,
            ToolVar::Base => &self.base,
            ToolVar::Merged => &self.merged,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    Var(ToolVar),
}

/// Gabarit analysé : programme et arguments, variables repérées.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCommand {
    /// Le premier argument est le programme (texte seul).
    args: Vec<Vec<Piece>>,
}

/// Découpe un gabarit en arguments : blancs séparateurs, guillemets doubles qui regroupent, rien d'autre.
fn split_arguments(template: &str) -> Result<Vec<String>, GitError> {
    let (mut args, mut current, mut started, mut quoted) = (Vec::new(), String::new(), false, false);
    for c in template.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err(GitError::Refused(tr!("Guillemet non fermé dans la commande.").into()));
    }
    if started {
        args.push(current);
    }
    Ok(args)
}

/// Texte d'un argument avec ses variables (`$LOCAL` mais pas `$LOCALE` ni `$local`).
fn split_variables(arg: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut rest = arg;
    while let Some(pos) = rest.find('$') {
        text.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        let found = ToolVar::ALL.into_iter().find(|v| {
            after
                .strip_prefix(v.name())
                .is_some_and(|tail| !tail.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
        });
        match found {
            Some(var) => {
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(Piece::Var(var));
                rest = &after[var.name().len()..];
            }
            None => {
                text.push('$');
                rest = after;
            }
        }
    }
    text.push_str(rest);
    if !text.is_empty() || pieces.is_empty() {
        pieces.push(Piece::Text(text));
    }
    pieces
}

/// `texte` entre apostrophes pour `sh` (une apostrophe devient `'\''`).
fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

impl ToolCommand {
    /// Analyse un gabarit (syntaxe en tête du module).
    pub fn parse(template: &str) -> Result<ToolCommand, GitError> {
        let args = split_arguments(template)?;
        let Some(program) = args.first() else {
            return Err(GitError::Refused(tr!("La commande est vide.").into()));
        };
        if program.is_empty() {
            return Err(GitError::Refused(tr!("Le nom du programme est vide.").into()));
        }
        let pieces: Vec<Vec<Piece>> = args.iter().map(|a| split_variables(a)).collect();
        if pieces[0].iter().any(|p| matches!(p, Piece::Var(_))) {
            return Err(GitError::Refused(tr!("Le nom du programme ne peut pas contenir de variable.").into()));
        }
        Ok(ToolCommand { args: pieces })
    }

    /// Le programme (premier argument, tel qu'écrit).
    pub fn program(&self) -> String {
        self.args[0].iter().map(|p| if let Piece::Text(t) = p { t.as_str() } else { "" }).collect()
    }

    /// Programme et arguments, variables remplacées : à passer tels quels à `Command` (jamais à un interpréteur).
    pub fn argv(&self, files: &ToolFiles) -> Vec<OsString> {
        self.args
            .iter()
            .map(|arg| {
                let mut out = OsString::new();
                for piece in arg {
                    match piece {
                        Piece::Text(t) => out.push(t),
                        Piece::Var(v) => out.push(files.get(*v)),
                    }
                }
                out
            })
            .collect()
    }

    /// Le programme est un script `.cmd` ou `.bat` : lancé par la bibliothèque de `sh`, il passe par `cmd.exe`, qui
    /// interprète les `&`, `%`… des arguments (les noms de fichiers du dépôt, ici).
    fn runs_through_cmd(&self) -> bool {
        let program = self.program().to_lowercase();
        program.ends_with(".cmd") || program.ends_with(".bat")
    }

    /// Un texte de la commande contient une suite de blancs (ou un blanc autre que l'espace) : git exécute la ligne avec
    /// `eval $cmd`, sans guillemets, si bien que sa découpe en mots fondrait ces blancs en un seul espace dans un
    /// argument entre guillemets. Les variables ne sont pas concernées (`"$LOCAL"` est développé après la découpe).
    fn collapsing_whitespace(&self) -> bool {
        let mut previous_blank = false;
        for piece in self.args.iter().flatten() {
            let Piece::Text(text) = piece else { continue };
            for c in text.chars() {
                let blank = c.is_whitespace();
                if blank && (c != ' ' || previous_blank) {
                    return true;
                }
                previous_blank = blank;
            }
            previous_blank = false;
        }
        false
    }

    /// Ligne pour `mergetool.<outil>.cmd`, exécutée par le `sh` de git : le texte entre apostrophes, les variables
    /// entre guillemets (`"$LOCAL"`). Le programme utilise des `/` (Windows les accepte, et `sh` n'a pas à traiter
    /// des `\` dans son chemin).
    pub fn sh_command(&self) -> String {
        let quoted: Vec<String> = self
            .args
            .iter()
            .enumerate()
            .map(|(i, arg)| {
                arg.iter()
                    .map(|piece| match piece {
                        Piece::Text(t) if i == 0 => sh_quote(&t.replace('\\', "/")),
                        Piece::Text(t) => sh_quote(t),
                        Piece::Var(v) => format!("\"${}\"", v.name()),
                    })
                    .collect::<String>()
            })
            .collect();
        quoted.join(" ")
    }
}

/// Caractères d'un nom de fichier que `cmd.exe` interpréterait (enchaînement, redirection, variables, guillemets). Les
/// blancs s'y ajoutent : `cmd /c` perd ses guillemets quand le programme et un argument en ont.
const CMD_UNSAFE: &str = "&|<>^%!\"()";

/// Nom d'outil pour `--tool=` et les clés `mergetool.<nom>.*` : lettres, chiffres et tirets seulement.
fn valid_tool_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

impl Repo {
    /// Lance `git mergetool` sur un fichier en conflit avec l'outil `tool`, sans `mergetool.<nom>.cmd` écrit dans la
    /// configuration : la commande passe par `-c`. `name` nomme l'outil pour git (par exemple `dragontree-kdiff3`).
    ///
    /// `trust_exit_code` : le code de sortie de l'outil dit si le fichier est résolu. Sans lui, git compare le fichier
    /// avant et après et, s'il n'a pas changé, ne le tient pas pour résolu. À réserver aux outils dont le code de sortie
    /// est fiable : sinon un outil fermé sans enregistrer (code 0) ferait indexer un fichier qui garde ses marqueurs.
    ///
    /// `mergetool.writeToTemp=false` est imposé : `$LOCAL`, `$REMOTE` et `$BASE` sont des fichiers voisins de
    /// `$MERGED`, nommés d'après son chemin (le refus des scripts `.cmd` ci-dessous s'appuie sur ce nom), jamais des
    /// chemins `/tmp` que les outils Windows ne sauraient pas ouvrir.
    pub fn run_mergetool_with(
        &self,
        name: &str,
        tool: &ToolCommand,
        path: &str,
        trust_exit_code: bool,
    ) -> Result<(), GitError> {
        if !valid_tool_name(name) {
            return Err(GitError::Refused(tr!("Nom d'outil invalide : {name}", name)));
        }
        if tool.collapsing_whitespace() {
            return Err(GitError::Refused(
                tr!(
                    "La commande contient une suite d'espaces (ou une tabulation) : git la réduirait à un seul espace."
                )
                .into(),
            ));
        }
        // Un script .cmd reçoit les noms de fichiers du dépôt par cmd.exe, qui les interpréterait (ou casserait la ligne
        // sur un blanc) : refusé plutôt que risqué. Les programmes .exe et les scripts sh n'ont pas ce défaut.
        if tool.runs_through_cmd()
            && path.chars().any(|c| CMD_UNSAFE.contains(c) || c.is_whitespace() || c.is_control())
        {
            return Err(GitError::Refused(tr!(
                "Le nom du fichier contient un blanc ou un caractère que cmd.exe interpréterait ({path}) : un outil .cmd (comme code.cmd) ne peut pas être lancé dessus. Utilisez un outil .exe ou renommez le fichier.",
                path
            )));
        }
        let cmd = format!("mergetool.{name}.cmd={}", tool.sh_command());
        let trust = format!("mergetool.{name}.trustExitCode={trust_exit_code}");
        let tool_arg = format!("--tool={name}");
        // Pas de conversion des arguments en chemins par le `sh` de git (un `/c` deviendrait `C:/`).
        self.write_paths()
            .env("MSYS_NO_PATHCONV", "1")
            .env("MSYS2_ARG_CONV_EXCL", "*")
            .args([
                "-c",
                &cmd,
                "-c",
                &trust,
                "-c",
                "mergetool.writeToTemp=false",
                "mergetool",
                &tool_arg,
                "--no-prompt",
                "--",
                path,
            ])
            .run()
            .map(|_| ())
    }

    /// Contenu d'un fichier tel qu'il serait dans la copie de travail (`git cat-file --filters` : fins de ligne,
    /// Git LFS…), à une révision (`rev` vide : l'index). `None` : le fichier n'existe pas à cette révision ;
    /// toute autre lecture qui échoue est une erreur.
    pub fn file_version(&self, rev: &str, path: &str) -> Result<Option<Vec<u8>>, GitError> {
        let present = if rev.is_empty() {
            self.read_paths().args(["ls-files", "-s", "-z", "--", path]).run()?.stdout
        } else {
            self.read_paths().args(["ls-tree", "-z", rev, "--", path]).run()?.stdout
        };
        if present.is_empty() {
            return Ok(None);
        }
        let spec = format!("{rev}:{path}");
        Ok(Some(self.read().args(["cat-file", "--filters", &spec]).run()?.stdout))
    }

    /// Lance l'outil de diff configuré dans git (`git difftool --no-prompt`) sur un fichier : entre l'index et la copie
    /// de travail (`revs` vide), entre HEAD et l'index (`cached`), ou entre deux révisions.
    pub fn run_difftool(&self, revs: &[&str], cached: bool, path: &str) -> Result<(), GitError> {
        let mut cmd = self.read_paths().args(["difftool", "--no-prompt"]);
        if cached {
            cmd = cmd.arg("--cached");
        }
        cmd.args(revs).args(["--", path]).run().map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> ToolFiles {
        ToolFiles {
            local: PathBuf::from(r"C:\Temp\a b\x&calc %PATH% ^y.txt"),
            remote: PathBuf::from("r.txt"),
            base: PathBuf::from("b.txt"),
            merged: PathBuf::from("m.txt"),
        }
    }

    fn strings(argv: Vec<OsString>) -> Vec<String> {
        argv.into_iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn a_template_is_split_into_program_and_arguments() {
        let tool = ToolCommand::parse(
            r#"  "C:\Program Files\KDiff3\kdiff3.exe"  --auto -o $MERGED $BASE   $LOCAL "$REMOTE" "#,
        )
        .unwrap();
        assert_eq!(tool.program(), r"C:\Program Files\KDiff3\kdiff3.exe");
        assert_eq!(
            strings(tool.argv(&files())),
            [
                r"C:\Program Files\KDiff3\kdiff3.exe",
                "--auto",
                "-o",
                "m.txt",
                "b.txt",
                r"C:\Temp\a b\x&calc %PATH% ^y.txt",
                "r.txt"
            ]
        );
    }

    #[test]
    fn variables_may_sit_inside_an_argument_and_a_value_stays_one_argument() {
        let tool =
            ToolCommand::parse(r#"tool -mergeoutput=$MERGED "--title=Fusion de $LOCAL" $LOCALE $local $"#).unwrap();
        assert_eq!(
            strings(tool.argv(&files())),
            [
                "tool",
                "-mergeoutput=m.txt",
                r"--title=Fusion de C:\Temp\a b\x&calc %PATH% ^y.txt",
                "$LOCALE",
                "$local",
                "$"
            ]
        );
        // Un chemin qui ressemble à un guillemet, une redirection ou un second argument reste dans son argument.
        let evil = ToolFiles { local: PathBuf::from(r#"a" & calc "b > c"#), ..ToolFiles::default() };
        assert_eq!(tool.argv(&evil).len(), 6);
        assert_eq!(strings(vec![tool.argv(&evil)[2].clone()]), [r#"--title=Fusion de a" & calc "b > c"#]);
    }

    #[test]
    fn invalid_templates_are_refused() {
        for bad in ["", "   ", r#"prog "non fermé"#, "$LOCAL --x", r#""" $LOCAL"#, "pro$MERGED gram"] {
            assert!(ToolCommand::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_sh_command_quotes_text_in_apostrophes_and_variables_in_double_quotes() {
        let tool =
            ToolCommand::parse(r#""C:\Program Files\KDiff3\kdiff3.exe" --auto -o $MERGED $BASE -t=$LOCAL"#).unwrap();
        assert_eq!(
            tool.sh_command(),
            r#"'C:/Program Files/KDiff3/kdiff3.exe' '--auto' '-o' "$MERGED" "$BASE" '-t='"$LOCAL""#
        );
        // Une apostrophe, un `$` ou des guillemets inversés du texte ne sont jamais interprétés par sh.
        let tool = ToolCommand::parse(r#"outil "l'été" 'x' `date` $(id) $$"#).unwrap();
        assert_eq!(tool.sh_command(), r#"'outil' 'l'\''été' ''\''x'\''' '`date`' '$(id)' '$$'"#);
        assert_eq!(sh_quote(""), "''");
    }

    #[test]
    fn runs_of_blanks_in_text_are_detected_but_variables_and_single_spaces_are_fine() {
        let ok = |t: &str| !ToolCommand::parse(t).unwrap().collapsing_whitespace();
        assert!(ok(r#""C:\Program Files\x.exe" --label "un deux" $LOCAL"#));
        assert!(!ok(r#"prog "un  deux""#));
        assert!(!ok("prog \"un\tdeux\""));
        assert!(ok("prog   a    b"), "les blancs entre arguments ne comptent pas");
    }

    #[test]
    fn tool_names_are_restricted() {
        assert!(valid_tool_name("dragontree-kdiff3"));
        for bad in ["", "a b", "a.b", "a=b", "é"] {
            assert!(!valid_tool_name(bad), "{bad:?}");
        }
    }
}
