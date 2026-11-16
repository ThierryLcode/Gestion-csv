//! Étapes appliquées à un seul fichier : colonnes, tri, doublons, filtre,
//! nettoyage / colonnes calculées. Ce sont les briques des recettes.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::csvio::{rows, Res, Row, Scratch, Sink, Written};
use crate::sort::{sort_file, SortKey};
use crate::text::{format_number, parse_date_auto, parse_number, DateFormat};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Step {
    /// Sélectionner, réordonner et renommer des colonnes.
    Columns { columns: Vec<ColumnSpec> },
    /// Trier sur une ou plusieurs colonnes.
    Sort { keys: Vec<SortKey> },
    /// Supprimer ou isoler des doublons.
    Dedupe {
        /// Colonnes formant la clé ; vide = ligne entière.
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        mode: DedupeMode,
        #[serde(default)]
        keep: Keep,
        #[serde(default)]
        case_insensitive: bool,
        #[serde(default)]
        trim: bool,
    },
    /// Conserver les lignes respectant des conditions.
    Filter {
        conditions: Vec<Condition>,
        #[serde(default)]
        combine: Combine,
    },
    /// Nettoyage et colonnes calculées.
    Transform { operations: Vec<Transform> },
}

pub fn col_index(headers: &[String], name: &str) -> Res<usize> {
    headers.iter().position(|h| h == name).ok_or_else(|| format!("Colonne introuvable : « {name} »"))
}

pub fn col_indices(headers: &[String], names: &[String]) -> Res<Vec<usize>> {
    names.iter().map(|n| col_index(headers, n)).collect()
}

/// Hachage 128 bits d'une clé (collisions négligeables même sur des milliards de lignes).
pub fn hash_key<S: AsRef<str>>(parts: &[S]) -> u128 {
    let mut h1 = DefaultHasher::new();
    let mut h2 = DefaultHasher::new();
    0xA5u8.hash(&mut h2);
    for p in parts {
        p.as_ref().hash(&mut h1);
        p.as_ref().hash(&mut h2);
    }
    ((h1.finish() as u128) << 64) | h2.finish() as u128
}

/// Exécute une étape sur un fichier normalisé et produit un nouveau fichier.
pub fn run_step(src: &Path, headers: &[String], step: &Step, scratch: &Scratch) -> Res<Written> {
    match step {
        Step::Sort { keys } => sort_file(src, headers, keys, scratch),
        Step::Dedupe { columns, mode, keep, case_insensitive, trim } => {
            let cols = if columns.is_empty() { (0..headers.len()).collect() } else { col_indices(headers, columns)? };
            dedupe(src, headers, &cols, *mode, *keep, *case_insensitive, *trim, scratch)
        }
        Step::Columns { columns } => {
            let (h, f) = plan_columns(headers, columns)?;
            run_rowwise(src, h, f, scratch)
        }
        Step::Filter { conditions, combine } => {
            let f = plan_filter(headers, conditions, *combine)?;
            run_rowwise(src, headers.to_vec(), f, scratch)
        }
        Step::Transform { operations } => {
            let (h, f) = plan_transform(headers, operations)?;
            run_rowwise(src, h, f, scratch)
        }
    }
}

/// Applique une suite d'étapes ; les fichiers intermédiaires sont supprimés.
pub fn run_steps(src: &Path, headers: &[String], steps: &[Step], scratch: &Scratch) -> Res<Written> {
    if steps.is_empty() {
        return Err("La recette ne contient aucune étape".into());
    }
    let mut current: Option<Written> = None;
    for (i, step) in steps.iter().enumerate() {
        let (p, h) = match &current {
            Some(w) => (w.path.clone(), w.headers.clone()),
            None => (src.to_path_buf(), headers.to_vec()),
        };
        let next = run_step(&p, &h, step, scratch).map_err(|e| format!("Étape {} : {e}", i + 1));
        if let Some(prev) = current.take() {
            prev.discard();
        }
        current = Some(next?);
    }
    Ok(current.unwrap())
}

