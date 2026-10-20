//! Aperçu avant/après des images modifiées (au lieu de « Fichier binaire »).

use std::path::PathBuf;

use dragontree_git::{Oid, Repo};
use dragontree_i18n::tr;
use egui::{Color32, ColorImage, Pos2, Rect, RichText, Sense, TextureHandle, TextureOptions, Ui, Vec2};

use super::{RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::settings::ImageDiffMode;
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

/// Partie d'une image visible d'un côté du curseur de balayage : rectangle à l'écran et rectangle de la texture
/// (coordonnées UV) qui l'habille.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slice {
    pub rect: Rect,
    pub uv: Rect,
}

/// Les deux moitiés du balayage : l'ancienne image à gauche du curseur, la nouvelle à sa droite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwipeLayout {
    pub old: Option<Slice>,
    pub new: Option<Slice>,
}

/// Géométrie du balayage. Les deux images sont alignées en haut à gauche sur `origin` et à la même échelle
/// (`scale`, appliquée à leurs tailles d'origine `old_size` et `new_size`, en pixels) ; `split_x` est l'abscisse
/// du curseur à l'écran. Une moitié vide (curseur hors de l'image) vaut `None`.
pub fn swipe_layout(origin: Pos2, scale: f32, old_size: Vec2, new_size: Vec2, split_x: f32) -> SwipeLayout {
    SwipeLayout {
        old: clip_x(origin, scale, old_size, f32::NEG_INFINITY, split_x),
        new: clip_x(origin, scale, new_size, split_x, f32::INFINITY),
    }
}

/// Partie de l'image (posée sur `origin`) comprise entre les abscisses `min_x` et `max_x` de l'écran.
fn clip_x(origin: Pos2, scale: f32, size: Vec2, min_x: f32, max_x: f32) -> Option<Slice> {
    let full = Rect::from_min_size(origin, size * scale);
    let (x0, x1) = (full.left().max(min_x), full.right().min(max_x));
    if x1 <= x0 || full.width() <= 0.0 {
        return None;
    }
    let u = |x: f32| (x - full.left()) / full.width();
    Some(Slice {
        rect: Rect::from_min_max(Pos2::new(x0, full.top()), Pos2::new(x1, full.bottom())),
        uv: Rect::from_min_max(Pos2::new(u(x0), 0.0), Pos2::new(u(x1), 1.0)),
    })
}

struct Shown {
    texture: TextureHandle,
    width: u32,
    height: u32,
    bytes: usize,
}

impl Shown {
    /// Taille d'origine en pixels (la texture peut avoir été réduite, voir `MAX_SIDE`).
    fn size(&self) -> Vec2 {
        egui::vec2(self.width as f32, self.height as f32)
    }
}

pub struct ImagePreview {
    key: Option<String>,
    loading: bool,
    old: Option<Shown>,
    new: Option<Shown>,
    /// Position du curseur de balayage, de 0 (à gauche) à 1 (à droite) ; propre à la session.
    split: f32,
    /// Opacité de la nouvelle image en superposition ; propre à la session.
    opacity: f32,
}

impl Default for ImagePreview {
    fn default() -> Self {
        ImagePreview { key: None, loading: false, old: None, new: None, split: 0.5, opacity: 0.5 }
    }
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

    /// Légende d'une version : titre, dimensions d'origine et poids.
    fn caption(title: &str, s: &Shown) -> String {
        tr!(
            "{title} — {width}×{height} px, {size} Ko",
            title,
            width = s.width,
            height = s.height,
            size = format!("{:.1}", s.bytes as f32 / 1024.0)
        )
    }

