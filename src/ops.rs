//! Opérations sur les tables : fusion, découpage, jointure, filtrage.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use regex::{Regex, RegexBuilder};
use serde::Deserialize;

use crate::csvdata::{unique_headers, Table};

// ---------------------------------------------------------------------------
// Fusion (concaténation verticale)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MergeMode {
    /// Toutes les colonnes de tous les fichiers ; cellules manquantes laissées vides.
    #[default]
    Union,
    /// Uniquement les colonnes présentes dans tous les fichiers.
    Intersection,
    /// Les fichiers doivent avoir exactement les mêmes colonnes (ordre libre).
    Strict,
    /// Colonnes alignées par position, sans tenir compte des noms.
    Position,
}

pub struct MergeOptions<'a> {
    pub mode: MergeMode,
    /// Si renseigné, ajoute une colonne contenant le nom du fichier d'origine.
    pub source_column: Option<&'a str>,
    pub dedupe: bool,
}

pub fn merge(inputs: &[(&str, &Table)], opts: &MergeOptions) -> Result<Table, String> {
    if inputs.len() < 2 {
        return Err("La fusion nécessite au moins deux fichiers".into());
    }
    let first = inputs[0].1;
    let mut headers: Vec<String> = match opts.mode {
        MergeMode::Union => {
            let mut seen = HashSet::new();
            let mut h = Vec::new();
            for (_, t) in inputs {
                for c in &t.headers {
                    if seen.insert(c.clone()) {
                        h.push(c.clone());
                    }
                }
            }
            h
        }
        MergeMode::Intersection => {
            let h: Vec<String> = first
                .headers
                .iter()
                .filter(|c| inputs.iter().all(|(_, t)| t.headers.contains(c)))
                .cloned()
                .collect();
            if h.is_empty() {
                return Err("Aucune colonne commune à tous les fichiers".into());
            }
            h
        }
        MergeMode::Strict => {
            let reference: HashSet<&String> = first.headers.iter().collect();
            for (name, t) in &inputs[1..] {
                let other: HashSet<&String> = t.headers.iter().collect();
                if other != reference {
                    let missing: Vec<_> = reference.difference(&other).map(|s| s.as_str()).collect();
                    let extra: Vec<_> = other.difference(&reference).map(|s| s.as_str()).collect();
                    return Err(format!(
                        "« {name} » n'a pas les mêmes colonnes que « {} » (manquantes : [{}], en trop : [{}])",
                        inputs[0].0,
                        missing.join(", "),
                        extra.join(", ")
                    ));
                }
            }
            first.headers.clone()
        }
        MergeMode::Position => {
            let width = inputs.iter().map(|(_, t)| t.headers.len()).max().unwrap_or(0);
            let mut h = first.headers.clone();
            for i in h.len()..width {
                h.push(format!("Colonne {}", i + 1));
            }
            h
        }
    };

    let mut rows = Vec::new();
    for (name, t) in inputs {
        // Pour chaque colonne de sortie : index dans la table source (ou None).
        let mapping: Vec<Option<usize>> = if opts.mode == MergeMode::Position {
            (0..headers.len()).map(|i| (i < t.headers.len()).then_some(i)).collect()
        } else {
            headers.iter().map(|h| t.headers.iter().position(|c| c == h)).collect()
        };
        for r in &t.rows {
            let mut row: Vec<String> =
                mapping.iter().map(|m| m.map(|i| r[i].clone()).unwrap_or_default()).collect();
            if opts.source_column.is_some() {
                row.push(name.to_string());
            }
            rows.push(row);
        }
    }
    if let Some(col) = opts.source_column {
        headers.push(if col.trim().is_empty() { "fichier_source".into() } else { col.to_string() });
        headers = unique_headers(headers);
    }
    let mut table = Table::new(headers, rows);
    if opts.dedupe {
        dedupe(&mut table);
    }
    Ok(table)
}

/// Supprime les lignes strictement identiques (conserve la première occurrence).
pub fn dedupe(table: &mut Table) {
    let mut seen: HashSet<Vec<String>> = HashSet::new();
    table.rows.retain(|r| seen.insert(r.clone()));
}

