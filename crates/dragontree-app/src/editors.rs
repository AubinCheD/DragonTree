//! Éditeurs externes de « Ouvrir dans » : détection (PATH et emplacements d'installation usuels), recherche des
//! solutions Visual Studio d'un dépôt et construction de la commande de lancement.
//!
//! La détection touche le disque (Program Files, `%LOCALAPPDATA%\JetBrains\Toolbox`, PATH) : elle est faite une fois
//! au démarrage, hors du thread de l'interface (`AppEvent::EditorsDetected`). Elle prend ses dossiers en paramètres
//! pour être testée avec de faux dossiers d'installation. Les éditeurs sont lancés directement (programme et
//! arguments séparés), jamais par un interpréteur : un chemin n'est jamais interprété comme une ligne de commande.

use std::path::{Path, PathBuf};
use std::process::Command;

use dragontree_i18n::tr;

use crate::icons;

/// Éditeur proposé par « Ouvrir dans ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorKind {
    VsCode,
    Cursor,
    VisualStudio,
    Rider,
    Sublime,
    NotepadPlusPlus,
    /// L'application associée au dossier par Windows : toujours proposée.
    SystemDefault,
}

impl EditorKind {
    /// Ordre de la liste affichée.
    pub const ALL: [EditorKind; 7] = [
        EditorKind::VsCode,
        EditorKind::Cursor,
        EditorKind::VisualStudio,
        EditorKind::Rider,
        EditorKind::Sublime,
        EditorKind::NotepadPlusPlus,
        EditorKind::SystemDefault,
    ];

    /// Identifiant stable, enregistré dans les réglages (dernier éditeur utilisé).
    pub fn id(self) -> &'static str {
        match self {
            EditorKind::VsCode => "vscode",
            EditorKind::Cursor => "cursor",
            EditorKind::VisualStudio => "visualstudio",
            EditorKind::Rider => "rider",
            EditorKind::Sublime => "sublime",
            EditorKind::NotepadPlusPlus => "notepadpp",
            EditorKind::SystemDefault => "default",
        }
    }

    /// Nom affiché (les noms de produits ne se traduisent pas).
    pub fn label(self) -> &'static str {
        match self {
            EditorKind::VsCode => "VS Code",
            EditorKind::Cursor => "Cursor",
            EditorKind::VisualStudio => "Visual Studio",
            EditorKind::Rider => "Rider",
            EditorKind::Sublime => "Sublime Text",
            EditorKind::NotepadPlusPlus => "Notepad++",
            EditorKind::SystemDefault => tr!("Éditeur par défaut"),
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            EditorKind::SystemDefault => icons::FOLDER_OPEN,
            _ => icons::CODE,
        }
    }
}

/// Éditeur détecté : son genre et le programme à lancer (`None` pour l'éditeur par défaut de Windows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    pub kind: EditorKind,
    pub program: Option<PathBuf>,
}

impl Editor {
    /// L'éditeur par défaut de Windows, seul proposé tant que la détection n'a pas répondu.
    pub fn system_default() -> Editor {
        Editor { kind: EditorKind::SystemDefault, program: None }
    }
}

/// Dossiers où chercher les éditeurs ; injectés dans les tests.
#[derive(Debug, Clone, Default)]
pub struct SearchDirs {
    /// Valeur de PATH.
    pub path_var: Option<std::ffi::OsString>,
    /// `Program Files`, `Program Files (x86)`…
    pub program_files: Vec<PathBuf>,
    /// `%LOCALAPPDATA%`.
    pub local_app_data: Option<PathBuf>,
}