    /// Dessine l'aperçu dans le mode du réglage ; renvoie le mode choisi par l'utilisateur, s'il l'a changé. Le
    /// balayage et la superposition comparent deux versions : sans l'une des deux (image ajoutée ou supprimée), seul
    /// le côte à côte a un sens et le sélecteur est omis.
    fn ui(&mut self, ui: &mut Ui, p: &Palette, mode: ImageDiffMode) -> Option<ImageDiffMode> {
        if self.loading {
            ui.centered_and_justified(|ui| ui.spinner());
            return None;
        }
        let mut chosen = None;
        if self.old.is_some() && self.new.is_some() {
            ui.horizontal(|ui| {
                for (m, label) in [
                    (ImageDiffMode::SideBySide, tr!("Côte à côte")),
                    (ImageDiffMode::Swipe, tr!("Balayage")),
                    (ImageDiffMode::Overlay, tr!("Superposition")),
                ] {
                    if ui.selectable_label(mode == m, label).clicked() && mode != m {
                        chosen = Some(m);
                    }
                }
            });
        }
        // Le choix s'applique dès cette image.
        match (chosen.unwrap_or(mode), &self.old, &self.new) {
            (ImageDiffMode::Swipe, Some(old), Some(new)) => {
                Self::caption_row(ui, p, old, new);
                let (rect, scale) = Self::canvas(ui, p, old, new);
                self.split = Self::swipe(ui, p, rect, scale, old, new, self.split);
            }
            (ImageDiffMode::Overlay, Some(old), Some(new)) => {
                ui.horizontal(|ui| {
                    ui.label(tr!("Opacité de la nouvelle image"));
                    ui.add(egui::Slider::new(&mut self.opacity, 0.0..=1.0));
                });
                Self::caption_row(ui, p, old, new);
                let (rect, scale) = Self::canvas(ui, p, old, new);
                let whole = |s: &Shown| Rect::from_min_size(rect.min, s.size() * scale);
                let full_uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                ui.painter().image(old.texture.id(), whole(old), full_uv, Color32::WHITE);
                let tint = Color32::from_white_alpha((self.opacity * 255.0).round() as u8);
                ui.painter().image(new.texture.id(), whole(new), full_uv, tint);
            }
            _ => self.side_by_side(ui, p),
        }
        chosen
    }

