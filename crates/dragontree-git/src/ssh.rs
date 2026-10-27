//! Clés SSH : lister les paires du dossier `.ssh`, en générer, et choisir celle que git utilise
//! (`core.sshCommand`). Le dossier `.ssh` et l'exécutable `ssh-keygen` sont des paramètres : les tests
//! n'utilisent que des dossiers temporaires.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::runner::{Git, GitCommand};

/// Type de clé généré par « Générer une clé… ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    Ed25519,
    /// RSA 4096 bits.
    Rsa4096,
}

impl KeyType {
    /// Nom de fichier proposé par défaut (convention d'OpenSSH).
    pub fn default_file_name(self) -> &'static str {
        match self {
            KeyType::Ed25519 => "id_ed25519",
            KeyType::Rsa4096 => "id_rsa",
        }
    }
}

/// Paire de clés `x` / `x.pub` du dossier `.ssh`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshKey {
    /// Nom du fichier de la clé privée (`id_ed25519`).
    pub name: String,
    pub private_path: PathBuf,
    pub public_path: PathBuf,
    /// Type affiché par ssh-keygen (`ED25519`, `RSA`…).
    pub key_type: String,
    pub bits: u32,
    pub comment: String,
    /// Empreinte (`SHA256:…`).
    pub fingerprint: String,
    /// Contenu de la clé publique (une ligne), à coller chez le service git.
    pub public_key: String,
}

/// Où écrire `core.sshCommand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigScope {
    /// `git config --global` (respecte `GIT_CONFIG_GLOBAL`).
    Global,
    /// Le dépôt du dossier de travail passé aux fonctions.
    Local,
}

impl ConfigScope {
    pub(crate) fn flag(self) -> &'static str {
        match self {
            ConfigScope::Global => "--global",
            ConfigScope::Local => "--local",
        }
    }
}

/// `ssh-keygen` de Git pour Windows (`<racine>\usr\bin\ssh-keygen.exe`), déduit du chemin de `git.exe`
/// (`<racine>\cmd`, `<racine>\bin` ou `<racine>\mingw64\bin`) ; sinon celui du PATH.
pub fn find_ssh_keygen(git_exe: &Path) -> PathBuf {
    let path_var = std::env::var_os("PATH");
    let from_git = crate::runner::git_install_roots(git_exe, path_var.as_deref(), |p| p.is_file())
        .into_iter()
        .map(|root| root.join("usr").join("bin").join("ssh-keygen.exe"))
        .find(|candidate| candidate.is_file());
    from_git.unwrap_or_else(|| PathBuf::from("ssh-keygen"))
}

fn keygen_failure(e: std::io::Error) -> GitError {
    match e.kind() {
        std::io::ErrorKind::NotFound => GitError::Refused(
            tr!("ssh-keygen est introuvable : installez Git pour Windows ou ajoutez OpenSSH au PATH.").to_string(),
        ),
        _ => GitError::Spawn(format!("ssh-keygen : {e}")),
    }
}

/// Lance `ssh-keygen` sans console : il ne peut jamais attendre une saisie. `input` (la phrase secrète, deux fois) est
/// écrit sur son entrée, qu'il lit faute de terminal ; sans `input`, l'entrée est vide.
fn run_keygen(keygen: &Path, args: &[&OsStr], input: Option<&str>) -> Result<String, GitError> {
    use std::io::Write;
    let mut command = Command::new(keygen);
    command.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // Jamais de programme « askpass » : la phrase vient de l'entrée.
    command.env("SSH_ASKPASS_REQUIRE", "never").env_remove("DISPLAY");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(keygen_failure)?;
    if let Some(mut stdin) = child.stdin.take() {
        // Une erreur d'écriture (ssh-keygen déjà arrêté) se lit dans son code de sortie.
        let _ = stdin.write_all(input.unwrap_or_default().as_bytes());
    }
    let out = child.wait_with_output().map_err(keygen_failure)?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    // La ligne de commande n'est pas reprise : elle peut contenir la phrase secrète.
    Err(GitError::Failed {
        cmd: "ssh-keygen".into(),
        code: out.status.code().unwrap_or(-1),
        stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
    })
}