impl SearchDirs {
    /// Les dossiers de la machine (variables d'environnement de Windows).
    pub fn system() -> SearchDirs {
        let mut program_files: Vec<PathBuf> = Vec::new();
        for var in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            if let Some(dir) = std::env::var_os(var).map(PathBuf::from)
                && !program_files.contains(&dir)
            {
                program_files.push(dir);
            }
        }
        SearchDirs { path_var: std::env::var_os("PATH"), program_files, local_app_data: dirs::data_local_dir() }
    }

    fn in_path(&self, exe: &str) -> Option<PathBuf> {
        std::env::split_paths(self.path_var.as_deref()?).map(|dir| dir.join(exe)).find(|p| p.is_file())
    }

    /// Premier fichier existant parmi `<Program Files>/<relative>`.
    fn in_program_files(&self, relative: &str) -> Option<PathBuf> {
        self.program_files.iter().map(|pf| pf.join(relative)).find(|p| p.is_file())
    }

    fn in_local_app_data(&self, relative: &str) -> Option<PathBuf> {
        self.local_app_data.as_ref().map(|d| d.join(relative)).filter(|p| p.is_file())
    }
}

/// Sous-dossiers de `dir`, les versions les plus récentes d'abord ([`VersionKey`]), puis les autres noms du dernier au
/// premier dans l'ordre alphabétique.
fn subdirs_descending(dir: &Path) -> Vec<PathBuf> {
    let mut subdirs: Vec<PathBuf> =
        std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    subdirs.sort_by_cached_key(|p| {
        std::cmp::Reverse(VersionKey::of(&p.file_name().unwrap_or_default().to_string_lossy()))
    });
    subdirs
}

/// Clé de tri d'un nom de dossier : un nom fait de nombres séparés par des points est une version, comparée nombre par
/// nombre (`243.10` après `243.9`) ; une année de Visual Studio devient son numéro de version (2017, 2019, 2022 → 15,
/// 16, 17), pour passer avant `18`, le dossier de Visual Studio 2026. Les autres noms viennent après les versions.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum VersionKey {
    Other(String),
    Version(Vec<u64>),
}

impl VersionKey {
    fn of(name: &str) -> VersionKey {
        match name.split('.').map(|part| part.parse::<u64>().ok()).collect::<Option<Vec<u64>>>() {
            Some(parts) => VersionKey::Version(match parts.as_slice() {
                [2017] => vec![15],
                [2019] => vec![16],
                [2022] => vec![17],
                _ => parts,
            }),
            None => VersionKey::Other(name.to_string()),
        }
    }
}

/// `devenv.exe` de la plus récente installation de Visual Studio (`<année>/<édition>/Common7/IDE`).
fn find_visual_studio(dirs: &SearchDirs) -> Option<PathBuf> {
    for pf in &dirs.program_files {
        for year in subdirs_descending(&pf.join("Microsoft Visual Studio")) {
            for edition in subdirs_descending(&year) {
                let devenv = edition.join("Common7").join("IDE").join("devenv.exe");
                if devenv.is_file() {
                    return Some(devenv);
                }
            }
        }
    }
    dirs.in_path("devenv.exe")
}

/// `rider64.exe` de la version la plus récente installée par la Toolbox JetBrains (`apps/Rider/<canal>/<version>`)
/// ou par l'installateur (`Program Files/JetBrains/JetBrains Rider …`), sinon le script de la Toolbox.
fn find_rider(dirs: &SearchDirs) -> Option<PathBuf> {
    if let Some(local) = &dirs.local_app_data {
        let apps = local.join("JetBrains").join("Toolbox").join("apps").join("Rider");
        for channel in subdirs_descending(&apps) {
            for version in subdirs_descending(&channel) {
                let exe = version.join("bin").join("rider64.exe");
                if exe.is_file() {
                    return Some(exe);
                }
            }
        }
    }
    for pf in &dirs.program_files {
        for install in subdirs_descending(&pf.join("JetBrains")) {
            let is_rider = install.file_name().is_some_and(|n| n.to_string_lossy().starts_with("JetBrains Rider"));
            let exe = install.join("bin").join("rider64.exe");
            if is_rider && exe.is_file() {
                return Some(exe);
            }
        }
    }
    dirs.in_path("rider64.exe")
        .or_else(|| dirs.in_path("rider.cmd"))
        .or_else(|| dirs.in_local_app_data("JetBrains/Toolbox/scripts/rider.cmd"))
}

