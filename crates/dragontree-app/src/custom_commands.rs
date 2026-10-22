//! Commandes personnalisées (Réglages › Commandes) : une ligne de commande exécutée par `cmd.exe` dans le dossier
//! du dépôt, ou une URL ouverte dans le navigateur, sur le dépôt, une branche, un commit ou un fichier.
//!
//! Variables : `${repo}` (dossier de la copie de travail), `${branch}`, `${sha}`, `${file}` (chemin relatif à la
//! racine) et `${remote_url}` ; un `${nom}` inconnu reste tel quel. Leurs valeurs viennent de git : un nom de branche
//! ou de fichier peut contenir des espaces, `"`, `&`, `|`, `^`, `%`… et ne doit jamais devenir une commande.
//!
//! - **Script** ([`expand_script`]) : chaque valeur est citée pour cmd là où elle se trouve, comme la bibliothèque
//!   standard de Rust cite les arguments d'un `.bat` : entre guillemets (cmd n'y voit `&`, `|`, `<`, `>`, `^`, `(`
//!   que comme du texte), jamais deux guillemets de suite (`""`, qu'un programme lirait comme un guillemet). La ligne
//!   finie est ensuite vérifiée sur ce que lit chaque programme lancé (sans les `^` que cmd retire, une ligne
//!   d'arguments par commande entre `&` et `|`) : chaque caractère sensible d'une valeur est entre guillemets pour cmd,
//!   chaque blanc entre guillemets pour le programme, selon les règles récentes de Windows comme selon les anciennes de
//!   msvcrt (git pour Windows).
//!   Refus plutôt que risque — valeur avec un caractère de contrôle, un guillemet (CWE-88 : elle sortirait de son
//!   argument), un `%` (cmd développe `%…%` avant toute analyse, et le relit de part et d'autre d'un `|`), ou un
//!   caractère que la page de code ANSI changerait en `"`, `\`, `%` ou blanc pour un programme qui lit sa ligne en
//!   ANSI (best fit : `＂` → `"`, `＼` → `\` en page 1252) ; `%` du modèle qui touche une variable (`%${branch}%` avec
//!   la branche `DT_FILE` formerait `%DT_FILE%`) ; `%` du modèle en nombre impair avant une variable, ou de part et
//!   d'autre d'elle (`%X:a= ${branch} %` : la substitution de cmd déplacerait les guillemets de la valeur ; cmd
//!   développe `%…%` sur toute la ligne, `&` et `|` compris, et reprend après un nom non défini au `%` qui le
//!   fermait) ; guillemet échappé (`^"`, `\"`) dans un modèle qui a des variables ;
//!   variable juste après un `^` ; ligne qui lit `%DT_…%` ou `!DT_…!` ; ligne qui lit `CMDCMDLINE` (cmd y recopie sa
//!   ligne brute, valeurs citées comprises, et le guillemet ouvert après `/C` laisse le `&` d'une valeur hors
//!   guillemets).
//!   Garantie : une valeur ne devient pas une commande, et reste **un seul argument** pour un programme qui lit sa ligne
//!   avec les règles de Windows (msvcrt, UCRT, en Unicode ou en ANSI) — argument qui peut commencer par `-` (`--`
//!   devant un nom de fichier). Elle vaut pour **une** lecture de la ligne par cmd : une commande qui la fait relire
//!   (`call`, `cmd /c` imbriqué, `for /f`) la perd. La ligne est lancée par `dragontree_git::shell` (`/V:OFF`).
//!   De part et d'autre d'un `|`, chaque côté est relu par un cmd fils lancé sans `/V:OFF` : les valeurs n'y ont ni
//!   guillemet ni `%`, mais un `!…!` peut y être développé si l'expansion différée est activée dans le registre (valeur
//!   changée ou fuite d'une variable d'environnement ; l'expansion différée vient après l'analyse, rien ne s'exécute).
//!   Les valeurs acceptées sont aussi dans l'environnement du script : `DT_REPO`, `DT_BRANCH`, `DT_SHA`, `DT_FILE`,
//!   `DT_REMOTE_URL` (une valeur qui serait refusée dans la ligne n'y est pas définie). Elles se lisent dans un script
//!   `.cmd` ou `.ps1` appelé par la ligne, **jamais** en `%DT_…%` dans la ligne elle-même (cmd la développerait avant
//!   d'analyser `&`, `|` et les guillemets : ligne refusée, signalée dans l'éditeur). Dans un `.cmd`, entre
//!   guillemets (`"%DT_FILE%"`) et sans expansion différée (qui interpréterait les `!` de la valeur).
//! - **URL** ([`expand_url`]) : chaque valeur est encodée (`%XX`) sauf lettres, chiffres et `-._~` (et `/` pour
//!   `${file}`) : elle ne peut ajouter ni identifiants, ni port, ni segment hors de sa place ; `${remote_url}` en tête
//!   du modèle donne l'adresse entière. L'adresse n'est ouverte (par `egui::OpenUrl`, jamais par un shell) que si son
//!   schéma est `http`, `https` ou `mailto`.

use dragontree_i18n::tr;
use serde::{Deserialize, Serialize};

/// Élément sur lequel porte une commande : elle apparaît dans Dépôt › Commandes personnalisées (dépôt) ou dans le
/// menu contextuel des branches, des commits ou des fichiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CommandTarget {
    #[default]
    Repo,
    Branch,
    Commit,
    File,
}

/// Nature d'une commande : ligne de commande ou URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CommandKind {
    #[default]
    Script,
    Url,
}

/// Commande définie par l'utilisateur (réglage `custom_commands`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CustomCommand {
    /// Libellé dans les menus.
    pub name: String,
    pub target: CommandTarget,
    pub kind: CommandKind,
    /// Ligne de commande (script) ou modèle d'URL, avec ses variables `${…}`.
    pub command: String,
    /// Demander confirmation (la ligne ou l'URL complète est montrée).
    pub confirm: bool,
    /// Script : afficher sa sortie dans une fenêtre.
    pub show_output: bool,
    /// Script : relire le dépôt après son exécution.
    pub refresh: bool,
}

/// Variable d'un modèle (`${repo}`…).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Var {
    Repo,
    Branch,
    Sha,
    File,
    RemoteUrl,
}

impl Var {
    pub const ALL: [Var; 5] = [Var::Repo, Var::Branch, Var::Sha, Var::File, Var::RemoteUrl];

    /// Nom entre `${` et `}`.
    pub fn name(self) -> &'static str {
        match self {
            Var::Repo => "repo",
            Var::Branch => "branch",
            Var::Sha => "sha",
            Var::File => "file",
            Var::RemoteUrl => "remote_url",
        }
    }

    /// Variable d'environnement qui porte aussi la valeur pour un script.
    pub fn env_name(self) -> &'static str {
        match self {
            Var::Repo => "DT_REPO",
            Var::Branch => "DT_BRANCH",
            Var::Sha => "DT_SHA",
            Var::File => "DT_FILE",
            Var::RemoteUrl => "DT_REMOTE_URL",
        }
    }

    /// La variable telle qu'écrite dans un modèle (`${sha}`).
    pub fn token(self) -> String {
        format!("${{{}}}", self.name())
    }

    fn from_name(name: &str) -> Option<Var> {
        Var::ALL.into_iter().find(|v| v.name() == name)
    }
}

/// Valeurs des variables pour une exécution (vides quand l'élément n'en a pas).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vars {
    /// Dossier de la copie de travail (chemin Windows).
    pub repo: String,
    pub branch: String,
    /// SHA complet.
    pub sha: String,
    /// Chemin relatif à la racine du dépôt, séparateurs `/` (forme de git).
    pub file: String,
    pub remote_url: String,
}

impl Vars {
    pub fn get(&self, var: Var) -> &str {
        match var {
            Var::Repo => &self.repo,
            Var::Branch => &self.branch,
            Var::Sha => &self.sha,
            Var::File => &self.file,
            Var::RemoteUrl => &self.remote_url,
        }
    }

