//! « Ouvrir un terminal » : choix du terminal (Réglages › Terminal) et construction de la commande de lancement.
//!
//! Le terminal est lancé directement (programme et arguments séparés, échappement de Rust), jamais par une ligne de
//! commande construite à partir du chemin : le dossier passe par `current_dir`, sauf pour Windows Terminal qui
//! n'accepte que `-d` (où `;` doit être échappé : il y sépare les commandes). La détection ne fait que quelques
//! tests d'existence de fichier : elle est faite au clic.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use dragontree_git::runner::git_install_roots;
use dragontree_i18n::tr;
use serde::{Deserialize, Serialize};

/// Terminal choisi dans les réglages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalChoice {
    /// Windows Terminal s'il est installé, sinon Windows PowerShell (comportement historique).
    #[default]
    Auto,
    WindowsTerminal,
    /// `pwsh` (PowerShell 7) s'il est installé, sinon Windows PowerShell.
    PowerShell,
    /// Déduit du chemin de git (`git-bash.exe`, sinon `bin\bash.exe`).
    GitBash,
    Cmd,
}

impl TerminalChoice {
    pub const ALL: [TerminalChoice; 5] = [
        TerminalChoice::Auto,
        TerminalChoice::WindowsTerminal,
        TerminalChoice::PowerShell,
        TerminalChoice::GitBash,
        TerminalChoice::Cmd,
    ];

    /// Nom affiché (les noms de produits ne se traduisent pas).
    pub fn label(self) -> &'static str {
        match self {
            TerminalChoice::Auto => tr!("Automatique"),
            TerminalChoice::WindowsTerminal => "Windows Terminal",
            TerminalChoice::PowerShell => "PowerShell",
            TerminalChoice::GitBash => "Git Bash",
            TerminalChoice::Cmd => tr!("Invite de commandes"),
        }
    }
}

/// Programmes trouvés sur la machine (`None` : absent).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tools {
    pub wt: Option<PathBuf>,
    pub pwsh: Option<PathBuf>,
    /// `git-bash.exe` ou `bin\bash.exe` déduit du chemin de git.
    pub git_bash: Option<GitBash>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitBash {
    /// `git-bash.exe` : ouvre sa propre fenêtre, dans le dossier courant.
    Launcher(PathBuf),
    /// `bin\bash.exe --login -i` dans une nouvelle console.
    Bash(PathBuf),
}

/// Commande de lancement prête à être exécutée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub current_dir: Option<PathBuf>,
    /// Programme console (PowerShell, cmd, bash) : à ouvrir dans sa propre console.
    pub new_console: bool,
}

impl Launch {
    fn new(program: impl Into<OsString>, args: &[&str], dir: Option<&Path>, new_console: bool) -> Launch {
        Launch {
            program: program.into(),
            args: args.iter().map(OsString::from).collect(),
            current_dir: dir.map(Path::to_path_buf),
            new_console,
        }
    }

    pub fn spawn(&self) -> std::io::Result<()> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        if let Some(dir) = &self.current_dir {
            cmd.current_dir(dir);
        }
        #[cfg(windows)]
        if self.new_console {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
            cmd.creation_flags(CREATE_NEW_CONSOLE);
        }
        cmd.spawn().map(|_| ())
    }
}

/// Chemin pour `wt.exe -d` : Windows Terminal lit `;` comme un séparateur de commandes, il s'écrit `\;`.
pub fn wt_directory_arg(dir: &Path) -> OsString {
    OsString::from(crate::util::native_path(dir).to_string_lossy().replace(';', "\\;"))
}

