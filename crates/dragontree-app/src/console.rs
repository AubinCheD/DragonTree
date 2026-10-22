//! Messages de la ligne de commande dans la console qui a lancé DragonTree (P4).
//!
//! L'exécutable release est une application graphique (`windows_subsystem = "windows"`, `main.rs`) : Windows ne lui
//! donne ni console ni flux standard. Lancé depuis cmd ou PowerShell sans redirection, ce qu'il écrit (aide de
//! `--help`, option inconnue) n'irait nulle part. Avant d'écrire ces messages, [`attach_parent_console`] le rattache à
//! la console de son parent (`AttachConsole(ATTACH_PARENT_PROCESS)`), seulement si la sortie d'erreur n'est pas déjà
//! utilisable : un tube (Git Bash dans mintty), un fichier ou `NUL` restent tels quels.
//!
//! Constaté sous Windows 11 (voir la feuille de route, P4) :
//! - `AttachConsole` peut remplacer par la console un flux déjà redirigé (`dragontree --help > aide.txt` depuis cmd :
//!   la sortie standard perdait son fichier). Les flux utilisables avant l'attache sont donc remis ensuite
//!   (`SetStdHandle`).
//! - Il peut aussi laisser un flux inutilisable : Git Bash dans une console transmet à l'application graphique un
//!   handle de sortie d'erreur qui n'est pas valide chez elle, et `AttachConsole` ne le remplace pas. Un flux de sortie
//!   encore inutilisable après l'attache est ouvert sur `CONOUT$`.
//! - La bibliothèque standard relit `GetStdHandle` à chaque écriture et écrit dans une console en UTF-16 : `eprintln!`
//!   affiche ensuite le texte, accents compris.
//!
//! Limites :
//! - cmd et PowerShell n'attendent pas la fin d'une application graphique lancée directement : leur invite revient
//!   aussitôt et le texte s'affiche après elle (Entrée redonne une invite propre). `start /wait`, un script `.cmd` ou
//!   `Start-Process -NoNewWindow -Wait` attendent la fin et affichent le texte avant l'invite.
//! - Réservé aux messages écrits juste avant de quitter : une application rattachée à une console en reçoit aussi les
//!   Ctrl+C.

/// État d'un flux standard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    /// Pas de handle utilisable (nul, invalide) : application graphique lancée sans redirection.
    Missing,
    /// Handle utilisable : console, tube, fichier ou périphérique (`NUL`).
    Usable,
}

/// Ce qu'il faut faire avant d'écrire sur la sortie d'erreur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Se rattacher à la console du processus parent.
    pub attach: bool,
    /// Flux (entrée, sortie, erreur) à remettre après l'attache, tels qu'ils étaient avant.
    pub restore: [bool; 3],
}

/// Décision selon l'état des flux (entrée, sortie, erreur) au moment d'écrire : se rattacher seulement si la sortie
/// d'erreur, celle des messages, manque ; remettre ensuite les flux qui étaient utilisables (redirections), que
/// `AttachConsole` peut remplacer par la console.
pub fn plan(streams: [Stream; 3]) -> Plan {
    let attach = streams[2] == Stream::Missing;
    Plan { attach, restore: streams.map(|s| attach && s == Stream::Usable) }
}

/// Résultat de [`attach_parent_console`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attach {
    /// La sortie d'erreur était déjà utilisable : rien n'a changé.
    NotNeeded,
    /// Rattaché à la console du processus parent.
    Attached,
    /// Aucune console à rejoindre (lancement depuis l'Explorateur, parent sans console) : le message est perdu.
    NoConsole,
}

/// À appeler juste avant d'écrire un message de la ligne de commande sur la sortie d'erreur, puis de quitter : voir la
/// doc du module.
pub fn attach_parent_console() -> Attach {
    sys::attach_parent_console()
}

#[cfg(windows)]
mod sys {
    use windows_sys::Win32::Foundation::{
        GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, NO_ERROR, SetLastError,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_UNKNOWN, GetFileType, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE, SetStdHandle,
    };

    use super::{Attach, Stream};

