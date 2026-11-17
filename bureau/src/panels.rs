//! Contenu des onglets d'opérations.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use eframe::egui::{self, Color32, RichText, Ui};
use moteur::multi::{CompareSummary, JoinKind, MergeMode};
use moteur::profile::Profile;
use moteur::service::{CompareRequest, JoinRequest, MergeRequest, RecipeRequest, SplitRequest, Values};
use moteur::sort::{SortKey, SortMode};
use moteur::steps::{CaseMode, ColumnSpec, Combine, Condition, DedupeMode, Keep, Operator, Step, Transform};
use moteur::sql::quote_ident;
use moteur::{Entry, Recipe};

use crate::app::{GestionCsv, Outcome, Tab};
use crate::widgets::*;

type Preview = (Vec<String>, Vec<Vec<String>>);

#[derive(Default)]
pub struct Panels {
    pub profiles: HashMap<String, Profile>,
    profile_requested: HashSet<String>,
    pub values: HashMap<(String, String), Values>,

    columns: ColumnsUi,
    sort: SortUi,
    dedupe: DedupeUi,
    filter: FilterUi,
    transform: TransformUi,

    split_file: String,
    split_cols: Vec<String>,
    split_remove: bool,
    pub split_result: Vec<String>,

    merge: MergeUi,
    join: JoinUi,
    compare: CompareUi,
    pub compare_result: Option<(String, CompareSummary, Preview)>,

    sql_query: String,
    sql_name: String,
    pub sql_result: Option<(String, Preview)>,

    pub recipe_from: Option<String>,
    recipe_name: String,
    recipe_desc: String,
    recipe_targets: HashMap<String, Vec<String>>,
}

pub fn show(app: &mut GestionCsv, ui: &mut Ui) {
    match app.tab {
        Tab::Preview => {}
        Tab::Profile => profile(app, ui),
        Tab::Columns => columns(app, ui),
        Tab::Sort => sort(app, ui),
        Tab::Dedupe => dedupe(app, ui),
        Tab::Filter => filter(app, ui),
        Tab::Transform => transform(app, ui),
        Tab::Merge => merge(app, ui),
        Tab::Join => join(app, ui),
        Tab::Compare => compare(app, ui),
        Tab::Split => split(app, ui),
        Tab::Sql => sql(app, ui),
        Tab::Recipes => recipes(app, ui),
    }
}

/// Fichier de travail, ou message invitant à en choisir un.
fn need_active(app: &GestionCsv, ui: &mut Ui) -> Option<Arc<Entry>> {
    let f = app.active_file();
    if f.is_none() {
        ui.add_space(20.0);
        ui.label(RichText::new("Choisissez d'abord un fichier de travail (liste ci-dessus ou panneau de gauche).").weak().size(15.0));
    }
    f
}

fn run_step(app: &mut GestionCsv, ui: &Ui, id: &str, step: Step, what: &'static str, label: &str) {
    let id = id.to_string();
    app.spawn(ui.ctx(), label, move |st| GestionCsv::done(st.apply_step(&id, &step, None)?, what));
}

// ---------------------------------------------------------------------------
// Profil
// ---------------------------------------------------------------------------

fn profile(app: &mut GestionCsv, ui: &mut Ui) {
    let Some(f) = need_active(app, ui) else { return };
    intro(ui, "Vue d'ensemble de chaque colonne : type détecté, cellules vides, valeurs distinctes, extrêmes et valeurs les plus fréquentes.");
    let Some(p) = app.panels.profiles.get(&f.id) else {
        if !app.busy() && app.panels.profile_requested.insert(f.id.clone()) {
            let id = f.id.clone();
            app.spawn(ui.ctx(), "Analyse du fichier…", move |st| Ok(Outcome::Profile(id.clone(), st.profile_of(&id)?)));
        }
        if ui.button("Analyser").clicked() {
            app.panels.profile_requested.remove(&f.id);
        }
        return;
    };
    let p_rows = p.rows;
    let empty_cols = p.columns.iter().filter(|c| c.empty > 0).count();
    ui.horizontal_wrapped(|ui| {
        for (v, l) in [
            (fmt_n(p_rows), "lignes"),
            (fmt_n(p.columns.len() as u64), "colonnes"),
            (fmt_n(empty_cols as u64), "colonnes avec des vides"),
            (fmt_bytes(f.size), "taille"),
        ] {
            stat(ui, &v, l, None);
        }
    });
    ui.add_space(6.0);
    let mut export = false;
    let mut reanalyse = false;
    ui.horizontal(|ui| {
        reanalyse = ui.button("Analyser à nouveau").clicked();
        export = ui.button("Exporter le profil (CSV)…").clicked();
    });
    ui.add_space(6.0);
    {
        use egui_extras::{Column, TableBuilder};
        egui::ScrollArea::horizontal().id_salt("pf-h").show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt(("profile", f.id.as_str()))
                .striped(true)
                .resizable(true)
                .vscroll(false)
                .column(Column::initial(150.0).clip(true))
                .column(Column::initial(80.0))
                .column(Column::initial(130.0))
                .column(Column::initial(80.0))
                .columns(Column::initial(110.0).clip(true), 2)
                .column(Column::initial(90.0))
                .column(Column::initial(80.0))
                .column(Column::remainder().at_least(200.0).clip(true))
                .header(26.0, |mut row| {
                    for t in ["Colonne", "Type", "Remplissage", "Distinctes", "Min", "Max", "Moyenne", "Longueur", "Valeurs les plus fréquentes"] {
                        row.col(|ui| {
                            ui.label(RichText::new(t).strong());
                        });
                    }
                })
                .body(|mut body| {
                    for c in &p.columns {
                        body.row(30.0, |mut row| {
                            row.col(|ui| {
                                ui.label(RichText::new(&c.name).strong()).on_hover_text(&c.name);
                            });
                            row.col(|ui| {
                                let color = match c.kind.as_str() {
                                    "entier" | "décimal" => GREEN,
                                    "date" => ORANGE,
                                    "booléen" => ACCENT,
                                    _ => ui.visuals().weak_text_color(),
                                };
                                ui.label(RichText::new(&c.kind).color(color));
                            });
                            row.col(|ui| {
                                let fill = if p_rows > 0 { c.filled as f32 / p_rows as f32 } else { 0.0 };
                                let txt = format!("{:.0} %{}", fill * 100.0, if c.empty > 0 { format!(" · {} vides", fmt_n(c.empty)) } else { String::new() });
                                ui.add(egui::ProgressBar::new(fill).text(txt).desired_width(120.0));
                            });
                            row.col(|ui| {
                                let mut t = format!("{}{}", if c.distinct_capped { "> " } else { "" }, fmt_n(c.distinct));
                                if c.distinct == c.filled && c.filled > 1 && !c.distinct_capped {
                                    t.push_str(" (unique)");
                                }
                                ui.label(t);
                            });
                            row.col(|ui| {
                                ui.label(c.min.clone().unwrap_or("—".into()));
                            });
                            row.col(|ui| {
                                ui.label(c.max.clone().unwrap_or("—".into()));
                            });
                            row.col(|ui| {
                                ui.label(c.mean.map(|m| format!("{m:.2}").replace('.', ",")).unwrap_or("—".into()));
                            });
                            row.col(|ui| {
                                ui.label(if c.filled == 0 {
                                    "—".into()
                                } else if c.min_length == c.max_length {
                                    c.min_length.to_string()
                                } else {
                                    format!("{}–{}", c.min_length, c.max_length)
                                });
                            });
                            row.col(|ui| {
                                let t = c.top.iter().take(5).map(|(v, n)| format!("{v} ({})", fmt_n(*n))).collect::<Vec<_>>().join(" · ");
                                ui.add(egui::Label::new(&t).truncate()).on_hover_text(&t);
                            });
                        });
                    }
                });
        });
    }
    if reanalyse {
        app.panels.profiles.remove(&f.id);
        app.panels.profile_requested.remove(&f.id);
    }
    if export {
        export_profile(app, &f);
    }
}

fn export_profile(app: &mut GestionCsv, f: &Entry) {
    let Some(p) = app.panels.profiles.get(&f.id) else { return };
    let Some(dest) = rfd::FileDialog::new()
        .set_file_name(format!("profil_{}.csv", moteur::service::stem(&f.name())))
        .add_filter("Fichier CSV", &["csv"])
        .save_file()
    else {
        return;
    };
    let esc = |v: &str| if v.contains([';', '"', '\n']) { format!("\"{}\"", v.replace('"', "\"\"")) } else { v.to_string() };
    let mut out = String::from("\u{feff}colonne;type;remplies;vides;distinctes;min;max;moyenne;longueur_min;longueur_max;valeurs_frequentes\r\n");
    for c in &p.columns {
        let top = c.top.iter().map(|(v, n)| format!("{v} ({n})")).collect::<Vec<_>>().join(" | ");
        let fields = [
            c.name.clone(),
            c.kind.clone(),
            c.filled.to_string(),
            c.empty.to_string(),
            c.distinct.to_string(),
            c.min.clone().unwrap_or_default(),
            c.max.clone().unwrap_or_default(),
            c.mean.map(|m| m.to_string().replace('.', ",")).unwrap_or_default(),
            c.min_length.to_string(),
            c.max_length.to_string(),
            top,
        ];
        out.push_str(&fields.iter().map(|v| esc(v)).collect::<Vec<_>>().join(";"));
        out.push_str("\r\n");
    }
    match std::fs::write(&dest, out) {
        Ok(()) => app.info(format!("Profil enregistré : {}", dest.display())),
        Err(e) => app.error(format!("Enregistrement impossible : {e}")),
    }
}