// ---------------------------------------------------------------------------
// Découpage
// ---------------------------------------------------------------------------

/// Découpe une table selon les valeurs combinées des colonnes données.
/// Les groupes sont renvoyés dans l'ordre de première apparition.
pub fn split(
    table: &Table,
    cols: &[usize],
    remove_columns: bool,
    max_groups: usize,
) -> Result<Vec<(Vec<String>, Table)>, String> {
    if cols.is_empty() {
        return Err("Choisissez au moins une colonne de découpage".into());
    }
    let kept: Vec<usize> =
        (0..table.headers.len()).filter(|i| !(remove_columns && cols.contains(i))).collect();
    let headers: Vec<String> = kept.iter().map(|&i| table.headers[i].clone()).collect();

    let mut index: HashMap<Vec<String>, usize> = HashMap::new();
    let mut groups: Vec<(Vec<String>, Table)> = Vec::new();
    for r in &table.rows {
        let key: Vec<String> = cols.iter().map(|&i| r[i].clone()).collect();
        let g = match index.get(&key) {
            Some(&g) => g,
            None => {
                if groups.len() >= max_groups {
                    return Err(format!(
                        "Le découpage produirait plus de {max_groups} fichiers : choisissez une colonne avec moins de valeurs distinctes"
                    ));
                }
                index.insert(key.clone(), groups.len());
                groups.push((key, Table::new(headers.clone(), Vec::new())));
                groups.len() - 1
            }
        };
        groups[g].1.rows.push(kept.iter().map(|&i| r[i].clone()).collect());
    }
    Ok(groups)
}

// ---------------------------------------------------------------------------
// Jointure
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JoinKind {
    /// Lignes ayant une correspondance des deux côtés.
    #[default]
    Inner,
    /// Toutes les lignes de gauche, complétées par la droite si correspondance.
    Left,
    /// Toutes les lignes de droite, complétées par la gauche si correspondance.
    Right,
    /// Toutes les lignes des deux fichiers.
    Full,
    /// Lignes de gauche sans correspondance à droite.
    LeftAnti,
    /// Lignes de droite sans correspondance à gauche.
    RightAnti,
    /// Lignes de gauche ayant au moins une correspondance (sans dupliquer, colonnes de gauche seules).
    LeftSemi,
}

pub struct JoinOptions {
    pub kind: JoinKind,
    pub case_insensitive: bool,
    pub trim: bool,
    /// Suffixe ajouté aux colonnes de droite dont le nom existe déjà à gauche.
    pub right_suffix: String,
}

