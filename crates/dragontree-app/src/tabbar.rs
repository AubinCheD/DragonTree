//! Barre d'onglets : un onglet par dépôt ouvert, créé manuellement.

use dragontree_i18n::tr;
use egui::{Align2, CornerRadius, FontId, Id, Rect, Sense, Stroke, Ui};

use crate::icons;
use crate::theme::Palette;

pub struct TabInfo {
    pub id: u64,
    pub title: String,
    pub tooltip: String,
    pub busy: bool,
    pub is_repo: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabBarAction {
    Select(usize),
    Close(usize),
    CloseOthers(usize),
    CloseAll,
    New,
    Move(usize, usize),
    ToggleRepoList,
    OpenExplorer(usize),
    CopyPath(usize),
}

const TAB_H: f32 = 32.0;

pub fn show(ui: &mut Ui, tabs: &[TabInfo], active: usize, repo_list_visible: bool, p: &Palette) -> Vec<TabBarAction> {
    let mut actions = Vec::new();
    let prev_rects: Vec<Rect> = ui.ctx().data(|d| d.get_temp(Id::unique("tab-rects"))).unwrap_or_default();
    let mut rects = Vec::with_capacity(tabs.len());

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        let reserved = 40.0;
        egui::ScrollArea::horizontal()
            .id_salt("tabs-scroll")
            .max_width((ui.available_width() - reserved).max(100.0))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 1.0;
                    for (i, tab) in tabs.iter().enumerate() {
                        let rect = tab_widget(ui, tab, i, i == active, &prev_rects, &mut actions, p);
                        rects.push(rect);
                    }
                    ui.add_space(4.0);
                    let plus = ui
                        .add(egui::Button::new(egui::RichText::new(icons::PLUS).size(16.0)).frame(false))
                        .on_hover_text(tr!("Nouvel onglet (Ctrl+T)"));
                    if plus.clicked() {
                        actions.push(TabBarAction::New);
                    }
                });
            });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tip = if repo_list_visible {
                tr!("Masquer la liste des dépôts (Ctrl+B)")
            } else {
                tr!("Afficher la liste des dépôts (Ctrl+B)")
            };
            let icon = egui::RichText::new(icons::SIDEBAR_SIMPLE).size(17.0).color(if repo_list_visible {
                p.accent
            } else {
                p.weak
            });
            if ui.add(egui::Button::new(icon).frame(false)).on_hover_text(tip).clicked() {
                actions.push(TabBarAction::ToggleRepoList);
            }
        });
    });
    ui.ctx().data_mut(|d| d.insert_temp(Id::unique("tab-rects"), rects));
    actions
}

fn tab_widget(
    ui: &mut Ui,
    tab: &TabInfo,
    index: usize,
    active: bool,
    prev_rects: &[Rect],
    actions: &mut Vec<TabBarAction>,
    p: &Palette,
) -> Rect {
    let font = FontId::proportional(13.0);
    let title_galley = ui.painter().layout_no_wrap(tab.title.clone(), font.clone(), p.text);
    let width = (title_galley.size().x + 58.0).clamp(110.0, 240.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, TAB_H), Sense::click_and_drag());
    crate::widgets::a11y(&resp, &tab.title, true);
    let resp = resp.on_hover_text(&tab.tooltip);
    let hovered = resp.hovered();

    let bg = if active {
        p.bg
    } else if hovered {
        p.hover
    } else {
        p.sidebar
    };
    let shape_rect = rect.shrink2(egui::vec2(0.0, 2.0)).translate(egui::vec2(0.0, 2.0));
    ui.painter().rect_filled(shape_rect, CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }, bg);
    if active {
        ui.painter().hline(shape_rect.x_range().shrink(6.0), shape_rect.top() + 0.5, Stroke::new(2.0, p.accent));
    }

    let icon = if tab.is_repo { icons::GIT_BRANCH } else { icons::HOUSE };
    let icon_pos = egui::pos2(rect.left() + 14.0, rect.center().y + 2.0);
    if tab.busy {
        let r = Rect::from_center_size(icon_pos, egui::vec2(13.0, 13.0));
        egui::Spinner::new().size(13.0).color(p.accent).paint_at(ui, r);
    } else {
        ui.painter().text(
            icon_pos,
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(14.0),
            if active { p.accent } else { p.weak },
        );
    }

    let text_rect =
        Rect::from_min_max(egui::pos2(rect.left() + 26.0, rect.top()), egui::pos2(rect.right() - 24.0, rect.bottom()));
    let mut job = egui::text::LayoutJob::single_section(
        tab.title.clone(),
        egui::TextFormat { font_id: font, color: if active { p.text } else { p.weak }, ..Default::default() },
    );
    job.wrap = egui::text::TextWrapping {
        max_width: text_rect.width(),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(egui::pos2(text_rect.left(), rect.center().y + 2.0 - galley.size().y / 2.0), galley, p.text);

    // Bouton de fermeture (visible sur l'onglet actif ou survolé).
    let close_rect =
        Rect::from_center_size(egui::pos2(rect.right() - 13.0, rect.center().y + 2.0), egui::vec2(18.0, 18.0));
    let close = ui.interact(close_rect, Id::unique(("tab-close", tab.id)), Sense::click());
    if active || hovered || close.hovered() {
        if close.hovered() {
            ui.painter().rect_filled(close_rect, CornerRadius::same(4), p.border);
        }
        ui.painter().text(close_rect.center(), Align2::CENTER_CENTER, icons::X, FontId::proportional(12.0), p.weak);
    }

    if close.clicked() || resp.middle_clicked() {
        actions.push(TabBarAction::Close(index));
    } else if resp.clicked() || resp.drag_started() {
        actions.push(TabBarAction::Select(index));
    }

    // Glisser pour réordonner : on échange avec le voisin quand on dépasse son milieu.
    if resp.dragged()
        && let Some(pointer) = ui.ctx().pointer_interact_pos()
    {
        if index > 0 && prev_rects.get(index - 1).is_some_and(|r| pointer.x < r.center().x) {
            actions.push(TabBarAction::Move(index, index - 1));
        } else if prev_rects.get(index + 1).is_some_and(|r| pointer.x > r.center().x) {
            actions.push(TabBarAction::Move(index, index + 1));
        }
    }

    resp.context_menu(|ui| {
        if ui.button(tr!("Fermer l'onglet")).clicked() {
            actions.push(TabBarAction::Close(index));
            ui.close();
        }
        if ui.button(tr!("Fermer les autres onglets")).clicked() {
            actions.push(TabBarAction::CloseOthers(index));
            ui.close();
        }
        if ui.button(tr!("Fermer tous les onglets")).clicked() {
            actions.push(TabBarAction::CloseAll);
            ui.close();
        }
        if tab.is_repo {
            ui.separator();
            if ui.button(format!("{} {}", icons::FOLDER_OPEN, tr!("Ouvrir dans l'Explorateur"))).clicked() {
                actions.push(TabBarAction::OpenExplorer(index));
                ui.close();
            }
            if ui.button(format!("{} {}", icons::COPY, tr!("Copier le chemin"))).clicked() {
                actions.push(TabBarAction::CopyPath(index));
                ui.close();
            }
        }
    });
    rect
}