fn stat(ui: &mut Ui, value: &str, label: &str, color: Option<Color32>) {
    let stroke = match color {
        Some(c) => egui::Stroke::new(2.0, c),
        None => ui.visuals().widgets.noninteractive.bg_stroke,
    };
    egui::Frame::group(ui.style()).stroke(stroke).inner_margin(10.0).corner_radius(6.0).show(ui, |ui| {
        ui.set_min_width(120.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(value).size(22.0).strong());
            ui.label(RichText::new(label).weak());
        });
    });
}

// ---------------------------------------------------------------------------
// Colonnes
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ColumnsUi {
    file: String,
    rows: Vec<(String, bool, String)>, // (source, conserver, nouveau nom)
}

fn columns(app: &mut GestionCsv, ui: &mut Ui) {
    let Some(f) = need_active(app, ui) else { return };
    intro(ui, "Cochez les colonnes à conserver, réordonnez-les avec les flèches et renommez-les si besoin.");
    let st = &mut app.panels.columns;
    if st.file != f.id {
        st.file = f.id.clone();
        st.rows = f.headers.iter().map(|h| (h.clone(), true, String::new())).collect();
    }
    let mut apply = false;
    section(ui, "", |ui| {
        ui.horizontal(|ui| {
            if ui.button("Tout cocher").clicked() {
                st.rows.iter_mut().for_each(|r| r.1 = true);
            }
            if ui.button("Tout décocher").clicked() {
                st.rows.iter_mut().for_each(|r| r.1 = false);
            }
            if ui.button("Trier A → Z").clicked() {
                st.rows.sort_by_key(|a| moteur::text::fold(&a.0));
            }
            if ui.button("Ordre d'origine").clicked() {
                st.rows.sort_by_key(|r| f.headers.iter().position(|h| *h == r.0));
            }
        });
        ui.add_space(6.0);
        let n = st.rows.len();
        let mut swap: Option<(usize, usize)> = None;
        for (i, (src, keep, name)) in st.rows.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.checkbox(keep, "");
                let txt = RichText::new(src.as_str()).strong();
                let label = egui::Label::new(if *keep { txt } else { txt.strikethrough().weak() }).truncate();
                ui.allocate_ui_with_layout(egui::vec2(220.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_width(220.0);
                    ui.add(label);
                });
                ui.add_enabled(*keep, egui::TextEdit::singleline(name).hint_text("nouveau nom (facultatif)").desired_width(240.0));
                if ui.add_enabled(i > 0, egui::Button::new("↑")).on_hover_text("Monter").clicked() {
                    swap = Some((i, i - 1));
                }
                if ui.add_enabled(i + 1 < n, egui::Button::new("↓")).on_hover_text("Descendre").clicked() {
                    swap = Some((i, i + 1));
                }
            });
        }
        if let Some((a, b)) = swap {
            st.rows.swap(a, b);
        }
    });
    ui.add_space(8.0);
    if primary_button(ui, "Appliquer") {
        apply = true;
    }
    if apply {
        let cols: Vec<ColumnSpec> = app.panels.columns.rows.iter().filter(|r| r.1).map(|r| ColumnSpec { source: r.0.clone(), name: r.2.trim().to_string() }).collect();
        if cols.is_empty() {
            return app.error("Conservez au moins une colonne.");
        }
        run_step(app, ui, &f.id, Step::Columns { columns: cols }, "Colonnes modifiées", "Réorganisation des colonnes…");
    }
}

// ---------------------------------------------------------------------------
// Tri
// ---------------------------------------------------------------------------

#[derive(Default)]
struct SortUi {
    file: String,
    keys: Vec<(String, SortMode, bool)>,
}

fn sort(app: &mut GestionCsv, ui: &mut Ui) {
    let Some(f) = need_active(app, ui) else { return };
    intro(ui, "Trie les lignes sur une ou plusieurs colonnes (la première est prioritaire). Les cellules vides sont toujours placées à la fin. Fonctionne aussi sur des fichiers plus gros que la mémoire.");
    let st = &mut app.panels.sort;
    if st.file != f.id {
        st.file = f.id.clone();
        st.keys = vec![(f.headers.first().cloned().unwrap_or_default(), SortMode::Text, false)];
    }
    section(ui, "Critères de tri", |ui| {
        let mut remove = None;
        for (i, (col, mode, desc)) in st.keys.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}.", i + 1)).weak());
                column_combo(ui, ("sort-col", i), &f.headers, col, 220.0);
                egui::ComboBox::from_id_salt(("sort-mode", i)).width(170.0).selected_text(sort_mode_label(*mode)).show_ui(ui, |ui| {
                    for m in [SortMode::Text, SortMode::Number, SortMode::Natural, SortMode::Date] {
                        ui.selectable_value(mode, m, sort_mode_label(m));
                    }
                });
                egui::ComboBox::from_id_salt(("sort-dir", i)).width(120.0).selected_text(if *desc { "Décroissant" } else { "Croissant" }).show_ui(ui, |ui| {
                    ui.selectable_value(desc, false, "Croissant");
                    ui.selectable_value(desc, true, "Décroissant");
                });
                if ui.small_button("×").on_hover_text("Retirer").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            st.keys.remove(i);
        }
        if ui.button("+ Ajouter un critère").clicked() {
            st.keys.push((f.headers.first().cloned().unwrap_or_default(), SortMode::Text, false));
        }
    });
    ui.add_space(8.0);
    if primary_button(ui, "Trier") {
        let keys: Vec<SortKey> =
            app.panels.sort.keys.iter().map(|(c, m, d)| SortKey { column: c.clone(), mode: *m, descending: *d }).collect();
        if keys.is_empty() {
            return app.error("Ajoutez au moins un critère de tri.");
        }
        run_step(app, ui, &f.id, Step::Sort { keys }, "Tri terminé", "Tri en cours…");
    }
}

// ---------------------------------------------------------------------------
// Doublons
// ---------------------------------------------------------------------------

#[derive(Default)]
struct DedupeUi {
    file: String,
    cols: Vec<String>,
    mode: DedupeMode,
    keep: Keep,
    ci: bool,
    trim: bool,
}

fn dedupe(app: &mut GestionCsv, ui: &mut Ui) {
    let Some(f) = need_active(app, ui) else { return };
    intro(ui, "Repère les lignes en double selon certaines colonnes (ou la ligne entière) pour les supprimer ou les isoler.");
    let st = &mut app.panels.dedupe;
    if st.file != f.id {
        *st = DedupeUi { file: f.id.clone(), trim: true, ..Default::default() };
    }
    section(ui, "Colonnes formant l'identifiant", |ui| {
        ui.label(RichText::new("Aucune colonne cochée : deux lignes sont des doublons si toutes leurs cellules sont identiques.").small().weak());
        ordered_checklist(ui, &f.headers, &mut st.cols);
    });
    ui.add_space(6.0);
    section(ui, "Résultat souhaité", |ui| {
        ui.radio_value(&mut st.mode, DedupeMode::Remove, "Supprimer les doublons — une seule ligne par identifiant");
        ui.radio_value(&mut st.mode, DedupeMode::Duplicates, "Isoler les doublons — toutes les lignes dont l'identifiant apparaît plusieurs fois");
        ui.radio_value(&mut st.mode, DedupeMode::Removed, "Lignes écartées — uniquement les lignes que la suppression retirerait");
        ui.radio_value(&mut st.mode, DedupeMode::Singles, "Lignes uniques — uniquement les identifiants présents une seule fois");
        if matches!(st.mode, DedupeMode::Remove | DedupeMode::Removed) {
            ui.horizontal(|ui| {
                ui.radio_value(&mut st.keep, Keep::First, "Garder la première occurrence");
                ui.radio_value(&mut st.keep, Keep::Last, "Garder la dernière occurrence");
            });
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut st.ci, "Ignorer la casse");
            ui.checkbox(&mut st.trim, "Ignorer les espaces autour des valeurs");
        });
    });
    ui.add_space(8.0);
    if primary_button(ui, "Appliquer") {
        let st = &app.panels.dedupe;
        let step = Step::Dedupe { columns: st.cols.clone(), mode: st.mode, keep: st.keep, case_insensitive: st.ci, trim: st.trim };
        let what = if st.mode == DedupeMode::Remove { "Doublons supprimés" } else { "Doublons isolés" };
        run_step(app, ui, &f.id, step, what, "Recherche des doublons…");
    }
}

