//! Opérations de haut niveau sur l'espace de travail : chaque méthode prend des
//! identifiants de fichiers, exécute le traitement (de façon bloquante) et
//! ajoute le résultat à l'espace de travail.

use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::csvio::{self, ImportOptions, Res, WriteOptions};
use crate::multi::{self, CompareOptions, CompareSummary, Input, JoinKind, MergeMode};
use crate::profile::{self, Profile};
use crate::sql::SqlTable;
use crate::steps::{self, DedupeMode, Step};
use crate::store::{AppState, Entry, NewFile, Recipe, INDEX_STEP};

/// Nombre maximal de fichiers produits par un découpage.
pub const MAX_SPLIT_GROUPS: usize = 5000;

// ---------------------------------------------------------------------------
// Noms de fichiers
// ---------------------------------------------------------------------------

/// Nom sans extension.
pub fn stem(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

/// Ajoute « .csv » si le nom n'a pas d'extension de fichier texte.
pub fn csv_filename(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.ends_with(".csv") || lower.ends_with(".tsv") || lower.ends_with(".txt") {
        name.to_string()
    } else {
        format!("{name}.csv")
    }
}

fn output_name(custom: Option<String>, default: String) -> String {
    match custom {
        Some(n) if !n.trim().is_empty() => csv_filename(n.trim()),
        _ => default,
    }
}

/// Transforme une valeur en fragment de nom de fichier sûr.
pub fn sanitize_part(v: &str) -> String {
    let s: String = v
        .trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || "-_.".contains(c) { c } else { '_' })
        .take(60)
        .collect();
    if s.trim_matches('_').is_empty() { "vide".into() } else { s }
}

fn step_suffix(step: &Step) -> &'static str {
    match step {
        Step::Columns { .. } => "colonnes",
        Step::Sort { .. } => "trie",
        Step::Dedupe { mode, .. } => match mode {
            DedupeMode::Remove => "dedoublonne",
            DedupeMode::Duplicates => "doublons",
            DedupeMode::Removed => "doublons_ecartes",
            DedupeMode::Singles => "uniques",
        },
        Step::Filter { .. } => "filtre",
        Step::Transform { .. } => "nettoye",
    }
}

fn step_origin(step: &Step) -> &'static str {
    match step {
        Step::Columns { .. } => "colonnes",
        Step::Sort { .. } => "tri",
        Step::Dedupe { .. } => "doublons",
        Step::Filter { .. } => "filtre",
        Step::Transform { .. } => "nettoyage",
    }
}

fn new_source(name: String, origin: &str, delimiter: u8) -> NewFile {
    NewFile { name, origin: origin.into(), delimiter, encoding: "UTF-8".into(), steps: vec![] }
}

// ---------------------------------------------------------------------------
// Requêtes
// ---------------------------------------------------------------------------

/// Options d'export telles que saisies par l'utilisateur.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ExportOptions {
    /// Séparateur ; vide ou « source » = celui du fichier d'origine.
    pub delimiter: Option<String>,
    /// `utf-8`, `utf-8-bom`, `windows-1252`, `iso-8859-15`…
    pub encoding: Option<String>,
    /// `necessary` (défaut) ou `always`.
    pub quote: Option<String>,
    /// `lf` (défaut) ou `crlf`.
    pub line_ending: Option<String>,
    pub headers: Option<bool>,
}

