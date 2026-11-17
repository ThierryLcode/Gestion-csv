//! Petits composants d'interface réutilisables et descriptions en français.

use std::sync::Arc;

use eframe::egui::{self, Color32, RichText, Ui};
use moteur::sort::SortMode;
use moteur::steps::{CaseMode, Combine, DedupeMode, Operator, Step, Transform};
use moteur::Entry;

pub const ACCENT: Color32 = Color32::from_rgb(37, 99, 235);
pub const GREEN: Color32 = Color32::from_rgb(46, 157, 87);
pub const RED: Color32 = Color32::from_rgb(214, 69, 69);
pub const ORANGE: Color32 = Color32::from_rgb(217, 144, 43);

/// Nombre formaté à la française : 1 234 567.
pub fn fmt_n(n: impl Into<u64>) -> String {
    let s = n.into().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('\u{202f}');
        }
        out.push(c);
    }
    out
}

pub fn fmt_bytes(n: u64) -> String {
    let units = ["o", "Ko", "Mo", "Go", "To"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} o") } else { format!("{:.1} {}", v, units[i]).replace('.', ",") }
}

pub fn delim_label(d: u8) -> String {
    match d {
        b',' => "virgule".into(),
        b';' => "point-virgule".into(),
        b'\t' => "tabulation".into(),
        b'|' => "barre".into(),
        b':' => "deux-points".into(),
        b' ' => "espace".into(),
        c => format!("« {} »", c as char),
    }
}

/// Liste déroulante de fichiers. Renvoie vrai si la sélection a changé.
pub fn file_combo(ui: &mut Ui, id: &str, files: &[Arc<Entry>], selected: &mut Option<String>, width: f32) -> bool {
    let text = selected
        .as_ref()
        .and_then(|s| files.iter().find(|f| &f.id == s))
        .map(|f| format!("{} ({} lignes)", f.name(), fmt_n(f.rows)))
        .unwrap_or_else(|| if files.is_empty() { "— aucun fichier —".into() } else { "— choisir un fichier —".into() });
    let before = selected.clone();
    egui::ComboBox::from_id_salt(id).width(width).selected_text(text).show_ui(ui, |ui| {
        for f in files {
            ui.selectable_value(selected, Some(f.id.clone()), format!("{} ({} lignes)", f.name(), fmt_n(f.rows)));
        }
    });
    *selected != before
}

/// Liste déroulante de colonnes.
pub fn column_combo(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, columns: &[String], selected: &mut String, width: f32) {
    egui::ComboBox::from_id_salt(id).width(width).selected_text(selected.as_str()).show_ui(ui, |ui| {
        for c in columns {
            ui.selectable_value(selected, c.clone(), c);
        }
    });
}

/// Liste à cocher dont l'ordre de sélection est mémorisé (numéros affichés).
pub fn ordered_checklist(ui: &mut Ui, items: &[String], order: &mut Vec<String>) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for item in items {
            let pos = order.iter().position(|o| o == item);
            let mut checked = pos.is_some();
            let label = match pos {
                Some(p) => RichText::new(format!("{item}  {}", p + 1)),
                None => RichText::new(item),
            };
            if ui.checkbox(&mut checked, label).changed() {
                if checked {
                    order.push(item.clone());
                } else {
                    order.retain(|o| o != item);
                }
                changed = true;
            }
            ui.add_space(8.0);
        }
    });
    changed
}

/// Sélecteur de colonnes avec option « toutes les colonnes » (liste vide = toutes).
pub fn columns_or_all(ui: &mut Ui, items: &[String], all: &mut bool, order: &mut Vec<String>) {
    ui.checkbox(all, RichText::new("Toutes les colonnes").strong());
    if !*all {
        ordered_checklist(ui, items, order);
    }
}

/// Encadré de section.
pub fn section<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .inner_margin(10.0)
        .corner_radius(6.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(RichText::new(title).strong());
                ui.add_space(4.0);
            }
            add(ui)
        })
        .inner
}

