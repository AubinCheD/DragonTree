//! Notifications (en haut à droite) et journal des commandes git exécutées.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use dragontree_git::runner::{CommandRecord, set_command_observer};
use dragontree_i18n::tr;
use egui::{Align2, Color32, Frame, Margin, RichText};

use crate::icons;
use crate::theme::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub kind: ToastKind,
    pub title: String,
    pub detail: Option<String>,
    /// Bouton d'action de la notification (traité par l'application).
    pub action: Option<ToastAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastAction {
    OpenGitIdentity,
}

impl ToastAction {
    fn label(self) -> String {
        match self {
            ToastAction::OpenGitIdentity => tr!("Régler l'identité git…").to_string(),
        }
    }
}

impl Toast {
    pub fn info(title: impl Into<String>) -> Toast {
        Toast { kind: ToastKind::Info, title: title.into(), detail: None, action: None }
    }

    pub fn success(title: impl Into<String>) -> Toast {
        Toast { kind: ToastKind::Success, title: title.into(), detail: None, action: None }
    }

    pub fn error(title: impl Into<String>, detail: impl Into<String>) -> Toast {
        let detail = detail.into();
        Toast {
            kind: ToastKind::Error,
            title: title.into(),
            detail: (!detail.trim().is_empty()).then_some(detail),
            action: None,
        }
    }

    pub fn with_action(mut self, action: ToastAction) -> Toast {
        self.action = Some(action);
        self
    }
}

struct LiveToast {
    id: u64,
    toast: Toast,
    born: Instant,
}

#[derive(Default)]
pub struct Toasts {
    items: Vec<LiveToast>,
    next_id: u64,
}

const TOAST_LIFETIME: Duration = Duration::from_secs(5);

impl Toasts {
    pub fn push(&mut self, toast: Toast) {
        self.next_id += 1;
        self.items.push(LiveToast { id: self.next_id, toast, born: Instant::now() });
        if self.items.len() > 6 {
            self.items.remove(0);
        }
    }

    /// Affiche les notifications ; renvoie l'action dont le bouton a été cliqué.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette) -> Option<ToastAction> {
        let mut clicked = None;
        let now = Instant::now();
        // Les erreurs restent jusqu'à fermeture ; les autres disparaissent seules.
        self.items.retain(|t| t.toast.kind == ToastKind::Error || now - t.born < TOAST_LIFETIME);
        if self.items.iter().any(|t| t.toast.kind != ToastKind::Error) {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        let mut close = None;
        egui::Area::new(egui::Id::unique("dragontree-toasts"))
            .anchor(Align2::RIGHT_TOP, egui::vec2(-12.0, 44.0))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                for t in self.items.iter().rev() {
                    let (icon, color) = match t.toast.kind {
                        ToastKind::Info => (icons::INFO, p.accent),
                        ToastKind::Success => (icons::CHECK_CIRCLE, p.added_fg),
                        ToastKind::Error => (icons::WARNING_CIRCLE, p.removed_fg),
                    };
                    Frame::popup(ui.style()).inner_margin(Margin::same(10)).show(ui, |ui| {
                        ui.set_width(380.0);
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new(icon).size(18.0).color(color));
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&t.toast.title).strong());
                                if let Some(detail) = &t.toast.detail {
                                    egui::ScrollArea::vertical().max_height(160.0).id_salt(t.id).show(ui, |ui| {
                                        ui.label(RichText::new(detail).monospace().size(12.0));
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.small_button(format!("{} {}", icons::COPY, tr!("Copier"))).clicked() {
                                            ui.ctx().copy_text(format!("{}\n{detail}", t.toast.title));
                                        }
                                        if let Some(action) = t.toast.action
                                            && ui.small_button(action.label()).clicked()
                                        {
                                            clicked = Some(action);
                                            close = Some(t.id);
                                        }
                                    });
                                }
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                if ui
                                    .add(egui::Button::new(icons::X).frame(false))
                                    .on_hover_text(tr!("Fermer"))
                                    .clicked()
                                {
                                    close = Some(t.id);
                                }
                            });
                        });
                    });
                    ui.add_space(6.0);
                }
            });
        if let Some(id) = close {
            self.items.retain(|t| t.id != id);
        }
        clicked
    }
}

/// Entrée du journal git.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub at: SystemTime,
    pub record: CommandRecord,
}

/// Journal partagé alimenté par l'observateur de `dragontree-git`.
#[derive(Clone, Default)]
pub struct CommandLog {
    entries: Arc<Mutex<VecDeque<LogEntry>>>,
}

const LOG_CAPACITY: usize = 600;

impl CommandLog {
    /// Installe l'observateur (une seule fois par processus).
    pub fn install(ctx: egui::Context) -> CommandLog {
        let log = CommandLog::default();
        let entries = log.entries.clone();
        set_command_observer(move |r| {
            let mut e = entries.lock().unwrap_or_else(|p| p.into_inner());
            e.push_back(LogEntry { at: SystemTime::now(), record: r.clone() });
            while e.len() > LOG_CAPACITY {
                e.pop_front();
            }
            if !r.read_only {
                ctx.request_repaint();
            }
        });
        log
    }

    pub fn snapshot(&self, include_reads: bool) -> Vec<LogEntry> {
        let e = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        e.iter().filter(|x| include_reads || !x.record.read_only).cloned().collect()
    }

    /// Fenêtre « Journal git ».
    pub fn window(&self, ctx: &egui::Context, open: &mut bool, include_reads: &mut bool, p: &Palette) {
        crate::widgets::centered_window(ctx, format!("{} {}", icons::TERMINAL_WINDOW, tr!("Journal git")))
            .id(egui::Id::unique("log-window"))
            .open(open)
            .default_size([760.0, 420.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(include_reads, tr!("Afficher aussi les lectures (status, log, diff…)"));
                });
                ui.separator();
                let entries = self.snapshot(*include_reads);
                egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                    for e in &entries {
                        let r = &e.record;
                        let ok = r.code == Some(0);
                        let color = if r.cancelled {
                            p.weak
                        } else if ok {
                            p.added_fg
                        } else {
                            p.removed_fg
                        };
                        let when = chrono::DateTime::<chrono::Local>::from(e.at).format("%H:%M:%S");
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("{when}")).weak().monospace().size(11.5));
                            ui.label(RichText::new(if ok { icons::CHECK } else { icons::X }).color(color));
                            ui.label(RichText::new(&r.display).monospace().size(12.0));
                            ui.label(
                                RichText::new(format!("{} ms · {}", r.duration.as_millis(), r.cwd.display()))
                                    .weak()
                                    .size(11.0),
                            );
                        });
                        if !ok && !r.stderr.trim().is_empty() {
                            ui.label(RichText::new(r.stderr.trim()).monospace().size(11.5).color(color));
                        }
                    }
                    if entries.is_empty() {
                        ui.label(RichText::new(tr!("Aucune commande pour l'instant.")).color(Color32::GRAY));
                    }
                });
            });
    }
}
