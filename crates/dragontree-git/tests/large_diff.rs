//! RV2 : le diff d'un très gros fichier est coupé à la limite ([`MAX_DIFF_BYTES`]) pendant la lecture de la sortie de
//! git, sans jamais en garder davantage en mémoire. Un allocateur qui compte les octets alloués mesure le pic de
//! mémoire pendant chaque diff ; ce fichier ne contient qu'un test, pour que rien d'autre n'alloue en même temps.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::TestRepo;
use dragontree_git::diff::MAX_DIFF_BYTES;
use dragontree_git::{ChangeKind, DiffOptions, FileChange, Oid, Repo};

/// Allocateur du système qui tient le compte des octets alloués et de leur maximum.
struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn allocated(n: usize) {
    let now = CURRENT.fetch_add(n, Ordering::SeqCst) + n;
    PEAK.fetch_max(now, Ordering::SeqCst);
}

// SAFETY: délègue chaque appel à `System` avec les mêmes arguments ; seuls les compteurs sont ajoutés.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: contrat de `GlobalAlloc::alloc`, transmis tel quel.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            allocated(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: contrat de `GlobalAlloc::alloc_zeroed`, transmis tel quel.
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            allocated(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: contrat de `GlobalAlloc::dealloc`, transmis tel quel.
        unsafe { System.dealloc(ptr, layout) };
        CURRENT.fetch_sub(layout.size(), Ordering::SeqCst);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: contrat de `GlobalAlloc::realloc`, transmis tel quel.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            if new_size >= layout.size() {
                allocated(new_size - layout.size());
            } else {
                CURRENT.fetch_sub(layout.size() - new_size, Ordering::SeqCst);
            }
        }
        p
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Résultat de `f` et pic de mémoire allouée pendant son exécution, au-delà de ce qui l'était avant.
fn peak_during<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let base = CURRENT.load(Ordering::SeqCst);
    PEAK.store(base, Ordering::SeqCst);
    let out = f();
    (out, PEAK.load(Ordering::SeqCst).saturating_sub(base))
}

/// Texte de 50 Mio : lignes de 64 octets commençant par `word`.
fn fifty_mib(word: &str) -> String {
    const SIZE: usize = 50 * 1024 * 1024;
    let mut text = String::with_capacity(SIZE);
    for i in 0..SIZE / 64 {
        let _ = writeln!(text, "{word:<10} ligne {i:>12} {:.<33}", "");
    }
    assert_eq!(text.len(), SIZE);
    text
}

#[test]
fn the_diff_of_a_50_mib_file_is_truncated_without_holding_more_than_the_limit() {
    let t = TestRepo::new();
    t.write("petit.txt", "un\n");
    t.write("gros.txt", &fifty_mib("ancienne"));
    t.commit_all("Ajoute un gros fichier");
    t.write("petit.txt", "un\ndeux\n");
    t.write("gros.txt", &fifty_mib("nouvelle"));
    let repo = Repo::open(&t.git, t.path()).unwrap();
    let head = Oid::from_hex(t.run(&["rev-parse", "HEAD"]).trim()).unwrap();
    let options = DiffOptions::default();
    // Au-delà de la limite (8 Mio) : les tampons de lecture des deux tubes et les morceaux en transit, 64 Kio chacun
    // (huit au plus en attente) ; mesuré : environ 210 Kio. La marge est assez étroite pour voir une file de morceaux
    // sans borne : avec elle, l'un des quatre diffs au moins dépassait 512 Kio (mesuré : de 450 à 900 Kio).
    let max = MAX_DIFF_BYTES + 512 * 1024;

    // Commit qui ajoute le fichier : plus de 50 Mio de sortie.
    let added = FileChange { path: "gros.txt".into(), old_path: None, kind: ChangeKind::Added };
    let (diff, peak) = peak_during(|| repo.diff_commit_file(head, None, &added, &options).unwrap());
    assert!(diff.truncated && diff.hunks.is_empty(), "diff du commit tronqué");
    assert_eq!(diff.new_path.as_deref(), Some("gros.txt"));
    assert!(peak <= max, "diff du commit : pic de {peak} octets (limite {max})");

    // Copie de travail, puis index : toutes les lignes changent, plus de 100 Mio de sortie.
    let (diff, peak) = peak_during(|| repo.diff_worktree_file("gros.txt", false, false, &options).unwrap());
    assert!(diff.truncated && diff.hunks.is_empty(), "diff non indexé tronqué");
    assert!(peak <= max, "diff non indexé : pic de {peak} octets (limite {max})");
    t.run(&["add", "gros.txt"]);
    let (diff, peak) = peak_during(|| repo.diff_worktree_file("gros.txt", true, false, &options).unwrap());
    assert!(diff.truncated && diff.hunks.is_empty(), "diff indexé tronqué");
    assert!(peak <= max, "diff indexé : pic de {peak} octets (limite {max})");

    // Comparaison de révisions (même lecture).
    let (diff, peak) = peak_during(|| repo.compare_file_diff("HEAD", None, &added, &options).unwrap());
    assert!(diff.truncated, "comparaison tronquée");
    assert!(peak <= max, "comparaison : pic de {peak} octets (limite {max})");

    // Un petit fichier n'est pas touché.
    let small = repo.diff_worktree_file("petit.txt", false, false, &options).unwrap();
    assert!(!small.truncated);
    assert_eq!(small.added_count(), 1);
}
