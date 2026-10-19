//! Apparence : polices (Segoe UI, Cascadia Mono, Phosphor), thèmes clair/sombre, couleurs métier.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};

/// Famille de la police grasse (`FontFamily::Name("bold")`).
pub const BOLD: &str = "bold";

pub fn bold_family() -> FontFamily {
    FontFamily::Name(BOLD.into())
}

fn load_system_font(names: &[&str]) -> Option<Vec<u8>> {
    let fonts_dir = std::env::var_os("WINDIR").map(std::path::PathBuf::from)?.join("Fonts");
    names.iter().find_map(|n| std::fs::read(fonts_dir.join(n)).ok())
}

/// Polices : Segoe UI pour l'interface, Cascadia Mono/Consolas pour le code, Phosphor pour les
/// icônes ; les polices par défaut d'egui restent en dernier recours (emoji, autres alphabets).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let default_prop = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let default_mono = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();

    fonts.font_data.insert(
        "phosphor".into(),
        Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/Phosphor.ttf"))),
    );

    let mut prop = Vec::new();
    let mut bold = Vec::new();
    let mut mono = Vec::new();
    if let Some(bytes) = load_system_font(&["segoeui.ttf"]) {
        fonts.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(bytes)));
        prop.push("segoe".to_string());
    }
    if let Some(bytes) = load_system_font(&["segoeuisb.ttf", "seguisb.ttf", "segoeuib.ttf"]) {
        fonts.font_data.insert("segoe-bold".into(), Arc::new(FontData::from_owned(bytes)));
        bold.push("segoe-bold".to_string());
    }
    if let Some(bytes) = load_system_font(&["CascadiaMono.ttf", "consola.ttf"]) {
        fonts.font_data.insert("mono".into(), Arc::new(FontData::from_owned(bytes)));
        mono.push("mono".to_string());
    }

    prop.push("phosphor".into());
    prop.extend(default_prop.iter().cloned());
    bold.extend(prop.iter().cloned());
    mono.push("phosphor".into());
    mono.extend(default_mono.iter().cloned());
    mono.extend(default_prop.iter().cloned());

    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(FontFamily::Monospace, mono);
    fonts.families.insert(bold_family(), bold);
    ctx.set_fonts(fonts);
}

/// Couleurs propres à DragonTree, déclinées selon le thème.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub sidebar: Color32,
    pub border: Color32,
    pub text: Color32,
    pub weak: Color32,
    pub accent: Color32,
    pub selection: Color32,
    pub selection_text: Color32,
    pub hover: Color32,
    pub row_alt: Color32,
    pub added_bg: Color32,
    pub added_hl: Color32,
    pub removed_bg: Color32,
    pub removed_hl: Color32,
    pub hunk_bg: Color32,
    pub gutter: Color32,
    pub added_fg: Color32,
    pub removed_fg: Color32,
    pub modified_fg: Color32,
    pub renamed_fg: Color32,
    pub conflict_fg: Color32,
    pub warning_bg: Color32,
    pub badge_local: (Color32, Color32),
    pub badge_remote: (Color32, Color32),
    pub badge_tag: (Color32, Color32),
    pub badge_head: (Color32, Color32),
    pub graph: [Color32; 10],
}

const GRAPH_LIGHT: [Color32; 10] = [
    Color32::from_rgb(0x2F, 0x80, 0xED),
    Color32::from_rgb(0xE0, 0x4F, 0x4F),
    Color32::from_rgb(0x27, 0xAE, 0x60),
    Color32::from_rgb(0xF2, 0x99, 0x4A),
    Color32::from_rgb(0x9B, 0x51, 0xE0),
    Color32::from_rgb(0x00, 0xA3, 0xA3),
    Color32::from_rgb(0xD4, 0xA0, 0x00),
    Color32::from_rgb(0xE9, 0x1E, 0x63),
    Color32::from_rgb(0x3B, 0xB4, 0xE5),
    Color32::from_rgb(0x8D, 0x6E, 0x63),
];

const GRAPH_DARK: [Color32; 10] = [
    Color32::from_rgb(0x5A, 0x9B, 0xF5),
    Color32::from_rgb(0xF0, 0x6C, 0x6C),
    Color32::from_rgb(0x4C, 0xC7, 0x7F),
    Color32::from_rgb(0xF5, 0xAD, 0x6A),
    Color32::from_rgb(0xB3, 0x7C, 0xEB),
    Color32::from_rgb(0x2E, 0xC4, 0xC4),
    Color32::from_rgb(0xE8, 0xC1, 0x3F),
    Color32::from_rgb(0xF0, 0x5C, 0x8E),
    Color32::from_rgb(0x6C, 0xCB, 0xF2),
    Color32::from_rgb(0xB0, 0x90, 0x84),
];

