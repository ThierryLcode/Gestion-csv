//! Fenêtre principale : espace de travail, onglets, tâches en arrière-plan,
//! aperçu et export.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Key, RichText, Ui};
use moteur::csvio::{self, ImportOptions};
use moteur::multi::CompareSummary;
use moteur::profile::Profile;
use moteur::service::{csv_filename, ExportOptions, Values};
use moteur::{AppState, Entry, Res};

use crate::panels::Panels;
use crate::widgets::*;

// ---------------------------------------------------------------------------
// Tâches en arrière-plan
// ---------------------------------------------------------------------------

/// Résultat d'une tâche exécutée hors du fil de l'interface.
pub enum Outcome {
    /// Nouveaux fichiers dans l'espace de travail. `open` : afficher le dernier dans l'aperçu.
    Files { files: Vec<Arc<Entry>>, message: String, open: bool },
    Split(Vec<Arc<Entry>>),
    Compare(Arc<Entry>, CompareSummary),
    Sql(Arc<Entry>),
    Profile(String, Profile),
    Values(String, String, Values),
    Message(String),
}

struct Job {
    label: String,
    started: Instant,
    rx: Receiver<Res<Outcome>>,
}

// ---------------------------------------------------------------------------
// Onglets
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Preview,
    Profile,
    Columns,
    Sort,
    Dedupe,
    Filter,
    Transform,
    Merge,
    Join,
    Compare,
    Split,
    Sql,
    Recipes,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Preview => "Aperçu & export",
            Tab::Profile => "Profil",
            Tab::Columns => "Colonnes",
            Tab::Sort => "Trier",
            Tab::Dedupe => "Doublons",
            Tab::Filter => "Filtrer",
            Tab::Transform => "Nettoyer & calculer",
            Tab::Merge => "Fusionner",
            Tab::Join => "Joindre",
            Tab::Compare => "Comparer",
            Tab::Split => "Découper",
            Tab::Sql => "SQL",
            Tab::Recipes => "Recettes",
        }
    }

    /// Onglets qui travaillent sur le « fichier de travail ».
    pub fn single(self) -> bool {
        matches!(
            self,
            Tab::Preview | Tab::Profile | Tab::Columns | Tab::Sort | Tab::Dedupe | Tab::Filter | Tab::Transform | Tab::Split
        )
    }
}

const TAB_GROUPS: [(&str, &[Tab]); 4] = [
    ("FICHIER", &[Tab::Preview, Tab::Profile]),
    ("TRANSFORMER", &[Tab::Columns, Tab::Sort, Tab::Dedupe, Tab::Filter, Tab::Transform]),
    ("PLUSIEURS FICHIERS", &[Tab::Merge, Tab::Join, Tab::Compare, Tab::Split]),
    ("AVANCÉ", &[Tab::Sql, Tab::Recipes]),
];

// ---------------------------------------------------------------------------
// Options d'import / export
// ---------------------------------------------------------------------------

const DELIMS: [(&str, &str); 8] = [
    ("auto", "Détection auto"),
    (",", "Virgule ( , )"),
    (";", "Point-virgule ( ; )"),
    ("tab", "Tabulation"),
    ("|", "Barre verticale ( | )"),
    (":", "Deux-points ( : )"),
    ("space", "Espace"),
    ("custom", "Autre…"),
];
const ENCODINGS_IN: [(&str, &str); 6] = [
    ("auto", "Détection auto"),
    ("utf-8", "UTF-8"),
    ("windows-1252", "Windows-1252 (Excel)"),
    ("iso-8859-15", "ISO-8859-15"),
    ("iso-8859-1", "ISO-8859-1"),
    ("utf-16le", "UTF-16 LE"),
];
const DELIMS_OUT: [(&str, &str); 5] =
    [("source", "Identique à la source"), (",", "Virgule ( , )"), (";", "Point-virgule ( ; )"), ("tab", "Tabulation"), ("|", "Barre verticale ( | )")];
const ENCODINGS_OUT: [(&str, &str); 4] =
    [("utf-8", "UTF-8"), ("utf-8-bom", "UTF-8 avec BOM (Excel)"), ("windows-1252", "Windows-1252"), ("iso-8859-15", "ISO-8859-15")];