// ---------------------------------------------------------------------------
// Filtre
// ---------------------------------------------------------------------------

struct CondUi {
    column: String,
    op: Operator,
    case_sensitive: bool,
    values: Vec<String>,
    input: String,
    v1: String,
    v2: String,
    search: String,
    picker: bool,
}

impl CondUi {
    fn new(column: String) -> Self {
        CondUi { column, op: Operator::In, case_sensitive: false, values: vec![], input: String::new(), v1: String::new(), v2: String::new(), search: String::new(), picker: false }
    }
}

#[derive(Default)]
struct FilterUi {
    file: String,
    combine: Combine,
    conds: Vec<CondUi>,
}

const OPERATORS: [Operator; 14] = [
    Operator::In,
    Operator::NotIn,
    Operator::Contains,
    Operator::NotContains,
    Operator::StartsWith,
    Operator::EndsWith,
    Operator::Empty,
    Operator::NotEmpty,
    Operator::Gt,
    Operator::Ge,
    Operator::Lt,
    Operator::Le,
    Operator::Between,
    Operator::Regex,
];

fn is_multi(op: Operator) -> bool {
    matches!(op, Operator::In | Operator::NotIn | Operator::Contains | Operator::NotContains | Operator::StartsWith | Operator::EndsWith)
}

fn filter(app: &mut GestionCsv, ui: &mut Ui) {
    let Some(f) = need_active(app, ui) else { return };
    intro(ui, "Conserve uniquement les lignes qui respectent les conditions.");
    let first_col = f.headers.first().cloned().unwrap_or_default();
    {
        let st = &mut app.panels.filter;
        if st.file != f.id {
            *st = FilterUi { file: f.id.clone(), combine: Combine::All, conds: vec![CondUi::new(first_col.clone())] };
        }
        ui.horizontal(|ui| {
            ui.radio_value(&mut st.combine, Combine::All, "Toutes les conditions (ET)");
            ui.radio_value(&mut st.combine, Combine::Any, "Au moins une condition (OU)");
        });
    }
    ui.add_space(6.0);
    let mut remove = None;
    let mut load_values: Option<String> = None;
    let values_cache = &app.panels.values;
    for (i, c) in app.panels.filter.conds.iter_mut().enumerate() {
        section(ui, "", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{}.", i + 1)).weak());
                let before = c.column.clone();
                column_combo(ui, ("f-col", i), &f.headers, &mut c.column, 200.0);
                if before != c.column {
                    c.values.clear();
                    c.picker = false;
                }
                egui::ComboBox::from_id_salt(("f-op", i)).width(260.0).selected_text(operator_label(c.op)).show_ui(ui, |ui| {
                    for op in OPERATORS {
                        ui.selectable_value(&mut c.op, op, operator_label(op));
                    }
                });
                ui.checkbox(&mut c.case_sensitive, "Respecter la casse");
                if ui.small_button("×").on_hover_text("Supprimer la condition").clicked() {
                    remove = Some(i);
                }
            });
            match c.op {
                Operator::Empty | Operator::NotEmpty => {}
                Operator::Between => {
                    ui.horizontal(|ui| {
                        ui.label("entre");
                        ui.add(egui::TextEdit::singleline(&mut c.v1).desired_width(140.0));
                        ui.label("et");
                        ui.add(egui::TextEdit::singleline(&mut c.v2).desired_width(140.0));
                    });
                }
                op if !is_multi(op) => {
                    let hint = if op == Operator::Regex { "ex. ^(Paris|Lyon)$" } else { "valeur (nombre, date JJ/MM/AAAA ou texte)" };
                    ui.add(egui::TextEdit::singleline(&mut c.v1).hint_text(hint).desired_width(360.0));
                }
                op => {
                    if !c.values.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            let mut del = None;
                            for (k, v) in c.values.iter().enumerate() {
                                let label = if v.is_empty() { "(vide)  ×".to_string() } else { format!("{v}  ×") };
                                if ui.add(egui::Button::new(RichText::new(label).color(ACCENT)).corner_radius(10.0)).on_hover_text("Retirer").clicked() {
                                    del = Some(k);
                                }
                            }
                            if let Some(k) = del {
                                c.values.remove(k);
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        let r = ui.add(egui::TextEdit::singleline(&mut c.input).hint_text("Saisissez une valeur puis Entrée").desired_width(300.0));
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (ui.button("Ajouter").clicked() || enter) && !c.input.trim().is_empty() {
                            let v = c.input.trim().to_string();
                            if !c.values.contains(&v) {
                                c.values.push(v);
                            }
                            c.input.clear();
                            if enter {
                                r.request_focus();
                            }
                        }
                        if matches!(op, Operator::In | Operator::NotIn) && ui.button(if c.picker { "Masquer les valeurs" } else { "Choisir parmi les valeurs existantes…" }).clicked() {
                            c.picker = !c.picker;
                        }
                    });
                    if c.picker && matches!(op, Operator::In | Operator::NotIn) {
                        match values_cache.get(&(f.id.clone(), c.column.clone())) {
                            None => load_values = Some(c.column.clone()),
                            Some(vals) => value_picker(ui, i, c, vals),
                        }
                    }
                }
            }
        });
        ui.add_space(4.0);
    }
    if let Some(i) = remove {
        app.panels.filter.conds.remove(i);
    }
    if let Some(col) = load_values
        && !app.busy()
    {
        let (id, c2) = (f.id.clone(), col.clone());
        app.spawn(ui.ctx(), "Lecture des valeurs de la colonne…", move |st| Ok(Outcome::Values(id.clone(), c2.clone(), st.values(&id, &c2, 1000)?)));
    }
    ui.horizontal(|ui| {
        if ui.button("+ Ajouter une condition").clicked() {
            app.panels.filter.conds.push(CondUi::new(first_col.clone()));
        }
        if primary_button(ui, "Filtrer") {
            let st = &mut app.panels.filter;
            if st.conds.is_empty() {
                return app.error("Ajoutez au moins une condition.");
            }
            let conditions: Vec<Condition> = st
                .conds
                .iter_mut()
                .map(|c| {
                    if !c.input.trim().is_empty() && !c.values.contains(&c.input.trim().to_string()) {
                        c.values.push(c.input.trim().to_string());
                        c.input.clear();
                    }
                    let values = match c.op {
                        op if is_multi(op) => c.values.clone(),
                        Operator::Between => vec![c.v1.clone(), c.v2.clone()],
                        Operator::Empty | Operator::NotEmpty => vec![],
                        _ => vec![c.v1.clone()],
                    };
                    Condition { column: c.column.clone(), operator: c.op, values, case_sensitive: c.case_sensitive }
                })
                .collect();
            let step = Step::Filter { conditions, combine: st.combine };
            run_step(app, ui, &f.id, step, "Filtrage terminé", "Filtrage en cours…");
        }
    });
}

fn value_picker(ui: &mut Ui, i: usize, c: &mut CondUi, vals: &Values) {
    egui::Frame::group(ui.style()).inner_margin(8.0).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut c.search).hint_text("Rechercher une valeur…").desired_width(240.0));
            let q = c.search.to_lowercase();
            let visible: Vec<&String> = vals.values.iter().map(|(v, _)| v).filter(|v| v.to_lowercase().contains(&q)).collect();
            if ui.small_button("Tout cocher").clicked() {
                for v in &visible {
                    if !c.values.contains(v) {
                        c.values.push((*v).clone());
                    }
                }
            }
            if ui.small_button("Tout décocher").clicked() {
                c.values.retain(|x| !visible.contains(&x));
            }
        });
        let q = c.search.to_lowercase();
        egui::ScrollArea::vertical().id_salt(("picker", i)).max_height(220.0).show(ui, |ui| {
            for (v, n) in vals.values.iter().filter(|(v, _)| v.to_lowercase().contains(&q)) {
                let mut on = c.values.contains(v);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut on, if v.is_empty() { "(vide)" } else { v.as_str() }).changed() {
                        if on {
                            c.values.push(v.clone());
                        } else {
                            c.values.retain(|x| x != v);
                        }
                    }
                    ui.label(RichText::new(fmt_n(*n)).weak().small());
                });
            }
        });
        if vals.distinct > vals.values.len() {
            ui.label(
                RichText::new(format!(
                    "Seules les {} valeurs les plus fréquentes sur {}{} sont listées ; saisissez les autres manuellement.",
                    fmt_n(vals.values.len() as u64),
                    if vals.capped { "plus de " } else { "" },
                    fmt_n(vals.distinct as u64)
                ))
                .small()
                .weak(),
            );
        }
    });
}

// ---------------------------------------------------------------------------
// Nettoyer & calculer
// ---------------------------------------------------------------------------

