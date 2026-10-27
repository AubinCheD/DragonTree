//! Intégrations Windows : Explorateur, terminal, éditeur, ouverture de fichiers et d'URL.

use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
pub(crate) fn detached(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
pub(crate) fn detached(cmd: &mut Command) -> &mut Command {
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

/// Ouvre un fichier avec l'application associée, par le shell de Windows : pas par `explorer.exe`, qui découpe son
/// argument aux virgules (un nom de fichier `a,b` ne désignerait plus ce fichier).
#[cfg(windows)]
pub fn open_with_default_app(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain([0]).collect::<Vec<u16>>();
    let (verb, file) = (wide("open".as_ref()), wide(native_path(path).as_os_str()));
    // SAFETY : chaînes terminées par un zéro, vivantes pendant l'appel ; aucune fenêtre parente.
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
}

#[cfg(not(windows))]
pub fn open_with_default_app(path: &Path) {
    let _ = Command::new("xdg-open").arg(path).spawn();
}

/// Extensions que Windows exécute au lieu de les ouvrir dans une application : programmes, scripts, raccourcis,
/// installeurs, fichiers de registre ou d'aide compilée… (et celles de `PATHEXT`).
const RUNNABLE: &[&str] = &[
    "appinstaller",
    "application",
    "appref-ms",
    "appx",
    "appxbundle",
    "bat",
    "chm",
    "cmd",
    "com",
    "cpl",
    "diagcab",
    "exe",
    "hta",
    "inf",
    "iqy",
    "jar",
    "js",
    "jse",
    "library-ms",
    "lnk",
    "msc",
    "msi",
    "msix",
    "msixbundle",
    "msp",
    "mst",
    "pif",
    "ps1",
    "psm1",
    "reg",
    "scf",
    "scr",
    "searchconnector-ms",
    "settingcontent-ms",
    "slk",
    "url",
    "vbe",
    "vbs",
    "website",
    "wsf",
    "wsh",
    "xll",
];

/// Ouvrir ce fichier avec l'application associée le lancerait-il ? Un fichier d'un dépôt cloné n'a pas la marque
/// « venu d'Internet » : ni Windows ni SmartScreen ne préviendraient.
pub fn is_runnable(path: &Path) -> bool {
    let Some(ext) = path.extension().map(|e| e.to_string_lossy().to_lowercase()) else { return false };
    RUNNABLE.contains(&ext.as_str())
        || std::env::var("PATHEXT")
            .is_ok_and(|exts| exts.split(';').any(|e| e.trim_start_matches('.').eq_ignore_ascii_case(&ext)))
}

pub fn open_url(ctx: &egui::Context, url: &str) {
    ctx.open_url(egui::OpenUrl::new_tab(url));
}

/// Adresse lue dans une réponse d'une forge qu'on peut ouvrir : `https://` seulement (défense en profondeur : une
/// réponse inattendue ne doit pas lancer un fichier local, un autre protocole ou une page en clair).
fn is_forge_url(url: &str) -> bool {
    url.get(..8).is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://")) && url.len() > 8
}

/// Ouvre une adresse lue dans une réponse d'une forge (pull request créée, liste des pull requests) si c'est une
/// adresse `https://` ; une autre est ignorée (et signalée sur la sortie d'erreur).
pub fn open_forge_url(ctx: &egui::Context, url: &str) {
    if is_forge_url(url) {
        open_url(ctx, url);
    } else {
        eprintln!("DragonTree : adresse d'une forge ignorée, elle n'est pas en https:// ({url})");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_addresses_from_a_forge_are_opened() {
        for url in ["https://github.com/contoso/northwind/pull/1", "HTTPS://dev.azure.com/fabrikam/_git/api"] {
            assert!(is_forge_url(url), "{url}");
        }
        for url in [
            "http://github.com/contoso/northwind/pull/1",
            "file:///C:/Windows/System32/calc.exe",
            "C:\\Windows\\System32\\calc.exe",
            "javascript:alert(1)",
            " https://github.com",
            "https://",
            "",
            "httpsé://x",
        ] {
            assert!(!is_forge_url(url), "{url}");
        }
    }

    #[test]
    fn explorer_select_uses_windows_separators() {
        let file = Path::new(r"C:\DEV\Peppol - Copie").join("src/Factures/a.cs");
        assert_eq!(explorer_select_arg(&file), r#"/select,"C:\DEV\Peppol - Copie\src\Factures\a.cs""#);
    }

    #[test]
    fn programs_scripts_and_shortcuts_are_runnable() {
        for name in
            ["setup.EXE", "build.bat", "a.cmd", "x.ps1", "Lien.lnk", "site.url", "app.hta", "cle.reg", "aide.chm"]
        {
            assert!(is_runnable(Path::new(name)), "{name}");
        }
        for name in ["README.md", "main.rs", "Projet.sln", "image.png", "Makefile", "archive.tar.gz", "bat"] {
            assert!(!is_runnable(Path::new(name)), "{name}");
        }
    }
}
