//! Petits widgets et utilitaires d'affichage partagés.

use dragontree_i18n::{tr, trn};
use egui::{CornerRadius, FontId, Rect, Response, RichText, Sense, Ui};

use crate::theme::Palette;

/// Libellé d'accessibilité d'un élément dessiné à la main (lecteurs d'écran, tests d'interface).
pub fn a11y(resp: &Response, label: &str, enabled: bool) {
    let label = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Button, enabled, label.clone()));
}

/// Fenêtre flottante centrée en haut de la fenêtre principale, non repliable.
pub fn centered_window<'a>(ctx: &egui::Context, title: impl egui::IntoAtoms<'a>) -> egui::Window<'a> {
    egui::Window::new(title)
        .collapsible(false)
        .pivot(egui::Align2::CENTER_TOP)
        .default_pos(ctx.content_rect().center_top() + egui::vec2(0.0, 56.0))
}

/// Bouton icône sans cadre, avec info-bulle.
pub fn icon_button(ui: &mut Ui, icon: &str, tooltip: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(icon).size(16.0)).frame(false)).on_hover_text(tooltip)
}

/// Bouton de barre d'outils : icône au-dessus, libellé dessous.
pub fn toolbar_button(ui: &mut Ui, icon: &str, label: &str, enabled: bool, busy: bool) -> Response {
    let size = egui::vec2(58.0, 46.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    a11y(&resp, label, enabled);
    let p = Palette::current(ui.ctx());
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), p.hover);
    }
    let color = if enabled { p.text } else { p.weak.gamma_multiply(0.6) };
    let icon_color = if enabled { p.accent } else { color };
    if busy {
        let r = Rect::from_center_size(rect.center_top() + egui::vec2(0.0, 15.0), egui::vec2(16.0, 16.0));
        egui::Spinner::new().size(16.0).color(p.accent).paint_at(ui, r);
    } else {
        ui.painter().text(
            rect.center_top() + egui::vec2(0.0, 15.0),
            egui::Align2::CENTER_CENTER,
            icon,
            FontId::proportional(20.0),
            icon_color,
        );
    }
    ui.painter().text(
        rect.center_bottom() - egui::vec2(0.0, 10.0),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.5),
        color,
    );
    resp
}

/// Champ de recherche avec icône loupe et bouton d'effacement.
pub fn search_field(ui: &mut Ui, text: &mut String, hint: &str, id_salt: &str) -> Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let width = ui.available_width() - if text.is_empty() { 0.0 } else { 22.0 };
        let resp = ui.add(
            egui::TextEdit::singleline(text)
                .id_salt(id_salt)
                .hint_text(format!("{}  {hint}", crate::icons::MAGNIFYING_GLASS))
                .desired_width(width.max(40.0)),
        );
        if !text.is_empty()
            && ui.add(egui::Button::new(crate::icons::X).frame(false)).on_hover_text(tr!("Effacer")).clicked()
        {
            text.clear();
        }
        resp
    })
    .inner
}

/// Pastille de nombre (compteurs).
pub fn count_pill(ui: &mut Ui, n: usize, p: &Palette) {
    if n > 0 {
        let text = if n > 9999 { "9999+".to_string() } else { n.to_string() };
        ui.label(RichText::new(text).size(11.0).color(p.weak));
    }
}

/// Raccourcit le milieu d'un chemin trop long (`C:/…/dossier/fichier.rs`).
pub fn ellipsize_middle(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars || max_chars < 5 {
        return s.to_string();
    }
    let keep = max_chars - 1;
    let head = keep / 3;
    let tail = keep - head;
    let start: String = s.chars().take(head).collect();
    let end: String = s.chars().skip(count - tail).collect();
    format!("{start}…{end}")
}

/// Date relative pour les dates récentes (« il y a 5 min »), absolue sinon.
pub fn format_time(ts: i64, now: i64) -> String {
    let delta = now - ts;
    if (0..60).contains(&delta) {
        return tr!("à l'instant").into();
    }
    if (0..3600).contains(&delta) {
        return tr!("il y a {n} min", n = delta / 60);
    }
    if (0..86_400).contains(&delta) {
        return tr!("il y a {n} h", n = delta / 3600);
    }
    if (0..7 * 86_400).contains(&delta) {
        let days = delta / 86_400;
        return if days == 1 { tr!("hier").into() } else { trn!(days, "il y a {n} jour", "il y a {n} jours") };
    }
    format_date(ts)
}

pub fn format_date(ts: i64) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_opt(ts, 0) {
        chrono::LocalResult::Single(d) | chrono::LocalResult::Ambiguous(d, _) => {
            // Le format vient du catalogue : s'il était invalide, chrono échouerait (et `to_string` paniquerait).
            use std::fmt::Write as _;
            let mut out = String::new();
            if write!(out, "{}", d.format(tr!("%d/%m/%Y %H:%M"))).is_err() {
                out = d.format("%Y-%m-%d %H:%M").to_string();
            }
            out
        }
        chrono::LocalResult::None => String::new(),
    }
}