/// Analyse une ligne de `ssh-keygen -lf` : `256 SHA256:xxx un commentaire (ED25519)`.
/// Renvoie (bits, empreinte, commentaire, type).
pub fn parse_fingerprint_line(line: &str) -> Option<(u32, String, String, String)> {
    let (bits, rest) = line.trim().split_once(' ')?;
    let (fingerprint, rest) = rest.trim_start().split_once(' ')?;
    let rest = rest.trim().strip_suffix(')')?;
    // Le type est entre la dernière parenthèse ouvrante et la fin ; le commentaire peut en contenir.
    let open = rest.rfind('(')?;
    let (comment, key_type) = (rest[..open].trim(), &rest[open + 1..]);
    Some((bits.parse().ok()?, fingerprint.to_string(), comment.to_string(), key_type.to_string()))
}

/// Lit une paire : empreinte via `ssh-keygen -lf`, clé publique depuis le fichier `.pub`.
fn read_key(keygen: &Path, private_path: &Path) -> Result<SshKey, GitError> {
    let name = private_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let public_path = PathBuf::from(format!("{}.pub", private_path.display()));
    let line = run_keygen(keygen, &[OsStr::new("-l"), OsStr::new("-f"), public_path.as_os_str()], None)?;
    let (bits, fingerprint, comment, key_type) = parse_fingerprint_line(line.lines().next().unwrap_or_default())
        .ok_or_else(|| GitError::Parse(format!("ssh-keygen -lf : {}", line.trim())))?;
    let public_key = std::fs::read_to_string(&public_path).map_err(|e| GitError::Io(e.to_string()))?.trim().to_string();
    Ok(SshKey {
        name,
        private_path: private_path.to_path_buf(),
        public_path,
        key_type,
        bits,
        comment,
        fingerprint,
        public_key,
    })
}

/// Paires `x` / `x.pub` du dossier (une clé publique sans clé privée est ignorée), triées par nom. Un dossier
/// absent donne une liste vide ; un fichier `.pub` illisible par ssh-keygen est ignoré.
pub fn list_keys(ssh_dir: &Path, keygen: &Path) -> Result<Vec<SshKey>, GitError> {
    let entries = match std::fs::read_dir(ssh_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(GitError::Io(e.to_string())),
    };
    let mut privates: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "pub"))
        .map(|p| p.with_extension(""))
        .filter(|p| p.is_file())
        .collect();
    privates.sort();
    let mut keys = Vec::new();
    for private in privates {
        match read_key(keygen, &private) {
            Ok(key) => keys.push(key),
            // ssh-keygen introuvable : inutile d'insister sur chaque clé.
            Err(e @ (GitError::Refused(_) | GitError::Spawn(_))) => return Err(e),
            Err(_) => {}
        }
    }
    Ok(keys)
}

/// Nom de fichier acceptable pour une nouvelle clé : un simple nom, sans dossier ni caractère interdit.
fn valid_file_name(name: &str) -> bool {
    let forbidden = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\'', '$', '`'];
    !name.is_empty()
        && name.trim() == name
        && name != "."
        && name != ".."
        && !name.ends_with('.')
        && !name.ends_with(".pub")
        && !name.contains(forbidden)
        && !name.chars().any(char::is_control)
}