    fn side_by_side(&self, ui: &mut Ui, p: &Palette) {
        ui.columns(2, |cols| {
            let (left, right) = cols.split_at_mut(1);
            for (col, title, shown) in
                [(&mut left[0], tr!("Avant"), &self.old), (&mut right[0], tr!("Après"), &self.new)]
            {
                col.vertical_centered(|ui| match shown {
                    Some(s) => {
                        ui.label(RichText::new(Self::caption(title, s)).color(p.weak));
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

    fn caption_row(ui: &mut Ui, p: &Palette, old: &Shown, new: &Shown) {
        let text = format!("{}   |   {}", Self::caption(tr!("Avant"), old), Self::caption(tr!("Après"), new));
        ui.vertical_centered(|ui| ui.label(RichText::new(text).color(p.weak)));
    }

    /// Place des deux versions superposées : alignées en haut à gauche, à la même échelle (celle qui fait tenir la
    /// plus grande dans la place disponible, sans agrandir). Renvoie le rectangle (fond posé) et l'échelle.
    fn canvas(ui: &mut Ui, p: &Palette, old: &Shown, new: &Shown) -> (Rect, f32) {
        let canvas = old.size().max(new.size());
        let avail = ui.available_size() - egui::vec2(8.0, 8.0);
        let scale = (avail.x / canvas.x).min(avail.y / canvas.y).clamp(0.01, 1.0);
        let rect = ui.vertical_centered(|ui| ui.allocate_exact_size(canvas * scale, Sense::hover()).0).inner;
        ui.painter().rect_filled(rect, 0.0, p.panel);
        (rect, scale)
    }

    /// Balayage : l'ancienne version à gauche du curseur, la nouvelle à sa droite ; le curseur est une poignée
    /// verticale à glisser. Renvoie sa position (0 à 1). Les textures sont celles déjà chargées : seules leurs
    /// coordonnées changent d'une image à l'autre (voir [`swipe_layout`]).
    fn swipe(ui: &mut Ui, p: &Palette, rect: Rect, scale: f32, old: &Shown, new: &Shown, split: f32) -> f32 {
        let mut split = split.clamp(0.0, 1.0);
        let x = rect.left() + split * rect.width();
        let handle = Rect::from_x_y_ranges((x - 6.0)..=(x + 6.0), rect.y_range());
        let resp = ui.interact(handle, ui.scope_id().with("swipe-handle"), Sense::drag());
        crate::widgets::a11y(&resp, tr!("Curseur de balayage"), true);
        if resp.dragged() && rect.width() > 0.0 {
            split = (split + resp.drag_delta().x / rect.width()).clamp(0.0, 1.0);
        }
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        let x = rect.left() + split * rect.width();
        let layout = swipe_layout(rect.min, scale, old.size(), new.size(), x);
        for (slice, shown) in [(layout.old, old), (layout.new, new)] {
            if let Some(s) = slice {
                ui.painter().image(shown.texture.id(), s.rect, s.uv, Color32::WHITE);
            }
        }
        ui.painter().vline(x, rect.y_range(), egui::Stroke::new(2.0, p.accent));
        ui.painter().circle(egui::pos2(x, rect.center().y), 7.0, p.accent, egui::Stroke::new(1.5, Color32::WHITE));
        split
    }
}

impl RepoTab {
    /// Affiche l'aperçu ; lance le chargement si la clé a changé.
    pub(super) fn image_preview_ui(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        history: bool,
        key: String,
        old: ImageSource,
        new: ImageSource,
        p: &Palette,
    ) {
        let preview = if history { &mut self.history.image } else { &mut self.changes.image };
        if preview.key.as_ref() != Some(&key) {
            *preview = ImagePreview { key: Some(key.clone()), loading: true, ..ImagePreview::default() };
            if let Some(repo) = self.repo.clone() {
                let id = self.id;
                shared.jobs.spawn("images", move || {
                    let (o, n) = (read(&repo, &old), read(&repo, &new));
                    Some(AppEvent::Repo(id, RepoEvent::Images { history, key, old: o, new: n }))
                });
            }
        }
        let preview = if history { &mut self.history.image } else { &mut self.changes.image };
        if let Some(mode) = preview.ui(ui, p, shared.settings.image_diff_mode) {
            shared.settings.image_diff_mode = mode;
            shared.settings_dirty = true;
        }
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

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
    }

    #[test]
    fn swipe_of_equal_sizes_splits_the_image_at_the_cursor() {
        let size = egui::vec2(100.0, 50.0);
        let layout = swipe_layout(Pos2::new(10.0, 20.0), 1.0, size, size, 60.0);
        assert_eq!(layout.old, Some(Slice { rect: rect(10.0, 20.0, 60.0, 70.0), uv: rect(0.0, 0.0, 0.5, 1.0) }));
        assert_eq!(layout.new, Some(Slice { rect: rect(60.0, 20.0, 110.0, 70.0), uv: rect(0.5, 0.0, 1.0, 1.0) }));
    }

    #[test]
    fn swipe_applies_the_same_scale_to_both_images() {
        let size = egui::vec2(100.0, 50.0);
        let layout = swipe_layout(Pos2::ZERO, 0.5, size, size, 10.0);
        assert_eq!(layout.old, Some(Slice { rect: rect(0.0, 0.0, 10.0, 25.0), uv: rect(0.0, 0.0, 0.2, 1.0) }));
        assert_eq!(layout.new, Some(Slice { rect: rect(10.0, 0.0, 50.0, 25.0), uv: rect(0.2, 0.0, 1.0, 1.0) }));
    }

    #[test]
    fn swipe_of_different_sizes_aligns_them_top_left() {
        // Ancienne 100x50, nouvelle 40x80 : le curseur à 30 coupe les deux, chacune par rapport à sa propre taille.
        let layout = swipe_layout(Pos2::ZERO, 1.0, egui::vec2(100.0, 50.0), egui::vec2(40.0, 80.0), 30.0);
        assert_eq!(layout.old, Some(Slice { rect: rect(0.0, 0.0, 30.0, 50.0), uv: rect(0.0, 0.0, 0.3, 1.0) }));
        assert_eq!(layout.new, Some(Slice { rect: rect(30.0, 0.0, 40.0, 80.0), uv: rect(0.75, 0.0, 1.0, 1.0) }));
        // Au-delà du bord droit de la nouvelle image, il ne reste que l'ancienne à droite du curseur... et rien de la
        // nouvelle ; à gauche de tout, l'inverse.
        let right = swipe_layout(Pos2::ZERO, 1.0, egui::vec2(100.0, 50.0), egui::vec2(40.0, 80.0), 70.0);
        assert!(right.new.is_none());
        assert_eq!(right.old.map(|s| s.rect), Some(rect(0.0, 0.0, 70.0, 50.0)));
        let left = swipe_layout(Pos2::ZERO, 1.0, egui::vec2(100.0, 50.0), egui::vec2(40.0, 80.0), 0.0);
        assert!(left.old.is_none());
        assert_eq!(left.new.map(|s| s.uv), Some(rect(0.0, 0.0, 1.0, 1.0)));
    }

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