    /// Valeur pour un script : le chemin d'un fichier avec les séparateurs de Windows.
    fn for_script(&self, var: Var) -> String {
        match var {
            Var::File => self.file.replace('/', "\\"),
            _ => self.get(var).to_string(),
        }
    }
}

/// Pourquoi une commande n'a pas été lancée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandError {
    /// La valeur de la variable contient un retour à la ligne ou un autre caractère de contrôle.
    ControlChar(Var),
    /// Une variable suit un `^` hors guillemets (il s'appliquerait à la citation de la valeur).
    CaretBeforeVar(Var),
    /// La valeur de la variable contient un guillemet.
    Quote(Var),
    /// La valeur de la variable contient un `%`.
    Percent(Var),
    /// La page de code ANSI de Windows changerait un caractère de la valeur en `"`, `\`, `%` ou en blanc.
    CodePage(Var),
    /// Un `%` du modèle touche la variable : avec la valeur, il pourrait former un nom `%…%`.
    PercentNextToVar(Var),
    /// Un nombre impair de `%` du modèle précède la variable, ou d'autres la suivent : cmd pourrait lire la valeur
    /// dans un nom `%…%`.
    PercentAroundVar(Var),
    /// Le modèle contient un guillemet échappé (`^"`, `\"`) et des variables.
    EscapedQuote,
    /// Autour de la variable, cmd et le programme lancé ne liraient pas les mêmes guillemets.
    QuotesDiffer(Var),
    /// La ligne lit une variable d'environnement `DT_…` (`%DT_…%`, `!DT_…!`).
    EnvInLine,
    /// La ligne lit `CMDCMDLINE`, la ligne brute de cmd (valeurs citées comprises).
    CmdLineInLine,
    /// L'URL obtenue n'est ni `http`, ni `https`, ni `mailto`.
    UrlScheme(String),
}

impl std::fmt::Display for ExpandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            ExpandError::ControlChar(var) => {
                tr!("la valeur de {var} contient un retour à la ligne ou un caractère de contrôle", var = var.token())
            }
            ExpandError::CaretBeforeVar(var) => {
                tr!("la variable {var} suit un « ^ », qui s'appliquerait à sa valeur", var = var.token())
            }
            ExpandError::Quote(var) => tr!(
                "la valeur de {var} contient un guillemet (\"), qui changerait les arguments du programme",
                var = var.token()
            ),
            ExpandError::Percent(var) => {
                tr!("la valeur de {var} contient un « % », que cmd pourrait développer", var = var.token())
            }
            ExpandError::CodePage(var) => tr!(
                "la valeur de {var} contient un caractère que la page de code de Windows changerait en guillemet, « \\ », « % » ou espace pour le programme lancé",
                var = var.token()
            ),
            ExpandError::PercentNextToVar(var) => tr!(
                "un « % » touche {var} dans la ligne : avec la valeur, il pourrait former une variable %…% que cmd développerait ; séparez-les par une espace",
                var = var.token()
            ),
            ExpandError::PercentAroundVar(var) => tr!(
                "un « % » seul précède {var}, ou des « % » l'entourent dans la ligne : cmd pourrait lire la valeur dans un nom %…% et déplacer ses guillemets ; placez les %…% complets d'un seul côté des variables",
                var = var.token()
            ),
            ExpandError::EscapedQuote => tr!(
                "la ligne contient un guillemet échappé (^\" ou \\\") : cmd et le programme lancé ne liraient pas les mêmes guillemets autour des valeurs"
            )
            .to_string(),
            ExpandError::QuotesDiffer(var) => tr!(
                "autour de {var}, cmd et le programme lancé ne liraient pas les mêmes guillemets (\\\", ^\" ou \"\" dans la ligne)",
                var = var.token()
            ),
            ExpandError::EnvInLine => tr!(
                "la ligne lit une variable DT_… : cmd y interpréterait sa valeur. Écrivez ${…} dans la ligne, ou lisez DT_… dans un script .cmd ou .ps1"
            )
            .to_string(),
            ExpandError::CmdLineInLine => tr!(
                "la ligne lit CMDCMDLINE : cmd y recopierait sa propre ligne, valeurs et guillemets compris, et une valeur pourrait sortir de ses guillemets"
            )
            .to_string(),
            ExpandError::UrlScheme(url) => tr!("seules les adresses http, https et mailto sont ouvertes : {url}", url),
        };
        f.write_str(&text)
    }
}

/// Commande prête à lancer : sa ligne de commande (script) ou son URL, variables remplacées.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub name: String,
    pub kind: CommandKind,
    pub text: String,
    /// Valeurs des variables dans l'environnement d'un script (`DT_REPO`…) ; `None` : la valeur serait refusée dans la
    /// ligne (guillemet, `%`…), la variable n'est pas définie.
    pub env: Vec<(String, Option<String>)>,
    pub show_output: bool,
    pub refresh: bool,
}

impl CustomCommand {
    /// Remplace les variables de la commande par les valeurs de l'élément.
    pub fn prepare(&self, vars: &Vars) -> Result<Prepared, ExpandError> {
        let script = self.kind == CommandKind::Script;
        let template = self.command.trim();
        let text = if script { expand_script(template, vars)? } else { expand_url(template, vars)? };
        Ok(Prepared {
            name: self.name.trim().to_string(),
            kind: self.kind,
            text,
            env: Var::ALL
                .iter()
                .map(|&v| {
                    let value = vars.for_script(v);
                    (v.env_name().to_string(), value_error(v, &value).is_none().then_some(value))
                })
                .collect(),
            show_output: script && self.show_output,
            refresh: script && self.refresh,
        })
    }
}

/// Commandes à proposer pour `target`, avec leur position dans la liste des réglages ; une commande sans nom ou
/// sans ligne n'est pas proposée.
pub fn for_target(list: &[CustomCommand], target: CommandTarget) -> impl Iterator<Item = (usize, &CustomCommand)> {
    list.iter()
        .enumerate()
        .filter(move |(_, c)| c.target == target && !c.name.trim().is_empty() && !c.command.trim().is_empty())
}

/// Morceau d'un modèle : texte tel quel ou variable connue.
enum Piece<'a> {
    Text(&'a str),
    Var(Var),
}

/// Découpe un modèle ; un `${nom}` inconnu ou non fermé reste du texte.
fn pieces(template: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        match after.find('}').and_then(|end| Var::from_name(&after[..end]).map(|var| (end, var))) {
            Some((end, var)) => {
                if start > 0 {
                    out.push(Piece::Text(&rest[..start]));
                }
                out.push(Piece::Var(var));
                rest = &after[end + 1..];
            }
            None => {
                out.push(Piece::Text(&rest[..start + 2]));
                rest = after;
            }
        }
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest));
    }
    out
}

/// La ligne lit une variable d'environnement `DT_…` (`%DT_…%`, `!DT_…!`, sans tenir compte de la casse) : cmd
/// développe `%…%` avant d'analyser `&`, `|` et les guillemets, la valeur y deviendrait une commande.
pub fn reads_dt_env(template: &str) -> bool {
    let upper = template.to_ascii_uppercase();
    upper.contains("%DT_") || upper.contains("!DT_")
}

/// La ligne lit `CMDCMDLINE` (`%CMDCMDLINE%`, `%CMDCMDLINE:~0%`, `!CMDCMDLINE!`, sans tenir compte de la casse) : cmd
/// y développe sa propre ligne brute, valeurs citées comprises, et le guillemet ouvert après `/C` décale ceux de la
/// ligne développée : le `&` d'une valeur s'y retrouve hors guillemets, avant comme après la variable.
pub fn reads_cmd_line(template: &str) -> bool {
    template.to_ascii_uppercase().contains("CMDCMDLINE")
}