type RowFn = Box<dyn FnMut(Row) -> Option<Row> + Send>;

fn run_rowwise(src: &Path, headers: Vec<String>, mut f: RowFn, scratch: &Scratch) -> Res<Written> {
    let mut sink = Sink::create(scratch.path("csv"), headers)?;
    for row in rows(src)? {
        if let Some(out) = f(row?) {
            sink.write(&out)?;
        }
    }
    sink.finish()
}

// ---------------------------------------------------------------------------
// Colonnes
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColumnSpec {
    /// Colonne d'origine.
    pub source: String,
    /// Nom dans le fichier produit (vide = inchangé).
    #[serde(default)]
    pub name: String,
}

fn plan_columns(headers: &[String], specs: &[ColumnSpec]) -> Res<(Vec<String>, RowFn)> {
    if specs.is_empty() {
        return Err("Conservez au moins une colonne".into());
    }
    let mut idx = Vec::new();
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    for s in specs {
        idx.push(col_index(headers, &s.source)?);
        let name = if s.name.trim().is_empty() { s.source.clone() } else { s.name.trim().to_string() };
        if !seen.insert(name.clone()) {
            return Err(format!("Le nom de colonne « {name} » est utilisé deux fois"));
        }
        names.push(name);
    }
    Ok((names, Box::new(move |row: Row| Some(idx.iter().map(|&i| row[i].clone()).collect()))))
}

// ---------------------------------------------------------------------------
// Doublons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DedupeMode {
    /// Supprimer les doublons : une seule ligne par clé.
    #[default]
    Remove,
    /// Isoler les doublons : toutes les lignes dont la clé apparaît plusieurs fois.
    Duplicates,
    /// Les lignes qui seraient supprimées par « Remove ».
    Removed,
    /// Les lignes dont la clé n'apparaît qu'une fois.
    Singles,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Keep {
    #[default]
    First,
    Last,
}

#[allow(clippy::too_many_arguments)]
fn dedupe(
    src: &Path,
    headers: &[String],
    cols: &[usize],
    mode: DedupeMode,
    keep: Keep,
    case_insensitive: bool,
    trim: bool,
    scratch: &Scratch,
) -> Res<Written> {
    let key = |row: &Row| -> u128 {
        let parts: Vec<String> = cols
            .iter()
            .map(|&i| {
                let v = if trim { row[i].trim() } else { row[i].as_str() };
                if case_insensitive { v.to_lowercase() } else { v.to_string() }
            })
            .collect();
        hash_key(&parts)
    };
    let mut sink = Sink::create(scratch.path("csv"), headers.to_vec())?;

    // Un seul passage suffit pour garder / écarter la première occurrence.
    if keep == Keep::First && matches!(mode, DedupeMode::Remove | DedupeMode::Removed) {
        let mut seen: HashSet<u128> = HashSet::new();
        for row in rows(src)? {
            let row = row?;
            let first = seen.insert(key(&row));
            if first == (mode == DedupeMode::Remove) {
                sink.write(&row)?;
            }
        }
        return sink.finish();
    }

    // Sinon : 1er passage pour compter, 2e passage pour décider.
    let mut stats: HashMap<u128, (u64, u64, u64)> = HashMap::new(); // (nombre, première, dernière)
    for (i, row) in rows(src)?.enumerate() {
        let e = stats.entry(key(&row?)).or_insert((0, i as u64, i as u64));
        e.0 += 1;
        e.2 = i as u64;
    }
    for (i, row) in rows(src)?.enumerate() {
        let row = row?;
        let (count, first, last) = stats[&key(&row)];
        let chosen = if keep == Keep::First { first } else { last };
        let i = i as u64;
        let out = match mode {
            DedupeMode::Remove => i == chosen,
            DedupeMode::Removed => i != chosen,
            DedupeMode::Duplicates => count > 1,
            DedupeMode::Singles => count == 1,
        };
        if out {
            sink.write(&row)?;
        }
    }
    sink.finish()
}

