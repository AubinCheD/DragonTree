//! Régénère `assets/dragontree.ico` à partir du dessin de l'icône (`dragontree_app::app_icon`), après un changement
//! de ce dessin : `cargo run -p dragontree-app --example app_icon`. Le fichier est versionné ; `build.rs` l'intègre à
//! l'exécutable.

fn main() -> std::io::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/dragontree.ico");
    std::fs::write(&path, dragontree_app::app_icon::ico_file())?;
    println!("{}", path.display());
    Ok(())
}