/// Ligne de commande de `template` pour `cmd.exe`, chaque variable remplacée par sa valeur citée (`CmdLine`),
/// puis vérifiée indépendamment de sa construction (`check_values`). Refus : ligne qui lit une variable `DT_…` ou
/// `CMDCMDLINE`, valeur refusée (`value_error`), `%` qui touche une variable, `%` en nombre impair avant une variable
/// ou de part et d'autre d'elle, guillemet échappé (`^"`, `\"`) dans un modèle qui a des variables, variable juste
/// après un `^` hors guillemets (il échapperait le guillemet ouvrant de la valeur), guillemets lus différemment par cmd
/// et par le programme autour d'une valeur.
pub fn expand_script(template: &str, vars: &Vars) -> Result<String, ExpandError> {
    if reads_dt_env(template) {
        return Err(ExpandError::EnvInLine);
    }
    if reads_cmd_line(template) {
        return Err(ExpandError::CmdLineInLine);
    }
    let pieces = pieces(template);
    if let Some(var) = percent_next_to_var(&pieces) {
        return Err(ExpandError::PercentNextToVar(var));
    }
    if let Some(var) = percent_around_var(&pieces) {
        return Err(ExpandError::PercentAroundVar(var));
    }
    let mut line = CmdLine::default();
    for piece in pieces {
        match piece {
            Piece::Text(text) => line.push_text(text),
            Piece::Var(var) => line.push_value(var, &vars.for_script(var))?,
        }
    }
    line.finish()
}

/// Pourquoi une valeur ne peut entrer ni dans une ligne de commande, ni dans l'environnement du script : caractère de
/// contrôle (un retour à la ligne terminerait la commande), guillemet (la valeur sortirait de son argument), `%`
/// (cmd développe `%…%` avant toute analyse, et le relit de part et d'autre d'un `|`), ou caractère que la page de
/// code ANSI changerait en l'un de ceux-là pour le programme lancé (`＂` → `"`, `＼` → `\` en page 1252).
fn value_error(var: Var, value: &str) -> Option<ExpandError> {
    if value.chars().any(char::is_control) {
        Some(ExpandError::ControlChar(var))
    } else if value.contains('"') {
        Some(ExpandError::Quote(var))
    } else if value.contains('%') {
        Some(ExpandError::Percent(var))
    } else if code_page_adds_sensitive(value, dragontree_git::shell::ansi_view(value).as_deref()) {
        Some(ExpandError::CodePage(var))
    } else {
        None
    }
}

/// La vue ANSI de `value` (`None` si la conversion a échoué) contient plus de `"`, `\`, `%` ou blancs que la valeur :
/// un programme lancé lirait ces caractères là où cmd ne les voit pas.
fn code_page_adds_sensitive(value: &str, view: Option<&str>) -> bool {
    let Some(view) = view else { return true };
    ['"', '\\', '%', ' ', '\t'].into_iter().any(|c| view.matches(c).count() > value.matches(c).count())
}

/// Variable qu'un `%` du modèle touche, sans espace ni guillemet entre eux (une autre variable entre les deux ne les
/// sépare pas) : avec la valeur, il pourrait former un nom `%…%` que cmd développerait — `%${branch}%` avec la branche
/// `DT_FILE`, ou `%D${branch}E%` avec la branche `T_FIL`.
fn percent_next_to_var(pieces: &[Piece]) -> Option<Var> {
    // Le modèle caractère par caractère, `None` à la place de chaque variable.
    let mut chars: Vec<Option<char>> = Vec::new();
    let mut vars = Vec::new();
    for piece in pieces {
        match *piece {
            Piece::Text(text) => chars.extend(text.chars().map(Some)),
            Piece::Var(var) => {
                vars.push((chars.len(), var));
                chars.push(None);
            }
        }
    }
    let touches = |side: &mut dyn Iterator<Item = &Option<char>>| {
        side.flatten().take_while(|c| !c.is_whitespace() && **c != '"').any(|c| *c == '%')
    };
    vars.into_iter()
        .find(|&(at, _)| touches(&mut chars[..at].iter().rev()) || touches(&mut chars[at + 1..].iter()))
        .map(|(_, var)| var)
}

/// Variable que des `%` du modèle, même séparés d'elle, peuvent faire entrer dans un nom `%…%` : un nombre impair de
/// `%` avant elle (un `%` seul compris), ou des `%` avant et après elle. cmd développe `%…%` sur toute la ligne avant
/// de la découper aux `&` et aux `|`, et après un nom non défini reprend la lecture au `%` qui le fermait : le dernier
/// `%` avant la valeur peut ouvrir un nom que ferme le premier `%` après elle. `%X:a= ${branch} %` remplace alors les
/// `a` de X par la valeur citée, dont les guillemets ne sont plus à leur place (de même `%NOPE%X:a= ${branch} %`).
fn percent_around_var(pieces: &[Piece]) -> Option<Var> {
    let percents = |piece: &Piece| match *piece {
        Piece::Text(text) => text.matches('%').count(),
        Piece::Var(_) => 0,
    };
    let total: usize = pieces.iter().map(percents).sum();
    let mut before = 0;
    for piece in pieces {
        before += percents(piece);
        if let Piece::Var(var) = *piece
            && (before % 2 == 1 || (before > 0 && before < total))
        {
            return Some(var);
        }
    }
    None
}

/// Caractère d'une valeur qui oblige à la mettre entre guillemets pour cmd : blanc, ou ASCII autre qu'une lettre, un
/// chiffre et `#$*+-./:?@\_` (les règles de la bibliothèque standard de Rust pour un argument de `.bat`).
fn needs_cmd_quotes(c: char) -> bool {
    const SAFE: &str = r"#$*+-./:?@\_";
    c.is_whitespace() || (c.is_ascii() && !(c.is_ascii_alphanumeric() || SAFE.contains(c)))
}

/// Ligne de commande en construction, et l'état de cmd à sa fin. Une valeur à citer hors guillemets ouvre une région
/// entre guillemets, refermée juste avant le texte suivant de l'utilisateur ou en fin de ligne : des variables voisines
/// (`${a}${b}`) la partagent. Deux guillemets ne se suivent jamais (`""`, qu'un programme lit comme un guillemet, et
/// qui ferme les guillemets selon les anciennes règles de msvcrt) : une valeur qui suit un guillemet fermant rouvre la
/// région qu'il fermait, une valeur suivie du guillemet ouvrant de l'utilisateur se prolonge dans sa région. Les `\`
/// d'une valeur qui précèdent un guillemet sont doublés (lecture des arguments par Windows).
#[derive(Default)]
struct CmdLine {
    out: String,
    /// cmd est entre guillemets.
    quoted: bool,
    /// Un `^` hors guillemets attend le caractère qu'il échappe.
    caret: bool,
    /// `\` du modèle juste avant la position courante, tels que les lit le programme (cmd retire les `^` qui
    /// échappent : ils ne coupent pas la suite).
    text_backslashes: usize,
    /// Le modèle contient un guillemet échappé : `^"` hors guillemets (littéral pour cmd, guillemet pour le programme)
    /// ou précédé d'un nombre impair de `\` (guillemet pour cmd, littéral pour le programme).
    escaped_quote: bool,
    /// La région entre guillemets courante a été ouverte ici, autour de valeurs, et n'est pas refermée.
    ours: bool,
    /// Nombre de `\` venus d'une valeur à la fin de `out` (à doubler devant un guillemet).
    value_backslashes: usize,
    /// `out` finit par un guillemet qui a fermé une région, précédé de ce nombre de `\` doublés.
    closed: Option<usize>,
    /// Position de chaque valeur dans la ligne.
    values: Vec<(std::ops::Range<usize>, Var)>,
}