fn combo_pairs(ui: &mut Ui, id: &str, pairs: &[(&str, &str)], value: &mut String, width: f32) {
    let text = pairs.iter().find(|(k, _)| *k == value.as_str()).map(|(_, l)| *l).unwrap_or("");
    egui::ComboBox::from_id_salt(id).width(width).selected_text(text).show_ui(ui, |ui| {
        for (k, l) in pairs {
            ui.selectable_value(value, k.to_string(), *l);
        }
    });
}

pub struct ImportUi {
    delimiter: String,
    custom: String,
    encoding: String,
    has_headers: bool,
    trim: bool,
}

pub struct ExportUi {
    pub delimiter: String,
    pub encoding: String,
    pub quote_all: bool,
    pub crlf: bool,
    pub headers: bool,
}

impl ExportUi {
    pub fn options(&self) -> ExportOptions {
        ExportOptions {
            delimiter: Some(self.delimiter.clone()),
            encoding: Some(self.encoding.clone()),
            quote: Some(if self.quote_all { "always" } else { "necessary" }.into()),
            line_ending: Some(if self.crlf { "crlf" } else { "lf" }.into()),
            headers: Some(self.headers),
        }
    }
}

// ---------------------------------------------------------------------------
// Aperçu virtuel
// ---------------------------------------------------------------------------

/// Lignes chargées par bloc, à la demande, pendant le défilement.
const BLOCK: u64 = 500;

#[derive(Default)]
struct PreviewCache {
    file: String,
    blocks: HashMap<u64, Vec<Vec<String>>>,
    scroll_to: Option<usize>,
    goto: String,
}

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

pub struct GestionCsv {
    pub state: AppState,
    pub files: Vec<Arc<Entry>>,
    pub active: Option<String>,
    pub tab: Tab,
    job: Option<Job>,
    message: Option<(String, bool, Instant)>,
    import: ImportUi,
    pub export: ExportUi,
    preview: PreviewCache,
    renaming: Option<(String, String)>,
    confirm: Option<(String, Confirm)>,
    pub panels: Panels,
    data_dir: PathBuf,
    /// Fichiers passés en ligne de commande (« Ouvrir avec… »), importés au premier affichage.
    pending: Vec<PathBuf>,
}

enum Confirm {
    Delete(String),
    DeleteMany(Vec<String>),
    Clear,
    DeleteRecipe(String),
}

impl GestionCsv {
    pub fn new(state: AppState, data_dir: PathBuf, pending: Vec<PathBuf>) -> Self {
        let mut app = GestionCsv {
            state,
            files: Vec::new(),
            active: None,
            tab: Tab::Preview,
            job: None,
            message: None,
            import: ImportUi { delimiter: "auto".into(), custom: String::new(), encoding: "auto".into(), has_headers: true, trim: false },
            export: ExportUi { delimiter: "source".into(), encoding: "utf-8".into(), quote_all: false, crlf: false, headers: true },
            preview: PreviewCache::default(),
            renaming: None,
            confirm: None,
            panels: Panels::default(),
            data_dir,
            pending,
        };
        app.refresh();
        app
    }

    pub fn refresh(&mut self) {
        self.files = self.state.list();
        if let Some(a) = &self.active
            && !self.files.iter().any(|f| &f.id == a)
        {
            self.active = None;
        }
    }

    pub fn file(&self, id: &str) -> Option<Arc<Entry>> {
        self.files.iter().find(|f| f.id == id).cloned()
    }