/// Bouton d'action principal.
pub fn primary_button(ui: &mut Ui, text: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(text).strong().color(Color32::WHITE)).fill(ACCENT).min_size(egui::vec2(110.0, 30.0)))
        .clicked()
}

pub fn intro(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).weak());
    ui.add_space(6.0);
}

/// Petit diagramme de Venn pour les types de jointure.
#[derive(Clone, Copy)]
pub enum VennFill {
    Inter,
    A,
    B,
    Both,
    AOnly,
    BOnly,
}

pub fn venn(ui: &mut Ui, fill: VennFill) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(60.0, 36.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let r = 14.0;
    let ca = rect.center() - egui::vec2(8.0, 0.0);
    let cb = rect.center() + egui::vec2(8.0, 0.0);
    let color = ACCENT.gamma_multiply(0.75);
    let mut mesh = egui::Mesh::default();
    let step = 1.0;
    let mut y = rect.top();
    while y < rect.bottom() {
        let mut x = rect.left();
        while x < rect.right() {
            let p = egui::pos2(x + step / 2.0, y + step / 2.0);
            let (ina, inb) = (p.distance(ca) <= r, p.distance(cb) <= r);
            let on = match fill {
                VennFill::Inter => ina && inb,
                VennFill::A => ina,
                VennFill::B => inb,
                VennFill::Both => ina || inb,
                VennFill::AOnly => ina && !inb,
                VennFill::BOnly => inb && !ina,
            };
            if on {
                mesh.add_colored_rect(egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(step, step)), color);
            }
            x += step;
        }
        y += step;
    }
    painter.add(egui::Shape::mesh(mesh));
    let stroke = egui::Stroke::new(1.5, ui.visuals().weak_text_color());
    painter.circle_stroke(ca, r, stroke);
    painter.circle_stroke(cb, r, stroke);
}

// ---------------------------------------------------------------------------
// Descriptions des étapes (recettes)
// ---------------------------------------------------------------------------

pub fn quoted_list(cols: &[String]) -> String {
    if cols.is_empty() { "toutes les colonnes".into() } else { cols.iter().map(|c| format!("« {c} »")).collect::<Vec<_>>().join(", ") }
}

pub fn operator_label(op: Operator) -> &'static str {
    match op {
        Operator::In => "est égal à l'une des valeurs",
        Operator::NotIn => "n'est égal à aucune des valeurs",
        Operator::Contains => "contient",
        Operator::NotContains => "ne contient pas",
        Operator::StartsWith => "commence par",
        Operator::EndsWith => "se termine par",
        Operator::Empty => "est vide",
        Operator::NotEmpty => "n'est pas vide",
        Operator::Gt => "> (supérieur à)",
        Operator::Ge => "≥ (supérieur ou égal)",
        Operator::Lt => "< (inférieur à)",
        Operator::Le => "≤ (inférieur ou égal)",
        Operator::Between => "est compris entre",
        Operator::Regex => "correspond à l'expression régulière",
    }
}

pub fn sort_mode_label(m: SortMode) -> &'static str {
    match m {
        SortMode::Text => "Alphabétique",
        SortMode::Number => "Numérique",
        SortMode::Natural => "Naturel (a2 < a10)",
        SortMode::Date => "Chronologique",
        SortMode::Raw => "Brut",
    }
}

pub fn case_label(m: CaseMode) -> &'static str {
    match m {
        CaseMode::Upper => "MAJUSCULES",
        CaseMode::Lower => "minuscules",
        CaseMode::Title => "Première Lettre De Chaque Mot",
        CaseMode::Sentence => "Première lettre de la cellule",
    }
}

