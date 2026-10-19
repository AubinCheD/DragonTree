//! Navigation à gauche : changements locaux, historique, branches, remotes, tags, stashes, sous-modules.

use std::collections::{BTreeMap, HashSet};

use dragontree_git::{LocalBranch, MergeMode, Oid, RemoteBranch, ResetMode, TagRef, WorktreeStart};
use dragontree_i18n::{tr, trn};
use egui::{Align2, FontId, RichText, Sense, Ui};

use super::actions::StashOp;
use super::patches::PatchSource;
use super::scope::{Visibility, branch_pattern, folder_pattern, remote_branch_pattern, remote_pattern, visibility};
use super::{Dialog, RepoTab, RepoView};
use crate::app::{AppAction, Shared};
use crate::icons;
use crate::theme::{Palette, bold_family};
use crate::widgets;

pub struct SidebarState {
    pub filter: String,
    /// Sections repliées (branches, remotes…).
    closed: HashSet<String>,
    /// Dossiers dépliés dans les arbres de branches.
    open_folders: HashSet<String>,
}

impl Default for SidebarState {
    fn default() -> Self {
        // Les tags sont souvent très nombreux : repliés par défaut.
        SidebarState { filter: String::new(), closed: ["tags".to_string()].into(), open_folders: HashSet::new() }
    }
}

pub(super) const ROW_H: f32 = 22.0;
/// Largeur d'une icône à droite d'une ligne (œil, entonnoir).
const TOOL_W: f32 = 22.0;

/// Icônes d'une ligne : œil (masquer du graphe, absent pour les tags) et entonnoir (filtrer l'historique).
#[derive(Clone, Copy)]
struct Tools {
    /// État de l'œil ; `None` : la ligne n'en a pas.
    eye: Option<Visibility>,
    /// L'élément fait partie du filtre de l'historique.
    picked: bool,
}

impl Tools {
    /// Ligne atténuée : l'élément est masqué du graphe.
    fn dim(self) -> bool {
        self.eye.is_some_and(|state| state != Visibility::Shown)
    }

    /// Place prise à droite de la ligne : les icônes sont dessinées au survol, et en permanence quand l'élément
    /// est masqué ou filtré.
    fn reserved(self, hovered: bool) -> f32 {
        if hovered || self.dim() || self.picked { (1 + usize::from(self.eye.is_some())) as f32 * TOOL_W } else { 0.0 }
    }
}

/// Nœud d'arbre de noms `a/b/c`.
struct Node<'a, T> {
    children: BTreeMap<String, Node<'a, T>>,
    leaf: Option<&'a T>,
}

impl<T> Default for Node<'_, T> {
    fn default() -> Self {
        Node { children: BTreeMap::new(), leaf: None }
    }
}

impl<'a, T> Node<'a, T> {
    fn insert(&mut self, name: &str, item: &'a T) {
        let mut node = self;
        let parts: Vec<&str> = name.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            node = node.children.entry(part.to_string()).or_default();
            if i == parts.len() - 1 {
                node.leaf = Some(item);
            }
        }
    }

    fn count(&self) -> usize {
        usize::from(self.leaf.is_some()) + self.children.values().map(Node::count).sum::<usize>()
    }
}