impl Palette {
    pub fn light() -> Palette {
        Palette {
            dark: false,
            bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            panel: Color32::from_rgb(0xFA, 0xFB, 0xFC),
            sidebar: Color32::from_rgb(0xF3, 0xF4, 0xF6),
            border: Color32::from_rgb(0xE1, 0xE4, 0xE8),
            text: Color32::from_rgb(0x1F, 0x23, 0x28),
            weak: Color32::from_rgb(0x6B, 0x72, 0x80),
            accent: Color32::from_rgb(0x1F, 0x6F, 0xEB),
            selection: Color32::from_rgb(0xCC, 0xE0, 0xFF),
            selection_text: Color32::from_rgb(0x0B, 0x2E, 0x6F),
            hover: Color32::from_rgb(0xEC, 0xF2, 0xFB),
            row_alt: Color32::from_rgb(0xFA, 0xFA, 0xFB),
            added_bg: Color32::from_rgb(0xE6, 0xFF, 0xEC),
            added_hl: Color32::from_rgb(0xAB, 0xF2, 0xBC),
            removed_bg: Color32::from_rgb(0xFF, 0xEB, 0xE9),
            removed_hl: Color32::from_rgb(0xFF, 0xC0, 0xC0),
            hunk_bg: Color32::from_rgb(0xEE, 0xF5, 0xFF),
            gutter: Color32::from_rgb(0xF6, 0xF8, 0xFA),
            added_fg: Color32::from_rgb(0x1A, 0x7F, 0x37),
            removed_fg: Color32::from_rgb(0xCF, 0x22, 0x2E),
            modified_fg: Color32::from_rgb(0xBC, 0x76, 0x00),
            renamed_fg: Color32::from_rgb(0x09, 0x69, 0xDA),
            conflict_fg: Color32::from_rgb(0xD1, 0x24, 0x2F),
            warning_bg: Color32::from_rgb(0xFF, 0xF4, 0xD6),
            badge_local: (Color32::from_rgb(0xDB, 0xEA, 0xFF), Color32::from_rgb(0x0B, 0x4F, 0xB3)),
            badge_remote: (Color32::from_rgb(0xEC, 0xE6, 0xF8), Color32::from_rgb(0x5B, 0x3B, 0x9A)),
            badge_tag: (Color32::from_rgb(0xFF, 0xF1, 0xC9), Color32::from_rgb(0x7A, 0x52, 0x00)),
            badge_head: (Color32::from_rgb(0xFF, 0xDF, 0xDB), Color32::from_rgb(0xA1, 0x1B, 0x1B)),
            graph: GRAPH_LIGHT,
        }
    }

    pub fn dark() -> Palette {
        Palette {
            dark: true,
            bg: Color32::from_rgb(0x1E, 0x1F, 0x22),
            panel: Color32::from_rgb(0x23, 0x25, 0x29),
            sidebar: Color32::from_rgb(0x27, 0x29, 0x2E),
            border: Color32::from_rgb(0x3A, 0x3D, 0x43),
            text: Color32::from_rgb(0xDF, 0xE1, 0xE5),
            weak: Color32::from_rgb(0x8C, 0x91, 0x99),
            accent: Color32::from_rgb(0x4C, 0x8D, 0xF6),
            selection: Color32::from_rgb(0x2D, 0x45, 0x6B),
            selection_text: Color32::from_rgb(0xF2, 0xF6, 0xFF),
            hover: Color32::from_rgb(0x2C, 0x30, 0x36),
            row_alt: Color32::from_rgb(0x21, 0x23, 0x26),
            added_bg: Color32::from_rgb(0x15, 0x2B, 0x1F),
            added_hl: Color32::from_rgb(0x1F, 0x5E, 0x37),
            removed_bg: Color32::from_rgb(0x32, 0x18, 0x1B),
            removed_hl: Color32::from_rgb(0x73, 0x2B, 0x31),
            hunk_bg: Color32::from_rgb(0x1C, 0x2A, 0x3B),
            gutter: Color32::from_rgb(0x24, 0x26, 0x2B),
            added_fg: Color32::from_rgb(0x56, 0xD3, 0x64),
            removed_fg: Color32::from_rgb(0xF8, 0x71, 0x6C),
            modified_fg: Color32::from_rgb(0xE3, 0xB3, 0x41),
            renamed_fg: Color32::from_rgb(0x58, 0xA6, 0xFF),
            conflict_fg: Color32::from_rgb(0xFF, 0x6B, 0x6B),
            warning_bg: Color32::from_rgb(0x3D, 0x32, 0x14),
            badge_local: (Color32::from_rgb(0x1E, 0x3A, 0x63), Color32::from_rgb(0xA8, 0xCB, 0xFF)),
            badge_remote: (Color32::from_rgb(0x3A, 0x2C, 0x58), Color32::from_rgb(0xD2, 0xBE, 0xF7)),
            badge_tag: (Color32::from_rgb(0x4A, 0x3A, 0x10), Color32::from_rgb(0xF5, 0xD3, 0x7A)),
            badge_head: (Color32::from_rgb(0x5A, 0x1E, 0x1E), Color32::from_rgb(0xFF, 0xB3, 0xAB)),
            graph: GRAPH_DARK,
        }
    }

    pub fn current(ctx: &egui::Context) -> Palette {
        if ctx.global_style().visuals.dark_mode { Palette::dark() } else { Palette::light() }
    }

    pub fn lane(&self, color: u8) -> Color32 {
        self.graph[color as usize % self.graph.len()]
    }
}

fn visuals_for(p: &Palette) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = if p.dark { Color32::from_rgb(0x18, 0x19, 0x1C) } else { Color32::WHITE };
    v.faint_bg_color = p.row_alt;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.0, p.selection_text);
    v.hyperlink_color = p.accent;
    v.override_text_color = None;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(5);
    }
    v.widgets.hovered.weak_bg_fill = p.hover;
    v.widgets.hovered.bg_fill = p.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.border);
    v
}

/// Applique les deux thèmes (egui choisit selon la préférence) et les tailles de texte.
pub fn apply(ctx: &egui::Context, choice: crate::settings::ThemeChoice, scale: f32) {
    use crate::settings::ThemeChoice;
    ctx.set_visuals_of(egui::Theme::Light, visuals_for(&Palette::light()));
    ctx.set_visuals_of(egui::Theme::Dark, visuals_for(&Palette::dark()));
    ctx.set_theme(match choice {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Light => egui::ThemePreference::Light,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
    });
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.5, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(18.0, bold_family())),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        style.spacing.interact_size.y = 22.0;
        style.interaction.selectable_labels = false;
    });
    ctx.set_zoom_factor(scale.clamp(0.75, 2.0));
}
