//! Mesure sur un vrai dossier : `DRAGONTREE_SCAN_ROOT=<dossier> cargo test --release --test discovery_real -- --ignored --nocapture`.

use dragontree_git::CancelToken;
use dragontree_git::discovery::{ScanOptions, repo_head, scan};

#[test]
#[ignore]
fn real_scan_timings() {
    let Ok(root) = std::env::var("DRAGONTREE_SCAN_ROOT") else { return };
    let t0 = std::time::Instant::now();
    let found = scan(&[root.clone().into()], &ScanOptions::default(), &CancelToken::new(), &|_| {});
    let elapsed = t0.elapsed();
    for r in &found {
        println!("{:<28} {:<20} {:?} {:?}", r.name, r.relative_parent(), r.kind, repo_head(&r.path));
    }
    println!("{} dépôts en {elapsed:?}", found.len());
    let t1 = std::time::Instant::now();
    let deep = scan(
        &[root.into()],
        &ScanOptions { include_submodules: true, ..ScanOptions::default() },
        &CancelToken::new(),
        &|_| {},
    );
    println!("profond : {} dépôts en {:?}", deep.len(), t1.elapsed());
}