enum Leaf<'a> {
    Local(&'a LocalBranch),
    Remote(&'a RemoteBranch),
    Tag(&'a TagRef),
}

impl RepoTab {
    pub(super) fn sidebar_ui(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        let changes = self
            .status
            .as_ref()
            .map_or(0, |s| s.entries.iter().filter(|e| e.worktree != dragontree_git::FileState::Ignored).count());
        if nav_item(ui, icons::GIT_DIFF, tr!("Modifications locales"), changes, self.view == RepoView::Changes, p)
            .clicked()
        {
            self.view = RepoView::Changes;
            self.ensure_change_selected(shared);
        }
        let commits = self.history.loaded.commits.len();
        if nav_item(ui, icons::GIT_COMMIT, tr!("Tous les commits"), 0, self.view == RepoView::History, p)
            .on_hover_text(trn!(commits, "{n} commit chargé", "{n} commits chargés"))
            .clicked()
        {
            self.view = RepoView::History;
        }
        ui.add_space(6.0);
        widgets::search_field(ui, &mut self.sidebar.filter, tr!("Filtrer les branches"), "branch-filter");
        ui.add_space(4.0);

        let Some(refs) = self.refs.clone() else {
            ui.spinner();
            return;
        };
        let filter = self.sidebar.filter.trim().to_string();

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;

            self.worktrees_section(ui, shared, p);

            // Branches locales.
            if self.section(ui, "branches", &format!("{} Branches", icons::GIT_BRANCH), refs.locals.len(), p) {
                let leaves: Vec<(String, Leaf)> =
                    refs.locals.iter().map(|b| (b.name.clone(), Leaf::Local(b))).collect();
                self.leaves_ui(ui, shared, "loc", leaves, &filter, p);
            }

            // Remotes : un sous-arbre par remote.
            let mut by_remote: BTreeMap<String, Vec<&RemoteBranch>> = BTreeMap::new();
            for r in &self.remotes.clone().iter().map(|r| r.name.clone()).collect::<Vec<_>>() {
                by_remote.entry(r.clone()).or_default();
            }
            for b in refs.remotes.iter().filter(|b| !b.is_symbolic) {
                by_remote.entry(b.remote.clone()).or_default().push(b);
            }
            let total: usize = by_remote.values().map(Vec::len).sum();
            if self.section(ui, "remotes", &format!("{} Remotes", icons::CLOUD), total, p) {
                for (remote, branches) in by_remote {
                    let key = format!("remote:{remote}");
                    // Remotes dépliés par défaut : la clé présente signifie « replié ».
                    let open = !self.sidebar.open_folders.contains(&key) || !filter.is_empty();
                    let pattern = remote_pattern(&remote);
                    let state = visibility(shared.settings.hidden_refs_of(&self.path), &pattern);
                    let tools = self.tools(Some(state), &pattern);
                    let resp = folder_row(ui, &key, &remote, icons::CLOUD, branches.len(), open, 0, tools, p);
                    if resp.clicked() {
                        toggle(&mut self.sidebar.open_folders, &key);
                    }
                    self.row_tools(ui, shared, &resp, &remote, Some((&pattern, state)), &pattern, p);
                    let remote_name = remote.clone();
                    resp.context_menu(|ui| {
                        if ui.button(format!("{} Fetch {remote_name}", icons::CLOUD_ARROW_DOWN)).clicked() {
                            self.op_fetch(shared, Some(remote_name.clone()), true, false);
                            ui.close();
                        }
                        if let Some(r) = self.remotes.iter().find(|r| r.name == remote_name)
                            && ui.button(format!("{} {}", icons::COPY, tr!("Copier l'URL"))).clicked()
                        {
                            ui.ctx().copy_text(r.fetch_url.clone());
                            ui.close();
                        }
                        if let Some(r) = self.remotes.iter().find(|r| r.name == remote_name).cloned()
                            && ui.button(format!("{} {}", icons::PENCIL_SIMPLE, tr!("Modifier l'URL…"))).clicked()
                        {
                            let push_url = (r.push_url != r.fetch_url).then(|| r.push_url.clone());
                            self.dialog = Some(Dialog::EditRemoteUrl {
                                name: remote_name.clone(),
                                url: r.fetch_url.clone(),
                                push_url,
                            });
                            ui.close();
                        }
                        if ui.button(format!("{} {}", icons::PLUGS_CONNECTED, tr!("Tester la connexion"))).clicked() {
                            self.test_remote_connection(shared, remote_name.clone());
                            ui.close();
                        }
                        ui.separator();
                        self.hide_menu_entry(ui, shared, &pattern);
                        self.filter_menu_entry(ui, shared, &pattern);
                        if ui.button(format!("{} {}", icons::TRASH, tr!("Supprimer le remote…"))).clicked() {
                            self.dialog = Some(Dialog::RemoveRemote { name: remote_name.clone() });
                            ui.close();
                        }
                    });
                    if open {
                        let leaves: Vec<(String, Leaf)> =
                            branches.iter().map(|b| (b.name.clone(), Leaf::Remote(b))).collect();
                        self.leaves_ui_indented(ui, shared, &format!("rem:{remote}"), leaves, &filter, 1, p);
                    }
                }
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("{}  {}", icons::PLUS, tr!("Ajouter un remote…")))
                                .size(12.0)
                                .color(p.weak),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    self.dialog = Some(Dialog::AddRemote { name: String::new(), url: String::new() });
                }
            }

            if self.section(ui, "tags", &format!("{} Tags", icons::TAG), refs.tags.len(), p) {
                let leaves: Vec<(String, Leaf)> =
                    refs.tags.iter().rev().map(|t| (t.name.clone(), Leaf::Tag(t))).collect();
                self.leaves_ui(ui, shared, "tag", leaves, &filter, p);
            }

            let stashes = self.stashes.clone();
            if self.section(ui, "stashes", &format!("{} Stashes", icons::STACK), stashes.len(), p) {
                for s in stashes.iter() {
                    let label = super::scope::stash_label(s.index, &s.message);
                    let selected = self.history.external.as_ref().is_some_and(|(o, _)| *o == s.oid);
                    let (rect, resp) = widgets::row_response(ui, ("stash", s.oid), ROW_H, selected, Sense::click());
                    widgets::a11y(&resp, &label, true);
                    paint_row(ui, rect, 1, icons::STACK, &label, false, p.weak, p);
                    let resp = resp.on_hover_text(format!("{}\n{}", s.message, widgets::format_date(s.time)));
                    if resp.clicked() {
                        self.select_external(shared, s.oid, label.clone());
                    }
                    let oid = s.oid;
                    resp.context_menu(|ui| self.stash_menu(ui, shared, oid, &label));
                }
            }

            let subs = self.submodules.clone();
            if !subs.is_empty()
                && self.section(ui, "submodules", &format!("{} {}", icons::CUBE, tr!("Sous-modules")), subs.len(), p)
            {
                for s in subs.iter() {
                    let (rect, resp) = widgets::row_response(ui, ("submodule", &s.path), ROW_H, false, Sense::click());
                    let color = match s.status {
                        '-' => p.weak,
                        '+' => p.modified_fg,
                        'U' => p.conflict_fg,
                        _ => p.text,
                    };
                    paint_row(ui, rect, 1, icons::CUBE, &s.path, false, color, p);
                    let tip = match s.status {
                        '-' => tr!("non initialisé"),
                        '+' => tr!("commit différent de celui enregistré"),
                        'U' => tr!("en conflit"),
                        _ => tr!("à jour"),
                    };
                    let resp = resp.on_hover_text(tr!(
                        "{path} — {tip}\nDouble-clic : ouvrir dans un nouvel onglet",
                        path = s.path,
                        tip
                    ));
                    let full = crate::util::native_path(&self.workdir().join(&s.path));
                    if resp.double_clicked() {
                        shared.actions.push(AppAction::OpenRepoNewTab(full.clone()));
                    }
                    resp.context_menu(|ui| {
                        if ui.button(format!("{} {}", icons::PLUS, tr!("Ouvrir dans un nouvel onglet"))).clicked() {
                            shared.actions.push(AppAction::OpenRepoNewTab(full.clone()));
                            ui.close();
                        }
                        if ui
                            .button(format!(
                                "{} {}",
                                icons::ARROWS_CLOCKWISE,
                                tr!("Mettre à jour (submodule update --init --recursive)")
                            ))
                            .clicked()
                        {
                            self.run_net(
                                shared,
                                tr!("Mise à jour des sous-modules"),
                                super::Refresh::ALL,
                                false,
                                |r, n, on| {
                                    r.update_submodules(n, on)?;
                                    Ok(Some(tr!("Sous-modules à jour").into()))
                                },
                            );
                            ui.close();
                        }
                    });
                }
            }
        });
    }

    /// En-tête de section repliable ; renvoie vrai si la section est ouverte.
    pub(super) fn section(&mut self, ui: &mut Ui, key: &str, title: &str, count: usize, p: &Palette) -> bool {
        ui.add_space(6.0);
        let open = !self.sidebar.closed.contains(key);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, 4.0, p.hover);
        }
        let caret = if open { icons::CARET_DOWN } else { icons::CARET_RIGHT };
        ui.painter().text(
            rect.left_center() + egui::vec2(7.0, 0.0),
            Align2::CENTER_CENTER,
            caret,
            FontId::proportional(11.0),
            p.weak,
        );
        ui.painter().text(
            rect.left_center() + egui::vec2(16.0, 0.0),
            Align2::LEFT_CENTER,
            title,
            FontId::new(12.5, bold_family()),
            p.text,
        );
        ui.painter().text(
            rect.right_center() - egui::vec2(6.0, 0.0),
            Align2::RIGHT_CENTER,
            count.to_string(),
            FontId::proportional(11.0),
            p.weak,
        );
        if resp.clicked() {
            toggle(&mut self.sidebar.closed, key);
        }
        open
    }

    fn leaves_ui(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        prefix: &str,
        leaves: Vec<(String, Leaf)>,
        filter: &str,
        p: &Palette,
    ) {
        self.leaves_ui_indented(ui, shared, prefix, leaves, filter, 0, p);
    }

    fn leaves_ui_indented(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        prefix: &str,
        leaves: Vec<(String, Leaf)>,
        filter: &str,
        indent: usize,
        p: &Palette,
    ) {
        if !filter.is_empty() {
            let mut matches: Vec<(i32, &(String, Leaf))> =
                leaves.iter().filter_map(|l| widgets::fuzzy_score(filter, &l.0).map(|s| (s, l))).collect();
            matches.sort_by_key(|a| std::cmp::Reverse(a.0));
            let total = matches.len();
            for (_, (name, leaf)) in matches.into_iter().take(300) {
                self.leaf_row(ui, shared, name, name, leaf, indent + 1, p);
            }
            if total > 300 {
                ui.label(RichText::new(trn!(total - 300, "… {n} autre", "… {n} autres")).size(11.5).color(p.weak));
            }
            return;
        }
        let mut root: Node<Leaf> = Node::default();
        for (name, leaf) in &leaves {
            root.insert(name, leaf);
        }
        self.node_ui(ui, shared, prefix, "", &root, indent + 1, p);
    }

    fn node_ui(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        prefix: &str,
        path: &str,
        node: &Node<Leaf>,
        indent: usize,
        p: &Palette,
    ) {
        // Dossiers d'abord, puis feuilles.
        for (name, child) in &node.children {
            let full = if path.is_empty() { name.clone() } else { format!("{path}/{name}") };
            if !child.children.is_empty() {
                let key = format!("{prefix}:{full}");
                let open = self.sidebar.open_folders.contains(&key);
                let pattern = folder_pattern(prefix, &full);
                // Les tags ne se masquent pas par l'œil : l'option « Afficher les tags » les couvre.
                let hide = (prefix != "tag")
                    .then(|| (pattern.as_str(), visibility(shared.settings.hidden_refs_of(&self.path), &pattern)));
                let tools = self.tools(hide.map(|(_, state)| state), &pattern);
                let resp = folder_row(ui, &key, name, icons::FOLDER_SIMPLE, child.count(), open, indent, tools, p);
                if resp.clicked() {
                    toggle(&mut self.sidebar.open_folders, &key);
                }
                self.row_tools(ui, shared, &resp, &full, hide, &pattern, p);
                resp.context_menu(|ui| {
                    if hide.is_some() {
                        self.hide_menu_entry(ui, shared, &pattern);
                    }
                    self.filter_menu_entry(ui, shared, &pattern);
                });
                if open {
                    self.node_ui(ui, shared, prefix, &full, child, indent + 1, p);
                }
                if let Some(leaf) = child.leaf {
                    self.leaf_row(ui, shared, &full, name, leaf, indent, p);
                }
            }
        }
        for (name, child) in &node.children {
            if child.children.is_empty()
                && let Some(leaf) = child.leaf
            {
                let full = if path.is_empty() { name.clone() } else { format!("{path}/{name}") };
                self.leaf_row(ui, shared, &full, name, leaf, indent, p);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn leaf_row(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        full: &str,
        label: &str,
        leaf: &Leaf,
        indent: usize,
        p: &Palette,
    ) {
        // Branche sortie dans un autre worktree : icône de worktree, et son dossier dans l'infobulle.
        let elsewhere = match leaf {
            Leaf::Local(b) if !b.is_head => {
                self.checked_out_elsewhere(&b.name).map(|w| crate::util::native_path(&w.path))
            }
            _ => None,
        };
        let (oid, icon, bold, color) = match leaf {
            Leaf::Local(b) => (
                b.oid,
                if b.is_head {
                    icons::CHECK
                } else if elsewhere.is_some() {
                    icons::TREE_STRUCTURE
                } else {
                    icons::GIT_BRANCH
                },
                b.is_head,
                if b.is_head { p.accent } else { p.text },
            ),
            Leaf::Remote(b) => (b.oid, icons::GIT_BRANCH, false, p.text),
            Leaf::Tag(t) => (t.oid, icons::TAG, false, p.text),
        };
        let selected = self.history.selected == Some(oid) && self.view == RepoView::History;
        let id_salt = match leaf {
            Leaf::Local(b) => ("local", b.name.as_str()),
            Leaf::Remote(b) => ("remote", b.full.as_str()),
            Leaf::Tag(t) => ("tag", t.name.as_str()),
        };
        let (rect, resp) = widgets::row_response(ui, id_salt, ROW_H, selected, Sense::click());
        widgets::a11y(&resp, full, true);
        // Œil : au survol, ou en permanence quand l'élément est masqué du graphe (ligne atténuée).
        let pattern = match leaf {
            Leaf::Local(b) => branch_pattern(&b.name),
            Leaf::Remote(b) => remote_branch_pattern(&b.full),
            Leaf::Tag(t) => format!("refs/tags/{}", t.name),
        };
        let hide = (!matches!(leaf, Leaf::Tag(_)))
            .then(|| (pattern.as_str(), visibility(shared.settings.hidden_refs_of(&self.path), &pattern)));
        let tools = self.tools(hide.map(|(_, state)| state), &pattern);
        let content = egui::Rect::from_min_max(
            rect.min,
            egui::pos2(rect.max.x - tools.reserved(ui.rect_contains_pointer(rect)), rect.max.y),
        );
        let color = if tools.dim() { p.weak } else { color };
        let text_right = paint_ahead_behind(ui, content, leaf, p);
        paint_row(
            ui,
            egui::Rect::from_min_max(rect.min, egui::pos2(text_right, rect.max.y)),
            indent,
            icon,
            label,
            bold,
            color,
            p,
        );
        self.row_tools(ui, shared, &resp, full, hide, &pattern, p);

        let mut tip = match leaf {
            Leaf::Local(b) => match &b.upstream {
                Some(u) if b.upstream_gone => tr!("{name}\nSuit {u} (supprimée sur le remote)", name = b.name, u),
                Some(u) => tr!(
                    "{name}\nSuit {u} — {ahead} en avance, {behind} en retard",
                    name = b.name,
                    u,
                    ahead = b.ahead,
                    behind = b.behind
                ),
                None => tr!("{name}\nAucune branche suivie", name = b.name),
            },
            Leaf::Remote(b) => b.full.clone(),
            Leaf::Tag(t) if t.annotated => tr!("{name} (annoté)", name = t.name),
            Leaf::Tag(t) => t.name.clone(),
        };
        if let Some(path) = &elsewhere {
            tip = format!("{tip}\n{}", tr!("Sortie dans le worktree {path}", path = path.display()));
        }
        let resp = resp.on_hover_text(tip);
        if resp.clicked() {
            self.view = RepoView::History;
            self.select_commit(shared, oid, true);
        }
        if resp.double_clicked() {
            match leaf {
                Leaf::Local(b) if !b.is_head => self.op_checkout(shared, b.name.clone()),
                Leaf::Remote(b) => {
                    self.dialog = Some(Dialog::CheckoutRemote { remote_full: b.full.clone(), local: b.name.clone() })
                }
                _ => {}
            }
        }
        let current = self.refs.as_ref().and_then(|r| r.current_branch().map(str::to_string));
        resp.context_menu(|ui| match leaf {
            Leaf::Local(b) => self.local_branch_menu(ui, shared, b, current.as_deref()),
            Leaf::Remote(b) => self.remote_branch_menu(ui, shared, b, current.as_deref()),
            Leaf::Tag(t) => self.tag_menu(ui, shared, t),
        });
        let _ = full;
    }

    /// Icônes d'une ligne pour l'élément `pattern` ; `eye` : état de l'œil si la ligne en a un.
    fn tools(&self, eye: Option<Visibility>, pattern: &str) -> Tools {
        Tools { eye, picked: self.history.filter_refs.iter().any(|p| p == pattern) }
    }

    /// Icônes à droite d'une ligne (dossier, branche, remote, tag) : l'œil, qui masque l'élément du graphe ou
    /// le réaffiche (`hide` : motif et état), puis l'entonnoir, qui l'ajoute au filtre de l'historique ou l'en
    /// retire (`pick` : motif). Dessinées au survol de la ligne et en permanence quand l'élément est masqué
    /// (œil barré) ou filtré ; toujours cliquables et exposées à l'accessibilité.
    #[allow(clippy::too_many_arguments)]
    fn row_tools(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        row: &egui::Response,
        name: &str,
        hide: Option<(&str, Visibility)>,
        pick: &str,
        p: &Palette,
    ) {
        let hovered = ui.rect_contains_pointer(row.rect);
        let mut slot = 0;
        if let Some((pattern, state)) = hide {
            self.eye_button(ui, shared, row, slot, hovered, name, pattern, state, p);
            slot += 1;
        }
        self.funnel_button(ui, shared, row, slot, hovered, name, pick, p);
    }

    /// Zone cliquable de la `slot`-ième icône à partir de la droite de la ligne.
    fn tool_rect(row: &egui::Response, slot: usize) -> egui::Rect {
        egui::Rect::from_center_size(
            egui::pos2(row.rect.right() - TOOL_W / 2.0 - slot as f32 * TOOL_W, row.rect.center().y),
            egui::vec2(TOOL_W - 2.0, ROW_H - 4.0),
        )
    }

    /// Œil : un élément masqué par un dossier se réaffiche par ce dossier (icône non cliquable).
    #[allow(clippy::too_many_arguments)]
    fn eye_button(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        row: &egui::Response,
        slot: usize,
        hovered: bool,
        name: &str,
        pattern: &str,
        state: Visibility,
        p: &Palette,
    ) {
        let rect = Self::tool_rect(row, slot);
        let usable = state != Visibility::HiddenByFolder;
        let resp = ui.interact(rect, row.id.with("eye"), if usable { Sense::click() } else { Sense::hover() });
        let label = if state == Visibility::Shown {
            tr!("Masquer du graphe : {name}", name)
        } else {
            tr!("Afficher dans le graphe : {name}", name)
        };
        widgets::a11y(&resp, &label, usable);
        if hovered || state != Visibility::Shown {
            let (icon, color) = match (state, resp.hovered()) {
                (_, true) => (if state == Visibility::Shown { icons::EYE } else { icons::EYE_SLASH }, p.text),
                (Visibility::Shown, false) => (icons::EYE, p.weak),
                _ => (icons::EYE_SLASH, p.weak),
            };
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, FontId::proportional(13.0), color);
        }
        let tip = match state {
            Visibility::HiddenByFolder => tr!("Masqué par un dossier : réafficher le dossier").to_string(),
            _ => label,
        };
        if resp.on_hover_text(tip).clicked() && usable {
            self.toggle_hidden(shared, pattern);
        }
    }

    /// Entonnoir : ajoute l'élément au filtre de l'historique (plusieurs à la fois) ou l'en retire.
    #[allow(clippy::too_many_arguments)]
    fn funnel_button(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        row: &egui::Response,
        slot: usize,
        hovered: bool,
        name: &str,
        pattern: &str,
        p: &Palette,
    ) {
        let rect = Self::tool_rect(row, slot);
        let picked = self.history.filter_refs.iter().any(|f| f == pattern);
        let resp = ui.interact(rect, row.id.with("funnel"), Sense::click());
        let label = if picked {
            tr!("Ne plus filtrer l'historique sur : {name}", name)
        } else {
            tr!("Filtrer l'historique sur : {name}", name)
        };
        widgets::a11y(&resp, &label, true);
        if hovered || picked {
            let color = if picked {
                p.accent
            } else if resp.hovered() {
                p.text
            } else {
                p.weak
            };
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, icons::FUNNEL, FontId::proportional(13.0), color);
        }
        if resp.on_hover_text(label).clicked() {
            self.toggle_filter_ref(shared, pattern);
        }
    }

    /// Entrée « Masquer du graphe / Afficher dans le graphe » des menus contextuels.
    fn hide_menu_entry(&mut self, ui: &mut Ui, shared: &mut Shared, pattern: &str) {
        let (icon, text) = match visibility(shared.settings.hidden_refs_of(&self.path), pattern) {
            Visibility::Shown => (icons::EYE_SLASH, tr!("Masquer du graphe")),
            Visibility::Hidden => (icons::EYE, tr!("Afficher dans le graphe")),
            Visibility::HiddenByFolder => return,
        };
        if ui.button(format!("{icon} {text}")).clicked() {
            self.toggle_hidden(shared, pattern);
            ui.close();
        }
    }

    /// Entrée « Filtrer l'historique sur cet élément » des menus contextuels (ou « Ne plus filtrer… »).
    fn filter_menu_entry(&mut self, ui: &mut Ui, shared: &mut Shared, pattern: &str) {
        let text = if self.history.filter_refs.iter().any(|f| f == pattern) {
            tr!("Ne plus filtrer l'historique sur cet élément")
        } else {
            tr!("Filtrer l'historique sur cet élément")
        };
        if ui.button(format!("{} {text}", icons::FUNNEL)).clicked() {
            self.toggle_filter_ref(shared, pattern);
            ui.close();
        }
    }

    fn local_branch_menu(&mut self, ui: &mut Ui, shared: &mut Shared, b: &LocalBranch, current: Option<&str>) {
        let name = b.name.clone();
        // Sortie dans un autre worktree (git en refuse le checkout ici) : ce worktree peut s'ouvrir.
        let elsewhere = self.checked_out_elsewhere(&name).map(|w| w.path.clone());
        if let Some(path) = elsewhere.clone()
            && ui.button(format!("{} {}", icons::TREE_STRUCTURE, tr!("Ouvrir le worktree"))).clicked()
        {
            shared.actions.push(AppAction::OpenRepo(path));
            ui.close();
        }
        if !b.is_head && ui.button(format!("{} {}", icons::CHECK, tr!("Checkout « {name} »", name))).clicked() {
            self.op_checkout(shared, name.clone());
            ui.close();
        }
        if let Some(cur) = current
            && !b.is_head
        {
            if ui
                .button(format!("{} {}", icons::GIT_MERGE, tr!("Fusionner « {name} » dans « {cur} »…", name, cur)))
                .clicked()
            {
                self.dialog = Some(Dialog::Merge { rev: name.clone(), mode: MergeMode::Default });
                ui.close();
            }
            if ui
                .button(format!("{} {}", icons::FLOW_ARROW, tr!("Rebaser « {cur} » sur « {name} »…", cur, name)))
                .clicked()
            {
                self.dialog = Some(Dialog::Rebase { onto: name.clone() });
                ui.close();
            }
        }
        ui.separator();
        if ui.button(format!("{} {}", icons::GIT_BRANCH, tr!("Nouvelle branche à partir d'ici…"))).clicked() {
            self.dialog = Some(Dialog::CreateBranch {
                start: name.clone(),
                start_label: name.clone(),
                name: String::new(),
                checkout: true,
            });
            ui.close();
        }
        if ui.button(format!("{} {}", icons::TAG, tr!("Nouveau tag ici…"))).clicked() {
            self.dialog = Some(Dialog::new_tag(b.oid.to_hex(), name.clone()));
            ui.close();
        }
        if !b.is_head
            && elsewhere.is_none()
            && ui.button(format!("{} {}", icons::TREE_STRUCTURE, tr!("Créer un worktree…"))).clicked()
        {
            self.open_create_worktree(Some(WorktreeStart::Branch(name.clone())));
            ui.close();
        }
        if b.is_head && ui.button(format!("{} Push…", icons::ARROW_LINE_UP)).clicked() {
            self.open_push_dialog();
            ui.close();
        }
        if !b.is_head
            && let Some(up) = b.upstream.clone()
            && b.behind > 0
            && b.ahead == 0
            && ui.button(format!("{} {}", icons::ARROW_LINE_DOWN, tr!("Avancer à {up} (fast-forward)", up))).clicked()
        {
            let target = name.clone();
            self.run_simple(shared, "Fast-forward", super::Refresh::REFS, move |r| {
                r.fast_forward_branch(&target, &up)?;
                Ok(Some(tr!("{target} avancée à {up}", target, up)))
            });
            ui.close();
        }
        ui.separator();
        if ui.button(format!("{} {}", icons::PENCIL_SIMPLE, tr!("Renommer…"))).clicked() {
            self.dialog = Some(Dialog::RenameBranch { old: name.clone(), new: name.clone() });
            ui.close();
        }
        if !b.is_head && ui.button(format!("{} {}", icons::TRASH, tr!("Supprimer…"))).clicked() {
            let remote =
                b.upstream.as_ref().and_then(|u| u.split_once('/')).map(|(r, br)| (r.to_string(), br.to_string()));
            self.dialog = Some(Dialog::DeleteBranch { name: name.clone(), force: false, remote, delete_remote: false });
            ui.close();
        }
        self.compare_branch_entry(ui, shared, &name, current);
        self.hide_menu_entry(ui, shared, &branch_pattern(&name));
        self.filter_menu_entry(ui, shared, &branch_pattern(&name));
        if ui.button(format!("{} {}", icons::COPY, tr!("Copier le nom"))).clicked() {
            ui.ctx().copy_text(name.clone());
            ui.close();
        }
    }

    fn remote_branch_menu(&mut self, ui: &mut Ui, shared: &mut Shared, b: &RemoteBranch, current: Option<&str>) {
        if ui.button(format!("{} {}", icons::CHECK, tr!("Checkout (créer une branche locale)…"))).clicked() {
            self.dialog = Some(Dialog::CheckoutRemote { remote_full: b.full.clone(), local: b.name.clone() });
            ui.close();
        }
        if let Some(cur) = current {
            let merge = tr!("Fusionner « {name} » dans « {cur} »…", name = b.full, cur);
            if ui.button(format!("{} {merge}", icons::GIT_MERGE)).clicked() {
                self.dialog = Some(Dialog::Merge { rev: b.full.clone(), mode: MergeMode::Default });
                ui.close();
            }
            let rebase = tr!("Rebaser « {cur} » sur « {name} »…", cur, name = b.full);
            if ui.button(format!("{} {rebase}", icons::FLOW_ARROW)).clicked() {
                self.dialog = Some(Dialog::Rebase { onto: b.full.clone() });
                ui.close();
            }
        }
        ui.separator();
        if ui.button(format!("{} {}", icons::GIT_BRANCH, tr!("Nouvelle branche à partir d'ici…"))).clicked() {
            self.dialog = Some(Dialog::CreateBranch {
                start: b.full.clone(),
                start_label: b.full.clone(),
                name: b.name.clone(),
                checkout: true,
            });
            ui.close();
        }
        if ui.button(format!("{} {}", icons::TREE_STRUCTURE, tr!("Créer un worktree…"))).clicked() {
            let start = WorktreeStart::Track { remote_branch: b.full.clone(), local: b.name.clone() };
            self.open_create_worktree(Some(start));
            ui.close();
        }
        if ui.button(format!("{} {}", icons::TRASH, tr!("Supprimer sur le remote…"))).clicked() {
            self.dialog = Some(Dialog::DeleteRemoteBranch { remote: b.remote.clone(), branch: b.name.clone() });
            ui.close();
        }
        self.compare_branch_entry(ui, shared, &b.full, current);
        self.hide_menu_entry(ui, shared, &remote_branch_pattern(&b.full));
        self.filter_menu_entry(ui, shared, &remote_branch_pattern(&b.full));
        if ui.button(format!("{} {}", icons::COPY, tr!("Copier le nom"))).clicked() {
            ui.ctx().copy_text(b.full.clone());
            ui.close();
        }
    }

    fn tag_menu(&mut self, ui: &mut Ui, shared: &mut Shared, t: &TagRef) {
        let name = t.name.clone();
        if ui.button(format!("{} {}", icons::CHECK, tr!("Checkout (HEAD détachée)"))).clicked() {
            self.op_checkout(shared, format!("refs/tags/{name}"));
            ui.close();
        }
        if ui.button(format!("{} {}", icons::GIT_BRANCH, tr!("Nouvelle branche à partir d'ici…"))).clicked() {
            self.dialog = Some(Dialog::CreateBranch {
                start: format!("refs/tags/{name}"),
                start_label: name.clone(),
                name: String::new(),
                checkout: true,
            });
            ui.close();
        }
        if let Some(remote) = self.default_remote()
            && ui.button(format!("{} {}", icons::ARROW_LINE_UP, tr!("Pousser vers {remote}", remote))).clicked()
        {
            let tag = name.clone();
            self.run_net(shared, tr!("Push du tag"), super::Refresh::REFS, false, move |r, n, on| {
                r.push_tag(&remote, &tag, n, on)?;
                Ok(Some(tr!("Tag {tag} poussé", tag)))
            });
            ui.close();
        }
        self.filter_menu_entry(ui, shared, &format!("refs/tags/{name}"));
        ui.separator();
        if ui.button(format!("{} {}", icons::TRASH, tr!("Supprimer…"))).clicked() {
            self.dialog = Some(Dialog::DeleteTag { name: name.clone(), remote_too: false });
            ui.close();
        }
        if ui.button(format!("{} {}", icons::COPY, tr!("Copier le nom"))).clicked() {
            ui.ctx().copy_text(name.clone());
            ui.close();
        }
    }

    /// Commit menu partagé (historique, branches) : opérations sur un commit précis.
    /// Menu contextuel d'un stash (barre latérale et lignes de stash du graphe) : appliquer, pop, renommer, supprimer.
    pub(super) fn stash_menu(&mut self, ui: &mut Ui, shared: &mut Shared, oid: Oid, label: &str) {
        if ui.button(format!("{} {}", icons::TRAY_ARROW_UP, tr!("Appliquer"))).clicked() {
            self.op_stash(shared, oid, label.to_string(), StashOp::Apply);
            ui.close();
        }
        if ui.button(format!("{} {}", icons::TRAY_ARROW_UP, tr!("Appliquer et supprimer (pop)"))).clicked() {
            self.op_stash(shared, oid, label.to_string(), StashOp::Pop);
            ui.close();
        }
        if ui.button(format!("{} {}", icons::PENCIL_SIMPLE, tr!("Renommer…"))).clicked() {
            let message = self.stashes.iter().find(|s| s.oid == oid).map(|s| s.message.clone()).unwrap_or_default();
            self.dialog = Some(Dialog::RenameStash { oid, label: label.to_string(), message });
            ui.close();
        }
        ui.separator();
        self.patch_menu_entries(ui, shared, PatchSource::Stash(oid));
        ui.separator();
        if ui.button(format!("{} {}", icons::TRASH, tr!("Supprimer…"))).clicked() {
            self.dialog = Some(Dialog::DropStash { oid, label: label.to_string() });
            ui.close();
        }
    }

    pub(super) fn commit_menu(&mut self, ui: &mut Ui, shared: &mut Shared, oid: Oid, summary: &str) {
        let short = oid.short();
        let current = self.refs.as_ref().and_then(|r| r.current_branch().map(str::to_string));
        if ui.button(format!("{} {}", icons::CHECK, tr!("Checkout {short} (HEAD détachée)…", short))).clicked() {
            self.dialog = Some(Dialog::Confirm {
                title: tr!("Checkout d'un commit").into(),
                text: tr!(
                    "HEAD sera détachée sur {short}. Les nouveaux commits n'appartiendront à aucune branche.",
                    short
                ),
                action: super::dialogs::ConfirmAction::CheckoutDetached(oid),
            });
            ui.close();
        }
        if ui.button(format!("{} {}", icons::GIT_BRANCH, tr!("Nouvelle branche ici…"))).clicked() {
            self.dialog = Some(Dialog::CreateBranch {
                start: oid.to_hex(),
                start_label: short.clone(),
                name: String::new(),
                checkout: true,
            });
            ui.close();
        }
        if ui.button(format!("{} {}", icons::TAG, tr!("Nouveau tag ici…"))).clicked() {
            self.dialog = Some(Dialog::new_tag(oid.to_hex(), short.clone()));
            ui.close();
        }
        if ui.button(format!("{} {}", icons::TREE_STRUCTURE, tr!("Créer un worktree ici…"))).clicked() {
            self.open_create_worktree(Some(WorktreeStart::Detached(oid.to_hex())));
            ui.close();
        }
        ui.separator();
        if let Some(cur) = &current {
            if ui.button(format!("{} {}", icons::GIT_MERGE, tr!("Fusionner dans « {cur} »…", cur))).clicked() {
                self.dialog = Some(Dialog::Merge { rev: oid.to_hex(), mode: MergeMode::Default });
                ui.close();
            }
            if ui.button(format!("{} {}", icons::FLOW_ARROW, tr!("Rebaser « {cur} » sur ce commit…", cur))).clicked()
            {
                self.dialog = Some(Dialog::Rebase { onto: oid.to_hex() });
                ui.close();
            }
            if ui
                .button(format!("{} {}", icons::LIST, tr!("Rebase interactif de « {cur} » depuis ici…", cur)))
                .clicked()
            {
                self.open_rebase_editor(shared, oid);
                ui.close();
            }
        }
        if ui.button(format!("{} Cherry-pick…", icons::ARROW_BEND_DOWN_RIGHT)).clicked() {
            self.dialog = Some(Dialog::CherryPick { ids: vec![oid], label: format!("{short} {summary}") });
            ui.close();
        }
        if ui.button(format!("{} Revert…", icons::ARROW_COUNTER_CLOCKWISE)).clicked() {
            self.dialog = Some(Dialog::Revert { id: oid, label: format!("{short} {summary}") });
            ui.close();
        }
        if let Some(cur) = &current
            && ui
                .button(format!("{} {}", icons::ARROW_COUNTER_CLOCKWISE, tr!("Réinitialiser « {cur} » ici…", cur)))
                .clicked()
        {
            self.dialog =
                Some(Dialog::Reset { rev: oid.to_hex(), label: format!("{short} {summary}"), mode: ResetMode::Mixed });
            ui.close();
        }
        ui.separator();
        self.compare_commit_entries(ui, shared, oid);
        ui.separator();
        self.patch_menu_entries(ui, shared, PatchSource::Commit(oid, summary.to_string()));
        if ui.button(format!("{} {}", icons::FLOPPY_DISK, tr!("Enregistrer les patchs jusqu'à HEAD…"))).clicked() {
            self.save_patch(shared, PatchSource::CommitsToHead(oid));
            ui.close();
        }
        ui.separator();
        if ui.button(format!("{} {}", icons::COPY, tr!("Copier le SHA"))).clicked() {
            ui.ctx().copy_text(oid.to_hex());
            ui.close();
        }
        if ui.button(format!("{} {}", icons::COPY, tr!("Copier le SHA court"))).clicked() {
            ui.ctx().copy_text(short.clone());
            ui.close();
        }
        if ui.button(format!("{} {}", icons::COPY, tr!("Copier le sujet"))).clicked() {
            ui.ctx().copy_text(summary.to_string());
            ui.close();
        }
    }
}