/// `git-bash.exe` (sinon `bin\bash.exe`) de l'installation de Git qui contient `git_exe` (voir
/// [`git_install_roots`] : un `git` seul est résolu par `path_var`). `exists` isole le disque pour les tests.
pub fn git_bash_from_git(
    git_exe: &Path,
    path_var: Option<&std::ffi::OsStr>,
    exists: impl Fn(&Path) -> bool,
) -> Option<GitBash> {
    git_install_roots(git_exe, path_var, &exists).into_iter().find_map(|root| {
        let launcher = root.join("git-bash.exe");
        let bash = root.join("bin").join("bash.exe");
        if exists(&launcher) {
            Some(GitBash::Launcher(launcher))
        } else if exists(&bash) {
            Some(GitBash::Bash(bash))
        } else {
            None
        }
    })
}

fn find_in_path(exe: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).map(|dir| dir.join(exe)).find(|p| p.is_file())
}

/// Détection sur la machine (quelques tests d'existence de fichier).
pub fn detect(git_exe: Option<&Path>) -> Tools {
    let wt = dirs::data_local_dir()
        .map(|d| d.join("Microsoft").join("WindowsApps").join("wt.exe"))
        .filter(|p| p.is_file())
        .or_else(|| find_in_path("wt.exe"));
    let program_files =
        std::env::var_os("ProgramFiles").map(|p| PathBuf::from(p).join("PowerShell").join("7").join("pwsh.exe"));
    let pwsh = find_in_path("pwsh.exe").or_else(|| program_files.filter(|p| p.is_file()));
    Tools {
        wt,
        pwsh,
        git_bash: git_exe.and_then(|g| git_bash_from_git(g, std::env::var_os("PATH").as_deref(), |p| p.is_file())),
    }
}

/// Windows PowerShell, présent sur toute installation de Windows.
fn windows_powershell(dir: &Path) -> Launch {
    Launch::new("powershell.exe", &["-NoExit"], Some(dir), true)
}

/// Ordre de repli de « Automatique » : Windows Terminal, pwsh, Windows PowerShell, invite de commandes.
pub fn auto_chain(dir: &Path, tools: &Tools) -> Vec<Launch> {
    let wt = tools.wt.as_ref().and_then(|_| build(TerminalChoice::WindowsTerminal, dir, tools));
    let pwsh = tools.pwsh.as_ref().and_then(|_| build(TerminalChoice::PowerShell, dir, tools));
    wt.into_iter().chain(pwsh).chain([windows_powershell(dir)]).chain(build(TerminalChoice::Cmd, dir, tools)).collect()
}

/// Commande pour le choix demandé, ou `None` si son programme est absent (l'appelant se replie sur « Automatique »).
pub fn build(choice: TerminalChoice, dir: &Path, tools: &Tools) -> Option<Launch> {
    match choice {
        TerminalChoice::Auto => auto_chain(dir, tools).into_iter().next(),
        TerminalChoice::WindowsTerminal => {
            let wt = tools.wt.as_ref()?;
            Some(Launch {
                program: wt.clone().into(),
                args: vec!["-d".into(), wt_directory_arg(dir)],
                current_dir: None,
                new_console: false,
            })
        }
        TerminalChoice::PowerShell => Some(match &tools.pwsh {
            Some(pwsh) => Launch::new(pwsh, &["-NoExit"], Some(dir), true),
            None => windows_powershell(dir),
        }),
        TerminalChoice::GitBash => Some(match tools.git_bash.as_ref()? {
            GitBash::Launcher(exe) => Launch::new(exe, &[], Some(dir), false),
            GitBash::Bash(exe) => Launch::new(exe, &["--login", "-i"], Some(dir), true),
        }),
        // Le dossier passe par `current_dir` : aucun chemin dans la ligne de commande que cmd.exe interpréterait.
        TerminalChoice::Cmd => Some(Launch::new("cmd.exe", &["/K"], Some(dir), true)),
    }
}

/// Pourquoi le terminal choisi n'a pas été utilisé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// Programme absent de la machine.
    Missing,
    /// Programme trouvé mais lancement impossible.
    Failed,
}