const DATE_FORMATS: [&str; 10] =
    ["AAAA-MM-JJ", "JJ/MM/AAAA", "JJ-MM-AAAA", "JJ.MM.AAAA", "AAAA/MM/JJ", "MM/JJ/AAAA", "JJ/MM/AA", "AAAAMMJJ", "AAAA-MM-JJ HH:mm:ss", "JJ/MM/AAAA HH:mm"];

enum OpUi {
    Trim { all: bool, cols: Vec<String>, collapse: bool },
    Case { all: bool, cols: Vec<String>, mode: CaseMode },
    Replace { all: bool, cols: Vec<String>, search: String, replacement: String, regex: bool, case_sensitive: bool },
    Date { cols: Vec<String>, input: String, output: String, clear: bool },
    Number { cols: Vec<String>, decimal: String },
    Concat { cols: Vec<String>, separator: String, target: String, skip_empty: bool },
    Extract { column: String, pattern: String, target: String, case_sensitive: bool },
    Constant { target: String, value: String },
}

const OP_KINDS: [&str; 8] = [
    "Supprimer les espaces superflus",
    "Changer la casse (MAJUSCULES, minuscules…)",
    "Rechercher / remplacer",
    "Convertir des dates",
    "Normaliser des nombres",
    "Concaténer des colonnes",
    "Extraire une partie (expression régulière)",
    "Ajouter une colonne à valeur fixe",
];

impl OpUi {
    fn new(kind: usize, first_col: &str) -> Self {
        match kind {
            0 => OpUi::Trim { all: true, cols: vec![], collapse: true },
            1 => OpUi::Case { all: true, cols: vec![], mode: CaseMode::Upper },
            2 => OpUi::Replace { all: true, cols: vec![], search: String::new(), replacement: String::new(), regex: false, case_sensitive: false },
            3 => OpUi::Date { cols: vec![], input: String::new(), output: "AAAA-MM-JJ".into(), clear: false },
            4 => OpUi::Number { cols: vec![], decimal: ".".into() },
            5 => OpUi::Concat { cols: vec![], separator: " ".into(), target: "concatenation".into(), skip_empty: true },
            6 => OpUi::Extract { column: first_col.into(), pattern: String::new(), target: "extrait".into(), case_sensitive: false },
            _ => OpUi::Constant { target: "nouvelle_colonne".into(), value: String::new() },
        }
    }

    fn title(&self) -> &'static str {
        OP_KINDS[match self {
            OpUi::Trim { .. } => 0,
            OpUi::Case { .. } => 1,
            OpUi::Replace { .. } => 2,
            OpUi::Date { .. } => 3,
            OpUi::Number { .. } => 4,
            OpUi::Concat { .. } => 5,
            OpUi::Extract { .. } => 6,
            OpUi::Constant { .. } => 7,
        }]
    }

    fn target(&self) -> Option<&str> {
        match self {
            OpUi::Concat { target, .. } | OpUi::Extract { target, .. } | OpUi::Constant { target, .. } => Some(target.trim()),
            _ => None,
        }
    }

    fn to_transform(&self) -> Transform {
        let pick = |all: bool, cols: &Vec<String>| if all { vec![] } else { cols.clone() };
        match self {
            OpUi::Trim { all, cols, collapse } => Transform::Trim { columns: pick(*all, cols), collapse: *collapse },
            OpUi::Case { all, cols, mode } => Transform::Case { columns: pick(*all, cols), mode: *mode },
            OpUi::Replace { all, cols, search, replacement, regex, case_sensitive } => Transform::Replace {
                columns: pick(*all, cols),
                search: search.clone(),
                replacement: replacement.clone(),
                regex: *regex,
                case_sensitive: *case_sensitive,
            },
            OpUi::Date { cols, input, output, clear } => {
                Transform::Date { columns: cols.clone(), input: input.clone(), output: output.clone(), clear_invalid: *clear }
            }
            OpUi::Number { cols, decimal } => Transform::Number { columns: cols.clone(), decimal: decimal.clone() },
            OpUi::Concat { cols, separator, target, skip_empty } => {
                Transform::Concat { columns: cols.clone(), separator: separator.clone(), target: target.clone(), skip_empty: *skip_empty }
            }
            OpUi::Extract { column, pattern, target, case_sensitive } => {
                Transform::Extract { column: column.clone(), pattern: pattern.clone(), target: target.clone(), case_sensitive: *case_sensitive }
            }
            OpUi::Constant { target, value } => Transform::Constant { target: target.clone(), value: value.clone() },
        }
    }

    fn ui(&mut self, ui: &mut Ui, i: usize, cols: &[String]) {
        match self {
            OpUi::Trim { all, cols: sel, collapse } => {
                columns_or_all(ui, cols, all, sel);
                ui.checkbox(collapse, "Réduire aussi les espaces multiples à un seul");
            }
            OpUi::Case { all, cols: sel, mode } => {
                columns_or_all(ui, cols, all, sel);
                egui::ComboBox::from_id_salt(("case", i)).width(260.0).selected_text(case_label(*mode)).show_ui(ui, |ui| {
                    for m in [CaseMode::Upper, CaseMode::Lower, CaseMode::Title, CaseMode::Sentence] {
                        ui.selectable_value(mode, m, case_label(m));
                    }
                });
            }
            OpUi::Replace { all, cols: sel, search, replacement, regex, case_sensitive } => {
                columns_or_all(ui, cols, all, sel);
                egui::Grid::new(("repl", i)).num_columns(2).show(ui, |ui| {
                    ui.label("Rechercher");
                    ui.add(egui::TextEdit::singleline(search).desired_width(300.0));
                    ui.end_row();
                    ui.label("Remplacer par");
                    ui.add(egui::TextEdit::singleline(replacement).hint_text("vide = supprimer").desired_width(300.0));
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    ui.checkbox(regex, "Expression régulière ($1, $2… pour les groupes)");
                    ui.checkbox(case_sensitive, "Respecter la casse");
                });
            }
            OpUi::Date { cols: sel, input, output, clear } => {
                ordered_checklist(ui, cols, sel);
                egui::Grid::new(("date", i)).num_columns(3).show(ui, |ui| {
                    ui.label("Format d'origine");
                    ui.add(egui::TextEdit::singleline(input).hint_text("vide = détection automatique").desired_width(220.0));
                    egui::ComboBox::from_id_salt(("din", i)).selected_text("Modèles").show_ui(ui, |ui| {
                        ui.selectable_value(input, String::new(), "Détection automatique");
                        for f in DATE_FORMATS {
                            ui.selectable_value(input, f.to_string(), f);
                        }
                    });
                    ui.end_row();
                    ui.label("Nouveau format");
                    ui.add(egui::TextEdit::singleline(output).desired_width(220.0));
                    egui::ComboBox::from_id_salt(("dout", i)).selected_text("Modèles").show_ui(ui, |ui| {
                        for f in DATE_FORMATS {
                            ui.selectable_value(output, f.to_string(), f);
                        }
                    });
                    ui.end_row();
                });
                ui.label(RichText::new("Jetons : AAAA (année), AA, MM (mois), M, JJ (jour), J, HH (heure), mm (minutes), ss (secondes).").small().weak());
                ui.checkbox(clear, "Vider les cellules dont la date n'est pas reconnue (sinon elles restent inchangées)");
            }
            OpUi::Number { cols: sel, decimal } => {
                ordered_checklist(ui, cols, sel);
                ui.horizontal(|ui| {
                    ui.label("Séparateur décimal");
                    ui.radio_value(decimal, ".".to_string(), "Point : 1234.5");
                    ui.radio_value(decimal, ",".to_string(), "Virgule : 1234,5");
                });
                ui.label(RichText::new("Les espaces de milliers sont retirés ; les valeurs non numériques restent inchangées.").small().weak());
            }
            OpUi::Concat { cols: sel, separator, target, skip_empty } => {
                ui.label(RichText::new("Cochez les colonnes dans l'ordre souhaité.").small().weak());
                ordered_checklist(ui, cols, sel);
                egui::Grid::new(("concat", i)).num_columns(2).show(ui, |ui| {
                    ui.label("Séparateur");
                    ui.add(egui::TextEdit::singleline(separator).desired_width(80.0));
                    ui.end_row();
                    ui.label("Colonne résultat");
                    ui.add(egui::TextEdit::singleline(target).hint_text("nouvelle ou existante").desired_width(220.0));
                    ui.end_row();
                });
                ui.checkbox(skip_empty, "Ignorer les cellules vides");
            }
            OpUi::Extract { column, pattern, target, case_sensitive } => {
                egui::Grid::new(("extract", i)).num_columns(2).show(ui, |ui| {
                    ui.label("Colonne source");
                    column_combo(ui, ("ex-col", i), cols, column, 220.0);
                    ui.end_row();
                    ui.label("Expression régulière");
                    ui.add(egui::TextEdit::singleline(pattern).hint_text(r"ex. (\d{5}) pour un code postal").desired_width(300.0));
                    ui.end_row();
                    ui.label("Colonne résultat");
                    ui.add(egui::TextEdit::singleline(target).desired_width(220.0));
                    ui.end_row();
                });
                ui.label(RichText::new("Le premier groupe entre parenthèses est extrait (sinon toute la correspondance).").small().weak());
                ui.checkbox(case_sensitive, "Respecter la casse");
            }
            OpUi::Constant { target, value } => {
                egui::Grid::new(("const", i)).num_columns(2).show(ui, |ui| {
                    ui.label("Nom de la colonne");
                    ui.add(egui::TextEdit::singleline(target).desired_width(220.0));
                    ui.end_row();
                    ui.label("Valeur");
                    ui.add(egui::TextEdit::singleline(value).desired_width(220.0));
                    ui.end_row();
                });
            }
        }
    }
}