fn toggle(set: &mut HashSet<String>, key: &str) {
    if !set.remove(key) {
        set.insert(key.to_string());
    }
}

fn nav_item(ui: &mut Ui, icon: &str, label: &str, count: usize, selected: bool, p: &Palette) -> egui::Response {
    let (rect, resp) = widgets::row_response(ui, ("nav", icon), 28.0, selected, Sense::click());
    widgets::a11y(&resp, label, true);
    let color = if selected { p.selection_text } else { p.text };
    ui.painter().text(
        rect.left_center() + egui::vec2(14.0, 0.0),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(15.0),
        if selected { p.accent } else { p.weak },
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(30.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::new(13.5, if selected { bold_family() } else { egui::FontFamily::Proportional }),
        color,
    );
    if count > 0 {
        let text = count.to_string();
        let galley = ui.painter().layout_no_wrap(text, FontId::proportional(11.0), p.bg);
        let w = galley.size().x + 12.0;
        let pill =
            egui::Rect::from_center_size(rect.right_center() - egui::vec2(8.0 + w / 2.0, 0.0), egui::vec2(w, 17.0));
        ui.painter().rect_filled(pill, 8.5, p.accent);
        ui.painter().galley(pill.center() - galley.size() / 2.0, galley, p.bg);
    }
    resp
}

#[allow(clippy::too_many_arguments)]
fn folder_row(
    ui: &mut Ui,
    key: &str,
    name: &str,
    icon: &str,
    count: usize,
    open: bool,
    indent: usize,
    tools: Tools,
    p: &Palette,
) -> egui::Response {
    let (rect, resp) = widgets::row_response(ui, ("folder", key), ROW_H, false, Sense::click());
    widgets::a11y(&resp, name, true);
    let dim = tools.dim();
    let x = rect.left() + 6.0 + indent as f32 * 12.0;
    let caret = if open { icons::CARET_DOWN } else { icons::CARET_RIGHT };
    ui.painter().text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, caret, FontId::proportional(10.0), p.weak);
    ui.painter().text(
        egui::pos2(x + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{icon} {name}"),
        FontId::proportional(13.0),
        if dim { p.weak } else { p.text },
    );
    ui.painter().text(
        rect.right_center() - egui::vec2(6.0 + tools.reserved(ui.rect_contains_pointer(rect)), 0.0),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::proportional(11.0),
        p.weak,
    );
    resp
}

#[allow(clippy::too_many_arguments)]
fn paint_row(
    ui: &Ui,
    rect: egui::Rect,
    indent: usize,
    icon: &str,
    label: &str,
    bold: bool,
    color: egui::Color32,
    p: &Palette,
) {
    let x = rect.left() + 6.0 + indent as f32 * 12.0;
    ui.painter().text(
        egui::pos2(x + 5.0, rect.center().y),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(13.0),
        if bold { p.accent } else { p.weak },
    );
    let family = if bold { bold_family() } else { egui::FontFamily::Proportional };
    let mut job = egui::text::LayoutJob::single_section(
        label.to_string(),
        egui::TextFormat { font_id: FontId::new(13.0, family), color, ..Default::default() },
    );
    job.wrap = egui::text::TextWrapping {
        max_width: (rect.right() - x - 18.0).max(10.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(egui::pos2(x + 16.0, rect.center().y - galley.size().y / 2.0), galley, color);
}

/// Pastilles ↑/↓ d'une branche locale ; renvoie la limite droite disponible pour le nom.
fn paint_ahead_behind(ui: &Ui, rect: egui::Rect, leaf: &Leaf, p: &Palette) -> f32 {
    let Leaf::Local(b) = leaf else { return rect.right() };
    let mut right = rect.right() - 6.0;
    let mut put = |text: String, color: egui::Color32| {
        let r = ui.painter().text(
            egui::pos2(right, rect.center().y),
            Align2::RIGHT_CENTER,
            text,
            FontId::proportional(11.0),
            color,
        );
        right = r.left() - 4.0;
    };
    if b.upstream_gone {
        put(icons::PROHIBIT.to_string(), p.weak);
    }
    if b.behind > 0 {
        put(format!("{}{}", icons::ARROW_DOWN, b.behind), p.accent);
    }
    if b.ahead > 0 {
        put(format!("{}{}", icons::ARROW_UP, b.ahead), p.added_fg);
    }
    right
}