/// Ouvre un terminal dans `dir`. `Ok(Some((choix, raison)))` : le choix était indisponible et un terminal de
/// repli (voir [`auto_chain`]) s'est ouvert à sa place ; `Err` : aucun terminal n'a pu être lancé (dernière erreur).
pub fn open(
    choice: TerminalChoice,
    dir: &Path,
    git_exe: Option<&Path>,
) -> Result<Option<(TerminalChoice, Unavailable)>, String> {
    let tools = detect(git_exe);
    let mut unavailable = None;
    if choice != TerminalChoice::Auto {
        match build(choice, dir, &tools) {
            None => unavailable = Some((choice, Unavailable::Missing)),
            Some(launch) => match launch.spawn() {
                Ok(()) => return Ok(None),
                Err(_) => unavailable = Some((choice, Unavailable::Failed)),
            },
        }
    }
    let mut last_error = String::new();
    for launch in auto_chain(dir, &tools) {
        match launch.spawn() {
            Ok(()) => return Ok(unavailable),
            Err(e) => last_error = format!("{} : {e}", launch.program.to_string_lossy()),
        }
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AWKWARD: [&str; 4] = [r"C:\DEV\Mon Projet", r"C:\DEV\a;b", r"C:\DEV\a&b", r"C:\DEV\100%done"];

    fn tools() -> Tools {
        Tools {
            wt: Some(PathBuf::from(r"C:\WindowsApps\wt.exe")),
            pwsh: Some(PathBuf::from(r"C:\PS7\pwsh.exe")),
            git_bash: Some(GitBash::Launcher(PathBuf::from(r"C:\Git\git-bash.exe"))),
        }
    }

    #[test]
    fn windows_terminal_escapes_semicolons_and_keeps_other_characters() {
        assert_eq!(wt_directory_arg(Path::new(r"C:\DEV\a;b;c")), OsString::from(r"C:\DEV\a\;b\;c"));
        for dir in AWKWARD {
            let launch = build(TerminalChoice::WindowsTerminal, Path::new(dir), &tools()).unwrap();
            assert_eq!(launch.program, OsString::from(r"C:\WindowsApps\wt.exe"));
            assert_eq!(launch.args[0], OsString::from("-d"));
            assert_eq!(launch.args[1], OsString::from(dir.replace(';', "\\;")));
            assert_eq!(launch.args.len(), 2);
            assert_eq!(launch.current_dir, None);
        }
    }

    #[test]
    fn console_terminals_get_the_directory_through_current_dir_only() {
        for dir in AWKWARD {
            let dir = Path::new(dir);
            let cmd = build(TerminalChoice::Cmd, dir, &tools()).unwrap();
            assert_eq!((cmd.program, cmd.args), (OsString::from("cmd.exe"), vec![OsString::from("/K")]));
            assert_eq!(cmd.current_dir.as_deref(), Some(dir));
            assert!(cmd.new_console);

            let ps = build(TerminalChoice::PowerShell, dir, &tools()).unwrap();
            assert_eq!((ps.program, ps.args), (OsString::from(r"C:\PS7\pwsh.exe"), vec![OsString::from("-NoExit")]));
            assert_eq!(ps.current_dir.as_deref(), Some(dir));

            let bash = build(TerminalChoice::GitBash, dir, &tools()).unwrap();
            assert_eq!((bash.program, bash.args.len()), (OsString::from(r"C:\Git\git-bash.exe"), 0));
            assert_eq!(bash.current_dir.as_deref(), Some(dir));
        }
    }

    #[test]
    fn powershell_falls_back_to_windows_powershell_without_pwsh() {
        let none = Tools { pwsh: None, ..tools() };
        let ps = build(TerminalChoice::PowerShell, Path::new(r"C:\x"), &none).unwrap();
        assert_eq!((ps.program, ps.args), (OsString::from("powershell.exe"), vec![OsString::from("-NoExit")]));
    }

    #[test]
    fn git_bash_without_launcher_uses_bash_with_login_shell() {
        let t = Tools { git_bash: Some(GitBash::Bash(PathBuf::from(r"C:\Git\bin\bash.exe"))), ..tools() };
        let bash = build(TerminalChoice::GitBash, Path::new(r"C:\x"), &t).unwrap();
        assert_eq!(bash.program, OsString::from(r"C:\Git\bin\bash.exe"));
        assert_eq!(bash.args, vec![OsString::from("--login"), OsString::from("-i")]);
        assert!(bash.new_console);
    }

    #[test]
    fn missing_programs_give_no_command_and_auto_always_has_one() {
        let nothing = Tools::default();
        assert_eq!(build(TerminalChoice::WindowsTerminal, Path::new(r"C:\x"), &nothing), None);
        assert_eq!(build(TerminalChoice::GitBash, Path::new(r"C:\x"), &nothing), None);
        let auto = build(TerminalChoice::Auto, Path::new(r"C:\x"), &nothing).unwrap();
        assert_eq!(auto.program, OsString::from("powershell.exe"));
        let auto = build(TerminalChoice::Auto, Path::new(r"C:\x"), &tools()).unwrap();
        assert_eq!(auto.program, OsString::from(r"C:\WindowsApps\wt.exe"));
    }

    #[test]
    fn git_bash_is_derived_from_the_git_path() {
        let existing = [r"C:\Git\git-bash.exe", r"C:\Old\bin\bash.exe"];
        let exists = |p: &Path| existing.iter().any(|e| Path::new(e) == p);
        for git in [r"C:\Git\cmd\git.exe", r"C:\Git\mingw64\bin\git.exe", r"C:\Git\bin\git.exe"] {
            assert_eq!(
                git_bash_from_git(Path::new(git), None, exists),
                Some(GitBash::Launcher(PathBuf::from(r"C:\Git\git-bash.exe"))),
                "{git}"
            );
        }
        assert_eq!(
            git_bash_from_git(Path::new(r"C:\Old\cmd\git.exe"), None, exists),
            Some(GitBash::Bash(PathBuf::from(r"C:\Old\bin\bash.exe")))
        );
        assert_eq!(git_bash_from_git(Path::new(r"C:\Other\cmd\git.exe"), None, exists), None);
    }

    #[test]
    fn a_bare_git_is_resolved_through_the_path_before_looking_for_git_bash() {
        let existing = [r"C:\Git\cmd\git.exe", r"C:\Git\git-bash.exe"];
        let exists = |p: &Path| existing.iter().any(|e| Path::new(e) == p);
        let path = std::env::join_paths([r"C:\Windows", r"C:\Git\cmd"]).unwrap();
        assert_eq!(
            git_bash_from_git(Path::new("git"), Some(&path), exists),
            Some(GitBash::Launcher(PathBuf::from(r"C:\Git\git-bash.exe")))
        );
        // Sans PATH ou sans git.exe dedans : introuvable, et jamais de chemin relatif au dossier courant.
        assert_eq!(git_bash_from_git(Path::new("git"), None, exists), None);
        let other = std::env::join_paths([r"C:\Windows"]).unwrap();
        assert_eq!(git_bash_from_git(Path::new("git"), Some(&other), exists), None);
    }

    #[test]
    fn automatic_falls_back_from_windows_terminal_through_pwsh_and_powershell_to_cmd() {
        let programs =
            |tools: &Tools| auto_chain(Path::new(r"C:\x"), tools).into_iter().map(|l| l.program).collect::<Vec<_>>();
        assert_eq!(
            programs(&tools()),
            ["C:\\WindowsApps\\wt.exe", "C:\\PS7\\pwsh.exe", "powershell.exe", "cmd.exe"].map(OsString::from)
        );
        assert_eq!(programs(&Tools::default()), ["powershell.exe", "cmd.exe"].map(OsString::from));
    }
}