pub fn join(
    left: &Table,
    right: &Table,
    lk: &[usize],
    rk: &[usize],
    opts: &JoinOptions,
) -> Result<Table, String> {
    if lk.is_empty() || lk.len() != rk.len() {
        return Err("Indiquez autant de colonnes de jointure à gauche qu'à droite (au moins une)".into());
    }
    let norm = |v: &str| -> String {
        let v = if opts.trim { v.trim() } else { v };
        if opts.case_insensitive { v.to_lowercase() } else { v.to_string() }
    };
    let key = |row: &[String], cols: &[usize]| -> Vec<String> { cols.iter().map(|&i| norm(&row[i])).collect() };

    let mut right_index: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
    for (i, r) in right.rows.iter().enumerate() {
        right_index.entry(key(r, rk)).or_default().push(i);
    }

    match opts.kind {
        JoinKind::LeftAnti | JoinKind::LeftSemi => {
            let want = opts.kind == JoinKind::LeftSemi;
            let rows = left
                .rows
                .iter()
                .filter(|r| right_index.contains_key(&key(r, lk)) == want)
                .cloned()
                .collect();
            return Ok(Table::new(left.headers.clone(), rows));
        }
        JoinKind::RightAnti => {
            let left_keys: HashSet<Vec<String>> = left.rows.iter().map(|r| key(r, lk)).collect();
            let rows =
                right.rows.iter().filter(|r| !left_keys.contains(&key(r, rk))).cloned().collect();
            return Ok(Table::new(right.headers.clone(), rows));
        }
        _ => {}
    }

    // En-têtes : toutes les colonnes de gauche, puis celles de droite hors clés.
    let right_cols: Vec<usize> = (0..right.headers.len()).filter(|i| !rk.contains(i)).collect();
    let mut headers = left.headers.clone();
    let mut taken: HashSet<String> = headers.iter().cloned().collect();
    for &i in &right_cols {
        let base = &right.headers[i];
        let mut name = if taken.contains(base) { format!("{base}{}", opts.right_suffix) } else { base.clone() };
        let mut n = 2;
        while taken.contains(&name) {
            name = format!("{base}{}{n}", opts.right_suffix);
            n += 1;
        }
        taken.insert(name.clone());
        headers.push(name);
    }

    let empty_right = vec![String::new(); right_cols.len()];
    let combine = |l: &[String], r: Option<&Vec<String>>| -> Vec<String> {
        let mut row = l.to_vec();
        match r {
            Some(r) => row.extend(right_cols.iter().map(|&i| r[i].clone())),
            None => row.extend(empty_right.iter().cloned()),
        }
        row
    };

    let mut rows = Vec::new();
    let mut matched = vec![false; right.rows.len()];
    for l in &left.rows {
        match right_index.get(&key(l, lk)) {
            Some(idx) => {
                for &ri in idx {
                    matched[ri] = true;
                    rows.push(combine(l, Some(&right.rows[ri])));
                }
            }
            None if matches!(opts.kind, JoinKind::Left | JoinKind::Full) => rows.push(combine(l, None)),
            None => {}
        }
    }
    if matches!(opts.kind, JoinKind::Right | JoinKind::Full) {
        for (ri, r) in right.rows.iter().enumerate() {
            if matched[ri] {
                continue;
            }
            // Ligne de droite seule : les colonnes clés de gauche reçoivent les valeurs de droite.
            let mut l = vec![String::new(); left.headers.len()];
            for (&li, &rki) in lk.iter().zip(rk) {
                l[li] = r[rki].clone();
            }
            rows.push(combine(&l, Some(r)));
        }
    }
    Ok(Table::new(headers, rows))
}

// ---------------------------------------------------------------------------
// Filtrage
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
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