fn find_program(kind: EditorKind, dirs: &SearchDirs) -> Option<PathBuf> {
    match kind {
        EditorKind::VsCode => dirs
            .in_path("code.cmd")
            .or_else(|| dirs.in_local_app_data("Programs/Microsoft VS Code/bin/code.cmd"))
            .or_else(|| dirs.in_program_files("Microsoft VS Code/bin/code.cmd")),
        EditorKind::Cursor => dirs
            .in_path("cursor.cmd")
            .or_else(|| dirs.in_local_app_data("Programs/cursor/resources/app/bin/cursor.cmd"))
            .or_else(|| dirs.in_program_files("cursor/resources/app/bin/cursor.cmd")),
        EditorKind::VisualStudio => find_visual_studio(dirs),
        EditorKind::Rider => find_rider(dirs),
        EditorKind::Sublime => dirs
            .in_path("subl.exe")
            .or_else(|| dirs.in_program_files("Sublime Text/sublime_text.exe"))
            .or_else(|| dirs.in_program_files("Sublime Text 3/sublime_text.exe")),
        EditorKind::NotepadPlusPlus => {
            dirs.in_path("notepad++.exe").or_else(|| dirs.in_program_files("Notepad++/notepad++.exe"))
        }
        EditorKind::SystemDefault => None,
    }
}

/// Éditeurs trouvés dans `dirs`, dans l'ordre de [`EditorKind::ALL`] ; l'éditeur par défaut est toujours là.
pub fn detect(dirs: &SearchDirs) -> Vec<Editor> {
    EditorKind::ALL
        .into_iter()
        .filter_map(|kind| match kind {
            EditorKind::SystemDefault => Some(Editor::system_default()),
            _ => find_program(kind, dirs).map(|program| Editor { kind, program: Some(program) }),
        })
        .collect()
}

/// Éditeurs de la machine (lit le disque : à appeler hors du thread de l'interface).
pub fn detect_system() -> Vec<Editor> {
    detect(&SearchDirs::system())
}

/// Position de l'éditeur du clic principal : le dernier utilisé s'il est (encore) détecté, sinon le premier.
pub fn main_editor(editors: &[Editor], last_used: Option<&str>) -> usize {
    last_used.and_then(|id| editors.iter().position(|e| e.kind.id() == id)).unwrap_or(0)
}

/// Solutions Visual Studio (`.sln`, `.slnx`) à la racine du dépôt ou un niveau en dessous, triées par chemin.
/// Les dossiers cachés et `node_modules` ne sont pas parcourus.
pub fn find_solutions(repo: &Path) -> Vec<PathBuf> {
    let is_solution = |p: &Path| {
        p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sln") || e.eq_ignore_ascii_case("slnx"))
    };
    let entries = |dir: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect()
    };
    let mut found = Vec::new();
    for entry in entries(repo) {
        if is_solution(&entry) {
            found.push(entry);
        } else if entry.is_dir() {
            let name = entry.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !name.starts_with('.') && name != "node_modules" {
                found.extend(entries(&entry).into_iter().filter(|p| is_solution(p)));
            }
        }
    }
    found.sort();
    found
}

/// Commande qui ouvre `target` (le dossier du dépôt, ou une solution pour Visual Studio) dans l'éditeur ;
/// `None` pour l'éditeur par défaut, qui passe par l'association de Windows (`util::open_with_default_app`).
/// Les scripts `.cmd` sont lancés directement : Rust échappe leurs arguments, un chemin avec `&`, `^` ou `%`
/// n'est pas interprété. Notepad++ n'ouvre un dossier que comme espace de travail (`-openFoldersAsWorkspace`, version
/// 7.8 et suivantes) ; une solution ou un fichier lui serait passé tel quel.
pub fn command(editor: &Editor, target: &Path) -> Option<Command> {
    let mut cmd = Command::new(editor.program.as_ref()?);
    if editor.kind == EditorKind::NotepadPlusPlus && target.is_dir() {
        cmd.arg("-openFoldersAsWorkspace");
    }
    cmd.arg(target);
    Some(cmd)
}