    pub fn active_file(&self) -> Option<Arc<Entry>> {
        self.active.as_deref().and_then(|a| self.file(a))
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), false, Instant::now()));
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), true, Instant::now()));
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Lance un traitement en arrière-plan ; l'interface reste réactive.
    pub fn spawn(&mut self, ctx: &egui::Context, label: &str, f: impl FnOnce(&AppState) -> Res<Outcome> + Send + 'static) {
        if self.job.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let st = self.state.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&st)))
                .unwrap_or_else(|_| Err("Erreur interne inattendue".into()));
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.job = Some(Job { label: label.to_string(), started: Instant::now(), rx });
    }

    fn poll_job(&mut self) {
        let Some(job) = &self.job else { return };
        let Ok(result) = job.rx.try_recv() else { return };
        self.job = None;
        self.refresh();
        match result {
            Err(e) => self.error(e),
            Ok(Outcome::Files { files, message, open }) => {
                self.info(message);
                if open && let Some(last) = files.last() {
                    self.set_active(Some(last.id.clone()));
                    self.tab = Tab::Preview;
                }
            }
            Ok(Outcome::Split(parts)) => {
                self.info(format!("Découpage terminé : {} fichiers créés.", fmt_n(parts.len() as u64)));
                self.panels.split_result = parts.iter().map(|p| p.id.clone()).collect();
            }
            Ok(Outcome::Compare(e, s)) => {
                self.info(format!(
                    "Comparaison terminée : {} ajoutées, {} supprimées, {} modifiées.",
                    fmt_n(s.added),
                    fmt_n(s.removed),
                    fmt_n(s.modified)
                ));
                self.panels.compare_result = Some((e.id.clone(), s, self.page(&e.id, 50)));
            }
            Ok(Outcome::Sql(e)) => {
                self.info(format!("Requête exécutée : « {} » — {} lignes, {} colonnes.", e.name(), fmt_n(e.rows), e.headers.len()));
                self.panels.sql_result = Some((e.id.clone(), self.page(&e.id, 50)));
            }
            Ok(Outcome::Profile(id, p)) => {
                self.panels.profiles.insert(id, p);
            }
            Ok(Outcome::Values(id, col, v)) => {
                self.panels.values.insert((id, col), v);
            }
            Ok(Outcome::Message(m)) => self.info(m),
        }
    }

    /// Premières lignes d'un fichier (aperçus compacts).
    pub fn page(&self, id: &str, n: usize) -> (Vec<String>, Vec<Vec<String>>) {
        self.state.rows(id, 0, n).map(|p| (p.headers, p.rows)).unwrap_or_default()
    }

    pub fn set_active(&mut self, id: Option<String>) {
        self.active = id;
        self.preview.blocks.clear();
        self.preview.scroll_to = Some(0);
    }

    /// Résultat d'une opération : message et ouverture dans l'aperçu.
    pub fn done(entry: Arc<Entry>, what: &str) -> Res<Outcome> {
        let _ = entry.index(); // prépare la pagination pendant qu'on est en arrière-plan
        let message = format!(
            "{what} : « {} » — {} lignes, {} colonnes. Le résultat est ajouté à vos fichiers.",
            entry.name(),
            fmt_n(entry.rows),
            entry.headers.len()
        );
        Ok(Outcome::Files { files: vec![entry], message, open: true })
    }

    // -----------------------------------------------------------------------
    // Import / export
    // -----------------------------------------------------------------------

    fn import_paths(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let delim = if self.import.delimiter == "custom" { self.import.custom.clone() } else { self.import.delimiter.clone() };
        let delimiter = match csvio::parse_delimiter(&delim) {
            Ok(d) => d,
            Err(e) => return self.error(e),
        };
        let opts = ImportOptions {
            delimiter,
            encoding: self.import.encoding.clone(),
            quote: b'"',
            has_headers: self.import.has_headers,
            trim: self.import.trim,
        };
        let label = if paths.len() == 1 { "Import et conversion du fichier…".to_string() } else { format!("Import de {} fichiers…", paths.len()) };
        self.spawn(ctx, &label, move |st| {
            let mut files = Vec::new();
            let mut lines = Vec::new();
            for p in &paths {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "fichier.csv".into());
                let e = st.import_path(p, &name, &opts).map_err(|e| format!("{name} : {e}"))?;
                let _ = e.index();
                lines.push(format!(
                    "« {name} » importé : {} lignes, {} colonnes, séparateur {}, {}",
                    fmt_n(e.rows),
                    e.headers.len(),
                    delim_label(e.delimiter),
                    e.encoding
                ));
                files.push(e);
            }
            Ok(Outcome::Files { files, message: lines.join("\n"), open: true })
        });
    }

    fn pick_and_import(&mut self, ctx: &egui::Context) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Ouvrir des fichiers CSV")
            .add_filter("Fichiers CSV", &["csv", "tsv", "txt"])
            .add_filter("Tous les fichiers", &["*"])
            .pick_files()
        {
            self.import_paths(ctx, paths);
        }
    }

    pub fn save_as(&mut self, ctx: &egui::Context, id: &str) {
        let Some(e) = self.file(id) else { return };
        let Some(dest) = rfd::FileDialog::new()
            .set_title("Enregistrer le fichier CSV")
            .set_file_name(csv_filename(&e.name()))
            .add_filter("Fichier CSV", &["csv", "tsv", "txt"])
            .save_file()
        else {
            return;
        };
        let (id, opts) = (id.to_string(), self.export.options());
        self.spawn(ctx, "Enregistrement…", move |st| {
            st.export_file(&id, &opts, &dest)?;
            Ok(Outcome::Message(format!("Fichier enregistré : {}", dest.display())))
        });
    }

    pub fn save_zip(&mut self, ctx: &egui::Context, ids: Vec<String>, name: String) {
        let Some(dest) = rfd::FileDialog::new().set_title("Enregistrer l'archive ZIP").set_file_name(name).add_filter("Archive ZIP", &["zip"]).save_file()
        else {
            return;
        };
        let opts = self.export.options();
        self.spawn(ctx, "Création de l'archive…", move |st| {
            st.export_zip(&ids, &opts, &dest)?;
            Ok(Outcome::Message(format!("Archive enregistrée : {}", dest.display())))
        });
    }

    pub fn ask_delete_many(&mut self, ids: Vec<String>) {
        self.confirm = Some((format!("Retirer ces {} fichiers de l'espace de travail ?", ids.len()), Confirm::DeleteMany(ids)));
    }

    pub fn ask_delete_recipe(&mut self, id: String, name: &str) {
        self.confirm = Some((format!("Supprimer la recette « {name} » ?"), Confirm::DeleteRecipe(id)));
    }

    // -----------------------------------------------------------------------
    // Interface
    // -----------------------------------------------------------------------

    fn sidebar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(6.0);
        section(ui, "Importer", |ui| {
            let btn = egui::Button::new(RichText::new("📂  Ouvrir des fichiers CSV…").strong()).min_size(egui::vec2(ui.available_width(), 34.0));
            if ui.add(btn).clicked() {
                self.pick_and_import(&ctx);
            }
            ui.label(RichText::new("ou glissez-déposez des fichiers sur la fenêtre — aucune limite de taille").small().weak());
            ui.add_space(4.0);
            egui::Grid::new("import-opts").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Séparateur");
                combo_pairs(ui, "imp-delim", &DELIMS, &mut self.import.delimiter, 170.0);
                ui.end_row();
                if self.import.delimiter == "custom" {
                    ui.label("Caractère");
                    ui.add(egui::TextEdit::singleline(&mut self.import.custom).char_limit(1).desired_width(40.0));
                    ui.end_row();
                }
                ui.label("Encodage");
                combo_pairs(ui, "imp-enc", &ENCODINGS_IN, &mut self.import.encoding, 170.0);
                ui.end_row();
            });
            ui.checkbox(&mut self.import.has_headers, "La première ligne contient les en-têtes");
            ui.checkbox(&mut self.import.trim, "Supprimer les espaces autour des valeurs");
        });
        ui.add_space(8.0);

        let files = self.files.clone();
        section(ui, "", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Fichiers").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !files.is_empty() && ui.add(egui::Button::new(RichText::new("Tout effacer").color(RED)).frame(false)).clicked() {
                        self.confirm = Some(("Retirer tous les fichiers de l'espace de travail ?".into(), Confirm::Clear));
                    }
                });
            });
            if files.is_empty() {
                ui.label(RichText::new("Aucun fichier. Importez un CSV pour commencer : les résultats des opérations apparaîtront aussi ici et pourront être réutilisés.").weak());
            }
            egui::ScrollArea::vertical().id_salt("files").auto_shrink([false, true]).show(ui, |ui| {
                for f in &files {
                    self.file_item(ui, f);
                    ui.add_space(4.0);
                }
            });
        });
    }

    fn file_item(&mut self, ui: &mut Ui, f: &Arc<Entry>) {
        let ctx = ui.ctx().clone();
        let selected = self.active.as_deref() == Some(&f.id);
        let fill = if selected { ACCENT.gamma_multiply(0.18) } else { Color32::TRANSPARENT };
        let stroke = if selected { egui::Stroke::new(1.5, ACCENT) } else { ui.visuals().widgets.noninteractive.bg_stroke };
        let resp = egui::Frame::new().fill(fill).stroke(stroke).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match &mut self.renaming {
                Some((id, text)) if *id == f.id => {
                    let r = ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY));
                    r.request_focus();
                    if ui.input(|i| i.key_pressed(Key::Enter)) && !text.trim().is_empty() {
                        self.state.rename(&f.id, text.trim().to_string());
                        self.renaming = None;
                        self.refresh();
                    } else if ui.input(|i| i.key_pressed(Key::Escape)) {
                        self.renaming = None;
                    }
                }
                _ => {
                    ui.label(RichText::new(f.name()).strong());
                }
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new(&f.origin).small().background_color(ui.visuals().faint_bg_color));
                if !f.steps.is_empty() {
                    let n = f.steps.len();
                    ui.label(RichText::new(format!("{n} étape{}", if n > 1 { "s" } else { "" })).small().color(ACCENT))
                        .on_hover_text("Opérations enregistrables comme recette");
                }
                ui.label(
                    RichText::new(format!("{} lignes · {} col. · {}", fmt_n(f.rows), f.headers.len(), fmt_bytes(f.size))).small().weak(),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let link = |ui: &mut Ui, t: &str, c: Option<Color32>| {
                    let mut rt = RichText::new(t);
                    if let Some(c) = c {
                        rt = rt.color(c);
                    }
                    ui.add(egui::Button::new(rt).frame(false)).clicked()
                };
                if link(ui, "Enregistrer sous…", Some(ACCENT)) {
                    self.save_as(&ctx, &f.id.clone());
                }
                if link(ui, "Renommer", None) {
                    self.renaming = Some((f.id.clone(), f.name()));
                }
                if !f.steps.is_empty() && link(ui, "Recette…", None) {
                    self.panels.recipe_from = Some(f.id.clone());
                    self.tab = Tab::Recipes;
                }
                if link(ui, "Supprimer", Some(RED)) {
                    self.confirm = Some((format!("Retirer « {} » de l'espace de travail ?", f.name()), Confirm::Delete(f.id.clone())));
                }
            });
        });
        if resp.response.interact(egui::Sense::click()).clicked() && !selected {
            self.set_active(Some(f.id.clone()));
        }
    }

    fn tabs(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            for (group, tabs) in TAB_GROUPS {
                ui.vertical(|ui| {
                    ui.label(RichText::new(group).small().weak());
                    ui.horizontal(|ui| {
                        for &t in tabs {
                            let sel = self.tab == t;
                            let text = if sel { RichText::new(t.label()).strong() } else { RichText::new(t.label()) };
                            if ui.selectable_label(sel, text).clicked() {
                                self.tab = t;
                            }
                        }
                    });
                });
                ui.add_space(14.0);
            }
        });
        ui.separator();
    }

    fn active_bar(&mut self, ui: &mut Ui) {
        egui::Frame::group(ui.style()).inner_margin(8.0).stroke(egui::Stroke::new(1.0, ACCENT.gamma_multiply(0.6))).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label("Fichier de travail");
                let mut sel = self.active.clone();
                let files = self.files.clone();
                if file_combo(ui, "active", &files, &mut sel, 360.0) {
                    self.set_active(sel);
                }
                if let Some(f) = self.active_file() {
                    ui.label(
                        RichText::new(format!(
                            "{} lignes · {} colonnes · {} · séparateur source {} · {}",
                            fmt_n(f.rows),
                            f.headers.len(),
                            fmt_bytes(f.size),
                            delim_label(f.delimiter),
                            f.encoding
                        ))
                        .weak(),
                    );
                }
            });
        });
        ui.add_space(6.0);
    }

    fn message_bar(&mut self, ui: &mut Ui) {
        let Some((text, is_err, at)) = &self.message else { return };
        if !*is_err && at.elapsed() > Duration::from_secs(15) {
            self.message = None;
            return;
        }
        let (bg, fg) = if *is_err {
            (RED.gamma_multiply(0.18), if ui.visuals().dark_mode { Color32::from_rgb(255, 170, 170) } else { Color32::from_rgb(160, 30, 30) })
        } else {
            (GREEN.gamma_multiply(0.18), if ui.visuals().dark_mode { Color32::from_rgb(143, 224, 168) } else { Color32::from_rgb(27, 107, 52) })
        };
        let text = text.clone();
        let mut close = false;
        egui::Frame::new().fill(bg).corner_radius(6.0).inner_margin(10.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(text).color(fg)).wrap());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    close = ui.small_button("×").clicked();
                });
            });
        });
        if close {
            self.message = None;
        }
        ui.add_space(6.0);
    }

    fn preview_panel(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        egui::CollapsingHeader::new(RichText::new("Options d'export").strong()).id_salt("export").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Séparateur");
                combo_pairs(ui, "ex-delim", &DELIMS_OUT, &mut self.export.delimiter, 170.0);
                ui.add_space(10.0);
                ui.label("Encodage");
                combo_pairs(ui, "ex-enc", &ENCODINGS_OUT, &mut self.export.encoding, 190.0);
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.export.quote_all, "Guillemets autour de toutes les valeurs");
                ui.checkbox(&mut self.export.crlf, "Fins de ligne Windows (CRLF)");
                ui.checkbox(&mut self.export.headers, "Inclure la ligne d'en-têtes");
            });
            ui.label(RichText::new("Ces options s'appliquent à tous les enregistrements (fichiers et archives ZIP).").small().weak());
        });
        ui.add_space(4.0);
        let Some(f) = self.active_file() else {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Importez ou choisissez un fichier pour l'afficher.").weak().size(16.0));
            });
            return;
        };
        ui.horizontal(|ui| {
            if primary_button(ui, "💾  Enregistrer sous…") {
                self.save_as(&ctx, &f.id);
            }
            ui.add_space(16.0);
            ui.label("Aller à la ligne");
            let r = ui.add(egui::TextEdit::singleline(&mut self.preview.goto).desired_width(90.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
                && let Ok(n) = self.preview.goto.trim().replace(['\u{202f}', ' '], "").parse::<usize>() {
                    self.preview.scroll_to = Some(n.saturating_sub(1).min(f.rows.saturating_sub(1) as usize));
                }
            ui.label(RichText::new(format!("{} lignes au total — défilement libre, chargement à la demande", fmt_n(f.rows))).weak());
        });
        ui.add_space(6.0);
        self.virtual_table(ui, &f);
    }

    /// Tableau virtuel : seules les lignes visibles sont lues sur le disque.
    fn virtual_table(&mut self, ui: &mut Ui, f: &Arc<Entry>) {
        use egui_extras::{Column, TableBuilder};
        if self.preview.file != f.id {
            self.preview.file = f.id.clone();
            self.preview.blocks.clear();
        }
        if self.preview.blocks.len() > 40 {
            self.preview.blocks.clear();
        }
        let total = f.rows as usize;
        let st = self.state.clone();
        let cache = &mut self.preview.blocks;
        let num_width = (fmt_n(f.rows).len() as f32 * 8.0).max(40.0);
        egui::ScrollArea::horizontal().id_salt("pv-h").show(ui, |ui| {
            let mut tb = TableBuilder::new(ui)
                .id_salt(("pv", f.id.as_str()))
                .striped(true)
                .resizable(true)
                .auto_shrink([false, false])
                .column(Column::exact(num_width))
                .columns(Column::initial(140.0).at_least(40.0).clip(true), f.headers.len());
            if let Some(row) = self.preview.scroll_to.take() {
                tb = tb.scroll_to_row(row, Some(egui::Align::TOP));
            }
            tb.header(26.0, |mut row| {
                row.col(|ui| {
                    ui.label(RichText::new("#").weak());
                });
                for h in &f.headers {
                    row.col(|ui| {
                        ui.label(RichText::new(h).strong()).on_hover_text(h);
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, total, |mut row| {
                    let i = row.index() as u64;
                    let block = i / BLOCK;
                    let data = cache
                        .entry(block)
                        .or_insert_with(|| st.rows(&f.id, block * BLOCK, BLOCK as usize).map(|p| p.rows).unwrap_or_default());
                    row.col(|ui| {
                        ui.label(RichText::new(fmt_n(i + 1)).weak().monospace());
                    });
                    match data.get((i % BLOCK) as usize) {
                        Some(cells) => {
                            for v in cells {
                                row.col(|ui| {
                                    ui.add(egui::Label::new(v).truncate()).on_hover_text(v);
                                });
                            }
                        }
                        None => {
                            row.col(|ui| {
                                ui.spinner();
                            });
                        }
                    }
                });
            });
        });
    }

    fn busy_overlay(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.job else { return };
        let secs = job.started.elapsed().as_secs();
        let label = job.label.clone();
        egui::Modal::new(egui::Id::unique("busy")).show(ctx, |ui| {
            ui.set_min_width(280.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(label).strong());
            });
            if secs >= 2 {
                ui.label(RichText::new(format!("{secs} s")).weak());
            }
        });
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some((question, _)) = &self.confirm else { return };
        let question = question.clone();
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(RichText::new(question).strong());
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Confirmer").color(Color32::WHITE)).fill(RED)).clicked() {
                    answer = Some(true);
                }
                if ui.button("Annuler").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                let (_, action) = self.confirm.take().unwrap();
                match action {
                    Confirm::Delete(id) => {
                        self.state.remove(&id);
                    }
                    Confirm::DeleteMany(ids) => {
                        ids.iter().for_each(|id| {
                            self.state.remove(id);
                        });
                        self.panels.split_result.clear();
                    }
                    Confirm::Clear => {
                        self.state.clear();
                        self.panels = Panels::default();
                    }
                    Confirm::DeleteRecipe(id) => {
                        if let Err(e) = self.state.remove_recipe(&id) {
                            self.error(e);
                        }
                    }
                }
                self.refresh();
            }
            Some(false) => self.confirm = None,
            None => {}
        }
    }

    fn handle_drop(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        let dropped: Vec<PathBuf> = dropped.into_iter().filter(|p| !p.as_os_str().is_empty()).collect();
        if !dropped.is_empty() {
            self.import_paths(ctx, dropped);
        }
        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            let screen = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop")));
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(140));
            painter.text(
                screen.center(),
                egui::Align2::CENTER_CENTER,
                "Déposez les fichiers CSV pour les importer",
                egui::FontId::proportional(24.0),
                Color32::WHITE,
            );
        }
    }
}