impl CmdLine {
    fn push_text(&mut self, text: &str) {
        let mut chars = text.chars().peekable();
        if std::mem::take(&mut self.ours) {
            if chars.peek() == Some(&'"') {
                // Guillemet ouvrant de l'utilisateur : sa région prolonge la nôtre.
                chars.next();
            } else {
                self.close_quote();
            }
        }
        for c in chars {
            if self.caret {
                self.caret = false;
                self.escaped_quote |= c == '"';
                self.text_backslashes = if c == '\\' { self.text_backslashes + 1 } else { 0 };
                self.push_plain(c);
            } else if c == '"' {
                self.escaped_quote |= self.text_backslashes % 2 == 1;
                self.text_backslashes = 0;
                if self.quoted {
                    self.close_quote();
                } else {
                    self.push_plain(c);
                    self.quoted = true;
                }
            } else {
                if c == '^' && !self.quoted {
                    self.caret = true;
                } else {
                    self.text_backslashes = if c == '\\' { self.text_backslashes + 1 } else { 0 };
                }
                self.push_plain(c);
            }
        }
    }

    fn push_value(&mut self, var: Var, value: &str) -> Result<(), ExpandError> {
        if self.caret {
            return Err(ExpandError::CaretBeforeVar(var));
        }
        if let Some(e) = value_error(var, value) {
            return Err(e);
        }
        self.text_backslashes = 0;
        let quote = value.is_empty() || value.ends_with('\\') || value.chars().any(needs_cmd_quotes);
        if !self.quoted && quote {
            match self.closed {
                // Rouvre la région que ce guillemet fermait (sans ses `\` doublés) plutôt que d'écrire `""`.
                Some(doubled) => {
                    self.out.pop();
                    self.out.truncate(self.out.len() - doubled);
                    self.value_backslashes = doubled;
                }
                None => {
                    // `\` de l'utilisateur juste avant : doublés, sinon le guillemet ouvrant serait lu comme un caractère.
                    let backslashes = self.out.chars().rev().take_while(|c| *c == '\\').count();
                    self.out.extend(std::iter::repeat_n('\\', backslashes));
                    self.out.push('"');
                }
            }
            self.quoted = true;
            self.ours = true;
        }
        self.closed = None;
        let start = self.out.len();
        for c in value.chars() {
            self.out.push(c);
            self.value_backslashes = if c == '\\' { self.value_backslashes + 1 } else { 0 };
        }
        self.values.push((start..self.out.len(), var));
        Ok(())
    }

    fn push_plain(&mut self, c: char) {
        self.out.push(c);
        self.value_backslashes = 0;
        self.closed = None;
    }

    /// Ferme la région entre guillemets courante.
    fn close_quote(&mut self) {
        let doubled = std::mem::take(&mut self.value_backslashes);
        self.out.extend(std::iter::repeat_n('\\', doubled));
        self.out.push('"');
        self.quoted = false;
        self.closed = Some(doubled);
    }

    fn finish(mut self) -> Result<String, ExpandError> {
        if std::mem::take(&mut self.ours) {
            self.close_quote();
        }
        if self.escaped_quote && !self.values.is_empty() {
            return Err(ExpandError::EscapedQuote);
        }
        check_values(&self.out, &self.values)?;
        Ok(self.out)
    }
}

/// Vérifie la ligne finie, indépendamment de sa construction : chaque caractère d'une valeur qui compte pour cmd est
/// entre guillemets pour cmd, et chaque blanc d'une valeur est entre guillemets pour le programme lancé — tel qu'il
/// lit sa ligne ([`program_view`]), selon les règles récentes de Windows comme selon les anciennes de msvcrt : une
/// valeur ne peut ni devenir une commande, ni déborder de son argument.
fn check_values(line: &str, values: &[(std::ops::Range<usize>, Var)]) -> Result<(), ExpandError> {
    let cmd = cmd_quoting(line);
    // Pour chaque octet : entre guillemets pour le programme, selon les anciennes puis les récentes règles.
    let mut argv = vec![[false; 2]; line.len()];
    for command in program_view(line) {
        for (rules, old_rules) in [(0, true), (1, false)] {
            for (&(at, _), quoted) in command.iter().zip(argv_quoting(&command, old_rules)) {
                argv[at][rules] = quoted;
            }
        }
    }
    for (range, var) in values {
        for (offset, c) in line[range.clone()].char_indices() {
            let at = range.start + offset;
            let for_cmd = !needs_cmd_quotes(c) || cmd[at];
            let for_program = !matches!(c, ' ' | '\t') || argv[at] == [true, true];
            if !(for_cmd && for_program) {
                return Err(ExpandError::QuotesDiffer(*var));
            }
        }
    }
    Ok(())
}

/// La ligne telle que la lisent les programmes lancés : cmd retire les `^` qui échappent hors guillemets, et chaque
/// `&` ou `|` hors guillemets commence une autre commande, donc une autre ligne d'arguments. Chaque caractère garde
/// sa position dans la ligne.
fn program_view(line: &str) -> Vec<Vec<(usize, char)>> {
    let mut commands = vec![Vec::new()];
    let (mut quoted, mut caret) = (false, false);
    for (at, c) in line.char_indices() {
        let current = commands.len() - 1;
        if caret {
            caret = false;
            commands[current].push((at, c));
            continue;
        }
        match c {
            '^' if !quoted => caret = true,
            '&' | '|' if !quoted => commands.push(Vec::new()),
            _ => {
                quoted ^= c == '"';
                commands[current].push((at, c));
            }
        }
    }
    commands
}

/// Pour chaque octet de `line` : cmd y est entre guillemets (hors guillemets, `^` échappe le caractère suivant).
fn cmd_quoting(line: &str) -> Vec<bool> {
    let mut quoted = vec![false; line.len()];
    let (mut inside, mut caret) = (false, false);
    for (at, c) in line.char_indices() {
        quoted[at] = inside;
        if caret {
            caret = false;
        } else if c == '"' {
            inside = !inside;
        } else if c == '^' && !inside {
            caret = true;
        }
    }
    quoted
}

/// Pour chaque caractère d'une ligne d'arguments : un programme qui la lit avec les règles de Windows y est entre
/// guillemets. `n` barres obliques inverses devant un guillemet en donnent `n / 2`, et le guillemet est littéral si
/// `n` est impair ; `""` entre guillemets donne un guillemet, et ferme les guillemets selon les anciennes règles de
/// msvcrt (`old_rules`, celles de git pour Windows) mais pas selon les récentes.
fn argv_quoting(chars: &[(usize, char)], old_rules: bool) -> Vec<bool> {
    let mut quoted = vec![false; chars.len()];
    let mut inside = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        let next_is_quote = chars.get(i + 1).is_some_and(|(_, c)| *c == '"');
        quoted[i] = inside;
        match c {
            '\\' => {
                let run = chars[i..].iter().take_while(|(_, c)| *c == '\\').count();
                quoted[i..i + run].fill(inside);
                i += run;
                if run % 2 == 1 && chars.get(i).is_some_and(|(_, c)| *c == '"') {
                    quoted[i] = inside;
                    i += 1;
                }
                continue;
            }
            '"' if inside && next_is_quote => {
                quoted[i + 1] = inside;
                inside = !old_rules;
                i += 2;
                continue;
            }
            '"' => inside = !inside,
            _ => {}
        }
        i += 1;
    }
    quoted
}