#[derive(Default)]
struct TransformUi {
    file: String,
    ops: Vec<OpUi>,
    kind: usize,
}

fn transform(app: &mut GestionCsv, ui: &mut Ui) {
    let Some(f) = need_active(app, ui) else { return };
    intro(ui, "Enchaînez des opérations de nettoyage et de calcul ; elles sont appliquées dans l'ordre, en un seul passage.");
    let st = &mut app.panels.transform;
    if st.file != f.id {
        *st = TransformUi { file: f.id.clone(), ..Default::default() };
    }
    let mut action: Option<(usize, i32)> = None; // (index, -1 monter / +1 descendre / 0 supprimer)
    let mut cols = f.headers.clone();
    let n = st.ops.len();
    for (i, op) in st.ops.iter_mut().enumerate() {
        section(ui, "", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}. {}", i + 1, op.title())).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text("Supprimer").clicked() {
                        action = Some((i, 0));
                    }
                    if ui.add_enabled(i + 1 < n, egui::Button::new("↓").small()).clicked() {
                        action = Some((i, 1));
                    }
                    if ui.add_enabled(i > 0, egui::Button::new("↑").small()).clicked() {
                        action = Some((i, -1));
                    }
                });
            });
            op.ui(ui, i, &cols);
        });
        // Les colonnes créées par une opération sont disponibles pour les suivantes.
        if let Some(t) = op.target()
            && !t.is_empty()
            && !cols.iter().any(|c| c == t)
        {
            cols.push(t.to_string());
        }
        ui.add_space(4.0);
    }
    match action {
        Some((i, 0)) => {
            st.ops.remove(i);
        }
        Some((i, d)) => st.ops.swap(i, (i as i32 + d) as usize),
        None => {}
    }
    if st.ops.is_empty() {
        ui.label(RichText::new("Aucune opération : choisissez-en une ci-dessous.").weak());
    }
    section(ui, "", |ui| {
        ui.horizontal(|ui| {
            ui.label("Ajouter une opération");
            egui::ComboBox::from_id_salt("tf-kind").width(330.0).selected_text(OP_KINDS[st.kind]).show_ui(ui, |ui| {
                for (k, label) in OP_KINDS.iter().enumerate() {
                    ui.selectable_value(&mut st.kind, k, *label);
                }
            });
            if ui.button("+ Ajouter").clicked() {
                st.ops.push(OpUi::new(st.kind, f.headers.first().map(String::as_str).unwrap_or("")));
            }
        });
    });
    ui.add_space(8.0);
    if primary_button(ui, "Appliquer") {
        let ops: Vec<Transform> = app.panels.transform.ops.iter().map(OpUi::to_transform).collect();
        if ops.is_empty() {
            return app.error("Ajoutez au moins une opération.");
        }
        run_step(app, ui, &f.id, Step::Transform { operations: ops }, "Nettoyage terminé", "Application des opérations…");
    }
}

// ---------------------------------------------------------------------------
// Découpage
// ---------------------------------------------------------------------------

fn split(app: &mut GestionCsv, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let Some(f) = need_active(app, ui) else { return };
    intro(ui, "Crée un fichier par valeur (ou combinaison de valeurs) des colonnes choisies.");
    let p = &mut app.panels;
    if p.split_file != f.id {
        p.split_file = f.id.clone();
        p.split_cols.clear();
    }
    let mut estimate = false;
    section(ui, "Colonnes de découpage", |ui| {
        ui.label(RichText::new("L'ordre de sélection détermine le nom des fichiers produits.").small().weak());
        ordered_checklist(ui, &f.headers, &mut p.split_cols);
        ui.checkbox(&mut p.split_remove, "Retirer ces colonnes des fichiers produits");
        estimate = !p.split_cols.is_empty() && ui.button("Estimer le nombre de fichiers").clicked();
    });
    if estimate {
        let (id, cols) = (f.id.clone(), p.split_cols.clone());
        app.spawn(&ctx, "Comptage des valeurs…", move |st| {
            let mut parts = Vec::new();
            for c in &cols {
                parts.push(format!("« {c} » : {} valeurs distinctes", fmt_n(st.values(&id, c, 0)?.distinct as u64)));
            }
            Ok(Outcome::Message(parts.join(" · ")))
        });
    }
    ui.add_space(8.0);
    if primary_button(ui, "Découper") {
        if app.panels.split_cols.is_empty() {
            return app.error("Cochez au moins une colonne de découpage.");
        }
        let req = SplitRequest { id: f.id.clone(), columns: app.panels.split_cols.clone(), remove_columns: app.panels.split_remove };
        app.spawn(&ctx, "Découpage en cours…", move |st| Ok(Outcome::Split(st.split_file(&req)?)));
    }
    let parts: Vec<Arc<Entry>> = app.panels.split_result.iter().filter_map(|id| app.file(id)).collect();
    if parts.is_empty() {
        return;
    }
    ui.add_space(10.0);
    section(ui, &format!("{} fichiers créés", fmt_n(parts.len() as u64)), |ui| {
        ui.horizontal(|ui| {
            if primary_button(ui, "Tout enregistrer (ZIP)…") {
                let name = format!("{}_decoupage.zip", moteur::service::stem(&f.name()));
                app.save_zip(&ctx, parts.iter().map(|p| p.id.clone()).collect(), name);
            }
            if ui.button("Retirer de l'espace de travail").clicked() {
                app.ask_delete_many(parts.iter().map(|p| p.id.clone()).collect());
            }
        });
        egui::ScrollArea::vertical().id_salt("parts").max_height(320.0).show(ui, |ui| {
            egui::Grid::new("parts-grid").striped(true).num_columns(3).show(ui, |ui| {
                for p in &parts {
                    if ui.link(p.name()).clicked() {
                        app.set_active(Some(p.id.clone()));
                        app.tab = Tab::Preview;
                    }
                    ui.label(RichText::new(format!("{} lignes", fmt_n(p.rows))).weak());
                    if ui.small_button("Enregistrer sous…").clicked() {
                        app.save_as(&ctx, &p.id);
                    }
                    ui.end_row();
                }
            });
        });
    });
}

// ---------------------------------------------------------------------------
// Fusion
// ---------------------------------------------------------------------------

struct MergeUi {
    order: Vec<String>,
    mode: MergeMode,
    source: bool,
    source_name: String,
    dedupe: bool,
    name: String,
}

impl Default for MergeUi {
    fn default() -> Self {
        MergeUi { order: vec![], mode: MergeMode::Union, source: false, source_name: "fichier_source".into(), dedupe: false, name: String::new() }
    }
}

fn merge(app: &mut GestionCsv, ui: &mut Ui) {
    intro(ui, "Empile les lignes de plusieurs fichiers dans un seul fichier. Cochez les fichiers dans l'ordre souhaité.");
    let files = app.files.clone();
    let st = &mut app.panels.merge;
    st.order.retain(|id| files.iter().any(|f| &f.id == id));
    section(ui, "Fichiers à fusionner", |ui| {
        if files.is_empty() {
            ui.label(RichText::new("Importez au moins deux fichiers.").weak());
        }
        for f in &files {
            let pos = st.order.iter().position(|o| *o == f.id);
            let mut on = pos.is_some();
            ui.horizontal(|ui| {
                let label = match pos {
                    Some(p) => RichText::new(format!("{}  {}", f.name(), p + 1)).strong(),
                    None => RichText::new(f.name()),
                };
                if ui.checkbox(&mut on, label).changed() {
                    if on {
                        st.order.push(f.id.clone());
                    } else {
                        st.order.retain(|o| *o != f.id);
                    }
                }
                let cols = f.headers.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
                ui.label(RichText::new(format!("— {} colonnes : {cols}{}", f.headers.len(), if f.headers.len() > 6 { "…" } else { "" })).small().weak());
            });
        }
    });
    ui.add_space(6.0);
    section(ui, "Alignement des colonnes", |ui| {
        ui.radio_value(&mut st.mode, MergeMode::Union, "Union — toutes les colonnes ; les cellules absentes restent vides");
        ui.radio_value(&mut st.mode, MergeMode::Intersection, "Intersection — uniquement les colonnes communes à tous les fichiers");
        ui.radio_value(&mut st.mode, MergeMode::Strict, "Stricte — erreur si les colonnes diffèrent");
        ui.radio_value(&mut st.mode, MergeMode::Position, "Par position — ignore les noms, aligne colonne 1 avec colonne 1…");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut st.source, "Ajouter une colonne indiquant le fichier d'origine");
            if st.source {
                ui.add(egui::TextEdit::singleline(&mut st.source_name).desired_width(160.0));
            }
        });
        ui.checkbox(&mut st.dedupe, "Supprimer les lignes en double");
        ui.horizontal(|ui| {
            ui.label("Nom du fichier résultat (facultatif)");
            ui.add(egui::TextEdit::singleline(&mut st.name).hint_text("fusion.csv").desired_width(220.0));
        });
    });
    ui.add_space(8.0);
    if primary_button(ui, "Fusionner") {
        let st = &app.panels.merge;
        if st.order.len() < 2 {
            return app.error("Cochez au moins deux fichiers à fusionner.");
        }
        let req = MergeRequest {
            ids: st.order.clone(),
            mode: st.mode,
            source_column: st.source.then(|| st.source_name.clone()),
            dedupe: st.dedupe,
            name: Some(st.name.clone()),
        };
        app.spawn(ui.ctx(), "Fusion en cours…", move |s| GestionCsv::done(s.merge_files(&req)?, "Fusion terminée"));
    }
}

