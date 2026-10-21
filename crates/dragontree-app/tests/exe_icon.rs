//! L'exécutable construit porte l'icône de l'application (ressources Windows RT_GROUP_ICON et RT_ICON, ajoutées par
//! `build.rs` depuis `assets/dragontree.ico`) : c'est elle que montrent l'Explorateur et les raccourcis. Le test lit
//! le répertoire des ressources du fichier PE, sans outil externe.

#![cfg(windows)]

/// Types de ressources Windows.
const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Fichier PE lu : contenu et table des sections (adresse virtuelle, taille, position dans le fichier).
struct Pe {
    bytes: Vec<u8>,
    sections: Vec<(u32, u32, u32)>,
    resources_rva: u32,
}

impl Pe {
    fn read(bytes: Vec<u8>) -> Pe {
        assert_eq!(&bytes[..2], b"MZ", "en-tête DOS");
        let pe = u32_at(&bytes, 0x3C) as usize;
        assert_eq!(&bytes[pe..pe + 4], b"PE\0\0", "signature PE");
        let sections_count = u16_at(&bytes, pe + 6) as usize;
        let optional_size = u16_at(&bytes, pe + 20) as usize;
        let optional = pe + 24;
        // Répertoires de données : après 96 octets (PE32) ou 112 (PE32+) ; les ressources sont le troisième.
        let directories = optional + if u16_at(&bytes, optional) == 0x20b { 112 } else { 96 };
        let resources_rva = u32_at(&bytes, directories + 2 * 8);
        let table = optional + optional_size;
        let sections = (0..sections_count)
            .map(|i| {
                let s = table + i * 40;
                let size = u32_at(&bytes, s + 8).max(u32_at(&bytes, s + 16));
                (u32_at(&bytes, s + 12), size, u32_at(&bytes, s + 20))
            })
            .collect();
        Pe { bytes, sections, resources_rva }
    }

    /// Position dans le fichier d'une adresse virtuelle relative.
    fn offset(&self, rva: u32) -> usize {
        let (va, _, raw) = self
            .sections
            .iter()
            .find(|(va, size, _)| (*va..va + size).contains(&rva))
            .unwrap_or_else(|| panic!("adresse {rva:#x} hors des sections"));
        (raw + (rva - va)) as usize
    }

    /// Entrées numérotées d'un répertoire de ressources (`dir` : position relative au début des ressources) :
    /// (numéro, position relative du sous-répertoire ou de l'entrée de données, vrai pour un sous-répertoire).
    fn entries(&self, dir: u32) -> Vec<(u32, u32, bool)> {
        let at = self.offset(self.resources_rva + dir);
        let (named, ids) = (u16_at(&self.bytes, at + 12) as usize, u16_at(&self.bytes, at + 14) as usize);
        (named..named + ids)
            .map(|i| {
                let entry = at + 16 + i * 8;
                let target = u32_at(&self.bytes, entry + 4);
                (u32_at(&self.bytes, entry), target & 0x7FFF_FFFF, target & 0x8000_0000 != 0)
            })
            .collect()
    }

    /// Données de la ressource `kind` / `id` (première langue).
    fn resource(&self, kind: u32, id: u32) -> Option<&[u8]> {
        let find = |dir: u32, id: u32| self.entries(dir).into_iter().find(|(n, _, _)| *n == id);
        let (_, by_id, _) = find(0, kind)?;
        let (_, by_lang, _) = find(by_id, id)?;
        let (_, data_entry, is_dir) = *self.entries(by_lang).first()?;
        assert!(!is_dir, "le niveau des langues mène aux données");
        let at = self.offset(self.resources_rva + data_entry);
        let (rva, size) = (u32_at(&self.bytes, at), u32_at(&self.bytes, at + 4) as usize);
        let start = self.offset(rva);
        Some(&self.bytes[start..start + size])
    }
}

/// Images du fichier `.ico` : (largeur, données).
fn ico_images(ico: &[u8]) -> Vec<(u32, &[u8])> {
    assert_eq!((u16_at(ico, 0), u16_at(ico, 2)), (0, 1), "en-tête d'un fichier .ico");
    (0..u16_at(ico, 4) as usize)
        .map(|i| {
            let e = 6 + i * 16;
            let width = if ico[e] == 0 { 256 } else { u32::from(ico[e]) };
            let (size, offset) = (u32_at(ico, e + 8) as usize, u32_at(ico, e + 12) as usize);
            (width, &ico[offset..offset + size])
        })
        .collect()
}

#[test]
fn the_executable_carries_the_application_icon_at_every_size() {
    let pe = Pe::read(std::fs::read(env!("CARGO_BIN_EXE_dragontree")).unwrap());
    assert_ne!(pe.resources_rva, 0, "l'exécutable n'a aucune ressource");
    // Le groupe d'icônes : celui que l'Explorateur affiche (le premier, numéro 1).
    let group = pe.resource(RT_GROUP_ICON, 1).expect("ressource RT_GROUP_ICON 1");
    assert_eq!((u16_at(group, 0), u16_at(group, 2)), (0, 1), "en-tête du groupe");
    let ico = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/dragontree.ico")).unwrap();
    let images = ico_images(&ico);
    let count = u16_at(group, 4) as usize;
    assert_eq!(count, images.len(), "une entrée par image du fichier .ico");
    let mut widths = Vec::new();
    for (i, (width, data)) in images.iter().enumerate() {
        // Entrée du groupe (14 octets) : largeur, hauteur, …, taille de l'image, numéro de la ressource RT_ICON.
        let e = 6 + i * 14;
        let group_width = if group[e] == 0 { 256 } else { u32::from(group[e]) };
        assert_eq!(group_width, *width);
        assert_eq!(u16_at(group, e + 4), 1, "plans");
        assert_eq!(u16_at(group, e + 6), 32, "bits par pixel");
        assert_eq!(u32_at(group, e + 8) as usize, data.len());
        let icon = pe.resource(RT_ICON, u32::from(u16_at(group, e + 12))).expect("ressource RT_ICON");
        assert!(icon == *data, "image de {width} px identique à celle du fichier .ico");
        widths.push(group_width);
    }
    assert_eq!(widths, [16, 24, 32, 48, 64, 256]);
}