/// URL de `template`, chaque variable remplacée par sa valeur encodée (`%XX`, sauf lettres, chiffres et `-._~`, et
/// `/` pour `${file}`) : une valeur ne peut ajouter ni identifiants (`@`), ni port (`:`), ni segment de chemin hors de
/// sa place. `${remote_url}` en tête du modèle donne l'adresse entière (seuls ses caractères interdits dans une URL
/// sont encodés). Refusée si elle n'est pas `http`, `https` ou `mailto` (jamais un fichier ni un programme local).
pub fn expand_url(template: &str, vars: &Vars) -> Result<String, ExpandError> {
    let mut url = String::with_capacity(template.len());
    for (i, piece) in pieces(template).into_iter().enumerate() {
        match piece {
            Piece::Text(text) => url.push_str(text),
            Piece::Var(Var::RemoteUrl) if i == 0 => {
                for b in vars.remote_url.trim().bytes() {
                    if b <= b' ' || !b.is_ascii() || b"\"<>\\^`{|}\x7f".contains(&b) {
                        url.push_str(&format!("%{b:02X}"));
                    } else {
                        url.push(char::from(b));
                    }
                }
            }
            Piece::Var(var) => percent_encode(&mut url, vars.get(var), var == Var::File),
        }
    }
    let url = url.trim().to_string();
    let scheme = url.split_once(':').map(|(scheme, _)| scheme.to_ascii_lowercase());
    match scheme.as_deref() {
        Some("http" | "https" | "mailto") => Ok(url),
        _ => Err(ExpandError::UrlScheme(url)),
    }
}

