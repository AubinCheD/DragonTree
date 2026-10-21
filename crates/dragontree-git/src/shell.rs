//! Lignes de commande écrites par l'utilisateur (commandes personnalisées de l'interface), exécutées par
//! l'interpréteur de Windows : seule exception à la règle « jamais via un shell » du moteur. Les valeurs venues de
//! git (noms de branches, de fichiers…) doivent avoir été citées par l'appelant avant d'entrer dans la ligne.

use std::cell::RefCell;
use std::path::Path;
use std::process::Command;

use crate::error::GitError;
use crate::runner::{CancelToken, Supervised};

/// Taille maximale conservée de la sortie (on garde la fin, où se trouvent les erreurs).
const MAX_OUTPUT: usize = 1024 * 1024;

/// Ligne terminée : son code de sortie et sa sortie (standard et d'erreur mêlées dans l'ordre d'arrivée).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellOutput {
    pub code: i32,
    pub output: String,
}

/// Exécute `line` dans `cwd` avec `cmd.exe /D /E:ON /V:OFF /S /C "<line>"` : sans les commandes AutoRun du
/// registre, extensions actives, sans expansion différée (`!` reste un caractère ordinaire) ; `/S` transmet la ligne
/// telle quelle, même si elle commence par un guillemet. `env` complète l'environnement : une valeur `None` retire la
/// variable (héritée de DragonTree par exemple). Pas de fenêtre ni d'entrée standard ; l'annulation tue aussi les
/// sous-processus. Un programme que la ligne lance en arrière-plan (`start`) n'est pas attendu. Un code de sortie
/// non nul n'est pas une erreur : il est rendu à l'appelant.
pub fn run_shell(
    cwd: &Path,
    line: &str,
    env: &[(String, Option<String>)],
    cancel: &CancelToken,
) -> Result<ShellOutput, GitError> {
    let mut command = shell_command(line);
    command
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    for (k, v) in env {
        match v {
            Some(v) => command.env(k, v),
            None => command.env_remove(k),
        };
    }
    let output = RefCell::new(Vec::new());
    let append = |bytes: &[u8]| {
        let mut out = output.borrow_mut();
        out.extend_from_slice(bytes);
        if out.len() > MAX_OUTPUT {
            let excess = out.len() - MAX_OUTPUT;
            out.drain(..excess);
        }
    };
    let mut on_stdout = |bytes: &[u8]| append(bytes);
    let mut on_stderr_line = |line: &str| append(format!("{line}\n").as_bytes());
    let run = Supervised { stdin: None, cancel: Some(cancel), detach_after_exit: true, stdout_limit: None };
    let done = run.run(command, |e| GitError::Spawn(e.to_string()), Some(&mut on_stdout), Some(&mut on_stderr_line))?;
    let output = output.into_inner();
    Ok(ShellOutput { code: done.code, output: String::from_utf8_lossy(&output).into_owned() })
}

/// Texte tel que le lit un programme qui reçoit sa ligne de commande dans la page de code ANSI de Windows (`main` de
/// msvcrt ou de l'UCRT) : Windows le convertit avec ses correspondances « au plus proche » (best fit : `＂` devient
/// `"`, `＼` devient `\` en page 1252…). `None` si la conversion échoue. Ailleurs qu'en Windows : le texte tel quel.
pub fn ansi_view(text: &str) -> Option<String> {
    #[cfg(windows)]
    {
        ansi_view_in(text, windows_sys::Win32::Globalization::CP_ACP)
    }
    #[cfg(not(windows))]
    {
        Some(text.to_string())
    }
}

/// [`ansi_view`] dans la page de code `code_page` : conversion avec les options par défaut (celles que Windows
/// applique à la ligne de commande), puis retour en Unicode.
#[cfg(windows)]
pub fn ansi_view_in(text: &str, code_page: u32) -> Option<String> {
    use windows_sys::Win32::Globalization::{MultiByteToWideChar, WideCharToMultiByte};

    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return Some(String::new());
    }
    let wide_len = i32::try_from(wide.len()).ok()?;
    // SAFETY : tampons alloués à la taille demandée par un premier appel, longueurs passées explicitement.
    unsafe {
        let (null, used_default) = (std::ptr::null_mut(), std::ptr::null_mut());
        let len = WideCharToMultiByte(code_page, 0, wide.as_ptr(), wide_len, null, 0, std::ptr::null(), used_default);
        if len <= 0 {
            return None;
        }
        let mut bytes = vec![0u8; len as usize];
        let len = WideCharToMultiByte(
            code_page,
            0,
            wide.as_ptr(),
            wide_len,
            bytes.as_mut_ptr(),
            len,
            std::ptr::null(),
            std::ptr::null_mut(),
        );
        if len <= 0 {
            return None;
        }
        let back_len = MultiByteToWideChar(code_page, 0, bytes.as_ptr(), len, std::ptr::null_mut(), 0);
        if back_len <= 0 {
            return None;
        }
        let mut back = vec![0u16; back_len as usize];
        let back_len = MultiByteToWideChar(code_page, 0, bytes.as_ptr(), len, back.as_mut_ptr(), back_len);
        if back_len <= 0 {
            return None;
        }
        back.truncate(back_len as usize);
        Some(String::from_utf16_lossy(&back))
    }
}

#[cfg(windows)]
fn shell_command(line: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let comspec = std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into());
    let mut command = Command::new(comspec);
    // Passé tel quel : la mise entre guillemets automatique de Rust ne suit pas les règles de cmd.
    command.raw_arg(format!("/D /E:ON /V:OFF /S /C \"{line}\""));
    command
}

#[cfg(not(windows))]
fn shell_command(line: &str) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(line);
    command
}