// ---------------------------------------------------------------------------
// Jointure
// ---------------------------------------------------------------------------

struct JoinUi {
    left: Option<String>,
    right: Option<String>,
    keys: Vec<(String, String)>,
    kind: JoinKind,
    ci: bool,
    trim: bool,
    suffix: String,
    name: String,
}

impl Default for JoinUi {
    fn default() -> Self {
        JoinUi { left: None, right: None, keys: vec![], kind: JoinKind::Inner, ci: false, trim: true, suffix: "_2".into(), name: String::new() }
    }
}

const JOIN_KINDS: [(JoinKind, &str, &str, VennFill); 7] = [
    (JoinKind::Inner, "Interne", "Lignes présentes des deux côtés", VennFill::Inter),
    (JoinKind::Left, "Gauche", "Toutes les lignes de gauche", VennFill::A),
    (JoinKind::Right, "Droite", "Toutes les lignes de droite", VennFill::B),
    (JoinKind::Full, "Complète", "Toutes les lignes des deux fichiers", VennFill::Both),
    (JoinKind::LeftAnti, "Gauche sans corresp.", "Lignes de gauche absentes à droite", VennFill::AOnly),
    (JoinKind::RightAnti, "Droite sans corresp.", "Lignes de droite absentes à gauche", VennFill::BOnly),
    (JoinKind::LeftSemi, "Semi-jointure", "Lignes de gauche ayant une correspondance (colonnes de gauche seules)", VennFill::Inter),
];

fn join(app: &mut GestionCsv, ui: &mut Ui) {
    intro(ui, "Associe les lignes de deux fichiers dont les colonnes clés ont les mêmes valeurs. Pour les gros volumes, placez le plus petit fichier à droite (il est chargé en mémoire).");
    let files = app.files.clone();
    let st = &mut app.panels.join;
    let mut changed = false;
    section(ui, "", |ui| {
        egui::Grid::new("join-files").num_columns(2).show(ui, |ui| {
            ui.label("Fichier de gauche");
            changed |= file_combo(ui, "jn-left", &files, &mut st.left, 360.0);
            ui.end_row();
            ui.label("Fichier de droite");
            changed |= file_combo(ui, "jn-right", &files, &mut st.right, 360.0);
            ui.end_row();
        });
    });
    let lf = st.left.as_ref().and_then(|id| files.iter().find(|f| &f.id == id)).cloned();
    let rf = st.right.as_ref().and_then(|id| files.iter().find(|f| &f.id == id)).cloned();
    if changed || (st.keys.is_empty() && lf.is_some() && rf.is_some()) {
        st.keys.clear();
        if let (Some(l), Some(r)) = (&lf, &rf) {
            let common = l.headers.iter().find(|c| r.headers.contains(c));
            st.keys.push((
                common.or(l.headers.first()).cloned().unwrap_or_default(),
                common.or(r.headers.first()).cloned().unwrap_or_default(),
            ));
        }
    }
    ui.add_space(6.0);
    section(ui, "Colonnes de correspondance", |ui| {
        let (Some(l), Some(r)) = (&lf, &rf) else {
            ui.label(RichText::new("Choisissez les deux fichiers.").weak());
            return;
        };
        let mut remove = None;
        for (i, (a, b)) in st.keys.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                column_combo(ui, ("jk-l", i), &l.headers, a, 220.0);
                ui.label("=");
                column_combo(ui, ("jk-r", i), &r.headers, b, 220.0);
                if ui.small_button("×").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            st.keys.remove(i);
        }
        if ui.button("+ Ajouter une colonne clé").clicked() {
            st.keys.push((l.headers[0].clone(), r.headers[0].clone()));
        }
    });
    ui.add_space(6.0);
    section(ui, "Type de jointure", |ui| {
        ui.horizontal_wrapped(|ui| {
            for (kind, title, desc, fill) in JOIN_KINDS {
                let sel = st.kind == kind;
                let stroke = if sel { egui::Stroke::new(2.0, ACCENT) } else { ui.visuals().widgets.noninteractive.bg_stroke };
                let bg = if sel { ACCENT.gamma_multiply(0.15) } else { Color32::TRANSPARENT };
                let r = egui::Frame::new().stroke(stroke).fill(bg).corner_radius(6.0).inner_margin(6.0).show(ui, |ui| {
                    ui.set_width(120.0);
                    ui.vertical_centered(|ui| {
                        venn(ui, fill);
                        ui.label(RichText::new(title).strong().size(12.5));
                    });
                });
                if r.response.interact(egui::Sense::click()).on_hover_text(desc).clicked() {
                    st.kind = kind;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut st.ci, "Ignorer la casse (A = a)");
            ui.checkbox(&mut st.trim, "Ignorer les espaces autour des clés");
        });
        egui::Grid::new("join-opts").num_columns(2).show(ui, |ui| {
            ui.label("Suffixe des colonnes de droite en doublon");
            ui.add(egui::TextEdit::singleline(&mut st.suffix).desired_width(80.0));
            ui.end_row();
            ui.label("Nom du fichier résultat (facultatif)");
            ui.add(egui::TextEdit::singleline(&mut st.name).hint_text("jointure.csv").desired_width(220.0));
            ui.end_row();
        });
    });
    ui.add_space(8.0);
    if primary_button(ui, "Joindre") {
        let st = &app.panels.join;
        let (Some(l), Some(r)) = (st.left.clone(), st.right.clone()) else {
            return app.error("Choisissez les fichiers de gauche et de droite.");
        };
        if st.keys.is_empty() {
            return app.error("Ajoutez au moins une colonne de correspondance.");
        }
        let req = JoinRequest {
            left_id: l,
            right_id: r,
            left_columns: st.keys.iter().map(|k| k.0.clone()).collect(),
            right_columns: st.keys.iter().map(|k| k.1.clone()).collect(),
            kind: st.kind,
            case_insensitive: st.ci,
            trim: st.trim,
            right_suffix: Some(st.suffix.clone()),
            name: Some(st.name.clone()),
        };
        app.spawn(ui.ctx(), "Jointure en cours…", move |s| GestionCsv::done(s.join_files(&req)?, "Jointure terminée"));
    }
}

// ---------------------------------------------------------------------------
// Comparaison
// ---------------------------------------------------------------------------

#[derive(Default)]
struct CompareUi {
    old: Option<String>,
    new: Option<String>,
    keys: Vec<String>,
    cols: Vec<String>,
    unchanged: bool,
    ci: bool,
    trim: bool,
}

fn result_actions(app: &mut GestionCsv, ui: &mut Ui, id: &str) {
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        if ui.button("Ouvrir dans l'aperçu").clicked() {
            app.set_active(Some(id.to_string()));
            app.tab = Tab::Preview;
        }
        if ui.button("Enregistrer sous…").clicked() {
            app.save_as(&ctx, id);
        }
    });
}