/// Génère une paire dans `ssh_dir` (créé au besoin). Refuse d'écraser un fichier existant. La phrase secrète
/// (vide : aucune) est écrite sur l'entrée de `ssh-keygen`, jamais passée en argument.
pub fn generate_key(
    ssh_dir: &Path,
    keygen: &Path,
    name: &str,
    key_type: KeyType,
    comment: &str,
    passphrase: &str,
) -> Result<SshKey, GitError> {
    if !valid_file_name(name) {
        return Err(GitError::Refused(tr!(
            "« {name} » n'est pas un nom de fichier valide pour une clé (pas de dossier ni de caractère spécial).",
            name
        )));
    }
    let private = ssh_dir.join(name);
    let public = ssh_dir.join(format!("{name}.pub"));
    if private.exists() || public.exists() {
        return Err(GitError::Refused(tr!(
            "Le fichier « {name} » existe déjà dans le dossier .ssh : DragonTree n'écrase jamais une clé.",
            name
        )));
    }
    std::fs::create_dir_all(ssh_dir).map_err(|e| GitError::Io(e.to_string()))?;
    let o = OsStr::new;
    let mut args = vec![o("-q"), o("-t")];
    match key_type {
        KeyType::Ed25519 => args.push(o("ed25519")),
        KeyType::Rsa4096 => args.extend([o("rsa"), o("-b"), o("4096")]),
    }
    args.extend([o("-f"), private.as_os_str(), o("-C"), o(comment)]);
    // La ligne de commande d'un processus est consignée par l'audit de Windows (événement 4688), Sysmon ou un EDR.
    let input = if passphrase.is_empty() {
        args.extend([o("-N"), o("")]);
        None
    } else if passphrase.contains(['\r', '\n']) {
        return Err(GitError::Refused(tr!("La phrase secrète ne peut pas contenir de retour à la ligne.").to_string()));
    } else {
        Some(format!("{passphrase}\n{passphrase}\n"))
    };
    run_keygen(keygen, &args, input.as_deref())?;
    read_key(keygen, &private)
}

/// Valeur de `core.sshCommand` pour utiliser `key` : `ssh -i "<clé>" -o IdentitiesOnly=yes`. Le chemin utilise
/// des `/` (git passe la commande à sh) et est entre guillemets pour supporter les espaces.
pub fn ssh_command_for(key: &Path) -> String {
    let path = key.to_string_lossy().replace('\\', "/");
    let mut quoted = String::with_capacity(path.len() + 2);
    quoted.push('"');
    for c in path.chars() {
        if matches!(c, '"' | '$' | '`' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    format!("ssh -i {quoted} -o IdentitiesOnly=yes")
}

/// Chemin de la clé d'un `core.sshCommand` de la forme produite par [`ssh_command_for`] ; `None` pour toute
/// autre commande.
pub fn key_from_ssh_command(command: &str) -> Option<String> {
    let rest = command.trim().strip_prefix("ssh -i ")?.strip_prefix('"')?;
    let mut path = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(path),
            '\\' => path.push(chars.next()?),
            c => path.push(c),
        }
    }
    None
}