/// Résultat de [`open`].
#[derive(Debug, PartialEq, Eq)]
pub enum OpenOutcome {
    Launched,
    /// Visual Studio : plusieurs solutions, à choisir.
    Choose(Vec<PathBuf>),
}

/// Ouvre le dépôt `dir` dans l'éditeur. Visual Studio ouvre la solution du dépôt (voir [`find_solutions`]), à choisir
/// s'il y en a plusieurs ; sans solution, il ouvre le dossier (« Ouvrir un dossier »). Erreur : le lancement a échoué.
pub fn open(editor: &Editor, dir: &Path) -> Result<OpenOutcome, String> {
    if editor.kind == EditorKind::VisualStudio {
        let mut solutions = find_solutions(dir);
        if solutions.len() > 1 {
            return Ok(OpenOutcome::Choose(solutions));
        }
        let target = if solutions.is_empty() { dir.to_path_buf() } else { solutions.remove(0) };
        launch(editor, &target)?;
        return Ok(OpenOutcome::Launched);
    }
    launch(editor, dir)?;
    Ok(OpenOutcome::Launched)
}

/// Lance l'éditeur sur `target`, sans fenêtre de console ; erreur : le message du système.
pub fn launch(editor: &Editor, target: &Path) -> Result<(), String> {
    match command(editor, target) {
        Some(mut cmd) => crate::util::detached(&mut cmd).spawn().map(|_| ()).map_err(|e| e.to_string()),
        None => {
            crate::util::open_with_default_app(target);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};

    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    fn kinds(editors: &[Editor]) -> Vec<EditorKind> {
        editors.iter().map(|e| e.kind).collect()
    }

    fn program(editors: &[Editor], kind: EditorKind) -> PathBuf {
        editors.iter().find(|e| e.kind == kind).and_then(|e| e.program.clone()).expect("éditeur détecté")
    }

    #[test]
    fn only_the_default_editor_is_listed_on_an_empty_machine() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = SearchDirs {
            path_var: Some(OsString::from(dir.path().join("vide"))),
            program_files: vec![dir.path().join("pf")],
            local_app_data: Some(dir.path().join("local")),
        };
        assert_eq!(kinds(&detect(&dirs)), [EditorKind::SystemDefault]);
        assert_eq!(detect(&SearchDirs::default()), [Editor::system_default()]);
    }

    #[test]
    fn editors_are_found_in_the_path_and_in_the_usual_install_folders() {
        let dir = tempfile::tempdir().unwrap();
        let (bin, pf, pf86, local) =
            (dir.path().join("bin"), dir.path().join("pf"), dir.path().join("pf86"), dir.path().join("local"));
        touch(&bin.join("cursor.cmd"));
        touch(&pf.join("Microsoft VS Code/bin/code.cmd"));
        touch(&pf.join("Sublime Text/sublime_text.exe"));
        touch(&pf86.join("Notepad++/notepad++.exe"));
        // Deux éditions et deux années : la plus récente gagne.
        touch(&pf.join("Microsoft Visual Studio/2019/Community/Common7/IDE/devenv.exe"));
        touch(&pf.join("Microsoft Visual Studio/2022/Professional/Common7/IDE/devenv.exe"));
        // Rider de la Toolbox : la version la plus récente du canal.
        touch(&local.join("JetBrains/Toolbox/apps/Rider/ch-0/241.1/bin/rider64.exe"));
        touch(&local.join("JetBrains/Toolbox/apps/Rider/ch-0/243.2/bin/rider64.exe"));
        let dirs = SearchDirs {
            path_var: Some(std::env::join_paths([dir.path().join("absent"), bin.clone()]).unwrap()),
            program_files: vec![pf.clone(), pf86.clone()],
            local_app_data: Some(local.clone()),
        };
        let editors = detect(&dirs);
        assert_eq!(
            kinds(&editors),
            [
                EditorKind::VsCode,
                EditorKind::Cursor,
                EditorKind::VisualStudio,
                EditorKind::Rider,
                EditorKind::Sublime,
                EditorKind::NotepadPlusPlus,
                EditorKind::SystemDefault
            ]
        );
        assert_eq!(program(&editors, EditorKind::VsCode), pf.join("Microsoft VS Code/bin/code.cmd"));
        assert_eq!(program(&editors, EditorKind::Cursor), bin.join("cursor.cmd"));
        assert_eq!(
            program(&editors, EditorKind::VisualStudio),
            pf.join("Microsoft Visual Studio/2022/Professional/Common7/IDE/devenv.exe")
        );
        assert_eq!(
            program(&editors, EditorKind::Rider),
            local.join("JetBrains/Toolbox/apps/Rider/ch-0/243.2/bin/rider64.exe")
        );
        assert_eq!(program(&editors, EditorKind::NotepadPlusPlus), pf86.join("Notepad++/notepad++.exe"));
    }

    #[test]
    fn version_folders_are_sorted_by_version_not_as_text() {
        let dir = tempfile::tempdir().unwrap();
        let (pf, local) = (dir.path().join("pf"), dir.path().join("local"));
        // Visual Studio 2026 s'installe sous `18`, qui passe avant `2022` (17) et `2019` (16) ; `Installer` en dernier.
        for year in ["2019", "2022", "18"] {
            touch(&pf.join(format!("Microsoft Visual Studio/{year}/Community/Common7/IDE/devenv.exe")));
        }
        std::fs::create_dir_all(pf.join("Microsoft Visual Studio/Installer")).unwrap();
        let names = |dir: &Path| -> Vec<String> {
            subdirs_descending(dir).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect()
        };
        assert_eq!(names(&pf.join("Microsoft Visual Studio")), ["18", "2022", "2019", "Installer"]);
        // Versions de Rider : composante par composante (`243.10` après `243.9`).
        touch(&local.join("JetBrains/Toolbox/apps/Rider/ch-0/243.9/bin/rider64.exe"));
        touch(&local.join("JetBrains/Toolbox/apps/Rider/ch-0/243.10/bin/rider64.exe"));
        let dirs = SearchDirs { path_var: None, program_files: vec![pf.clone()], local_app_data: Some(local.clone()) };
        let editors = detect(&dirs);
        assert_eq!(
            program(&editors, EditorKind::VisualStudio),
            pf.join("Microsoft Visual Studio/18/Community/Common7/IDE/devenv.exe")
        );
        assert_eq!(
            program(&editors, EditorKind::Rider),
            local.join("JetBrains/Toolbox/apps/Rider/ch-0/243.10/bin/rider64.exe")
        );
        // Les autres noms gardent l'ordre alphabétique inverse (éditions, canaux).
        for edition in ["Community", "Professional", "Enterprise"] {
            std::fs::create_dir_all(dir.path().join("editions").join(edition)).unwrap();
        }
        assert_eq!(names(&dir.path().join("editions")), ["Professional", "Enterprise", "Community"]);
    }

    #[test]
    fn rider_is_also_found_in_program_files_and_the_toolbox_script() {
        let dir = tempfile::tempdir().unwrap();
        let (pf, local) = (dir.path().join("pf"), dir.path().join("local"));
        touch(&pf.join("JetBrains/JetBrains Rider 2024.3/bin/rider64.exe"));
        touch(&pf.join("JetBrains/PyCharm 2024.3/bin/rider64.exe"));
        let dirs = SearchDirs { path_var: None, program_files: vec![pf.clone()], local_app_data: Some(local) };
        assert_eq!(
            program(&detect(&dirs), EditorKind::Rider),
            pf.join("JetBrains/JetBrains Rider 2024.3/bin/rider64.exe")
        );
        let only_script = tempfile::tempdir().unwrap();
        touch(&only_script.path().join("JetBrains/Toolbox/scripts/rider.cmd"));
        let dirs = SearchDirs { local_app_data: Some(only_script.path().to_path_buf()), ..SearchDirs::default() };
        assert_eq!(
            program(&detect(&dirs), EditorKind::Rider),
            only_script.path().join("JetBrains/Toolbox/scripts/rider.cmd")
        );
    }

    #[test]
    fn solutions_are_searched_at_the_root_and_one_level_down() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        touch(&repo.join("App.sln"));
        touch(&repo.join("src/Web.sln"));
        touch(&repo.join("src/Autre.slnx"));
        touch(&repo.join("src/profond/Trop.sln"));
        touch(&repo.join(".cache/Cache.sln"));
        touch(&repo.join("node_modules/Lib.sln"));
        touch(&repo.join("notes.txt"));
        let found: Vec<_> = find_solutions(repo).iter().map(|p| p.strip_prefix(repo).unwrap().to_path_buf()).collect();
        assert_eq!(
            found,
            [PathBuf::from("App.sln"), PathBuf::from("src").join("Autre.slnx"), PathBuf::from("src").join("Web.sln")]
        );
        assert!(find_solutions(&repo.join("absent")).is_empty());
    }

    #[test]
    fn each_editor_is_launched_directly_with_the_path_as_one_argument() {
        // Un dossier au nom que cmd.exe interpréterait.
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("x&calc %PATH% ^y");
        std::fs::create_dir_all(&target).unwrap();
        for kind in EditorKind::ALL.into_iter().filter(|k| *k != EditorKind::SystemDefault) {
            let editor = Editor { kind, program: Some(PathBuf::from(format!(r"C:\Outils\{}\prog.cmd", kind.id()))) };
            let cmd = command(&editor, &target).expect("commande");
            assert_eq!(Path::new(cmd.get_program()), editor.program.as_deref().unwrap(), "{kind:?}");
            let args: Vec<&OsStr> = cmd.get_args().collect();
            // Notepad++ n'ouvre un dossier que comme espace de travail ; les autres le reçoivent seul.
            let expected: Vec<&OsStr> = match kind {
                EditorKind::NotepadPlusPlus => vec![OsStr::new("-openFoldersAsWorkspace"), target.as_os_str()],
                _ => vec![target.as_os_str()],
            };
            assert_eq!(args, expected, "{kind:?}");
        }
        // Un fichier (une solution) est passé tel quel, même à Notepad++.
        let sln = base.path().join("App.sln");
        touch(&sln);
        let npp = Editor { kind: EditorKind::NotepadPlusPlus, program: Some("npp.exe".into()) };
        assert_eq!(command(&npp, &sln).unwrap().get_args().collect::<Vec<_>>(), [sln.as_os_str()]);
        assert!(command(&Editor::system_default(), &target).is_none(), "l'éditeur par défaut passe par Windows");
    }

    #[test]
    fn the_main_editor_is_the_last_used_one_else_the_first() {
        let editors = [
            Editor { kind: EditorKind::VsCode, program: Some("a".into()) },
            Editor { kind: EditorKind::Rider, program: Some("b".into()) },
            Editor::system_default(),
        ];
        assert_eq!(main_editor(&editors, None), 0);
        assert_eq!(main_editor(&editors, Some("rider")), 1);
        assert_eq!(main_editor(&editors, Some("sublime")), 0, "éditeur disparu : le premier");
        assert_eq!(main_editor(&[], Some("rider")), 0);
    }

    #[test]
    fn visual_studio_asks_for_a_choice_when_several_solutions_exist() {
        let dir = tempfile::tempdir().unwrap();
        let vs = Editor { kind: EditorKind::VisualStudio, program: Some(dir.path().join("devenv.exe")) };
        touch(&dir.path().join("A.sln"));
        touch(&dir.path().join("B.sln"));
        let both = vec![dir.path().join("A.sln"), dir.path().join("B.sln")];
        assert_eq!(open(&vs, dir.path()), Ok(OpenOutcome::Choose(both)));
    }

    #[test]
    fn a_launch_failure_is_reported_and_visual_studio_without_solution_opens_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let missing = Editor { kind: EditorKind::VisualStudio, program: Some(dir.path().join("absent.exe")) };
        // Sans solution : le dossier est ouvert (le lancement échoue ici, faute de programme, et l'erreur remonte).
        assert!(open(&missing, dir.path()).is_err());
        assert!(launch(&missing, dir.path()).is_err());
    }
}
