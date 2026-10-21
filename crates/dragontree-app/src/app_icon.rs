//! Icône de l'application, dessinée en code : trois nœuds reliés sur un disque bleu. Elle sert d'icône de fenêtre
//! ([`window_icon`]) et donne le fichier versionné `assets/dragontree.ico` ([`ico_file`]), que `build.rs` intègre à
//! l'exécutable (ressource Windows, affichée par l'Explorateur). Après un changement du dessin, régénérer le fichier :
//! `cargo run -p dragontree-app --example app_icon` (un test vérifie qu'il correspond au dessin).

use image::ImageEncoder;

/// Tailles des images du fichier `.ico` (Explorateur, barre des tâches, Alt+Tab, écrans à forte densité).
pub const ICO_SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

/// À partir de cette taille, l'image du `.ico` est compressée en PNG ; en dessous, c'est un bitmap 32 bits.
const PNG_FROM: u32 = 256;

/// Grille du dessin : les coordonnées ci-dessous sont en unités d'une icône de 64 px.
const GRID: f32 = 64.0;

/// Pixels RGBA (alpha non prémultiplié) de l'icône à `size` × `size`. Le dessin est défini sur une grille de 64
/// unités ; ses bords sont lissés sur un pixel quelle que soit la taille.
pub fn rgba(size: u32) -> Vec<u8> {
    let n = size as usize;
    let scale = size as f32 / GRID;
    let mut rgba = vec![0u8; n * n * 4];
    // Couleur `c` posée par-dessus le pixel avec l'opacité `a` (composition « over », alpha non prémultiplié).
    let over = |px: &mut [u8], c: [u8; 3], a: f32| {
        let a = a.clamp(0.0, 1.0);
        let below = px[3] as f32 / 255.0;
        let alpha = a + below * (1.0 - a);
        if alpha <= 0.0 {
            return;
        }
        for k in 0..3 {
            px[k] = ((c[k] as f32 * a + px[k] as f32 * below * (1.0 - a)) / alpha).round() as u8;
        }
        px[3] = (alpha * 255.0).round() as u8;
    };
    let seg_dist = |p: (f32, f32), a: (f32, f32), b: (f32, f32)| {
        let (vx, vy) = (b.0 - a.0, b.1 - a.1);
        let t = (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
        ((p.0 - a.0 - t * vx).powi(2) + (p.1 - a.1 - t * vy).powi(2)).sqrt()
    };
    let dist = |p: (f32, f32), c: (f32, f32)| ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt();
    let (bottom, left, right, mid) = ((32.0, 50.0), (20.0, 14.0), (44.0, 14.0), (32.0, 34.0));
    for y in 0..n {
        for x in 0..n {
            // Centre du pixel, en unités du dessin ; les distances sont ramenées en pixels pour le lissage.
            let p = ((x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale);
            let i = (y * n + x) * 4;
            let px = &mut rgba[i..i + 4];
            over(px, [0x1F, 0x6F, 0xEB], (31.5 - dist(p, (32.0, 32.0))) * scale);
            let line = seg_dist(p, bottom, mid).min(seg_dist(p, mid, left)).min(seg_dist(p, mid, right));
            over(px, [255, 255, 255], (3.2 - line) * scale);
            for c in [bottom, left, right] {
                over(px, [255, 255, 255], (6.5 - dist(p, c)) * scale);
            }
        }
    }
    rgba
}

/// Icône de la fenêtre (64 px).
pub fn window_icon() -> egui::IconData {
    let size = GRID as u32;
    egui::IconData { rgba: rgba(size), width: size, height: size }
}

/// Image d'une entrée du `.ico` : bitmap (en-tête BITMAPINFOHEADER, pixels BGRA de bas en haut, masque ET) ou PNG.
fn ico_image(size: u32) -> Vec<u8> {
    let pixels = rgba(size);
    if size >= PNG_FROM {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&pixels, size, size, image::ExtendedColorType::Rgba8)
            .expect("encodage PNG en mémoire");
        return png;
    }
    let n = size as usize;
    let mask_row = n.div_ceil(32) * 4;
    let mut bmp = Vec::with_capacity(40 + n * n * 4 + mask_row * n);
    // Hauteur doublée : l'image puis le masque.
    for v in [40, size, size * 2] {
        bmp.extend(v.to_le_bytes());
    }
    bmp.extend(1u16.to_le_bytes());
    bmp.extend(32u16.to_le_bytes());
    for v in [0, (n * n * 4 + mask_row * n) as u32, 0, 0, 0, 0] {
        bmp.extend(v.to_le_bytes());
    }
    for y in (0..n).rev() {
        for x in 0..n {
            let p = &pixels[(y * n + x) * 4..][..4];
            bmp.extend([p[2], p[1], p[0], p[3]]);
        }
    }
    // Masque ET (1 = transparent) pour les programmes qui ignorent le canal alpha : seuls les pixels entièrement
    // transparents, pour ne pas rogner le bord lissé là où le masque s'ajoute à l'alpha.
    for y in (0..n).rev() {
        let mut row = vec![0u8; mask_row];
        for x in 0..n {
            if pixels[(y * n + x) * 4 + 3] == 0 {
                row[x / 8] |= 0x80 >> (x % 8);
            }
        }
        bmp.extend(row);
    }
    bmp
}

/// Contenu du fichier `assets/dragontree.ico` : une image par taille de [`ICO_SIZES`].
pub fn ico_file() -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = ICO_SIZES.iter().map(|&s| (s, ico_image(s))).collect();
    let mut ico = Vec::new();
    for v in [0u16, 1, images.len() as u16] {
        ico.extend(v.to_le_bytes());
    }
    let mut offset = 6 + 16 * images.len();
    for (size, data) in &images {
        // 0 code la taille 256 ; ni palette ni champ réservé ; un plan, 32 bits par pixel.
        let side = if *size >= 256 { 0 } else { *size as u8 };
        ico.extend([side, side, 0, 0]);
        ico.extend(1u16.to_le_bytes());
        ico.extend(32u16.to_le_bytes());
        ico.extend((data.len() as u32).to_le_bytes());
        ico.extend((offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in &images {
        ico.extend(data);
    }
    ico
}

/// Images d'un fichier `.ico` : (côté en pixels, données de l'image), dans l'ordre du fichier ; `None` si le fichier
/// est mal formé.
pub fn ico_images(ico: &[u8]) -> Option<Vec<(u32, &[u8])>> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(ico.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(ico.get(at..at + 4)?.try_into().ok()?) as usize);
    if (u16_at(0)?, u16_at(2)?) != (0, 1) {
        return None;
    }
    (0..u16_at(4)? as usize)
        .map(|i| {
            let entry = 6 + i * 16;
            let side = match *ico.get(entry)? {
                0 => 256,
                s => u32::from(s),
            };
            let (len, offset) = (u32_at(entry + 8)?, u32_at(entry + 12)?);
            Some((side, ico.get(offset..offset.checked_add(len)?)?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Décode une image du `.ico` (bitmap ou PNG) en la plaçant seule dans un fichier `.ico`.
    fn decode(side: u32, data: &[u8]) -> Vec<u8> {
        let mut single = Vec::new();
        for v in [0u16, 1, 1] {
            single.extend(v.to_le_bytes());
        }
        let side_byte = if side >= 256 { 0 } else { side as u8 };
        single.extend([side_byte, side_byte, 0, 0]);
        single.extend(1u16.to_le_bytes());
        single.extend(32u16.to_le_bytes());
        single.extend((data.len() as u32).to_le_bytes());
        single.extend(22u32.to_le_bytes());
        single.extend(data);
        image::load_from_memory_with_format(&single, image::ImageFormat::Ico).unwrap().to_rgba8().into_raw()
    }

    #[test]
    fn every_image_of_the_ico_file_is_the_drawing_at_its_size() {
        let ico = ico_file();
        let images = ico_images(&ico).expect("fichier .ico lisible");
        assert_eq!(images.iter().map(|(side, _)| *side).collect::<Vec<_>>(), ICO_SIZES);
        for (side, data) in images {
            assert_eq!(data.starts_with(b"\x89PNG"), side >= 256, "PNG pour 256 px seulement");
            assert!(decode(side, data) == rgba(side), "image de {side} px");
        }
    }

    #[test]
    fn the_committed_ico_file_matches_the_drawing() {
        // Régénérer avec `cargo run -p dragontree-app --example app_icon` si le dessin change.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/dragontree.ico");
        let committed = std::fs::read(&path).expect("assets/dragontree.ico versionné");
        let images = ico_images(&committed).expect("fichier .ico lisible");
        assert_eq!(images.iter().map(|(side, _)| *side).collect::<Vec<_>>(), ICO_SIZES);
        let generated = ico_file();
        for ((side, data), (_, expected)) in images.into_iter().zip(ico_images(&generated).unwrap()) {
            // Les bitmaps sont identiques ; une image PNG peut être compressée autrement : on compare ses pixels.
            assert!(data == expected || decode(side, data) == rgba(side), "image de {side} px différente du dessin");
        }
    }

    #[test]
    fn the_window_icon_is_the_64_px_drawing_with_a_transparent_corner() {
        let icon = window_icon();
        assert_eq!((icon.width, icon.height), (64, 64));
        assert_eq!(icon.rgba, rgba(64));
        assert_eq!(icon.rgba[3], 0, "coin transparent");
        let center = (32 * 64 + 32) * 4;
        assert_eq!(icon.rgba[center..center + 4], [255, 255, 255, 255], "nœud central blanc et opaque");
        // Bord du disque lissé sans frange sombre : la couleur reste le bleu, seule l'opacité baisse.
        let edge: Vec<_> = icon.rgba.chunks(4).filter(|p| p[3] > 0 && p[3] < 255).collect();
        assert!(!edge.is_empty(), "bord lissé");
        assert!(edge.iter().all(|p| p[..3] == [0x1F, 0x6F, 0xEB]), "{edge:?}");
    }
}