// ---------------------------------------------------------------------------
// Filtre
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    In,
    NotIn,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    Empty,
    NotEmpty,
    Gt,
    Ge,
    Lt,
    Le,
    Between,
    Regex,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Condition {
    pub column: String,
    pub operator: Operator,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub case_sensitive: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Combine {
    #[default]
    All,
    Any,
}

struct CompiledCond {
    col: usize,
    op: Operator,
    values: Vec<String>,
    case_sensitive: bool,
    set: HashSet<String>,
    regex: Option<Regex>,
}

/// Compare numériquement, sinon chronologiquement, sinon alphabétiquement.
pub fn compare_values(cell: &str, value: &str) -> Ordering {
    if let (Some(a), Some(b)) = (parse_number(cell), parse_number(value)) {
        return a.partial_cmp(&b).unwrap_or(Ordering::Equal);
    }
    if let (Some(a), Some(b)) = (parse_date_auto(cell), parse_date_auto(value)) {
        return a.cmp(&b);
    }
    cell.trim().cmp(value.trim())
}

impl CompiledCond {
    fn new(headers: &[String], c: &Condition) -> Res<Self> {
        let col = col_index(headers, &c.column)?;
        let fold = |s: &str| if c.case_sensitive { s.to_string() } else { s.to_lowercase() };
        let values: Vec<String> = c.values.iter().map(|v| fold(v)).collect();
        let needs = |n: usize| -> Res<()> {
            if c.values.len() < n || c.values[..n].iter().any(|v| v.is_empty()) {
                Err(format!("La condition sur « {} » nécessite {n} valeur(s)", c.column))
            } else {
                Ok(())
            }
        };
        let mut regex = None;
        match c.operator {
            Operator::Empty | Operator::NotEmpty | Operator::In | Operator::NotIn => {}
            Operator::Between => needs(2)?,
            Operator::Regex => {
                needs(1)?;
                regex = Some(
                    RegexBuilder::new(&c.values[0])
                        .case_insensitive(!c.case_sensitive)
                        .build()
                        .map_err(|e| format!("Expression régulière invalide : {e}"))?,
                );
            }
            _ => needs(1)?,
        }
        Ok(CompiledCond { col, op: c.operator, set: values.iter().cloned().collect(), values, case_sensitive: c.case_sensitive, regex })
    }

    fn test(&self, row: &[String]) -> bool {
        let raw = row[self.col].as_str();
        let v = &self.values;
        let filled = !raw.trim().is_empty();
        match self.op {
            Operator::Empty => !filled,
            Operator::NotEmpty => filled,
            Operator::Gt => filled && compare_values(raw, &v[0]) == Ordering::Greater,
            Operator::Ge => filled && compare_values(raw, &v[0]) != Ordering::Less,
            Operator::Lt => filled && compare_values(raw, &v[0]) == Ordering::Less,
            Operator::Le => filled && compare_values(raw, &v[0]) != Ordering::Greater,
            Operator::Between => {
                filled && compare_values(raw, &v[0]) != Ordering::Less && compare_values(raw, &v[1]) != Ordering::Greater
            }
            Operator::Regex => self.regex.as_ref().is_some_and(|re| re.is_match(raw)),
            _ => {
                let cell = if self.case_sensitive { raw.to_string() } else { raw.to_lowercase() };
                match self.op {
                    Operator::In => self.set.contains(&cell),
                    Operator::NotIn => !self.set.contains(&cell),
                    Operator::Contains => v.iter().any(|x| cell.contains(x.as_str())),
                    Operator::NotContains => !v.iter().any(|x| cell.contains(x.as_str())),
                    Operator::StartsWith => v.iter().any(|x| cell.starts_with(x.as_str())),
                    Operator::EndsWith => v.iter().any(|x| cell.ends_with(x.as_str())),
                    _ => unreachable!(),
                }
            }
        }
    }
}

fn plan_filter(headers: &[String], conditions: &[Condition], combine: Combine) -> Res<RowFn> {
    if conditions.is_empty() {
        return Err("Ajoutez au moins une condition de filtrage".into());
    }
    let compiled: Vec<CompiledCond> = conditions.iter().map(|c| CompiledCond::new(headers, c)).collect::<Res<_>>()?;
    Ok(Box::new(move |row: Row| {
        let ok = match combine {
            Combine::All => compiled.iter().all(|c| c.test(&row)),
            Combine::Any => compiled.iter().any(|c| c.test(&row)),
        };
        ok.then_some(row)
    }))
}

// ---------------------------------------------------------------------------
// Nettoyage et colonnes calculées
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CaseMode {
    Upper,
    Lower,
    /// Première lettre de chaque mot en majuscule.
    Title,
    /// Première lettre de la cellule en majuscule.
    Sentence,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Transform {
    /// Concaténer plusieurs colonnes dans une colonne (nouvelle ou existante).
    Concat {
        columns: Vec<String>,
        #[serde(default)]
        separator: String,
        target: String,
        #[serde(default)]
        skip_empty: bool,
    },
    /// Extraire une partie d'une colonne via une expression régulière (1er groupe capturant, sinon la correspondance entière).
    Extract {
        column: String,
        pattern: String,
        target: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Rechercher / remplacer (texte simple ou expression régulière). `columns` vide = toutes.
    Replace {
        #[serde(default)]
        columns: Vec<String>,
        search: String,
        #[serde(default)]
        replacement: String,
        #[serde(default)]
        regex: bool,
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Changer la casse. `columns` vide = toutes.
    Case {
        #[serde(default)]
        columns: Vec<String>,
        mode: CaseMode,
    },
    /// Supprimer les espaces en début / fin (et éventuellement les espaces multiples). `columns` vide = toutes.
    Trim {
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        collapse: bool,
    },
    /// Convertir des dates d'un format à un autre (entrée vide = détection automatique).
    Date {
        columns: Vec<String>,
        #[serde(default)]
        input: String,
        output: String,
        /// Vider les cellules non reconnues (sinon elles sont conservées telles quelles).
        #[serde(default)]
        clear_invalid: bool,
    },
    /// Normaliser les nombres (« 1 234,50 » → « 1234.5 » ou « 1234,5 »).
    Number {
        columns: Vec<String>,
        #[serde(default = "default_decimal")]
        decimal: String,
    },
    /// Ajouter (ou écraser) une colonne avec une valeur fixe.
    Constant { target: String, #[serde(default)] value: String },
}

fn default_decimal() -> String {
    ".".into()
}

enum Op {
    Concat { cols: Vec<usize>, sep: String, target: usize, skip_empty: bool },
    Extract { col: usize, re: Regex, target: usize },
    Replace { cols: Vec<usize>, re: Regex, rep: String },
    Case { cols: Vec<usize>, mode: CaseMode },
    Trim { cols: Vec<usize>, collapse: bool },
    Date { cols: Vec<usize>, input: Option<DateFormat>, output: DateFormat, clear: bool },
    Number { cols: Vec<usize>, decimal: char },
    Constant { target: usize, value: String },
}

fn target_index(headers: &mut Vec<String>, name: &str) -> Res<usize> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Indiquez le nom de la colonne à créer".into());
    }
    Ok(match headers.iter().position(|h| h == name) {
        Some(i) => i,
        None => {
            headers.push(name.to_string());
            headers.len() - 1
        }
    })
}

fn cols_or_all(headers: &[String], cols: &[String]) -> Res<Vec<usize>> {
    if cols.is_empty() { Ok((0..headers.len()).collect()) } else { col_indices(headers, cols) }
}

fn change_case(s: &str, mode: CaseMode) -> String {
    match mode {
        CaseMode::Upper => s.to_uppercase(),
        CaseMode::Lower => s.to_lowercase(),
        CaseMode::Title => {
            let mut out = String::with_capacity(s.len());
            let mut start = true;
            for c in s.chars() {
                if c.is_alphanumeric() {
                    if start { out.extend(c.to_uppercase()) } else { out.extend(c.to_lowercase()) }
                    start = false;
                } else {
                    out.push(c);
                    start = c != '\'' && c != '’';
                }
            }
            out
        }
        CaseMode::Sentence => {
            let lower = s.to_lowercase();
            let mut chars = lower.chars();
            match chars.next() {
                Some(f) => f.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

fn plan_transform(headers: &[String], ops: &[Transform]) -> Res<(Vec<String>, RowFn)> {
    if ops.is_empty() {
        return Err("Ajoutez au moins une opération".into());
    }
    let mut h = headers.to_vec();
    let mut compiled = Vec::new();
    for t in ops {
        compiled.push(match t {
            Transform::Concat { columns, separator, target, skip_empty } => {
                if columns.is_empty() {
                    return Err("Choisissez les colonnes à concaténer".into());
                }
                let cols = col_indices(&h, columns)?;
                Op::Concat { cols, sep: separator.clone(), target: target_index(&mut h, target)?, skip_empty: *skip_empty }
            }
            Transform::Extract { column, pattern, target, case_sensitive } => {
                let col = col_index(&h, column)?;
                let re = RegexBuilder::new(pattern)
                    .case_insensitive(!case_sensitive)
                    .build()
                    .map_err(|e| format!("Expression régulière invalide : {e}"))?;
                Op::Extract { col, re, target: target_index(&mut h, target)? }
            }
            Transform::Replace { columns, search, replacement, regex, case_sensitive } => {
                if search.is_empty() {
                    return Err("Indiquez le texte à rechercher".into());
                }
                let pattern = if *regex { search.clone() } else { regex::escape(search) };
                let re = RegexBuilder::new(&pattern)
                    .case_insensitive(!case_sensitive)
                    .build()
                    .map_err(|e| format!("Expression régulière invalide : {e}"))?;
                let rep = if *regex { replacement.clone() } else { replacement.replace('$', "$$") };
                Op::Replace { cols: cols_or_all(&h, columns)?, re, rep }
            }
            Transform::Case { columns, mode } => Op::Case { cols: cols_or_all(&h, columns)?, mode: *mode },
            Transform::Trim { columns, collapse } => Op::Trim { cols: cols_or_all(&h, columns)?, collapse: *collapse },
            Transform::Date { columns, input, output, clear_invalid } => {
                let input = match input.trim() {
                    "" | "auto" => None,
                    f => Some(DateFormat::parse(f)?),
                };
                Op::Date { cols: col_indices(&h, columns)?, input, output: DateFormat::parse(output)?, clear: *clear_invalid }
            }
            Transform::Number { columns, decimal } => {
                Op::Number { cols: col_indices(&h, columns)?, decimal: if decimal.trim() == "," { ',' } else { '.' } }
            }
            Transform::Constant { target, value } => Op::Constant { target: target_index(&mut h, target)?, value: value.clone() },
        });
    }
    let width = h.len();
    let f = move |mut row: Row| {
        row.resize(width, String::new());
        for op in &compiled {
            match op {
                Op::Concat { cols, sep, target, skip_empty } => {
                    let parts: Vec<&str> =
                        cols.iter().map(|&i| row[i].as_str()).filter(|v| !*skip_empty || !v.trim().is_empty()).collect();
                    row[*target] = parts.join(sep);
                }
                Op::Extract { col, re, target } => {
                    row[*target] = re
                        .captures(&row[*col])
                        .and_then(|c| c.get(1).or_else(|| c.get(0)))
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default();
                }
                Op::Replace { cols, re, rep } => {
                    for &i in cols {
                        if let std::borrow::Cow::Owned(s) = re.replace_all(&row[i], rep.as_str()) {
                            row[i] = s;
                        }
                    }
                }
                Op::Case { cols, mode } => cols.iter().for_each(|&i| row[i] = change_case(&row[i], *mode)),
                Op::Trim { cols, collapse } => {
                    for &i in cols {
                        row[i] = if *collapse {
                            row[i].split_whitespace().collect::<Vec<_>>().join(" ")
                        } else {
                            row[i].trim().to_string()
                        };
                    }
                }
                Op::Date { cols, input, output, clear } => {
                    for &i in cols {
                        if row[i].trim().is_empty() {
                            continue;
                        }
                        let parsed = match input {
                            Some(f) => f.read(&row[i]),
                            None => parse_date_auto(&row[i]),
                        };
                        match parsed {
                            Some(dt) => row[i] = output.write(&dt),
                            None if *clear => row[i].clear(),
                            None => {}
                        }
                    }
                }
                Op::Number { cols, decimal } => {
                    for &i in cols {
                        if let Some(n) = parse_number(&row[i]) {
                            row[i] = format_number(n, *decimal);
                        }
                    }
                }
                Op::Constant { target, value } => row[*target] = value.clone(),
            }
        }
        Some(row)
    };
    Ok((h, Box::new(f)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csvio::tests::tmpdir;
    use crate::sort::SortMode;

    fn src(dir: &Path, headers: &[&str], data: &[&[&str]]) -> Written {
        let mut s = Sink::create(dir.join("src.csv"), headers.iter().map(|s| s.to_string()).collect()).unwrap();
        for r in data {
            s.write(r).unwrap();
        }
        s.finish().unwrap()
    }

    fn run(w: &Written, step: Step, dir: &Path) -> (Vec<String>, Vec<Row>) {
        let sc = Scratch { dir: dir.to_path_buf(), sort_budget: 1 << 20 };
        let out = run_step(&w.path, &w.headers, &step, &sc).unwrap();
        (out.headers.clone(), rows(&out.path).unwrap().map(Result::unwrap).collect())
    }

    fn cond(column: &str, operator: Operator, values: &[&str]) -> Condition {
        Condition { column: column.into(), operator, values: values.iter().map(|s| s.to_string()).collect(), case_sensitive: false }
    }

    #[test]
    fn colonnes() {
        let d = tmpdir("cols");
        let w = src(&d, &["a", "b", "c"], &[&["1", "2", "3"]]);
        let spec = |s: &str, n: &str| ColumnSpec { source: s.into(), name: n.into() };
        let (h, r) = run(&w, Step::Columns { columns: vec![spec("c", "C"), spec("a", "")] }, &d);
        assert_eq!(h, vec!["C", "a"]);
        assert_eq!(r, vec![vec!["3", "1"]]);
        let sc = Scratch { dir: d.clone(), sort_budget: 1 };
        assert!(run_step(&w.path, &w.headers, &Step::Columns { columns: vec![spec("a", "x"), spec("b", "x")] }, &sc).is_err());
    }

    #[test]
    fn doublons() {
        let d = tmpdir("dedupe");
        let w = src(&d, &["k", "v"], &[&["A", "1"], &["b", "2"], &["a ", "3"], &["c", "4"]]);
        let step = |mode, keep| Step::Dedupe { columns: vec!["k".into()], mode, keep, case_insensitive: true, trim: true };
        let vals = |r: Vec<Row>| r.into_iter().map(|r| r[1].clone()).collect::<Vec<_>>();
        assert_eq!(vals(run(&w, step(DedupeMode::Remove, Keep::First), &d).1), vec!["1", "2", "4"]);
        assert_eq!(vals(run(&w, step(DedupeMode::Remove, Keep::Last), &d).1), vec!["2", "3", "4"]);
        assert_eq!(vals(run(&w, step(DedupeMode::Removed, Keep::First), &d).1), vec!["3"]);
        assert_eq!(vals(run(&w, step(DedupeMode::Duplicates, Keep::First), &d).1), vec!["1", "3"]);
        assert_eq!(vals(run(&w, step(DedupeMode::Singles, Keep::First), &d).1), vec!["2", "4"]);
        let whole = Step::Dedupe { columns: vec![], mode: DedupeMode::Remove, keep: Keep::First, case_insensitive: false, trim: false };
        assert_eq!(run(&w, whole, &d).1.len(), 4);
    }

    #[test]
    fn filtres() {
        let d = tmpdir("filtre");
        let w = src(&d, &["ville", "montant", "date"], &[
            &["Paris", "1 200,50", "05/01/2024"],
            &["Lyon", "80", "31/12/2023"],
            &["paris", "", ""],
            &["Nice", "300", "15/02/2024"],
        ]);
        let n = |conds: Vec<Condition>, combine| run(&w, Step::Filter { conditions: conds, combine }, &d).1.len();
        assert_eq!(n(vec![cond("ville", Operator::In, &["paris", "Nice"])], Combine::All), 3);
        assert_eq!(n(vec![cond("montant", Operator::Gt, &["100"])], Combine::All), 2);
        assert_eq!(n(vec![cond("date", Operator::Ge, &["01/01/2024"])], Combine::All), 2);
        assert_eq!(n(vec![cond("ville", Operator::In, &["paris"]), cond("montant", Operator::NotEmpty, &[])], Combine::All), 1);
        assert_eq!(n(vec![cond("ville", Operator::StartsWith, &["ly"]), cond("montant", Operator::Between, &["250", "350"])], Combine::Any), 2);
        assert_eq!(n(vec![cond("ville", Operator::Regex, &["^(lyon|nice)$"])], Combine::All), 2);
    }

    #[test]
    fn transformations() {
        let d = tmpdir("transfo");
        let w = src(&d, &["nom", "prenom", "date", "montant", "code"], &[
            &["  DUPONT ", "jean-paul", "5/3/2024", "1 234,50", "REF-0042 x"],
        ]);
        let ops = vec![
            Transform::Trim { columns: vec![], collapse: true },
            Transform::Case { columns: vec!["nom".into(), "prenom".into()], mode: CaseMode::Title },
            Transform::Concat { columns: vec!["prenom".into(), "nom".into()], separator: " ".into(), target: "complet".into(), skip_empty: true },
            Transform::Date { columns: vec!["date".into()], input: "".into(), output: "AAAA-MM-JJ".into(), clear_invalid: false },
            Transform::Number { columns: vec!["montant".into()], decimal: ".".into() },
            Transform::Extract { column: "code".into(), pattern: r"ref-(\d+)".into(), target: "num".into(), case_sensitive: false },
            Transform::Replace { columns: vec!["code".into()], search: "ref".into(), replacement: "$R".into(), regex: false, case_sensitive: false },
            Transform::Constant { target: "pays".into(), value: "FR".into() },
        ];
        let (h, r) = run(&w, Step::Transform { operations: ops }, &d);
        assert_eq!(h, vec!["nom", "prenom", "date", "montant", "code", "complet", "num", "pays"]);
        assert_eq!(r[0], vec!["Dupont", "Jean-Paul", "2024-03-05", "1234.5", "$R-0042 x", "Jean-Paul Dupont", "0042", "FR"]);
    }

    #[test]
    fn suite_d_etapes() {
        let d = tmpdir("suite");
        let w = src(&d, &["n", "g"], &[&["3", "a"], &["1", "b"], &["2", "a"]]);
        let sc = Scratch { dir: d.clone(), sort_budget: 1 << 20 };
        let steps = vec![
            Step::Filter { conditions: vec![cond("g", Operator::In, &["a"])], combine: Combine::All },
            Step::Sort { keys: vec![SortKey { column: "n".into(), descending: false, mode: SortMode::Number }] },
            Step::Columns { columns: vec![ColumnSpec { source: "n".into(), name: "numero".into() }] },
        ];
        let out = run_steps(&w.path, &w.headers, &steps, &sc).unwrap();
        assert_eq!(out.headers, vec!["numero"]);
        let r: Vec<Row> = rows(&out.path).unwrap().map(Result::unwrap).collect();
        assert_eq!(r, vec![vec!["2"], vec!["3"]]);
        let bad = vec![steps[0].clone(), Step::Columns { columns: vec![ColumnSpec { source: "zz".into(), name: "".into() }] }];
        assert!(run_steps(&w.path, &w.headers, &bad, &sc).unwrap_err().starts_with("Étape 2"));
        // Un JSON de recette se relit à l'identique.
        let json = serde_json::to_string(&steps).unwrap();
        let back: Vec<Step> = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
    }
}