    /// Flux standard, dans l'ordre de [`super::plan`].
    const STREAMS: [STD_HANDLE; 3] = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];

    /// État d'un handle : manquant s'il est nul ou si Windows ne le reconnaît pas (`GetFileType` échoue).
    pub fn classify(handle: HANDLE) -> Stream {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Stream::Missing;
        }
        // SAFETY: `GetFileType` ne fait que lire le type du handle ; sur un handle invalide, elle échoue
        // (`FILE_TYPE_UNKNOWN` et une erreur), sans autre effet.
        let failed = unsafe {
            SetLastError(NO_ERROR);
            GetFileType(handle) == FILE_TYPE_UNKNOWN && GetLastError() != NO_ERROR
        };
        if failed { Stream::Missing } else { Stream::Usable }
    }

    fn std_handle(id: STD_HANDLE) -> HANDLE {
        // SAFETY: lecture d'un handle standard du processus.
        unsafe { GetStdHandle(id) }
    }

    pub fn attach_parent_console() -> Attach {
        let before = STREAMS.map(std_handle);
        let plan = super::plan(before.map(classify));
        if !plan.attach {
            return Attach::NotNeeded;
        }
        // SAFETY: appel sans pointeur ; échoue si le parent n'a pas de console (ou si ce processus en a déjà une).
        if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
            return Attach::NoConsole;
        }
        for ((id, handle), restore) in STREAMS.into_iter().zip(before).zip(plan.restore) {
            if restore {
                // SAFETY: `handle` est le handle standard d'avant l'attache, toujours ouvert.
                unsafe { SetStdHandle(id, handle) };
            }
        }
        for id in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            if classify(std_handle(id)) == Stream::Missing
                && let Some(console) = console_output()
            {
                // SAFETY: handle de console ouvert ci-dessus, jamais fermé (le processus quitte après le message).
                unsafe { SetStdHandle(id, console) };
            }
        }
        Attach::Attached
    }

    /// Tampon d'écran de la console rattachée (`CONOUT$`).
    fn console_output() -> Option<HANDLE> {
        let name: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        // SAFETY: nom terminé par un zéro ; pas d'attributs de sécurité ni de modèle.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        (classify(handle) == Stream::Usable).then_some(handle)
    }
}

#[cfg(not(windows))]
mod sys {
    use super::Attach;

    pub fn attach_parent_console() -> Attach {
        Attach::NotNeeded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Stream::{Missing, Usable};

    #[test]
    fn only_a_missing_error_output_attaches_and_redirections_are_kept() {
        // cmd ou PowerShell, sans redirection : aucun flux.
        assert_eq!(plan([Missing; 3]), Plan { attach: true, restore: [false; 3] });
        // Git Bash (tubes) ou console déjà là : rien à faire.
        assert_eq!(plan([Usable; 3]), Plan { attach: false, restore: [false; 3] });
        // `dragontree --help > aide.txt` depuis cmd : la sortie standard garde son fichier.
        assert_eq!(plan([Missing, Usable, Missing]), Plan { attach: true, restore: [false, true, false] });
        assert_eq!(plan([Usable, Usable, Missing]), Plan { attach: true, restore: [true, true, false] });
        // `dragontree --help 2> erreurs.txt` : le message va dans le fichier.
        assert_eq!(plan([Missing, Missing, Usable]), Plan { attach: false, restore: [false; 3] });
    }

    #[cfg(windows)]
    #[test]
    fn handles_are_classified_by_what_windows_knows_of_them() {
        use std::os::windows::io::AsRawHandle;

        use sys::classify;
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

        assert_eq!(classify(std::ptr::null_mut()), Missing);
        assert_eq!(classify(INVALID_HANDLE_VALUE), Missing);
        let file = std::fs::File::open(std::env::current_exe().unwrap()).unwrap();
        assert_eq!(classify(file.as_raw_handle()), Usable);
        let (reader, writer) = std::io::pipe().unwrap();
        assert_eq!(classify(reader.as_raw_handle()), Usable);
        assert_eq!(classify(writer.as_raw_handle()), Usable);
        // Valeur de handle jamais ouverte (multiple de 4, très haut) : Windows ne la reconnaît pas.
        assert_eq!(classify(0x7fff_fff0 as _), Missing);
    }

    /// Rôle du processus qui exécute le test suivant : absent (test lancé par cargo), `relais` ou `enfant`.
    #[cfg(windows)]
    const ROLE: &str = "DRAGONTREE_TEST_CONSOLE_ROLE";
    /// Résultat attendu chez l'enfant (`Attached`, `NoConsole`).
    #[cfg(windows)]
    const EXPECTED: &str = "DRAGONTREE_TEST_CONSOLE_EXPECTED";
    #[cfg(windows)]
    const SELF_TEST: &str = "console::tests::without_a_console_the_call_does_not_panic_and_keeps_the_redirected_output";

    /// L'appel dans un processus sans console ni sortie d'erreur, sortie standard redirigée, comme l'exécutable release
    /// lancé par `dragontree --help > aide.txt` depuis cmd. Le test se relance lui-même sur trois niveaux :
    /// - le relais, lancé avec une console cachée (`CREATE_NO_WINDOW`) ou sans console (`DETACHED_PROCESS`), sortie
    ///   standard dans un tube vers ce test ;
    /// - l'enfant, lancé par le relais (voir `console_relay`), sans console, sortie standard héritée du relais ;
    /// - l'enfant se rattache à la console du relais (`Attached`) ou constate qu'il n'y en a pas (`NoConsole`), sans
    ///   paniquer. Son compte rendu n'arrive dans le tube que si sa sortie standard a été remise : lancé comme cmd
    ///   lance un programme (sans `STARTF_USESTDHANDLES`), `AttachConsole` remplace tous ses flux par la console.
    ///   Vu rouge sans la remise (`Plan::restore` toujours faux) : sortie standard remplacée, compte rendu perdu.
    #[cfg(windows)]
    #[test]
    fn without_a_console_the_call_does_not_panic_and_keeps_the_redirected_output() {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, DETACHED_PROCESS};

        match std::env::var(ROLE).as_deref() {
            Ok("enfant") => return console_child(),
            Ok("relais") => return console_relay(),
            _ => {}
        }
        for (flags, expected) in [(CREATE_NO_WINDOW, "Attached"), (DETACHED_PROCESS, "NoConsole")] {
            let relay = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", SELF_TEST, "--nocapture", "--test-threads=1"])
                .env(ROLE, "relais")
                .env(EXPECTED, expected)
                .creation_flags(flags)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
                .unwrap();
            let report = String::from_utf8_lossy(&relay.stdout);
            assert!(relay.status.success(), "{expected} : {report}");
            assert!(report.contains(&format!("enfant : {expected}, sortie standard gardée")), "{expected} : {report}");
        }
    }