impl eframe::App for GestionCsv {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_job();
        self.handle_drop(&ctx);
        if !self.pending.is_empty() && !self.busy() {
            let paths = std::mem::take(&mut self.pending);
            self.import_paths(&ctx, paths);
        }

        egui::Panel::top("titre").frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(14, 8))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Gestion CSV").strong());
                ui.label(RichText::new("Fusionner · Découper · Joindre · Filtrer · Nettoyer · Comparer · SQL").weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::widgets::global_theme_preference_buttons(ui);
                    ui.label(RichText::new(format!("Données : {}", self.data_dir.display())).small().weak())
                        .on_hover_text("Recettes et fichiers de travail. Les fichiers de travail sont vidés à chaque démarrage.");
                });
            });
        });

        egui::Panel::left("espace").resizable(true).default_size(330.0).size_range(260.0..=520.0).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("sidebar").show(ui, |ui| self.sidebar(ui));
        });

        egui::CentralPanel::default().show(ui, |ui| {
            self.tabs(ui);
            if self.tab.single() {
                self.active_bar(ui);
            }
            self.message_bar(ui);
            if self.tab == Tab::Preview {
                self.preview_panel(ui);
            } else {
                egui::ScrollArea::vertical().id_salt(("panel", self.tab as u8)).auto_shrink([false, false]).show(ui, |ui| {
                    crate::panels::show(self, ui);
                });
            }
        });

        self.busy_overlay(&ctx);
        self.confirm_dialog(&ctx);
    }
}