impl ExportOptions {
    pub fn to_write_options(&self, source_delimiter: u8) -> Res<WriteOptions> {
        let delimiter = match self.delimiter.as_deref() {
            None | Some("") | Some("source") => source_delimiter,
            Some(d) => csvio::parse_delimiter(d)?.unwrap_or(source_delimiter),
        };
        Ok(WriteOptions {
            delimiter,
            quote_all: self.quote.as_deref() == Some("always"),
            crlf: self.line_ending.as_deref() == Some("crlf"),
            encoding: self.encoding.clone().unwrap_or_else(|| "utf-8".into()),
            include_headers: self.headers.unwrap_or(true),
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct MergeRequest {
    pub ids: Vec<String>,
    #[serde(default)]
    pub mode: MergeMode,
    pub source_column: Option<String>,
    #[serde(default)]
    pub dedupe: bool,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SplitRequest {
    pub id: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub remove_columns: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct JoinRequest {
    pub left_id: String,
    pub right_id: String,
    pub left_columns: Vec<String>,
    pub right_columns: Vec<String>,
    #[serde(default)]
    pub kind: JoinKind,
    #[serde(default)]
    pub case_insensitive: bool,
    #[serde(default)]
    pub trim: bool,
    pub right_suffix: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CompareRequest {
    pub old_id: String,
    pub new_id: String,
    pub keys: Vec<String>,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub include_unchanged: bool,
    #[serde(default)]
    pub case_insensitive: bool,
    #[serde(default)]
    pub trim: bool,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RecipeRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Étapes explicites (import d'une recette)…
    pub steps: Option<Vec<Step>>,
    /// …ou fichier dont on reprend l'historique des étapes.
    pub from_file: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Page {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub offset: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Values {
    pub values: Vec<(String, u64)>,
    pub distinct: usize,
    pub capped: bool,
}

// ---------------------------------------------------------------------------
// Opérations
// ---------------------------------------------------------------------------

impl AppState {
    pub fn entry(&self, id: &str) -> Res<Arc<Entry>> {
        self.get(id).ok_or_else(|| format!("Fichier introuvable : {id}"))
    }

    /// Importe un fichier brut (tout séparateur, tout encodage).
    pub fn import_path(&self, src: &Path, name: &str, opts: &ImportOptions) -> Res<Arc<Entry>> {
        let dest = self.scratch.path("csv");
        let imported = csvio::import(src, dest.clone(), opts).inspect_err(|_| {
            let _ = std::fs::remove_file(&dest);
        })?;
        Ok(self.add(
            imported.written,
            NewFile {
                name: name.to_string(),
                origin: "import".into(),
                delimiter: imported.delimiter,
                encoding: imported.encoding,
                steps: vec![],
            },
        ))
    }

    /// Lignes `offset..offset+limit` (pagination indexée, rapide même en fin de gros fichier).
    pub fn rows(&self, id: &str, offset: u64, limit: usize) -> Res<Page> {
        let e = self.entry(id)?;
        let offset = offset.min(e.rows);
        let idx = e.index()?;
        let rows = match idx.get((offset / INDEX_STEP) as usize) {
            Some(pos) => csvio::read_page(&e.path, pos, offset % INDEX_STEP, limit)?,
            None => Vec::new(),
        };
        Ok(Page { headers: e.headers.clone(), rows, offset, total: e.rows })
    }

    pub fn values(&self, id: &str, column: &str, limit: usize) -> Res<Values> {
        let e = self.entry(id)?;
        let col = steps::col_index(&e.headers, column)?;
        let (mut values, distinct, capped) = profile::distinct_values(&e.path, col, 1_000_000)?;
        values.truncate(limit);
        Ok(Values { values, distinct, capped })
    }

    pub fn profile_of(&self, id: &str) -> Res<Profile> {
        let e = self.entry(id)?;
        profile::profile(&e.path, &e.headers)
    }

    /// Exporte un fichier vers `dest` ; le fichier partiel est supprimé en cas d'erreur.
    pub fn export_file(&self, id: &str, opts: &ExportOptions, dest: &Path) -> Res<()> {
        let e = self.entry(id)?;
        let wopts = opts.to_write_options(e.delimiter)?;
        let r = (|| {
            let f = std::fs::File::create(dest).map_err(|err| format!("Écriture impossible : {err}"))?;
            let mut w = BufWriter::with_capacity(256 * 1024, f);
            csvio::export(&e.path, &wopts, &mut w)?;
            w.flush().map_err(|err| format!("Écriture impossible : {err}"))
        })();
        if r.is_err() {
            let _ = std::fs::remove_file(dest);
        }
        r
    }

    /// Crée une archive ZIP contenant les fichiers donnés.
    pub fn export_zip(&self, ids: &[String], opts: &ExportOptions, dest: &Path) -> Res<()> {
        let entries: Vec<Arc<Entry>> = ids.iter().map(|id| self.entry(id)).collect::<Res<_>>()?;
        if entries.is_empty() {
            return Err("Aucun fichier à archiver".into());
        }
        let r = (|| {
            let f = std::fs::File::create(dest).map_err(|e| format!("Écriture impossible : {e}"))?;
            let mut zip = zip::ZipWriter::new(BufWriter::new(f));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .large_file(true);
            let mut used = std::collections::HashSet::new();
            for e in &entries {
                let base = csv_filename(&e.name()).replace(['/', '\\'], "_");
                let mut fname = base.clone();
                let mut n = 2;
                while !used.insert(fname.clone()) {
                    fname = format!("{n}_{base}");
                    n += 1;
                }
                zip.start_file(fname, options).map_err(|e| e.to_string())?;
                csvio::export(&e.path, &opts.to_write_options(e.delimiter)?, &mut zip)?;
            }
            zip.finish().map_err(|e| e.to_string())?.flush().map_err(|e| e.to_string())
        })();
        if r.is_err() {
            let _ = std::fs::remove_file(dest);
        }
        r
    }

    /// Applique une étape (colonnes, tri, doublons, filtre, nettoyage) à un fichier.
    pub fn apply_step(&self, id: &str, step: &Step, name: Option<String>) -> Res<Arc<Entry>> {
        let e = self.entry(id)?;
        let w = steps::run_step(&e.path, &e.headers, step, &self.scratch)?;
        let mut lineage = e.steps.clone();
        lineage.push(step.clone());
        let name = output_name(name, format!("{}_{}.csv", stem(&e.name()), step_suffix(step)));
        Ok(self.add(w, NewFile { name, origin: step_origin(step).into(), delimiter: e.delimiter, encoding: "UTF-8".into(), steps: lineage }))
    }

    pub fn merge_files(&self, req: &MergeRequest) -> Res<Arc<Entry>> {
        let entries: Vec<Arc<Entry>> = req.ids.iter().map(|id| self.entry(id)).collect::<Res<_>>()?;
        let names: Vec<String> = entries.iter().map(|e| e.name()).collect();
        let default_name = format!("fusion_{}.csv", names.iter().map(|n| stem(n)).collect::<Vec<_>>().join("_"));
        let inputs: Vec<Input> =
            entries.iter().zip(&names).map(|(e, n)| Input { name: n, path: &e.path, headers: &e.headers }).collect();
        let opts = multi::MergeOptions { mode: req.mode, source_column: req.source_column.as_deref(), dedupe: req.dedupe };
        let w = multi::merge(&inputs, &opts, &self.scratch)?;
        let delimiter = entries.first().map(|e| e.delimiter).unwrap_or(b',');
        Ok(self.add(w, new_source(output_name(req.name.clone(), default_name), "fusion", delimiter)))
    }

    pub fn split_file(&self, req: &SplitRequest) -> Res<Vec<Arc<Entry>>> {
        let e = self.entry(&req.id)?;
        let cols = steps::col_indices(&e.headers, &req.columns)?;
        let groups = multi::split(&e.path, &e.headers, &cols, req.remove_columns, MAX_SPLIT_GROUPS, &self.scratch)?;
        let base = stem(&e.name()).to_string();
        Ok(groups
            .into_iter()
            .map(|(key, w)| {
                let suffix: Vec<String> = key.iter().map(|v| sanitize_part(v)).collect();
                self.add(w, new_source(format!("{base}_{}.csv", suffix.join("_")), "découpage", e.delimiter))
            })
            .collect())
    }

    pub fn join_files(&self, req: &JoinRequest) -> Res<Arc<Entry>> {
        let l = self.entry(&req.left_id)?;
        let r = self.entry(&req.right_id)?;
        let opts = multi::JoinOptions {
            kind: req.kind,
            case_insensitive: req.case_insensitive,
            trim: req.trim,
            right_suffix: req.right_suffix.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "_2".into()),
        };
        let (ln, rn) = (l.name(), r.name());
        let li = Input { name: &ln, path: &l.path, headers: &l.headers };
        let ri = Input { name: &rn, path: &r.path, headers: &r.headers };
        let w = multi::join(&li, &ri, &req.left_columns, &req.right_columns, &opts, &self.scratch)?;
        let default_name = format!("jointure_{}_{}.csv", stem(&ln), stem(&rn));
        Ok(self.add(w, new_source(output_name(req.name.clone(), default_name), "jointure", l.delimiter)))
    }

    pub fn compare_files(&self, req: &CompareRequest) -> Res<(Arc<Entry>, CompareSummary)> {
        let o = self.entry(&req.old_id)?;
        let n = self.entry(&req.new_id)?;
        let opts = CompareOptions {
            columns: req.columns.clone(),
            include_unchanged: req.include_unchanged,
            case_insensitive: req.case_insensitive,
            trim: req.trim,
        };
        let (on, nn) = (o.name(), n.name());
        let oi = Input { name: &on, path: &o.path, headers: &o.headers };
        let ni = Input { name: &nn, path: &n.path, headers: &n.headers };
        let (w, summary) = multi::compare(&oi, &ni, &req.keys, &opts, &self.scratch)?;
        let default_name = format!("comparaison_{}_{}.csv", stem(&on), stem(&nn));
        let e = self.add(w, new_source(output_name(req.name.clone(), default_name), "comparaison", n.delimiter));
        Ok((e, summary))
    }

    /// Exécute une requête SQL de lecture sur les fichiers de l'espace de travail.
    pub fn run_sql(&self, query: &str, name: Option<String>) -> Res<Arc<Entry>> {
        let files = self.list();
        let out = self.scratch.path("csv");
        let tables: Vec<SqlTable> =
            files.iter().map(|e| SqlTable { name: &e.sql_name, id: &e.id, path: &e.path, headers: &e.headers }).collect();
        let w = {
            let mut sql = self.sql.lock().map_err(|_| "Moteur SQL indisponible".to_string())?;
            sql.query(query, &tables, out.clone()).inspect_err(|_| {
                let _ = std::fs::remove_file(&out);
            })?
        };
        Ok(self.add(w, new_source(output_name(name, "resultat_sql.csv".into()), "sql", b',')))
    }

    pub fn create_recipe(&self, req: &RecipeRequest) -> Res<Recipe> {
        let steps = match (&req.steps, &req.from_file) {
            (Some(s), _) => s.clone(),
            (None, Some(id)) => self.entry(id)?.steps.clone(),
            (None, None) => return Err("Indiquez les étapes ou le fichier d'origine".into()),
        };
        self.add_recipe(Recipe { id: String::new(), name: req.name.clone(), description: req.description.clone(), steps, created_at: 0 })
    }

    /// Applique une recette à chacun des fichiers donnés.
    pub fn apply_recipe(&self, recipe_id: &str, ids: &[String]) -> Res<Vec<Arc<Entry>>> {
        let recipe = self.recipe(recipe_id).ok_or_else(|| format!("Recette introuvable : {recipe_id}"))?;
        if ids.is_empty() {
            return Err("Choisissez au moins un fichier".into());
        }
        let entries: Vec<Arc<Entry>> = ids.iter().map(|i| self.entry(i)).collect::<Res<_>>()?;
        let mut out = Vec::new();
        for e in entries {
            let w = steps::run_steps(&e.path, &e.headers, &recipe.steps, &self.scratch)
                .map_err(|err| format!("{} : {err}", e.name()))?;
            let mut lineage = e.steps.clone();
            lineage.extend(recipe.steps.iter().cloned());
            let name = format!("{}_{}.csv", stem(&e.name()), sanitize_part(&recipe.name));
            out.push(self.add(w, NewFile { name, origin: "recette".into(), delimiter: e.delimiter, encoding: "UTF-8".into(), steps: lineage }));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csvio::tests::tmpdir;
    use crate::steps::{Combine, Condition, Operator};

    #[test]
    fn enchainement_complet() {
        let d = tmpdir("service");
        let st = AppState::open(&d, 1 << 20).unwrap();
        let raw = d.join("a.csv");
        std::fs::write(&raw, "id;ville\n1;Paris\n2;Lyon\n3;Paris\n").unwrap();
        let a = st.import_path(&raw, "a.csv", &ImportOptions::default()).unwrap();
        assert_eq!((a.rows, a.delimiter), (3, b';'));

        let step = Step::Filter {
            conditions: vec![Condition { column: "ville".into(), operator: Operator::In, values: vec!["paris".into()], case_sensitive: false }],
            combine: Combine::All,
        };
        let f = st.apply_step(&a.id, &step, None).unwrap();
        assert_eq!((f.name().as_str(), f.rows, f.steps.len()), ("a_filtre.csv", 2, 1));

        let r = st.create_recipe(&RecipeRequest { name: "Paris".into(), description: String::new(), steps: None, from_file: Some(f.id.clone()) }).unwrap();
        let applied = st.apply_recipe(&r.id, std::slice::from_ref(&a.id)).unwrap();
        assert_eq!(applied[0].rows, 2);
        // Recette persistée : relue au redémarrage.
        drop(st);
        let st2 = AppState::open(&d, 1 << 20).unwrap();
        assert_eq!(st2.recipes().len(), 1);

        let a = st2.import_path(&raw, "a.csv", &ImportOptions::default()).unwrap();
        let page = st2.rows(&a.id, 1, 10).unwrap();
        assert_eq!(page.rows, vec![vec!["2", "Lyon"], vec!["3", "Paris"]]);
        let parts = st2.split_file(&SplitRequest { id: a.id.clone(), columns: vec!["ville".into()], remove_columns: false }).unwrap();
        assert_eq!(parts.iter().map(|p| p.name()).collect::<Vec<_>>(), vec!["a_Paris.csv", "a_Lyon.csv"]);
        let sql = st2.run_sql("SELECT ville, COUNT(*) n FROM a GROUP BY ville ORDER BY n DESC", None).unwrap();
        assert_eq!(st2.rows(&sql.id, 0, 5).unwrap().rows[0], vec!["Paris", "2"]);

        let dest = d.join("export.csv");
        let opts = ExportOptions { delimiter: Some("tab".into()), ..Default::default() };
        st2.export_file(&a.id, &opts, &dest).unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "id\tville\n1\tParis\n2\tLyon\n3\tParis\n");
        let z = d.join("x.zip");
        st2.export_zip(&parts.iter().map(|p| p.id.clone()).collect::<Vec<_>>(), &ExportOptions::default(), &z).unwrap();
        assert!(std::fs::metadata(&z).unwrap().len() > 100);
    }
}
