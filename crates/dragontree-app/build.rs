//! Intègre l'icône `assets/dragontree.ico` à l'exécutable (ressources Windows RT_ICON et RT_GROUP_ICON, affichées par
//! l'Explorateur), sans compilateur de ressources : le script écrit lui-même l'objet COFF que produirait `cvtres`
//! (section `.rsrc$01` : répertoire des ressources, `.rsrc$02` : leurs données) et le passe à l'éditeur de liens des
//! binaires du paquet. Le même objet convient à ld (toolchain GNU) et à link.exe (toolchain MSVC).
//!
//! Le fichier `.ico` est versionné ; il se régénère depuis le dessin de l'icône (`src/app_icon.rs`) avec
//! `cargo run -p dragontree-app --example app_icon`.

use std::path::PathBuf;

const ICO: &str = "../../assets/dragontree.ico";

/// Types de ressources Windows.
const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;
/// Langue des ressources : neutre.
const LANG_NEUTRAL: u32 = 0;

/// Section de données initialisées, en lecture seule (comme celles de cvtres).
const SECTION_FLAGS: u32 = 0x4000_0040;
/// Classe de stockage des symboles de section.
const IMAGE_SYM_CLASS_STATIC: u8 = 3;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICO}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Machine COFF et type de relocation « adresse relative à l'image, 32 bits » (ADDR32NB) de la cible.
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let (machine, addr32nb) = match arch.as_str() {
        "x86_64" => (0x8664, 0x0003),
        "x86" => (0x014C, 0x0007),
        "aarch64" => (0xAA64, 0x0002),
        other => {
            println!("cargo:warning=icône non intégrée à l'exécutable : architecture {other} non prise en charge");
            return;
        }
    };
    let ico = std::fs::read(ICO).unwrap_or_else(|e| panic!("{ICO} illisible : {e}"));
    let images = ico_images(&ico).unwrap_or_else(|| panic!("{ICO} : fichier .ico mal formé"));
    let object = coff_object(machine, addr32nb, arch == "x86", &icon_resources(&images));
    let ext = if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") { "obj" } else { "o" };
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join(format!("dragontree-icon.{ext}"));
    std::fs::write(&out, object).unwrap_or_else(|e| panic!("{} : {e}", out.display()));
    // Les tests et exemples n'en ont pas besoin : seulement les binaires (dragontree.exe).
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

/// Image d'un fichier `.ico` : les 12 premiers octets de son entrée (dimensions, couleurs, plans, bits par pixel,
/// taille), repris tels quels dans le groupe d'icônes, et ses données.
struct IcoImage<'a> {
    entry: &'a [u8],
    data: &'a [u8],
}

fn ico_images(ico: &[u8]) -> Option<Vec<IcoImage<'_>>> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(ico.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(ico.get(at..at + 4)?.try_into().ok()?) as usize);
    if (u16_at(0)?, u16_at(2)?) != (0, 1) || u16_at(4)? == 0 {
        return None;
    }
    (0..u16_at(4)? as usize)
        .map(|i| {
            let at = 6 + i * 16;
            let (len, offset) = (u32_at(at + 8)?, u32_at(at + 12)?);
            Some(IcoImage { entry: ico.get(at..at + 12)?, data: ico.get(offset..offset.checked_add(len)?)? })
        })
        .collect()
}

/// Ressource à intégrer : type, numéro, données.
struct Resource {
    kind: u32,
    id: u32,
    data: Vec<u8>,
}

/// Une ressource RT_ICON par image (numéros 1, 2…) et le groupe RT_GROUP_ICON numéro 1 qui les liste : l'icône de
/// l'exécutable est le premier groupe.
fn icon_resources(images: &[IcoImage]) -> Vec<Resource> {
    let mut group = Vec::new();
    for v in [0u16, 1, images.len() as u16] {
        group.extend(v.to_le_bytes());
    }
    let mut resources = Vec::new();
    for (i, image) in images.iter().enumerate() {
        let id = i as u32 + 1;
        // Entrée du groupe : celle du `.ico`, le numéro de la ressource à la place de la position dans le fichier.
        group.extend(image.entry);
        group.extend((id as u16).to_le_bytes());
        resources.push(Resource { kind: RT_ICON, id, data: image.data.to_vec() });
    }
    resources.push(Resource { kind: RT_GROUP_ICON, id: 1, data: group });
    resources
}

/// En-tête d'un répertoire de ressources qui n'a que des entrées numérotées.
fn directory(out: &mut Vec<u8>, entries: usize) {
    out.extend([0; 12]);
    out.extend(0u16.to_le_bytes());
    out.extend((entries as u16).to_le_bytes());
}

/// Entrée d'un répertoire : numéro, puis position du sous-répertoire (bit de poids fort) ou de l'entrée de données.
fn entry(out: &mut Vec<u8>, id: u32, target: usize, subdirectory: bool) {
    out.extend(id.to_le_bytes());
    out.extend((target as u32 | if subdirectory { 0x8000_0000 } else { 0 }).to_le_bytes());
}

