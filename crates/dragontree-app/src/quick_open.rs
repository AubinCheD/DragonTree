//! Palette Ctrl+P : dépôts, branches de l'onglet courant et commandes, avec recherche floue.

use std::path::PathBuf;

use dragontree_i18n::{tr, tr_mark, tr_str};
use egui::{Key, RichText};

use crate::app::{AppAction, Shared};
use crate::icons;
use crate::repo::RepoTab;
use crate::repo_list::RepoListState;
use crate::theme::Palette;
use crate::widgets;

#[derive(Default)]
pub struct QuickOpen {
    query: String,
    selected: usize,
    focused: bool,
}

#[derive(Clone)]
enum Item {
    Repo(PathBuf),
    Branch(String),
    Command(Command),
}

#[derive(Clone, Copy)]
enum Command {
    Fetch,
    Pull,
    Push,
    NewBranch,
    Stash,
    Refresh,
    Changes,
    History,
    Clone,
    Accounts,
    Settings,
    Log,
    Open,
}

const COMMANDS: &[(Command, &str, &str)] = &[
    (Command::Fetch, icons::CLOUD_ARROW_DOWN, "Fetch"),
    (Command::Pull, icons::ARROW_LINE_DOWN, "Pull…"),
    (Command::Push, icons::ARROW_LINE_UP, "Push…"),
    (Command::NewBranch, icons::GIT_BRANCH, tr_mark!("Nouvelle branche…")),
    (Command::Stash, icons::TRAY_ARROW_DOWN, "Stash…"),
    (Command::Refresh, icons::ARROWS_CLOCKWISE, tr_mark!("Actualiser")),
    (Command::Changes, icons::GIT_DIFF, tr_mark!("Modifications locales")),
    (Command::History, icons::GIT_COMMIT, tr_mark!("Tous les commits")),
    (Command::Clone, icons::DOWNLOAD_SIMPLE, tr_mark!("Cloner un dépôt…")),
    (Command::Accounts, icons::USER_CIRCLE, tr_mark!("Comptes…")),
    (Command::Settings, icons::GEAR_SIX, tr_mark!("Réglages…")),
    (Command::Log, icons::LIST, tr_mark!("Journal git")),
    (Command::Open, icons::FOLDER_OPEN, tr_mark!("Ouvrir un dépôt…")),
];

impl QuickOpen {
    /// Renvoie faux quand la palette se ferme.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        shared: &mut Shared,
        list: &RepoListState,
        tab: Option<&mut RepoTab>,
        p: &Palette,
    ) -> bool {
        let mut items: Vec<(i32, Item, String, &'static str)> = Vec::new();
        for r in &list.repos {
            let label = if r.relative_parent().is_empty() {
                r.name.clone()
            } else {
                format!("{}  ({})", r.name, r.relative_parent())
            };
            if let Some(s) = widgets::fuzzy_score(&self.query, &label) {
                items.push((s + 5, Item::Repo(r.path.clone()), label, icons::GIT_BRANCH));
            }
        }
        if let Some(t) = tab.as_deref()
            && let Some(refs) = &t.refs
        {
            for b in &refs.locals {
                if let Some(s) = widgets::fuzzy_score(&self.query, &b.name) {
                    items.push((s, Item::Branch(b.name.clone()), format!("Checkout {}", b.name), icons::CHECK));
                }
            }
        }
        for (cmd, icon, label) in COMMANDS {
            // Recherche dans le libellé affiché, donc traduit.
            let label = tr_str(label);
            if let Some(s) = widgets::fuzzy_score(&self.query, label) {
                items.push((s + 2, Item::Command(*cmd), label.to_string(), icon));
            }
        }
        if !self.query.trim().is_empty() {
            items.sort_by_key(|a| std::cmp::Reverse(a.0));
        }
        items.truncate(60);
        self.selected = self.selected.min(items.len().saturating_sub(1));

        let (up, down, enter, esc) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
            )
        });
        if down {
            self.selected = (self.selected + 1).min(items.len().saturating_sub(1));
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        let mut chosen = enter.then(|| items.get(self.selected).map(|x| x.1.clone())).flatten();
        let mut keep = !esc;

        let area = egui::Area::new(egui::Id::unique("quick-open"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 70.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                    ui.set_width(560.0);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text(tr!("Dépôt, branche ou commande…"))
                            .desired_width(f32::INFINITY),
                    );
                    if !self.focused {
                        resp.request_focus();
                        self.focused = true;
                    }
                    if resp.changed() {
                        self.selected = 0;
                    }
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                        for (i, (_, item, label, icon)) in items.iter().enumerate() {
                            let r = ui
                                .with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                                    ui.add(
                                        egui::Button::selectable(
                                            i == self.selected,
                                            RichText::new(format!("{icon}  {label}")),
                                        )
                                        .frame_when_inactive(false)
                                        .min_size(egui::vec2(0.0, 26.0)),
                                    )
                                })
                                .inner;
                            if r.clicked() {
                                chosen = Some(item.clone());
                            }
                            if i == self.selected && (up || down) {
                                r.scroll_to_me(None);
                            }
                        }
                        if items.is_empty() {
                            ui.label(RichText::new(tr!("Aucun résultat")).color(p.weak));
                        }
                    });
                });
            });
        let clicked_outside = ctx.input(|i| {
            i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|pos| !area.response.rect.contains(pos))
        });
        if clicked_outside {
            keep = false;
        }

        if let Some(item) = chosen {
            keep = false;
            match item {
                Item::Repo(path) => shared.actions.push(AppAction::OpenRepo(path)),
                Item::Branch(name) => {
                    if let Some(t) = tab {
                        t.quick_checkout(shared, name);
                    }
                }
                Item::Command(cmd) => match cmd {
                    Command::Clone => shared.actions.push(AppAction::ShowClone),
                    Command::Accounts => shared.actions.push(AppAction::ShowAccounts),
                    Command::Settings => shared.actions.push(AppAction::ShowSettings),
                    Command::Log => shared.actions.push(AppAction::ShowLog),
                    Command::Open => shared.actions.push(AppAction::PickAndOpenRepo),
                    other => {
                        if let Some(t) = tab {
                            t.quick_command(
                                shared,
                                match other {
                                    Command::Fetch => "fetch",
                                    Command::Pull => "pull",
                                    Command::Push => "push",
                                    Command::NewBranch => "branch",
                                    Command::Stash => "stash",
                                    Command::Refresh => "refresh",
                                    Command::Changes => "changes",
                                    _ => "history",
                                },
                            );
                        }
                    }
                },
            }
        }
        keep
    }
}