fn percent_encode(out: &mut String, value: &str, keep_slash: bool) {
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) || (keep_slash && b == b'/') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vars {
        Vars {
            repo: r"C:\DEV\fabrikam".into(),
            branch: "feature/x".into(),
            sha: "0123456789abcdef0123456789abcdef01234567".into(),
            file: "src/a.txt".into(),
            remote_url: "https://forge.contoso.test/fabrikam.git".into(),
        }
    }

    fn with_branch(branch: &str) -> Vars {
        Vars { branch: branch.into(), ..vars() }
    }

    fn with_file(file: &str) -> Vars {
        Vars { file: file.into(), ..vars() }
    }

    fn script(template: &str, vars: &Vars) -> String {
        expand_script(template, vars).unwrap_or_else(|e| panic!("{template} : {e:?}"))
    }

    #[test]
    fn plain_values_are_inserted_as_they_are() {
        assert_eq!(script("echo ${sha} > out.txt", &vars()), "echo 0123456789abcdef0123456789abcdef01234567 > out.txt");
        assert_eq!(script("git log ${branch} -- ${file}", &vars()), r"git log feature/x -- src\a.txt");
        assert_eq!(script("echo ${repo}", &vars()), r"echo C:\DEV\fabrikam");
    }

    #[test]
    fn values_with_spaces_or_cmd_characters_are_quoted() {
        let repo = Vars { repo: r"C:\DEV\Mon dépôt".into(), ..vars() };
        assert_eq!(script("code ${repo}", &repo), r#"code "C:\DEV\Mon dépôt""#);
        assert_eq!(script("git log ${branch}", &with_branch("a&calc")), r#"git log "a&calc""#);
        for hostile in ["a|b", "a>b", "a<b", "a^b", "(a)", "a,b", "a;b", "a=b", "a'b", "a!b"] {
            assert_eq!(script("x ${branch}", &with_branch(hostile)), format!("x \"{hostile}\""), "{hostile}");
        }
        assert_eq!(script("x ${branch}", &with_branch("")), r#"x """#, "une valeur vide reste un argument");
    }

    #[test]
    fn a_value_with_a_quote_is_refused() {
        // Un programme qui lit ses arguments avec les règles de msvcrt sortirait des guillemets au `""` : la suite de
        // la valeur deviendrait d'autres arguments, par exemple des options.
        let quoted_file = with_file(r#"x" --output=y"#);
        assert_eq!(expand_script("type ${file}", &quoted_file), Err(ExpandError::Quote(Var::File)));
        assert_eq!(expand_script(r#"notepad "${file}""#, &quoted_file), Err(ExpandError::Quote(Var::File)));
        assert_eq!(expand_script("x ${branch}", &with_branch(r#"a\"b"#)), Err(ExpandError::Quote(Var::Branch)));
        // Sans objet pour la commande : rien n'est refusé.
        assert!(expand_script("echo ${sha}", &quoted_file).is_ok());
    }

    #[test]
    fn a_variable_inside_the_users_quotes_gets_no_quotes_of_its_own() {
        assert_eq!(script(r#"notepad "${file}""#, &with_file("a&b c.txt")), r#"notepad "a&b c.txt""#);
        // Le `^` est littéral entre guillemets.
        assert_eq!(script(r#"echo "^${branch}""#, &with_branch("a&b")), r#"echo "^a&b""#);
    }

    #[test]
    fn adjacent_values_share_one_quoted_region_and_never_produce_two_quotes_in_a_row() {
        let both = Vars { branch: "a b".into(), file: "c&d".into(), ..vars() };
        assert_eq!(script("x ${branch}${file}", &both), r#"x "a bc&d""#);
        assert_eq!(script(r#"x "${branch}${file}""#, &both), r#"x "a bc&d""#);
        // Après le guillemet fermant de l'utilisateur, la région qu'il fermait est rouverte.
        assert_eq!(script(r#"x "${branch}"${file}"#, &both), r#"x "a bc&d""#);
        assert_eq!(script(r#"x "y"${file}"#, &both), r#"x "yc&d""#);
        // Avant son guillemet ouvrant, la valeur se prolonge dans sa région.
        assert_eq!(script(r#"x ${branch}"y z""#, &both), r#"x "a by z""#);
        // Une valeur sûre à côté d'une valeur citée reste dans le même argument.
        assert_eq!(script("x ${sha}${branch}", &both), format!(r#"x {}"a b""#, vars().sha));
        // Les `\` doublés devant le guillemet refermé sont retirés quand la région se rouvre.
        let root = Vars { repo: r"C:\".into(), file: "a b".into(), ..vars() };
        assert_eq!(script(r#"x "${repo}"${file}"#, &root), r#"x "C:\a b""#);
    }

    #[test]
    fn a_backslash_before_an_opening_quote_is_doubled() {
        assert_eq!(script(r"x \${branch}", &with_branch("a b")), r#"x \\"a b""#);
    }

    #[test]
    fn an_unbalanced_quote_of_the_template_keeps_the_value_quoted() {
        assert_eq!(script(r#"x "${branch}"#, &with_branch("a b")), r#"x "a b"#);
        assert_eq!(script(r#"x ${branch}""#, &with_branch("a b")), r#"x "a b"#);
    }

    #[test]
    fn templates_where_cmd_and_the_program_would_read_different_quotes_are_refused() {
        // `""` entre guillemets : msvcrt les ferme, l'espace de la valeur séparerait deux arguments.
        assert_eq!(
            expand_script(r#"x "y""${branch}""#, &with_branch("a b")),
            Err(ExpandError::QuotesDiffer(Var::Branch))
        );
    }

    #[test]
    fn an_escaped_quote_in_a_template_with_variables_is_refused() {
        // `^"` : littéral pour cmd, guillemet pour le programme (cmd retire le `^`) ; `\"` : l'inverse. Le programme
        // lit les `\` sans les `^` que cmd a retirés (`\^\\"` : trois `\` devant le guillemet).
        for template in [
            r#"git rev-parse --sq-quote x \^"y^" ${branch}"#,
            r#"echo ^" & git rev-parse ^"y ${branch}"#,
            r#"echo ^"${branch}"#,
            r#"x \"${branch}"#,
            r#"x \^\\"${branch}""#,
        ] {
            assert_eq!(expand_script(template, &with_branch("a b")), Err(ExpandError::EscapedQuote), "{template}");
        }
        // Sans variable, rien à protéger ; `\\"` (nombre pair) est un guillemet pour cmd comme pour le programme.
        assert!(expand_script(r#"echo ^"x"#, &vars()).is_ok());
        assert_eq!(script(r#"x \\"${branch}""#, &with_branch("a b")), r#"x \\"a b""#);
    }

    #[test]
    fn each_command_of_the_line_reads_its_own_arguments() {
        // Après `&`, le programme suivant lit une nouvelle ligne : le `""` de la commande précédente n'y compte pas.
        assert_eq!(script(r#"echo "a""b" & git log ${branch}"#, &with_branch("a b")), r#"echo "a""b" & git log "a b""#);
    }

    /// `argv_quoting` sur une ligne entière (une seule commande).
    fn argv(line: &str, old_rules: bool) -> Vec<bool> {
        argv_quoting(&line.char_indices().collect::<Vec<_>>(), old_rules)
    }

    #[test]
    fn the_program_reads_quotes_with_the_old_or_the_new_windows_rules() {
        let line = r#"x "a""b c" d"#;
        let quoted = |old| {
            let q = argv(line, old);
            [line.find("b c").unwrap() + 1, line.find(" d").unwrap()].map(|at| q[at])
        };
        // Anciennes règles (msvcrt) : `""` ferme les guillemets (et le `"` suivant les rouvre) ; récentes : il y reste.
        assert_eq!(quoted(true), [false, true]);
        assert_eq!(quoted(false), [true, false]);
        assert!(!argv(r#"\"a b"#, true)[3], "`\\\"` : guillemet littéral");
        assert!(argv(r#"\\"a b"#, true)[4], "`\\\\\"` : une barre, puis un guillemet");
    }

    #[test]
    fn a_value_with_a_percent_sign_is_refused() {
        // cmd développe `%…%` avant toute analyse, et de nouveau de part et d'autre d'un `|`.
        for template in ["x ${branch}", r#"x "${branch}""#, "echo ${branch} | sort"] {
            for value in ["%PATH%", "a%b", "%DT_FILE%", "DT_FILE%"] {
                assert_eq!(expand_script(template, &with_branch(value)), Err(ExpandError::Percent(Var::Branch)));
            }
        }
    }

    #[test]
    fn a_percent_sign_touching_a_variable_is_refused() {
        // Avec la valeur, il formerait un nom `%…%` : cmd développerait la variable que la valeur choisit (DT_FILE…).
        for template in [
            "echo %${branch}%",
            "echo %${branch}",
            "echo ${branch}%",
            r#"echo "%${branch}""#,
            r#"echo "%NOPE%${branch}""#,
            "echo %D${branch}E%",
            "echo %${sha}${branch}",
        ] {
            assert!(
                matches!(expand_script(template, &with_branch("DT_FILE")), Err(ExpandError::PercentNextToVar(_))),
                "{template}"
            );
        }
        // Séparés par une espace ou un guillemet, `%…%` complets d'un seul côté : aucun nom ne peut englober la valeur.
        for template in [r#""%LOCALAPPDATA%\outil.exe" ${file}"#, r#"type ${file} > "%TEMP%\x""#] {
            assert!(expand_script(template, &vars()).is_ok(), "{template}");
        }
    }

    #[test]
    fn percent_signs_around_a_variable_are_refused() {
        // cmd développe `%…%` sur toute la ligne avant de la découper aux `&` et `|`, et après un nom non défini
        // reprend la lecture au `%` qui le fermait : le dernier `%` avant la valeur peut ouvrir un nom fermé par le
        // premier `%` après elle (`%X:a= … %` remplace les `a` de X par la valeur, guillemets compris). Un `%` seul
        // est refusé aussi.
        for template in [
            "echo %X:a= ${branch} %",
            "echo %NOPE%X:a= ${branch} %",
            "echo %X:a= & echo ${branch} %",
            "echo %X:a= | sort ${branch} %",
            r#"echo "%X:a=" ${branch} "%""#,
            "echo %DATE% ${branch} %TIME%",
            "echo 100% ${branch}",
            "echo % ${branch}",
            "echo %DATE% ${sha} ${branch} %",
        ] {
            assert!(matches!(expand_script(template, &vars()), Err(ExpandError::PercentAroundVar(_))), "{template}");
        }
        assert_eq!(expand_script("echo %X:a= ${branch} %", &vars()), Err(ExpandError::PercentAroundVar(Var::Branch)));
        // `%…%` complets avant les variables, ou tous après elles.
        for template in [
            "echo %DATE% ${branch}",
            r#""%ProgramFiles%\outil.exe" ${file} ${sha}"#,
            "echo ${branch} %DATE% %TIME%",
            "echo %DATE% & echo ${branch}",
            "echo ${sha} 100%",
        ] {
            assert!(expand_script(template, &vars()).is_ok(), "{template}");
        }
    }

    #[test]
    fn a_value_that_the_ansi_code_page_turns_into_a_sensitive_character_is_refused() {
        #[cfg(windows)]
        {
            use dragontree_git::shell::ansi_view_in;
            // Page 1252 : `＂`, `″`, `ʺ` deviennent `"`, `＼` devient `\` pour un programme qui lit sa ligne en ANSI.
            for value in ["a\u{FF02} --output=evil", "a \u{FF3C}", "a\u{2033}b", "a\u{02BA}b"] {
                assert!(code_page_adds_sensitive(value, ansi_view_in(value, 1252).as_deref()), "{value}");
            }
            assert!(!code_page_adds_sensitive("é ü \\", ansi_view_in("é ü \\", 1252).as_deref()));
            // Dans la page de code de ce poste : refusé dès qu'elle fait apparaître un guillemet.
            let fullwidth = "a\u{FF02} --output=evil";
            if dragontree_git::shell::ansi_view(fullwidth).is_some_and(|v| v.contains('"')) {
                assert_eq!(
                    expand_script("x ${branch}", &with_branch(fullwidth)),
                    Err(ExpandError::CodePage(Var::Branch))
                );
                let both = Vars { branch: "a \u{FF3C}".into(), file: "f g".into(), ..vars() };
                assert_eq!(expand_script("x ${branch}${file}", &both), Err(ExpandError::CodePage(Var::Branch)));
            }
        }
        // Conversion impossible : on ne sait pas ce que lirait le programme, la valeur est refusée.
        assert!(code_page_adds_sensitive("a", None));
    }

    #[test]
    fn only_values_that_pass_the_checks_are_exported_to_the_environment() {
        let command = CustomCommand { command: "build.cmd ${sha}".into(), ..Default::default() };
        let values = Vars { file: r#"x" & echo y"#.into(), branch: "a%b".into(), ..vars() };
        let env: std::collections::HashMap<String, Option<String>> =
            command.prepare(&values).unwrap().env.into_iter().collect();
        assert_eq!(env["DT_FILE"], None);
        assert_eq!(env["DT_BRANCH"], None);
        assert_eq!(env["DT_SHA"].as_deref(), Some(vars().sha.as_str()));
        assert_eq!(env["DT_REPO"].as_deref(), Some(r"C:\DEV\fabrikam"));
    }

    #[test]
    fn the_line_cannot_read_the_dt_environment_variables() {
        for template in ["echo %DT_FILE%", "echo %dt_sha:~0,7%", "echo !DT_BRANCH!", "call echo %%DT_REPO%%"] {
            assert!(reads_dt_env(template), "{template}");
            assert_eq!(expand_script(template, &vars()), Err(ExpandError::EnvInLine), "{template}");
        }
        assert!(!reads_dt_env("build.cmd ${sha} %PATH%"));
        assert!(expand_script("build.cmd ${sha}", &vars()).is_ok());
    }

    #[test]
    fn the_line_cannot_read_cmd_s_own_command_line() {
        // `%CMDCMDLINE%` redonne la ligne brute de cmd, valeurs citées comprises : ses guillemets décalent ceux de la
        // ligne développée, et le `&` d'une valeur se retrouve hors guillemets.
        for template in [
            "echo %CMDCMDLINE% ${branch}",
            "echo %CMDCMDLINE:~0% ${branch}",
            "echo ${branch} %CMDCMDLINE%",
            "echo ${branch} & echo %cmdcmdline%",
            "echo !CmdCmdLine! ${sha}",
            "echo %CMDCMDLINE%",
        ] {
            assert!(reads_cmd_line(template), "{template}");
            assert_eq!(expand_script(template, &vars()), Err(ExpandError::CmdLineInLine), "{template}");
        }
        assert!(!reads_cmd_line("echo %DATE% ${branch}"));
        assert!(expand_script("echo %DATE% ${branch}", &vars()).is_ok());
    }

    #[test]
    fn a_trailing_backslash_does_not_escape_the_closing_quote() {
        let root = Vars { repo: r"C:\".into(), ..vars() };
        assert_eq!(script("cd ${repo}", &root), r#"cd "C:\\""#);
        assert_eq!(script(r#"cd "${repo}""#, &root), r#"cd "C:\\""#);
        assert_eq!(script(r#"cd "${repo}x""#, &root), r#"cd "C:\x""#, "pas de guillemet derrière : rien à doubler");
    }

    #[test]
    fn control_characters_and_a_caret_before_a_variable_are_refused() {
        assert_eq!(expand_script("type ${file}", &with_file("a\nb")), Err(ExpandError::ControlChar(Var::File)));
        assert_eq!(expand_script("x ${branch}", &with_branch("a\rb")), Err(ExpandError::ControlChar(Var::Branch)));
        assert_eq!(expand_script("echo ^${branch}", &vars()), Err(ExpandError::CaretBeforeVar(Var::Branch)));
        assert_eq!(script("echo ^^${branch}", &vars()), "echo ^^feature/x", "`^^` : le second est échappé");
        // Valeur sans objet pour la commande : un retour à la ligne ailleurs n'empêche rien.
        assert_eq!(script("echo ${sha}", &with_file("a\nb")), "echo 0123456789abcdef0123456789abcdef01234567");
    }

    /// Argument tel que `git rev-parse --sq-quote` le répète (`'` → `'\''`, `!` → `'\!'`).
    #[cfg(windows)]
    fn sq_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', r"'\''").replace('!', r"'\!'"))
    }

    /// Valeurs hostiles passées par un vrai `cmd.exe` : aucune commande glissée dans une valeur ne s'exécute, et le
    /// programme (git, qui lit ses arguments avec les anciennes règles de msvcrt) reçoit l'argument attendu, intact :
    /// une valeur seule, deux valeurs voisines, entre ou hors des guillemets de l'utilisateur, avec un guillemet de
    /// l'utilisateur non refermé, dans un bloc entre parenthèses. Une valeur avec `"` est refusée.
    #[cfg(windows)]
    #[test]
    fn hostile_values_reach_the_program_intact_through_cmd() {
        use dragontree_git::CancelToken;
        use dragontree_git::shell::run_shell;

        let git = crate::test_git();
        let dir = tempfile::tempdir().unwrap();
        let pwned = dir.path().join("pwned.txt");
        let hostile = [
            "a&echo PWNED>pwned.txt",
            "a|echo PWNED>pwned.txt",
            "(a) & (echo PWNED>pwned.txt)",
            "a) & echo PWNED>pwned.txt & (b",
            "^&^",
            "!PATH!",
            "a b",
            "-x --output=pwned.txt",
            r"C:\dossier\",
            r"\\serveur\partage\",
            r"feature\x",
            "é ü",
            "",
        ];
        let exe = git.exe().display().to_string();
        let sq = |template: &str| format!(r#""{exe}" rev-parse --sq-quote {template}"#);
        // Modèle, argument attendu à partir des valeurs de `${branch}` et `${file}`.
        type Expected = fn(&str, &str) -> String;
        let shapes: [(String, Expected); 8] = [
            (sq("${branch}"), |b, _| b.to_string()),
            (sq(r#""${branch}""#), |b, _| b.to_string()),
            (sq("${branch}${file}"), |b, f| format!("{b}{f}")),
            (sq(r#""${branch}${file}""#), |b, f| format!("{b}{f}")),
            (sq(r#""${branch}"${file}"#), |b, f| format!("{b}{f}")),
            (sq(r#"${branch}"x y""#), |b, _| format!("{b}x y")),
            (sq(r#""${branch}"#), |b, _| b.to_string()),
            (format!("({})", sq("${branch}")), |b, _| b.to_string()),
        ];
        for (template, expected) in &shapes {
            for (i, branch) in hostile.iter().enumerate() {
                let file = hostile[(i + 4) % hostile.len()];
                let vars = Vars { branch: branch.to_string(), file: file.to_string(), ..vars() };
                let line = expand_script(template, &vars).unwrap_or_else(|e| panic!("{template} : {e:?}"));
                let out = run_shell(dir.path(), &line, &[], &CancelToken::new()).unwrap();
                assert!(!pwned.exists(), "commande injectée par {branch:?}/{file:?} : {line}");
                assert_eq!(out.code, 0, "{line}\n{}", out.output);
                assert_eq!(out.output.trim(), sq_quote(&expected(branch, file)), "{line}");
            }
        }
        // Guillemet de l'utilisateur ouvert après la valeur : tout ce qui suit reste entre guillemets.
        for branch in hostile {
            let line = expand_script(&sq(r#"${branch}""#), &with_branch(branch)).unwrap();
            run_shell(dir.path(), &line, &[], &CancelToken::new()).unwrap();
            assert!(!pwned.exists(), "commande injectée par {branch:?} : {line}");
        }
        // De part et d'autre d'un tube, cmd relit la ligne dans un autre interpréteur : les guillemets tiennent.
        for branch in hostile {
            let line = expand_script(&format!("{} | sort", sq("${branch}")), &with_branch(branch)).unwrap();
            run_shell(dir.path(), &line, &[], &CancelToken::new()).unwrap();
            assert!(!pwned.exists(), "commande injectée par {branch:?} à travers un tube : {line}");
        }
        // Une valeur avec `"` ou `%` n'arrive jamais à cmd.
        for quoted in [r#"x" & echo PWNED>pwned.txt & ""#, r#"\"&echo PWNED>pwned.txt&\"#, r#"x" --output=pwned.txt"#] {
            assert_eq!(expand_script(&sq("${branch}"), &with_branch(quoted)), Err(ExpandError::Quote(Var::Branch)));
        }
        for percent in ["%PATH%", "%%", "%DT_FILE%"] {
            assert_eq!(expand_script(&sq("${branch}"), &with_branch(percent)), Err(ExpandError::Percent(Var::Branch)));
        }

        // `%` du modèle et valeur qui nomme une variable DT_… : refusé avant que cmd ne puisse la développer. Ce que
        // ferait la ligne si elle partait : `DT_FILE` contient une commande.
        let run = |template: &str, values: &Vars| {
            let command = CustomCommand { command: template.into(), ..Default::default() };
            if let Ok(prepared) = command.prepare(values) {
                let mut env = prepared.env.clone();
                env.push(("DT_FILE".into(), Some(r#"x" & echo PWNED>pwned.txt & ""#.into())));
                run_shell(dir.path(), &prepared.text, &env, &CancelToken::new()).unwrap();
            }
            assert!(!pwned.exists(), "commande injectée : {template} avec {values:?}");
        };
        run("echo %${branch}%", &with_branch("DT_FILE"));
        run(r#"echo "%${branch}""#, &with_branch("DT_FILE%"));
        run(r#"echo "%NOPE%${branch}""#, &with_branch("DT_FILE%"));
        run("echo ${branch} | sort", &with_branch("%DT_FILE%"));
        run("echo %D${branch}E%", &with_branch("T_FIL"));

        // `%` du modèle de part et d'autre d'une valeur : cmd lit la valeur dans un nom `%…%`. La substitution
        // `%XV:a= … %` avec XV = `a"a` recopie la valeur et ses guillemets de part et d'autre du guillemet de XV :
        // le `&` de la valeur se retrouve hors des guillemets. Refusé avant de partir, que le nom s'ouvre après un nom
        // non défini (`%NOPE%XV:…`) ou de l'autre côté d'un `&`.
        for template in ["echo %XV:a= ${branch} %", "echo %NOPE%XV:a= ${branch} %", "echo %XV:a= & echo ${branch} %"] {
            let command = CustomCommand { command: template.into(), ..Default::default() };
            if let Ok(prepared) = command.prepare(&with_branch("a&echo PWNED>pwned.txt")) {
                let mut env = prepared.env.clone();
                env.push(("XV".into(), Some(r#"a"a"#.into())));
                run_shell(dir.path(), &prepared.text, &env, &CancelToken::new()).unwrap();
            }
            assert!(!pwned.exists(), "commande injectée par la substitution de {template}");
        }

        // `%CMDCMDLINE%` : la ligne brute de cmd, avec la valeur et le guillemet ouvert après `/C`, recopiée dans la
        // ligne développée. Le guillemet en trop laisse le `&` de la valeur hors guillemets ; `&rem` neutralise la fin.
        for template in ["echo %CMDCMDLINE% ${branch}", "echo %CMDCMDLINE:~0% ${branch}", "echo ${branch} %CMDCMDLINE%"]
        {
            let command = CustomCommand { command: template.into(), ..Default::default() };
            if let Ok(prepared) = command.prepare(&with_branch("a&echo PWNED>pwned.txt&rem")) {
                run_shell(dir.path(), &prepared.text, &prepared.env, &CancelToken::new()).unwrap();
            }
            assert!(!pwned.exists(), "commande injectée par la ligne de cmd recopiée : {template}");
        }

        // Un script `.cmd` qui lit "%DT_FILE%" : la variable n'est définie que pour une valeur acceptée (sans `"` ni
        // `%`), qui reste alors entre ses guillemets.
        std::fs::write(dir.path().join("lire.cmd"), "@echo off\r\necho \"%DT_FILE%\"> lu.txt\r\n").unwrap();
        // Chemin explicite : cmd ne cherche pas dans le dossier courant si NoDefaultCurrentDirectoryInExePath est défini.
        let script_command = CustomCommand { command: r".\lire.cmd".into(), ..Default::default() };
        for (file, expected) in [
            (r#"x" & echo PWNED>pwned.txt & ""#, r#""""#),
            ("a & echo PWNED>pwned.txt", "\"a & echo PWNED>pwned.txt\""),
        ] {
            let prepared = script_command.prepare(&with_file(file)).unwrap();
            run_shell(dir.path(), &prepared.text, &prepared.env, &CancelToken::new()).unwrap();
            assert!(!pwned.exists(), "commande injectée par DT_FILE = {file:?}");
            let read = std::fs::read_to_string(dir.path().join("lu.txt")).unwrap();
            assert_eq!(read.trim(), expected, "{file:?}");
        }
    }

    #[test]
    fn unknown_or_unclosed_placeholders_are_left_as_they_are() {
        assert_eq!(script("echo ${foo} ${sha", &vars()), "echo ${foo} ${sha");
        assert_eq!(script("echo $${sha}}", &vars()), "echo $0123456789abcdef0123456789abcdef01234567}");
    }

    #[test]
    fn url_values_are_percent_encoded() {
        let url = |t: &str, v: &Vars| expand_url(t, v).unwrap_or_else(|e| panic!("{t} : {e:?}"));
        assert_eq!(
            url("https://forge.contoso.test/tree/${branch}", &with_branch("feature/x&y")),
            "https://forge.contoso.test/tree/feature%2Fx%26y"
        );
        assert_eq!(
            url("https://x.test/blob/${file}", &with_file("src/a b é.txt")),
            "https://x.test/blob/src/a%20b%20%C3%A9.txt"
        );
        assert_eq!(
            url("https://x.test/?q=${branch}", &with_branch("a?b#c%d+e")),
            "https://x.test/?q=a%3Fb%23c%25d%2Be"
        );
        assert_eq!(url("${remote_url}", &vars()), "https://forge.contoso.test/fabrikam.git");
        assert_eq!(
            url("mailto:dev@contoso.test?subject=${sha}", &vars()),
            format!("mailto:dev@contoso.test?subject={}", vars().sha)
        );
    }

    #[test]
    fn a_value_cannot_change_the_address_beyond_its_slot() {
        let url = |t: &str, v: &Vars| expand_url(t, v).unwrap_or_else(|e| panic!("{t} : {e:?}"));
        // Ni `@` (identifiants), ni `:` (port), ni `/` (chemin) : la valeur reste un seul segment.
        assert_eq!(
            url("https://forge.contoso.test/${branch}", &with_branch("x@evil.test:8080/y")),
            "https://forge.contoso.test/x%40evil.test%3A8080%2Fy"
        );
        assert_eq!(
            url("https://${sha}.contoso.test", &Vars { sha: "a/b@c".into(), ..vars() }),
            "https://a%2Fb%40c.contoso.test"
        );
        // `${file}` garde ses `/` (chemin dans le dépôt), pas ses `@` ni ses `:`.
        assert_eq!(url("https://x.test/blob/${file}", &with_file("a/b@c:d")), "https://x.test/blob/a/b%40c%3Ad");
        // Ailleurs qu'en tête, `${remote_url}` est une valeur comme les autres.
        assert_eq!(
            url("https://x.test/?remote=${remote_url}", &vars()),
            "https://x.test/?remote=https%3A%2F%2Fforge.contoso.test%2Ffabrikam.git"
        );
        // En tête, elle donne l'adresse entière (son schéma est vérifié), seuls ses caractères interdits encodés.
        let spaced = Vars { remote_url: "https://forge.contoso.test/mon dépôt.git".into(), ..vars() };
        assert_eq!(
            url("${remote_url}/tree/${branch}", &spaced),
            "https://forge.contoso.test/mon%20d%C3%A9p%C3%B4t.git/tree/feature%2Fx"
        );
    }

    #[test]
    fn only_web_and_mail_urls_are_opened() {
        let ssh = Vars { remote_url: "git@forge.contoso.test:fabrikam.git".into(), ..vars() };
        assert!(matches!(expand_url("${remote_url}", &ssh), Err(ExpandError::UrlScheme(_))));
        assert!(matches!(expand_url("file:///C:/${file}", &vars()), Err(ExpandError::UrlScheme(_))));
        // Une valeur ne choisit pas le schéma : ses `(` `)` sont encodés et le schéma obtenu est refusé.
        assert!(matches!(expand_url("${branch}", &with_branch("javascript:alert(1)")), Err(ExpandError::UrlScheme(_))));
        assert!(expand_url("HTTPS://x.test", &vars()).is_ok(), "schéma sans tenir compte de la casse");
    }

    #[test]
    fn commands_are_offered_for_their_target_only() {
        let cmd = |name: &str, target, command: &str| CustomCommand {
            name: name.into(),
            target,
            command: command.into(),
            ..Default::default()
        };
        let list = [
            cmd("Construire", CommandTarget::Repo, "build.cmd"),
            cmd("Copier le SHA", CommandTarget::Commit, "echo ${sha}"),
            cmd("", CommandTarget::Commit, "echo sans nom"),
            cmd("Sans ligne", CommandTarget::Commit, "  "),
            cmd("Ouvrir", CommandTarget::File, "notepad ${file}"),
            cmd("Journal", CommandTarget::Commit, "git show ${sha}"),
        ];
        let names = |target| for_target(&list, target).map(|(i, c)| (i, c.name.as_str())).collect::<Vec<_>>();
        assert_eq!(names(CommandTarget::Commit), [(1, "Copier le SHA"), (5, "Journal")]);
        assert_eq!(names(CommandTarget::Repo), [(0, "Construire")]);
        assert_eq!(names(CommandTarget::File), [(4, "Ouvrir")]);
        assert!(names(CommandTarget::Branch).is_empty());
    }

    #[test]
    fn a_command_with_missing_fields_still_loads() {
        let c: CustomCommand = serde_json::from_str(r#"{"name":"Build","command":"build.cmd"}"#).unwrap();
        assert_eq!((c.target, c.kind, c.confirm), (CommandTarget::Repo, CommandKind::Script, false));
    }
}
