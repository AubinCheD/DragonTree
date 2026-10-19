//! Intégrations Windows : Explorateur, terminal, éditeur, ouverture de fichiers et d'URL.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
fn detached(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
fn detached(cmd: &mut Command) -> &mut Command {
    cmd
}

/// Chemin avec les séparateurs de Windows (git renvoie des `/`, l'Explorateur ne les accepte pas).
pub fn native_path(path: &Path) -> PathBuf {
    if cfg!(windows) { PathBuf::from(path.to_string_lossy().replace('/', "\\")) } else { path.to_path_buf() }
}

pub fn open_in_explorer(path: &Path) {
    let _ = Command::new("explorer.exe").arg(native_path(path)).spawn();
}

/// Argument `/select,"<chemin>"` de l'Explorateur (forme exacte qu'il attend).
pub fn explorer_select_arg(path: &Path) -> String {
    format!("/select,\"{}\"", native_path(path).display())
}

/// Ouvre l'Explorateur avec le fichier sélectionné.
pub fn reveal_in_explorer(path: &Path) {
    let mut cmd = Command::new("explorer.exe");
    #[cfg(windows)]
    {
        // Passé tel quel : la mise entre guillemets automatique de Rust donnerait `"/select,…"`.
        use std::os::windows::process::CommandExt;
        cmd.raw_arg(explorer_select_arg(path));
    }
    #[cfg(not(windows))]
    cmd.arg(explorer_select_arg(path));
    let _ = cmd.spawn();
}

/// Ouvre un fichier avec l'application associée.
pub fn open_with_default_app(path: &Path) {
    let _ = Command::new("explorer.exe").arg(native_path(path)).spawn();
}

fn find_in(path_var: Option<&OsStr>, exe: &str) -> Option<PathBuf> {
    std::env::split_paths(path_var?).map(|dir| dir.join(exe)).find(|p| p.is_file())
}

fn in_path(exe: &str) -> bool {
    find_in(std::env::var_os("PATH").as_deref(), exe).is_some()
}

/// Terminal dans le dossier : Windows Terminal si disponible, sinon PowerShell.
pub fn open_terminal(path: &Path) {
    let wt = dirs::data_local_dir().map(|d| d.join("Microsoft").join("WindowsApps").join("wt.exe"));
    if (wt.as_ref().is_some_and(|p| p.exists()) || in_path("wt.exe"))
        && Command::new("wt.exe").arg("-d").arg(path).spawn().is_ok()
    {
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        let _ =
            Command::new("powershell.exe").arg("-NoExit").current_dir(path).creation_flags(CREATE_NEW_CONSOLE).spawn();
    }
}

pub fn vscode_available() -> bool {
    in_path("code.cmd")
}

/// Commande qui ouvre `path` dans VS Code (`path_var` : valeur de PATH où chercher `code.cmd`).
/// `code.cmd` est lancé directement, jamais via `cmd.exe /C` : Rust échappe alors les arguments
/// d'un script `.cmd`, et un chemin contenant `&`, `^` ou `%` n'est pas interprété.
pub fn vscode_command_in(path: &Path, path_var: Option<&OsStr>) -> Option<Command> {
    let mut cmd = Command::new(find_in(path_var, "code.cmd")?);
    cmd.arg(path);
    Some(cmd)
}

pub fn open_in_vscode(path: &Path) {
    if let Some(mut cmd) = vscode_command_in(path, std::env::var_os("PATH").as_deref()) {
        let _ = detached(&mut cmd).spawn();
    }
}

pub fn open_url(ctx: &egui::Context, url: &str) {
    ctx.open_url(egui::OpenUrl::new_tab(url));
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn vscode_is_launched_directly_without_cmd() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("VS Code").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("code.cmd"), "@echo off\r\n").unwrap();
        let path_var = std::env::join_paths([dir.path().join("vide"), bin.clone()]).unwrap();
        // Un nom de dossier avec « & » ne doit jamais passer par l'interpréteur cmd.exe.
        let target = Path::new(r"C:\DEV\x&calc");
        let cmd = vscode_command_in(target, Some(&path_var)).expect("VS Code trouvé dans le PATH");
        assert_eq!(Path::new(cmd.get_program()), bin.join("code.cmd"));
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), [OsStr::new(r"C:\DEV\x&calc")]);
        assert!(vscode_command_in(target, Some(OsStr::new(""))).is_none(), "absent du PATH : rien à lancer");
    }

    #[test]
    fn explorer_select_uses_windows_separators() {
        let file = Path::new(r"C:\DEV\Peppol - Copie").join("src/Factures/a.cs");
        assert_eq!(explorer_select_arg(&file), r#"/select,"C:\DEV\Peppol - Copie\src\Factures\a.cs""#);
    }
}