#[derive(Clone, Debug, Deserialize)]
pub struct Condition {
    pub column: String,
    pub operator: Operator,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub case_sensitive: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Combine {
    /// Toutes les conditions doivent être vraies.
    #[default]
    All,
    /// Au moins une condition doit être vraie.
    Any,
}

struct Compiled {
    col: usize,
    op: Operator,
    values: Vec<String>,
    case_sensitive: bool,
    set: HashSet<String>,
    regex: Option<Regex>,
}

/// Convertit un texte en nombre en acceptant les formats français (« 1 234,5 »).
pub fn parse_number(s: &str) -> Option<f64> {
    let cleaned: String = s
        .trim()
        .chars()
        .filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}'))
        .map(|c| if c == ',' { '.' } else { c })
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    cleaned.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Compare numériquement si possible, sinon alphabétiquement (adapté aux dates ISO).
fn compare(cell: &str, value: &str) -> Ordering {
    match (parse_number(cell), parse_number(value)) {
        (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
        _ => cell.trim().cmp(value.trim()),
    }
}

impl Compiled {
    fn new(table: &Table, c: &Condition) -> Result<Self, String> {
        let col = table.col_index(&c.column)?;
        let fold = |s: &str| if c.case_sensitive { s.to_string() } else { s.to_lowercase() };
        let values: Vec<String> = c.values.iter().map(|v| fold(v)).collect();
        let needs = |n: usize| -> Result<(), String> {
            if c.values.len() < n {
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
        Ok(Compiled {
            col,
            op: c.operator,
            set: values.iter().cloned().collect(),
            values,
            case_sensitive: c.case_sensitive,
            regex,
        })
    }

    fn test(&self, row: &[String]) -> bool {
        let raw = row[self.col].as_str();
        let cell = if self.case_sensitive { raw.to_string() } else { raw.to_lowercase() };
        let v = &self.values;
        match self.op {
            Operator::In => self.set.contains(&cell),
            Operator::NotIn => !self.set.contains(&cell),
            Operator::Contains => v.iter().any(|x| cell.contains(x.as_str())),
            Operator::NotContains => !v.iter().any(|x| cell.contains(x.as_str())),
            Operator::StartsWith => v.iter().any(|x| cell.starts_with(x.as_str())),
            Operator::EndsWith => v.iter().any(|x| cell.ends_with(x.as_str())),
            Operator::Empty => raw.trim().is_empty(),
            Operator::NotEmpty => !raw.trim().is_empty(),
            Operator::Gt => !raw.trim().is_empty() && compare(raw, &v[0]) == Ordering::Greater,
            Operator::Ge => !raw.trim().is_empty() && compare(raw, &v[0]) != Ordering::Less,
            Operator::Lt => !raw.trim().is_empty() && compare(raw, &v[0]) == Ordering::Less,
            Operator::Le => !raw.trim().is_empty() && compare(raw, &v[0]) != Ordering::Greater,
            Operator::Between => {
                !raw.trim().is_empty()
                    && compare(raw, &v[0]) != Ordering::Less
                    && compare(raw, &v[1]) != Ordering::Greater
            }
            Operator::Regex => self.regex.as_ref().is_some_and(|re| re.is_match(raw)),
        }
    }
}

pub fn filter(table: &Table, conditions: &[Condition], combine: Combine) -> Result<Table, String> {
    if conditions.is_empty() {
        return Err("Ajoutez au moins une condition de filtrage".into());
    }
    let compiled: Vec<Compiled> =
        conditions.iter().map(|c| Compiled::new(table, c)).collect::<Result<_, _>>()?;
    let rows = table
        .rows
        .iter()
        .filter(|r| match combine {
            Combine::All => compiled.iter().all(|c| c.test(r)),
            Combine::Any => compiled.iter().any(|c| c.test(r)),
        })
        .cloned()
        .collect();
    Ok(Table::new(table.headers.clone(), rows))
}

/// Valeurs distinctes d'une colonne avec leur nombre d'occurrences (les plus fréquentes d'abord).
pub fn distinct_values(table: &Table, col: usize) -> Vec<(String, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for r in &table.rows {
        *counts.entry(r[col].as_str()).or_default() += 1;
    }
    let mut v: Vec<(String, usize)> = counts.into_iter().map(|(k, n)| (k.to_string(), n)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(headers: &[&str], rows: &[&[&str]]) -> Table {
        Table::new(
            headers.iter().map(|s| s.to_string()).collect(),
            rows.iter().map(|r| r.iter().map(|s| s.to_string()).collect()).collect(),
        )
    }

    #[test]
    fn fusion_union_et_source() {
        let a = t(&["id", "nom"], &[&["1", "A"]]);
        let b = t(&["nom", "ville"], &[&["B", "Lyon"]]);
        let opts = MergeOptions { mode: MergeMode::Union, source_column: Some("source"), dedupe: false };
        let m = merge(&[("a.csv", &a), ("b.csv", &b)], &opts).unwrap();
        assert_eq!(m.headers, vec!["id", "nom", "ville", "source"]);
        assert_eq!(m.rows[1], vec!["", "B", "Lyon", "b.csv"]);
    }

    #[test]
    fn fusion_stricte_refuse_colonnes_differentes() {
        let a = t(&["id", "nom"], &[]);
        let b = t(&["id", "ville"], &[]);
        let opts = MergeOptions { mode: MergeMode::Strict, source_column: None, dedupe: false };
        assert!(merge(&[("a", &a), ("b", &b)], &opts).is_err());
    }

    #[test]
    fn fusion_intersection_dedoublonnee() {
        let a = t(&["id", "nom", "x"], &[&["1", "A", "z"]]);
        let b = t(&["nom", "id"], &[&["A", "1"], &["B", "2"]]);
        let opts = MergeOptions { mode: MergeMode::Intersection, source_column: None, dedupe: true };
        let m = merge(&[("a", &a), ("b", &b)], &opts).unwrap();
        assert_eq!(m.headers, vec!["id", "nom"]);
        assert_eq!(m.rows, vec![vec!["1", "A"], vec!["2", "B"]]);
    }

    #[test]
    fn decoupage_multi_colonnes() {
        let a = t(&["pays", "ville", "n"], &[&["FR", "Paris", "1"], &["FR", "Lyon", "2"], &["FR", "Paris", "3"]]);
        let g = split(&a, &[0, 1], true, 100).unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].0, vec!["FR", "Paris"]);
        assert_eq!(g[0].1.headers, vec!["n"]);
        assert_eq!(g[0].1.rows.len(), 2);
        assert!(split(&a, &[2], false, 2).is_err());
    }

    #[test]
    fn jointures() {
        let l = t(&["id", "nom"], &[&["1", "A"], &["2", "B"]]);
        let r = t(&["code", "nom"], &[&["1", "X"], &["3", "Z"]]);
        let mk = |kind| JoinOptions { kind, case_insensitive: false, trim: false, right_suffix: "_d".into() };

        let j = join(&l, &r, &[0], &[0], &mk(JoinKind::Inner)).unwrap();
        assert_eq!(j.headers, vec!["id", "nom", "nom_d"]);
        assert_eq!(j.rows, vec![vec!["1", "A", "X"]]);

        let j = join(&l, &r, &[0], &[0], &mk(JoinKind::Left)).unwrap();
        assert_eq!(j.rows.len(), 2);
        assert_eq!(j.rows[1], vec!["2", "B", ""]);

        let j = join(&l, &r, &[0], &[0], &mk(JoinKind::Full)).unwrap();
        assert_eq!(j.rows.len(), 3);
        assert_eq!(j.rows[2], vec!["3", "", "Z"]);

        let j = join(&l, &r, &[0], &[0], &mk(JoinKind::Right)).unwrap();
        assert_eq!(j.rows.len(), 2);

        let j = join(&l, &r, &[0], &[0], &mk(JoinKind::LeftAnti)).unwrap();
        assert_eq!(j.rows, vec![vec!["2", "B"]]);
        let j = join(&l, &r, &[0], &[0], &mk(JoinKind::RightAnti)).unwrap();
        assert_eq!(j.rows, vec![vec!["3", "Z"]]);
        let j = join(&l, &r, &[0], &[0], &mk(JoinKind::LeftSemi)).unwrap();
        assert_eq!(j.rows, vec![vec!["1", "A"]]);
    }

    #[test]
    fn jointure_insensible_casse_multi_cles() {
        let l = t(&["a", "b", "v"], &[&["X", " y ", "1"]]);
        let r = t(&["a", "b", "w"], &[&["x", "Y", "2"]]);
        let o = JoinOptions { kind: JoinKind::Inner, case_insensitive: true, trim: true, right_suffix: "_2".into() };
        let j = join(&l, &r, &[0, 1], &[0, 1], &o).unwrap();
        assert_eq!(j.rows, vec![vec!["X", " y ", "1", "2"]]);
    }

    #[test]
    fn filtres() {
        let a = t(&["ville", "montant"], &[&["Paris", "1 200,50"], &["Lyon", "80"], &["paris", ""], &["Nice", "300"]]);
        let c = |column: &str, operator, values: &[&str]| Condition {
            column: column.into(),
            operator,
            values: values.iter().map(|s| s.to_string()).collect(),
            case_sensitive: false,
        };
        let f = filter(&a, &[c("ville", Operator::In, &["paris", "Nice"])], Combine::All).unwrap();
        assert_eq!(f.rows.len(), 3);
        let f = filter(&a, &[c("montant", Operator::Gt, &["100"])], Combine::All).unwrap();
        assert_eq!(f.rows.len(), 2);
        let f = filter(
            &a,
            &[c("ville", Operator::In, &["paris"]), c("montant", Operator::NotEmpty, &[])],
            Combine::All,
        )
        .unwrap();
        assert_eq!(f.rows.len(), 1);
        let f = filter(
            &a,
            &[c("ville", Operator::StartsWith, &["ly"]), c("montant", Operator::Between, &["250", "350"])],
            Combine::Any,
        )
        .unwrap();
        assert_eq!(f.rows.len(), 2);
        let f = filter(&a, &[c("ville", Operator::Regex, &["^(lyon|nice)$"])], Combine::All).unwrap();
        assert_eq!(f.rows.len(), 2);
        assert!(filter(&a, &[c("inconnue", Operator::Empty, &[])], Combine::All).is_err());
    }
}
