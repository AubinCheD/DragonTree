//! Aperçu avant/après des images modifiées (au lieu de « Fichier binaire »).

use std::path::PathBuf;

use dragontree_git::{Oid, Repo};
use dragontree_i18n::tr;
use egui::{ColorImage, RichText, TextureHandle, TextureOptions, Ui};

use super::{RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::theme::Palette;

pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "ico"];

/// Taille maximale affichée (les grandes images sont réduites pour limiter la mémoire).
const MAX_SIDE: u32 = 1600;

pub fn is_image(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|e| IMAGE_EXTS.iter().any(|x| e.to_string_lossy().eq_ignore_ascii_case(x)))
}

pub struct DecodedImage {
    pub image: ColorImage,
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
}

/// Décode une image ; `None` si le format n'est pas reconnu.
pub fn decode(bytes: &[u8]) -> Option<DecodedImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let (width, height) = (img.width(), img.height());
    let shown = if width > MAX_SIDE || height > MAX_SIDE { img.thumbnail(MAX_SIDE, MAX_SIDE) } else { img };
    let rgba = shown.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(DecodedImage {
        image: ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
        width,
        height,
        bytes: bytes.len(),
    })
}

/// Origine d'une version de l'image.
#[derive(Clone, Debug)]
pub enum ImageSource {
    /// Blob à une révision (`""` = version indexée).
    Blob(String, String),
    File(PathBuf),
    Absent,
}

fn read(repo: &Repo, src: &ImageSource) -> Option<DecodedImage> {
    let bytes = match src {
        ImageSource::Blob(rev, path) => repo.blob(rev, path).ok()?,
        ImageSource::File(p) => std::fs::read(p).ok()?,
        ImageSource::Absent => return None,
    };
    decode(&bytes)
}

struct Shown {
    texture: TextureHandle,
    width: u32,
    height: u32,
    bytes: usize,
}

#[derive(Default)]
pub struct ImagePreview {
    key: Option<String>,
    loading: bool,
    old: Option<Shown>,
    new: Option<Shown>,
}

impl ImagePreview {
    pub fn set(&mut self, ctx: &egui::Context, key: String, old: Option<DecodedImage>, new: Option<DecodedImage>) {
        if self.key.as_ref() != Some(&key) {
            return;
        }
        self.loading = false;
        let mk = |name: &str, d: DecodedImage| Shown {
            texture: ctx.load_texture(format!("{key}:{name}"), d.image, TextureOptions::LINEAR),
            width: d.width,
            height: d.height,
            bytes: d.bytes,
        };
        self.old = old.map(|d| mk("avant", d));
        self.new = new.map(|d| mk("après", d));
    }

    fn ui(&mut self, ui: &mut Ui, p: &Palette) {
        if self.loading {
            ui.centered_and_justified(|ui| ui.spinner());
            return;
        }
        ui.columns(2, |cols| {
            let (left, right) = cols.split_at_mut(1);
            for (col, title, shown) in
                [(&mut left[0], tr!("Avant"), &self.old), (&mut right[0], tr!("Après"), &self.new)]
            {
                col.vertical_centered(|ui| match shown {
                    Some(s) => {
                        ui.label(
                            RichText::new(tr!(
                                "{title} — {width}×{height} px, {size} Ko",
                                title,
                                width = s.width,
                                height = s.height,
                                size = format!("{:.1}", s.bytes as f32 / 1024.0)
                            ))
                            .color(p.weak),
                        );
                        let avail = ui.available_size() - egui::vec2(8.0, 8.0);
                        let size = s.texture.size_vec2();
                        let scale = (avail.x / size.x).min(avail.y / size.y).min(1.0);
                        ui.add(egui::Image::new((s.texture.id(), size * scale)));
                    }
                    None => {
                        ui.label(RichText::new(tr!("{title} — absente", title)).color(p.weak));
                    }
                });
            }
        });
    }
}

impl RepoTab {
    /// Affiche l'aperçu ; lance le chargement si la clé a changé.
    pub(super) fn image_preview_ui(
        &mut self,
        ui: &mut Ui,
        shared: &Shared,
        history: bool,
        key: String,
        old: ImageSource,
        new: ImageSource,
        p: &Palette,
    ) {
        let preview = if history { &mut self.history.image } else { &mut self.changes.image };
        if preview.key.as_ref() != Some(&key) {
            *preview = ImagePreview { key: Some(key.clone()), loading: true, old: None, new: None };
            if let Some(repo) = self.repo.clone() {
                let id = self.id;
                shared.jobs.spawn("images", move || {
                    let (o, n) = (read(&repo, &old), read(&repo, &new));
                    Some(AppEvent::Repo(id, RepoEvent::Images { history, key, old: o, new: n }))
                });
            }
        }
        let preview = if history { &mut self.history.image } else { &mut self.changes.image };
        preview.ui(ui, p);
    }

    /// Sources avant/après d'une image modifiée par un commit.
    pub(super) fn commit_image_sources(
        commit: Oid,
        parent: Option<Oid>,
        change: &dragontree_git::FileChange,
    ) -> (ImageSource, ImageSource) {
        use dragontree_git::ChangeKind;
        let old_path = change.old_path.clone().unwrap_or_else(|| change.path.clone());
        let old = match (parent, change.kind) {
            (Some(p), k) if k != ChangeKind::Added => ImageSource::Blob(p.to_hex(), old_path),
            _ => ImageSource::Absent,
        };
        let new = if change.kind == ChangeKind::Deleted {
            ImageSource::Absent
        } else {
            ImageSource::Blob(commit.to_hex(), change.path.clone())
        };
        (old, new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_extensions() {
        assert!(is_image("logo.PNG"));
        assert!(is_image("a/b/photo.jpeg"));
        assert!(!is_image("doc.pdf"));
    }

    #[test]
    fn decode_small_png_and_reject_garbage() {
        let mut png = Vec::new();
        let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([255, 0, 0, 255]));
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let d = decode(&png).unwrap();
        assert_eq!((d.width, d.height), (3, 2));
        assert_eq!(d.image.size, [3, 2]);
        assert!(decode(b"pas une image").is_none());
    }
}