fn describe_transform(t: &Transform) -> String {
    match t {
        Transform::Trim { columns, .. } => format!("espaces superflus supprimés ({})", quoted_list(columns)),
        Transform::Case { columns, mode } => format!("casse {} ({})", case_label(*mode), quoted_list(columns)),
        Transform::Replace { columns, search, replacement, .. } => {
            format!("remplacer « {search} » par « {replacement} » ({})", quoted_list(columns))
        }
        Transform::Date { columns, input, output, .. } => {
            format!("dates {} : {} → {output}", quoted_list(columns), if input.is_empty() { "auto" } else { input })
        }
        Transform::Number { columns, decimal } => {
            format!("nombres {} au format {}", quoted_list(columns), if decimal == "," { "1234,5" } else { "1234.5" })
        }
        Transform::Concat { columns, separator, target, .. } => {
            format!("« {target} » = {} séparées par « {separator} »", quoted_list(columns))
        }
        Transform::Extract { column, pattern, target, .. } => format!("« {target} » = extrait de « {column} » via {pattern}"),
        Transform::Constant { target, value } => format!("« {target} » = « {value} »"),
    }
}

pub fn describe_step(s: &Step) -> String {
    match s {
        Step::Columns { columns } => format!(
            "Colonnes : {}",
            columns
                .iter()
                .map(|c| if c.name.is_empty() || c.name == c.source { c.source.clone() } else { format!("{} → {}", c.source, c.name) })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Step::Sort { keys } => format!(
            "Trier par {}",
            keys.iter()
                .map(|k| format!("{} ({}{})", k.column, sort_mode_label(k.mode).to_lowercase(), if k.descending { ", décroissant" } else { "" }))
                .collect::<Vec<_>>()
                .join(", puis ")
        ),
        Step::Dedupe { columns, mode, .. } => format!(
            "{} sur {}",
            match mode {
                DedupeMode::Remove => "Supprimer les doublons",
                DedupeMode::Duplicates => "Isoler les doublons",
                DedupeMode::Removed => "Garder les lignes écartées",
                DedupeMode::Singles => "Garder les lignes uniques",
            },
            if columns.is_empty() { "la ligne entière".into() } else { quoted_list(columns) }
        ),
        Step::Filter { conditions, combine } => format!(
            "Filtrer : {}",
            conditions
                .iter()
                .map(|c| {
                    let vals = c.values.iter().map(|v| format!("« {v} »")).collect::<Vec<_>>().join(", ");
                    format!("{} {}{}", c.column, operator_label(c.operator), if vals.is_empty() { String::new() } else { format!(" {vals}") })
                })
                .collect::<Vec<_>>()
                .join(if *combine == Combine::Any { " OU " } else { " ET " })
        ),
        Step::Transform { operations } => {
            format!("Nettoyage : {}", operations.iter().map(describe_transform).collect::<Vec<_>>().join(" ; "))
        }
    }
}

/// Tableau compact (aperçu de résultat).
pub fn small_table(ui: &mut Ui, id: &str, headers: &[String], rows: &[Vec<String>], height: f32) {
    use egui_extras::{Column, TableBuilder};
    egui::ScrollArea::horizontal().id_salt(format!("{id}-h")).show(ui, |ui| {
        TableBuilder::new(ui)
            .id_salt(id)
            .striped(true)
            .resizable(true)
            .max_scroll_height(height)
            .column(Column::auto().at_least(40.0))
            .columns(Column::initial(130.0).at_least(40.0).clip(true), headers.len())
            .header(24.0, |mut row| {
                row.col(|ui| {
                    ui.label(RichText::new("#").weak());
                });
                for h in headers {
                    row.col(|ui| {
                        ui.label(RichText::new(h).strong());
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, rows.len(), |mut row| {
                    let i = row.index();
                    row.col(|ui| {
                        ui.label(RichText::new(fmt_n((i + 1) as u64)).weak());
                    });
                    for v in &rows[i] {
                        row.col(|ui| {
                            ui.label(v).on_hover_text(v);
                        });
                    }
                });
            });
    });
}