    /// Relais : lance l'enfant. Avec une console (attendu : `Attached`), comme cmd lance un programme : flux hérités
    /// sans `STARTF_USESTDHANDLES`, et la console du relais, que l'enfant quitte (`FreeConsole`) pour être sans console
    /// comme une application graphique. Sans console (attendu : `NoConsole`) : enfant sans console (`DETACHED_PROCESS`),
    /// flux transmis par la bibliothèque standard.
    #[cfg(windows)]
    fn console_relay() {
        use std::os::windows::process::CommandExt;

        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            CreateProcessW, DETACHED_PROCESS, GetExitCodeProcess, INFINITE, PROCESS_INFORMATION, STARTUPINFOW,
            WaitForSingleObject,
        };

        let exe = std::env::current_exe().unwrap();
        let args = ["--exact", SELF_TEST, "--nocapture", "--test-threads=1"];
        if std::env::var(EXPECTED).as_deref() != Ok("Attached") {
            let status = std::process::Command::new(&exe)
                .args(args)
                .env(ROLE, "enfant")
                .creation_flags(DETACHED_PROCESS)
                .status()
                .unwrap();
            assert!(status.success(), "l'enfant a échoué");
            return;
        }
        // SAFETY: le relais n'exécute que ce test (`--test-threads=1`), aucun autre thread ne lit l'environnement.
        unsafe { std::env::set_var(ROLE, "enfant") };
        let line = format!("\"{}\" {}", exe.display(), args.join(" "));
        let mut line: Vec<u16> = line.encode_utf16().chain(Some(0)).collect();
        // SAFETY: structures initialisées à zéro puis remplies par l'API ; ligne de commande modifiable, terminée par
        // un zéro ; handles du processus fermés après l'attente.
        unsafe {
            let mut startup: STARTUPINFOW = std::mem::zeroed();
            startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut info: PROCESS_INFORMATION = std::mem::zeroed();
            let created = CreateProcessW(
                std::ptr::null(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                0,
                std::ptr::null(),
                std::ptr::null(),
                &startup,
                &mut info,
            );
            assert_ne!(created, 0, "{}", std::io::Error::last_os_error());
            WaitForSingleObject(info.hProcess, INFINITE);
            let mut code = 1u32;
            GetExitCodeProcess(info.hProcess, &mut code);
            CloseHandle(info.hThread);
            CloseHandle(info.hProcess);
            assert_eq!(code, 0, "l'enfant a échoué");
        }
    }

    /// Enfant : sans console, sortie standard héritée (le tube), sortie d'erreur absente comme dans une application
    /// graphique lancée depuis cmd.
    #[cfg(windows)]
    fn console_child() {
        use sys::classify;
        use windows_sys::Win32::System::Console::{
            FreeConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
        };

        // SAFETY: lecture et remplacement des handles standard de ce processus, qui n'exécute que ce test.
        let output = unsafe {
            FreeConsole();
            SetStdHandle(STD_ERROR_HANDLE, std::ptr::null_mut());
            GetStdHandle(STD_OUTPUT_HANDLE)
        };
        assert_eq!(classify(output), Usable, "tube hérité du relais");
        let attach = attach_parent_console();
        let after = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        if attach == Attach::Attached {
            assert_eq!(classify(unsafe { GetStdHandle(STD_ERROR_HANDLE) }), Usable, "sortie d'erreur : la console");
        }
        assert_eq!(after, output, "la sortie standard a perdu son tube");
        assert_eq!(format!("{attach:?}"), std::env::var(EXPECTED).unwrap());
        println!("enfant : {attach:?}, sortie standard gardée");
    }
}