fn compare(app: &mut GestionCsv, ui: &mut Ui) {
    intro(ui, "Compare deux versions d'un même fichier : lignes ajoutées, supprimées et modifiées (avec les anciennes valeurs). Les lignes sont reconnues grâce à une ou plusieurs colonnes identifiantes.");
    let files = app.files.clone();
    let st = &mut app.panels.compare;
    if st.old.is_none() && st.new.is_none() {
        st.trim = true;
    }
    let mut changed = false;
    section(ui, "", |ui| {
        egui::Grid::new("cmp-files").num_columns(2).show(ui, |ui| {
            ui.label("Ancienne version");
            changed |= file_combo(ui, "cp-old", &files, &mut st.old, 360.0);
            ui.end_row();
            ui.label("Nouvelle version");
            changed |= file_combo(ui, "cp-new", &files, &mut st.new, 360.0);
            ui.end_row();
        });
    });
    let find = |id: &Option<String>| id.as_ref().and_then(|id| files.iter().find(|f| &f.id == id)).cloned();
    let (o, n) = (find(&st.old), find(&st.new));
    let common: Vec<String> = match (&o, &n) {
        (Some(o), Some(n)) => n.headers.iter().filter(|c| o.headers.contains(c)).cloned().collect(),
        _ => vec![],
    };
    if changed {
        st.keys.clear();
        st.cols.clear();
    }
    st.keys.retain(|k| common.contains(k));
    st.cols.retain(|k| common.contains(k));
    if st.keys.is_empty()
        && let Some(c) = common.first()
        && changed
    {
        st.keys.push(c.clone());
    }
    ui.add_space(6.0);
    section(ui, "Colonnes identifiant une ligne", |ui| {
        if common.is_empty() {
            ui.label(RichText::new(if o.is_some() && n.is_some() { "Ces deux fichiers n'ont aucune colonne en commun." } else { "Choisissez les deux versions." }).weak());
        } else {
            ordered_checklist(ui, &common, &mut st.keys);
            ui.add_space(6.0);
            ui.label(RichText::new("Colonnes à comparer").strong());
            ui.label(RichText::new("Aucune colonne cochée : toutes les colonnes communes (hors identifiant) sont comparées.").small().weak());
            ordered_checklist(ui, &common, &mut st.cols);
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.checkbox(&mut st.unchanged, "Inclure les lignes identiques");
        ui.checkbox(&mut st.ci, "Ignorer la casse");
        ui.checkbox(&mut st.trim, "Ignorer les espaces autour des valeurs");
    });
    ui.add_space(8.0);
    if primary_button(ui, "Comparer") {
        let st = &app.panels.compare;
        let (Some(old_id), Some(new_id)) = (st.old.clone(), st.new.clone()) else {
            return app.error("Choisissez l'ancienne et la nouvelle version.");
        };
        if st.keys.is_empty() {
            return app.error("Cochez au moins une colonne identifiante.");
        }
        let req = CompareRequest {
            old_id,
            new_id,
            keys: st.keys.clone(),
            columns: st.cols.iter().filter(|c| !st.keys.contains(c)).cloned().collect(),
            include_unchanged: st.unchanged,
            case_insensitive: st.ci,
            trim: st.trim,
            name: None,
        };
        app.spawn(ui.ctx(), "Comparaison en cours…", move |s| {
            let (e, summary) = s.compare_files(&req)?;
            let _ = e.index();
            Ok(Outcome::Compare(e, summary))
        });
    }
    let Some((id, s, (headers, rows))) = &app.panels.compare_result else { return };
    let (id, headers, rows) = (id.clone(), headers.clone(), rows.clone());
    ui.add_space(10.0);
    ui.horizontal_wrapped(|ui| {
        stat(ui, &fmt_n(s.added), "ajoutées", Some(GREEN));
        stat(ui, &fmt_n(s.removed), "supprimées", Some(RED));
        stat(ui, &fmt_n(s.modified), "modifiées", Some(ORANGE));
        stat(ui, &fmt_n(s.unchanged), "identiques", None);
    });
    section(ui, "", |ui| {
        if !s.changes_by_column.is_empty() {
            ui.label(format!(
                "Modifications par colonne : {}",
                s.changes_by_column.iter().map(|(c, n)| format!("{c} ({})", fmt_n(*n))).collect::<Vec<_>>().join(", ")
            ));
        }
        if !s.columns_added.is_empty() {
            ui.label(format!("Colonnes apparues : {}", s.columns_added.join(", ")));
        }
        if !s.columns_removed.is_empty() {
            ui.label(format!("Colonnes disparues : {}", s.columns_removed.join(", ")));
        }
        if s.duplicate_keys > 0 {
            ui.label(RichText::new(format!(
                "Attention : {} identifiants en double ; ces lignes sont appariées dans l'ordre. Ajoutez des colonnes identifiantes pour lever l'ambiguïté.",
                fmt_n(s.duplicate_keys)
            )).color(RED));
        }
        ui.label(RichText::new("Le résultat contient une colonne « statut », la liste des colonnes modifiées et, pour chaque colonne comparée, l'ancienne valeur (« (avant) »). Filtrez-le sur « statut » pour isoler une catégorie.").small().weak());
    });
    ui.add_space(6.0);
    if let Some(e) = app.file(&id) {
        ui.label(RichText::new(format!("{} — {} lignes", e.name(), fmt_n(e.rows))).strong());
        result_actions(app, ui, &id);
        small_table(ui, "cmp-table", &headers, &rows, 300.0);
    }
}

// ---------------------------------------------------------------------------
// SQL
// ---------------------------------------------------------------------------

fn quote_col(c: &str) -> String {
    if c.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_') && c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        c.to_string()
    } else {
        quote_ident(c)
    }
}

fn sql(app: &mut GestionCsv, ui: &mut Ui) {
    intro(ui, "Interrogez vos fichiers en SQL (dialecte SQLite, lecture seule). Chaque fichier est une table dont le nom figure ci-dessous ; cliquez sur un nom pour l'ajouter à la requête.");
    let files = app.files.clone();
    let p = &mut app.panels;
    section(ui, "Tables disponibles", |ui| {
        if files.is_empty() {
            ui.label(RichText::new("Importez des fichiers pour les interroger.").weak());
        }
        egui::ScrollArea::vertical().id_salt("sql-tables").max_height(200.0).show(ui, |ui| {
            for f in &files {
                ui.horizontal(|ui| {
                    if ui.link(RichText::new(&f.sql_name).monospace().strong()).on_hover_text("Ajouter à la requête").clicked() {
                        p.sql_query.push_str(&f.sql_name);
                        p.sql_query.push(' ');
                    }
                    ui.label(RichText::new(format!("— {}, {} lignes", f.name(), fmt_n(f.rows))).small().weak());
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(16.0);
                    for c in &f.headers {
                        if ui.small_button(RichText::new(c).monospace()).clicked() {
                            p.sql_query.push_str(&quote_col(c));
                            p.sql_query.push(' ');
                        }
                    }
                });
            }
        });
    });
    ui.add_space(6.0);
    let mut run = false;
    section(ui, "Requête", |ui| {
        let r = ui.add(
            egui::TextEdit::multiline(&mut p.sql_query)
                .code_editor()
                .desired_rows(7)
                .desired_width(f32::INFINITY)
                .hint_text("SELECT ville, COUNT(*) AS nombre FROM clients GROUP BY ville ORDER BY nombre DESC"),
        );
        if r.has_focus() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) {
            run = true;
        }
        if let Some(t) = files.first() {
            let c0 = quote_col(t.headers.first().map(String::as_str).unwrap_or("colonne"));
            let c1 = quote_col(t.headers.get(1).map(String::as_str).unwrap_or("montant"));
            let mut examples = vec![
                ("Aperçu", format!("SELECT *\nFROM {}\nLIMIT 100", t.sql_name)),
                ("Compter par valeur", format!("SELECT {c0}, COUNT(*) AS nombre\nFROM {}\nGROUP BY {c0}\nORDER BY nombre DESC", t.sql_name)),
                ("Somme numérique", format!("SELECT {c0}, SUM(num({c1})) AS total\nFROM {}\nGROUP BY {c0}", t.sql_name)),
            ];
            if let Some(u) = files.get(1) {
                let on = match t.headers.iter().find(|c| u.headers.contains(c)) {
                    Some(c) => format!("a.{} = b.{}", quote_col(c), quote_col(c)),
                    None => format!("a.{c0} = b.{}", quote_col(u.headers.first().map(String::as_str).unwrap_or("id"))),
                };
                examples.push(("Jointure", format!("SELECT a.*, b.*\nFROM {} a\nLEFT JOIN {} b ON {on}", t.sql_name, u.sql_name)));
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Exemples :").weak());
                for (label, q) in examples {
                    if ui.small_button(label).clicked() {
                        p.sql_query = q;
                    }
                }
            });
        }
        egui::CollapsingHeader::new("Aide et fonctions disponibles").id_salt("sql-help").show(ui, |ui| {
            for line in [
                "Toutes les valeurs sont du texte. Pour calculer, utilisez num(colonne), qui comprend « 1 234,5 » : SUM(num(montant)), WHERE num(prix) > 100.",
                "date_iso(colonne) convertit « 31/12/2024 » en « 2024-12-31 » pour trier ou comparer des dates.",
                "sans_accents(colonne) renvoie la valeur en minuscules sans accents.",
                "colonne REGEXP '^(Paris|Lyon)$' teste une expression régulière.",
                "Noms avec espaces ou accents entre guillemets doubles : b.\"id client\". Seules les requêtes de lecture sont acceptées.",
            ] {
                ui.label(format!("• {line}"));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Nom du fichier résultat (facultatif)");
            ui.add(egui::TextEdit::singleline(&mut p.sql_name).hint_text("resultat_sql.csv").desired_width(220.0));
        });
    });
    ui.add_space(8.0);
    if primary_button(ui, "Exécuter (Ctrl + Entrée)") {
        run = true;
    }
    if run {
        if app.panels.sql_query.trim().is_empty() {
            return app.error("Saisissez une requête.");
        }
        let (q, name) = (app.panels.sql_query.clone(), app.panels.sql_name.clone());
        app.spawn(ui.ctx(), "Exécution de la requête…", move |s| {
            let e = s.run_sql(&q, Some(name))?;
            let _ = e.index();
            Ok(Outcome::Sql(e))
        });
    }
    if let Some((id, (headers, rows))) = app.panels.sql_result.clone()
        && let Some(e) = app.file(&id)
    {
        ui.add_space(10.0);
        ui.label(RichText::new(format!("{} — {} lignes", e.name(), fmt_n(e.rows))).strong());
        result_actions(app, ui, &id);
        small_table(ui, "sql-table", &headers, &rows, 300.0);
    }
}