/// Les deux chemins désignent la même clé (séparateurs et, sous Windows, casse ignorés).
pub fn same_key_path(key: &Path, configured: &str) -> bool {
    let norm = |s: &str| {
        let s = s.replace('\\', "/");
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    norm(&key.to_string_lossy()) == norm(configured)
}

fn config_cmd(git: &Git, scope: ConfigScope, cwd: &Path) -> GitCommand {
    git.cmd(cwd).arg("config").arg(scope.flag())
}

/// `core.sshCommand` de la portée donnée, s'il y en a un.
pub fn configured_ssh_command(git: &Git, scope: ConfigScope, cwd: &Path) -> Result<Option<String>, GitError> {
    let out = config_cmd(git, scope, cwd).args(["--get", "core.sshCommand"]).allow_codes(&[1]).run()?;
    let value = out.stdout_str().trim().to_string();
    Ok((!value.is_empty()).then_some(value))
}

/// Valeur de `core.sshCommand` qui n'a pas été écrite par DragonTree (écrite à la main, par exemple avec
/// `StrictHostKeyChecking`) : l'écraser ou la retirer demande une confirmation explicite.
pub fn foreign_ssh_command(git: &Git, scope: ConfigScope, cwd: &Path) -> Result<Option<String>, GitError> {
    Ok(configured_ssh_command(git, scope, cwd)?.filter(|value| key_from_ssh_command(value).is_none()))
}

/// Refuse de toucher une valeur étrangère sans `confirmed`.
fn guard_foreign(git: &Git, scope: ConfigScope, cwd: &Path, confirmed: bool) -> Result<(), GitError> {
    match foreign_ssh_command(git, scope, cwd)? {
        Some(value) if !confirmed => Err(GitError::Refused(tr!(
            "core.sshCommand contient une commande qui n'a pas été écrite par DragonTree : {value}",
            value
        ))),
        _ => Ok(()),
    }
}

/// Écrit `core.sshCommand` pour utiliser `key`. Une valeur étrangère n'est remplacée que si `confirmed`.
pub fn use_key(git: &Git, scope: ConfigScope, cwd: &Path, key: &Path, confirmed: bool) -> Result<(), GitError> {
    guard_foreign(git, scope, cwd, confirmed)?;
    config_cmd(git, scope, cwd).args(["core.sshCommand".to_string(), ssh_command_for(key)]).run().map(|_| ())
}

/// Retire `core.sshCommand` (configuration par défaut) ; sans effet s'il n'est pas défini. Une valeur étrangère
/// n'est retirée que si `confirmed`.
pub fn reset_key(git: &Git, scope: ConfigScope, cwd: &Path, confirmed: bool) -> Result<(), GitError> {
    guard_foreign(git, scope, cwd, confirmed)?;
    config_cmd(git, scope, cwd).args(["--unset-all", "core.sshCommand"]).allow_codes(&[5]).run().map(|_| ())
}

/// E-mail git global : commentaire proposé pour une nouvelle clé.
pub fn global_email(git: &Git, cwd: &Path) -> Option<String> {
    let out = config_cmd(git, ConfigScope::Global, cwd).args(["--get", "user.email"]).allow_codes(&[1]).run().ok()?;
    Some(out.stdout_str().trim().to_string()).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_quotes_a_path_with_spaces_and_uses_forward_slashes() {
        let key = Path::new(r"C:\Users\Contoso User\.ssh\id_ed25519");
        assert_eq!(ssh_command_for(key), r#"ssh -i "C:/Users/Contoso User/.ssh/id_ed25519" -o IdentitiesOnly=yes"#);
    }

    #[test]
    fn command_escapes_shell_characters_and_round_trips() {
        let key = Path::new(r"C:\a$b\k`1\id");
        let command = ssh_command_for(key);
        assert_eq!(command, r#"ssh -i "C:/a\$b/k\`1/id" -o IdentitiesOnly=yes"#);
        assert_eq!(key_from_ssh_command(&command).as_deref(), Some("C:/a$b/k`1/id"));
        assert!(same_key_path(key, "C:/a$b/k`1/id"));
    }

    #[test]
    fn foreign_ssh_commands_are_not_recognized() {
        assert_eq!(key_from_ssh_command("ssh -o StrictHostKeyChecking=no"), None);
        assert_eq!(key_from_ssh_command(r#"ssh -i "non terminé"#), None);
    }

    #[test]
    fn fingerprint_line_is_parsed_even_with_a_comment_containing_spaces_and_parentheses() {
        let parsed = parse_fingerprint_line("256 SHA256:AbC+/12= jean (poste) fabrikam (ED25519)\n").unwrap();
        assert_eq!(parsed, (256, "SHA256:AbC+/12=".into(), "jean (poste) fabrikam".into(), "ED25519".into()));
        let no_comment = parse_fingerprint_line("4096 SHA256:xyz  (RSA)").unwrap();
        assert_eq!((no_comment.0, no_comment.2.as_str(), no_comment.3.as_str()), (4096, "", "RSA"));
        assert_eq!(parse_fingerprint_line("n'importe quoi"), None);
    }

    #[test]
    fn file_names_cannot_escape_the_ssh_folder() {
        for bad in ["", "..", "a/b", r"a\b", "id.pub", " id", "id.", "a\"b", "x$y"] {
            assert!(!valid_file_name(bad), "{bad:?} devrait être refusé");
        }
        assert!(valid_file_name("id_ed25519") && valid_file_name("travail fabrikam"));
    }
}