pub fn now_ts() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Correspondance floue (sous-séquence, insensible à la casse et aux accents).
/// Score plus élevé = meilleur ; `None` si pas de correspondance.
pub fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = fold(query).chars().filter(|c| !c.is_whitespace()).collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = fold(text).chars().collect();
    let mut score = 0;
    let mut qi = 0;
    let mut prev_match: Option<usize> = None;
    for (i, &c) in t.iter().enumerate() {
        if qi < q.len() && c == q[qi] {
            score += 1;
            if prev_match == Some(i.wrapping_sub(1)) {
                score += 3; // caractères consécutifs
            }
            if i == 0 || matches!(t[i - 1], '/' | '\\' | '-' | '_' | ' ' | '.') {
                score += 2; // début de mot
            }
            prev_match = Some(i);
            qi += 1;
        }
    }
    if qi < q.len() {
        return None;
    }
    // Bonus si la requête apparaît telle quelle.
    if fold(text).contains(&fold(query)) {
        score += 10;
    }
    Some(score - (t.len() as i32 / 20))
}

/// Minuscules sans accents (comparaisons de recherche).
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    fold_into(&mut out, s);
    out
}

/// Comme [`fold`], dans `out` (vidé d'abord) : sans allocation quand on replie beaucoup de textes à la suite.
pub fn fold_into(out: &mut String, s: &str) {
    out.clear();
    if s.is_ascii() {
        // Cas courant, bien plus rapide : ni accent ni minuscule hors ASCII.
        out.push_str(s);
        out.make_ascii_lowercase();
        return;
    }
    for c in s.chars() {
        if c.is_ascii() {
            out.push(c.to_ascii_lowercase());
            continue;
        }
        out.extend(c.to_lowercase().map(|c| match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        }));
    }
}

/// Ligne cliquable pleine largeur (fond survolé/sélectionné).
/// Ligne pleine largeur. `id_salt` identifie l'élément affiché (commit, chemin…) : l'identifiant
/// ne dépend pas de la position, si bien qu'un menu contextuel ouvert reste attaché à son élément
/// quand la liste défile ou se recharge.
pub fn row_response(
    ui: &mut Ui,
    id_salt: impl egui::AsIdSalt,
    height: f32,
    selected: bool,
    sense: Sense,
) -> (Rect, Response) {
    let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), height));
    let resp = ui.interact(rect, ui.scope_id().with(id_salt), sense);
    let p = Palette::current(ui.ctx());
    if selected {
        ui.painter().rect_filled(rect, CornerRadius::same(4), p.selection);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(4), p.hover);
    }
    (rect, resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches_subsequence_and_ranks_prefix_higher() {
        assert!(fuzzy_score("drg", "Dragon").is_some());
        assert!(fuzzy_score("drgx", "Dragon").is_none());
        assert!(fuzzy_score("peppol cop", "Peppol - Copie").is_some());
        let exact = fuzzy_score("cock", "cockpit").unwrap();
        let scattered = fuzzy_score("cock", "c-o-c-k-x").unwrap();
        assert!(exact > scattered);
        assert!(fuzzy_score("depot", "Dépôt").is_some(), "insensible aux accents");
        assert_eq!(fuzzy_score("", "x"), Some(0));
    }

    #[test]
    fn folding_drops_case_and_accents_with_or_without_ascii_only_text() {
        assert_eq!(fold("Corrige le README"), "corrige le readme");
        assert_eq!(fold("Émile CORRIGE le Dépôt ÇA"), "emile corrige le depot ca");
        let mut out = String::from("reste");
        fold_into(&mut out, "Ñandú");
        assert_eq!(out, "nandu", "le tampon est vidé d'abord");
    }

    #[test]
    fn relative_times() {
        let now = 1_700_000_000;
        assert_eq!(format_time(now - 10, now), "à l'instant");
        assert_eq!(format_time(now - 5 * 60, now), "il y a 5 min");
        assert_eq!(format_time(now - 3 * 3600, now), "il y a 3 h");
        assert_eq!(format_time(now - 86_400 - 10, now), "hier");
        assert_eq!(format_time(now - 3 * 86_400, now), "il y a 3 jours");
        assert!(format_time(now - 30 * 86_400, now).contains('/'));
    }

    #[test]
    fn ellipsize_keeps_both_ends() {
        assert_eq!(ellipsize_middle("court", 10), "court");
        let s = ellipsize_middle("C:/Users/moi/DEV/projet/src/fichier.rs", 20);
        assert_eq!(s.chars().count(), 20);
        assert!(s.starts_with("C:/") && s.ends_with("fichier.rs") && s.contains('…'));
    }
}