// ---------------------------------------------------------------------------
// Recettes
// ---------------------------------------------------------------------------

fn recipes(app: &mut GestionCsv, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    intro(ui, "Une recette enregistre une suite d'opérations (colonnes, tri, doublons, filtre, nettoyage) pour la rejouer sur de nouveaux fichiers. Appliquez vos opérations à un fichier, puis enregistrez le résultat comme recette. Les recettes sont conservées entre deux lancements.");
    let files = app.files.clone();
    let with_steps: Vec<Arc<Entry>> = files.iter().filter(|f| !f.steps.is_empty()).cloned().collect();
    let p = &mut app.panels;
    // Source par défaut : choix explicite, sinon fichier de travail, sinon le plus récent.
    let valid = |id: &Option<String>| id.as_ref().is_some_and(|i| with_steps.iter().any(|f| &f.id == i));
    if !valid(&p.recipe_from) {
        p.recipe_from = if valid(&app.active) { app.active.clone() } else { with_steps.last().map(|f| f.id.clone()) };
    }
    let mut save = false;
    let mut import = false;
    section(ui, "Enregistrer une recette", |ui| {
        if with_steps.is_empty() {
            ui.label(RichText::new("Aucun fichier transformé : appliquez d'abord des colonnes, un tri, un filtre… à un fichier.").weak());
        } else {
            ui.horizontal(|ui| {
                ui.label("À partir du fichier");
                file_combo(ui, "rc-from", &with_steps, &mut p.recipe_from, 420.0);
            });
            if let Some(f) = p.recipe_from.as_ref().and_then(|id| with_steps.iter().find(|f| &f.id == id)) {
                for (i, s) in f.steps.iter().enumerate() {
                    ui.label(format!("{}. {}", i + 1, describe_step(s)));
                }
            }
            egui::Grid::new("rc-form").num_columns(2).show(ui, |ui| {
                ui.label("Nom de la recette");
                ui.add(egui::TextEdit::singleline(&mut p.recipe_name).hint_text("ex. Nettoyage export CRM").desired_width(300.0));
                ui.end_row();
                ui.label("Description (facultative)");
                ui.add(egui::TextEdit::singleline(&mut p.recipe_desc).desired_width(300.0));
                ui.end_row();
            });
        }
        ui.horizontal(|ui| {
            save = !with_steps.is_empty() && primary_button(ui, "Enregistrer");
            import = ui.button("Importer une recette (.json)…").clicked();
        });
    });
    if save {
        let name = app.panels.recipe_name.trim().to_string();
        if name.is_empty() {
            return app.error("Donnez un nom à la recette.");
        }
        let req = RecipeRequest { name, description: app.panels.recipe_desc.clone(), steps: None, from_file: app.panels.recipe_from.clone() };
        match app.state.create_recipe(&req) {
            Ok(r) => {
                app.info(format!("Recette « {} » enregistrée ({} étape{}).", r.name, r.steps.len(), if r.steps.len() > 1 { "s" } else { "" }));
                app.panels.recipe_name.clear();
                app.panels.recipe_desc.clear();
            }
            Err(e) => app.error(e),
        }
    }
    if import {
        import_recipes(app);
    }
    ui.add_space(8.0);
    let recipes = app.state.recipes();
    if recipes.is_empty() {
        ui.label(RichText::new("Aucune recette enregistrée.").weak());
    }
    for r in recipes {
        recipe_card(app, ui, &ctx, &r, &files);
        ui.add_space(6.0);
    }
}

fn recipe_card(app: &mut GestionCsv, ui: &mut Ui, ctx: &egui::Context, r: &Recipe, files: &[Arc<Entry>]) {
    let mut apply = false;
    section(ui, "", |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&r.name).strong().size(15.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new("Supprimer").color(RED)).frame(false)).clicked() {
                    app.ask_delete_recipe(r.id.clone(), &r.name);
                }
                if ui.button("Exporter…").clicked() {
                    export_recipe(app, r);
                }
            });
        });
        if !r.description.is_empty() {
            ui.label(RichText::new(&r.description).weak());
        }
        for (i, s) in r.steps.iter().enumerate() {
            ui.label(format!("{}. {}", i + 1, describe_step(s)));
        }
        egui::CollapsingHeader::new("Appliquer à des fichiers…").id_salt(("apply", r.id.as_str())).show(ui, |ui| {
            let targets = app.panels.recipe_targets.entry(r.id.clone()).or_default();
            targets.retain(|t| files.iter().any(|f| &f.id == t));
            if files.is_empty() {
                ui.label(RichText::new("Importez d'abord des fichiers.").weak());
            }
            egui::ScrollArea::vertical().id_salt(("targets", r.id.as_str())).max_height(180.0).show(ui, |ui| {
                for f in files {
                    let mut on = targets.contains(&f.id);
                    if ui.checkbox(&mut on, f.name()).changed() {
                        if on {
                            targets.push(f.id.clone());
                        } else {
                            targets.retain(|t| *t != f.id);
                        }
                    }
                }
            });
            apply = primary_button(ui, "Appliquer");
        });
    });
    if apply {
        let ids = app.panels.recipe_targets.get(&r.id).cloned().unwrap_or_default();
        if ids.is_empty() {
            return app.error("Cochez au moins un fichier.");
        }
        let id = r.id.clone();
        let name = r.name.clone();
        app.spawn(ctx, &format!("Application de « {name} »…"), move |s| {
            let out = s.apply_recipe(&id, &ids)?;
            out.iter().for_each(|e| drop(e.index()));
            let message = format!(
                "Recette appliquée : {}.",
                out.iter().map(|e| format!("« {} » ({} lignes)", e.name(), fmt_n(e.rows))).collect::<Vec<_>>().join(", ")
            );
            Ok(Outcome::Files { files: out, message, open: true })
        });
    }
}

fn export_recipe(app: &mut GestionCsv, r: &Recipe) {
    let fname: String = r.name.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' }).collect();
    let Some(dest) = rfd::FileDialog::new().set_file_name(format!("{fname}.recette.json")).add_filter("Recette JSON", &["json"]).save_file() else {
        return;
    };
    let json = serde_json::json!({ "name": r.name, "description": r.description, "steps": r.steps });
    match std::fs::write(&dest, serde_json::to_string_pretty(&json).unwrap_or_default()) {
        Ok(()) => app.info(format!("Recette exportée : {}", dest.display())),
        Err(e) => app.error(format!("Export impossible : {e}")),
    }
}

fn import_recipes(app: &mut GestionCsv) {
    let Some(path) = rfd::FileDialog::new().add_filter("Recette JSON", &["json"]).pick_file() else { return };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => return app.error(format!("Lecture impossible : {e}")),
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return app.error("Ce fichier n'est pas un JSON valide."),
    };
    let items = match value {
        serde_json::Value::Array(a) => a,
        v => vec![v],
    };
    let default_name = path.file_stem().map(|s| s.to_string_lossy().replace(".recette", "")).unwrap_or_default();
    let mut n = 0;
    for item in items {
        let steps: Vec<Step> = match serde_json::from_value(item.get("steps").cloned().unwrap_or_default()) {
            Ok(s) => s,
            Err(e) => return app.error(format!("Recette invalide : {e}")),
        };
        let req = RecipeRequest {
            name: item.get("name").and_then(|v| v.as_str()).unwrap_or(&default_name).to_string(),
            description: item.get("description").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            steps: Some(steps),
            from_file: None,
        };
        if let Err(e) = app.state.create_recipe(&req) {
            return app.error(e);
        }
        n += 1;
    }
    app.info(format!("{n} recette{} importée{}.", if n > 1 { "s" } else { "" }, if n > 1 { "s" } else { "" }));
}