/// Objet COFF des ressources. `.rsrc$01` : arbre type → numéro → langue, puis une entrée de données par ressource,
/// dont l'adresse (relative à l'image) est relogée vers sa place dans `.rsrc$02`.
fn coff_object(machine: u16, addr32nb: u16, x86: bool, resources: &[Resource]) -> Vec<u8> {
    let mut resources: Vec<&Resource> = resources.iter().collect();
    resources.sort_by_key(|r| (r.kind, r.id));
    let mut kinds: Vec<u32> = resources.iter().map(|r| r.kind).collect();
    kinds.dedup();
    let of_kind = |kind: u32| resources.iter().filter(move |r| r.kind == kind);

    // Positions : racine, répertoires des types, répertoires des langues, entrées de données.
    let root_len = 16 + 8 * kinds.len();
    let mut kind_dirs = Vec::new();
    let mut at = root_len;
    for &kind in &kinds {
        kind_dirs.push(at);
        at += 16 + 8 * of_kind(kind).count();
    }
    let lang_dirs: Vec<usize> = (0..resources.len()).map(|i| at + i * 24).collect();
    let data_entries: Vec<usize> = (0..resources.len()).map(|i| at + resources.len() * 24 + i * 16).collect();

    // Données (`.rsrc$02`), chacune alignée sur 8 octets.
    let mut data = Vec::new();
    let mut data_offsets = Vec::new();
    for r in &resources {
        data_offsets.push(data.len());
        data.extend(&r.data);
        data.resize(data.len().next_multiple_of(8), 0);
    }

    let mut tree = Vec::new();
    directory(&mut tree, kinds.len());
    for (&kind, &dir) in kinds.iter().zip(&kind_dirs) {
        entry(&mut tree, kind, dir, true);
    }
    let mut index = 0;
    for &kind in &kinds {
        directory(&mut tree, of_kind(kind).count());
        for r in of_kind(kind) {
            entry(&mut tree, r.id, lang_dirs[index], true);
            index += 1;
        }
    }
    for &data_entry in &data_entries {
        directory(&mut tree, 1);
        entry(&mut tree, LANG_NEUTRAL, data_entry, false);
    }
    let mut relocations = Vec::new();
    for (i, r) in resources.iter().enumerate() {
        // Adresse : position dans `.rsrc$02`, à laquelle l'éditeur de liens ajoute l'adresse de la section.
        relocations.push(tree.len());
        tree.extend((data_offsets[i] as u32).to_le_bytes());
        tree.extend((r.data.len() as u32).to_le_bytes());
        tree.extend([0; 8]);
    }

    // Fichier : en-tête, deux en-têtes de section, `.rsrc$01` et ses relocations, `.rsrc$02`, symboles, chaînes.
    let tree_at = 20 + 2 * 40;
    let relocations_at = tree_at + tree.len();
    let data_at = relocations_at + 10 * relocations.len();
    let symbols_at = data_at + data.len();
    let symbols = if x86 { 5 } else { 4 };
    let mut out = Vec::new();
    out.extend(machine.to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend((symbols_at as u32).to_le_bytes());
    out.extend((symbols as u32).to_le_bytes());
    out.extend([0; 4]);
    let section = |out: &mut Vec<u8>, name: &[u8; 8], len: usize, at: usize, relocs_at: usize, relocs: usize| {
        out.extend(name);
        out.extend([0; 8]);
        out.extend((len as u32).to_le_bytes());
        out.extend((at as u32).to_le_bytes());
        out.extend((relocs_at as u32).to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend((relocs as u16).to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(SECTION_FLAGS.to_le_bytes());
    };
    section(&mut out, b".rsrc$01", tree.len(), tree_at, relocations_at, relocations.len());
    section(&mut out, b".rsrc$02", data.len(), data_at, 0, 0);
    out.extend(&tree);
    // Relocations vers le symbole de `.rsrc$02` (numéro 2 : chaque symbole de section a un auxiliaire).
    for &offset in &relocations {
        out.extend((offset as u32).to_le_bytes());
        out.extend(2u32.to_le_bytes());
        out.extend(addr32nb.to_le_bytes());
    }
    out.extend(&data);
    let symbol = |out: &mut Vec<u8>, name: &[u8; 8], value: u32, section: i16, aux: u8| {
        out.extend(name);
        out.extend(value.to_le_bytes());
        out.extend(section.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend([IMAGE_SYM_CLASS_STATIC, aux]);
    };
    // Auxiliaire d'un symbole de section : taille, nombre de relocations, le reste à zéro.
    let section_aux = |out: &mut Vec<u8>, len: usize, relocs: usize| {
        out.extend((len as u32).to_le_bytes());
        out.extend((relocs as u16).to_le_bytes());
        out.extend([0; 12]);
    };
    symbol(&mut out, b".rsrc$01", 0, 1, 1);
    section_aux(&mut out, tree.len(), relocations.len());
    symbol(&mut out, b".rsrc$02", 0, 2, 1);
    section_aux(&mut out, data.len(), 0);
    if x86 {
        // Objet compatible SAFESEH (il ne contient aucun code), comme ceux de cvtres.
        symbol(&mut out, b"@feat.00", 1, -1, 0);
    }
    // Table des chaînes vide : sa seule taille.
    out.extend(4u32.to_le_bytes());
    out
}
