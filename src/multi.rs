//! Opérations sur plusieurs fichiers : fusion, découpage, jointure, comparaison.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::csvio::{rows, unique_headers, Res, Row, Scratch, Sink, Written};
use crate::sort::{sort_file, SortKey, SortMode};
use crate::steps::{col_indices, hash_key};

/// Un fichier source : nom affiché, chemin du fichier normalisé et en-têtes.
pub struct Input<'a> {
    pub name: &'a str,
    pub path: &'a Path,
    pub headers: &'a [String],
}

// ---------------------------------------------------------------------------
// Fusion
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MergeMode {
    #[default]
    Union,
    Intersection,
    Strict,
    Position,
}

pub struct MergeOptions<'a> {
    pub mode: MergeMode,
    pub source_column: Option<&'a str>,
    pub dedupe: bool,
}

pub fn merge(inputs: &[Input], opts: &MergeOptions, scratch: &Scratch) -> Res<Written> {
    if inputs.len() < 2 {
        return Err("La fusion nécessite au moins deux fichiers".into());
    }
    let first = &inputs[0];
    let mut headers: Vec<String> = match opts.mode {
        MergeMode::Union => {
            let mut seen = HashSet::new();
            inputs.iter().flat_map(|t| t.headers.iter()).filter(|c| seen.insert(*c)).cloned().collect()
        }
        MergeMode::Intersection => {
            let h: Vec<String> =
                first.headers.iter().filter(|c| inputs.iter().all(|t| t.headers.contains(c))).cloned().collect();
            if h.is_empty() {
                return Err("Aucune colonne commune à tous les fichiers".into());
            }
            h
        }
        MergeMode::Strict => {
            let reference: HashSet<&String> = first.headers.iter().collect();
            for t in &inputs[1..] {
                let other: HashSet<&String> = t.headers.iter().collect();
                if other != reference {
                    let missing: Vec<_> = reference.difference(&other).map(|s| s.as_str()).collect();
                    let extra: Vec<_> = other.difference(&reference).map(|s| s.as_str()).collect();
                    return Err(format!(
                        "« {} » n'a pas les mêmes colonnes que « {} » (manquantes : [{}], en trop : [{}])",
                        t.name,
                        first.name,
                        missing.join(", "),
                        extra.join(", ")
                    ));
                }
            }
            first.headers.to_vec()
        }
        MergeMode::Position => {
            let width = inputs.iter().map(|t| t.headers.len()).max().unwrap_or(0);
            let mut h = first.headers.to_vec();
            for i in h.len()..width {
                h.push(format!("Colonne {}", i + 1));
            }
            h
        }
    };
    let width = headers.len();
    if let Some(col) = opts.source_column {
        headers.push(if col.trim().is_empty() { "fichier_source".into() } else { col.trim().to_string() });
        headers = unique_headers(headers);
    }

    let mut sink = Sink::create(scratch.path("csv"), headers.clone())?;
    let mut seen: HashSet<u128> = HashSet::new();
    for t in inputs {
        let mapping: Vec<Option<usize>> = if opts.mode == MergeMode::Position {
            (0..width).map(|i| (i < t.headers.len()).then_some(i)).collect()
        } else {
            headers[..width].iter().map(|h| t.headers.iter().position(|c| c == h)).collect()
        };
        for row in rows(t.path)? {
            let row = row?;
            let mut out: Row = mapping.iter().map(|m| m.map(|i| row[i].clone()).unwrap_or_default()).collect();
            if opts.dedupe && !seen.insert(hash_key(&out)) {
                continue;
            }
            if opts.source_column.is_some() {
                out.push(t.name.to_string());
            }
            sink.write(&out)?;
        }
    }
    sink.finish()
}

// ---------------------------------------------------------------------------
// Découpage
// ---------------------------------------------------------------------------

/// Nombre maximal de fichiers écrits simultanément (limite de descripteurs).
const OPEN_FILES: usize = 200;

/// Un fichier par combinaison de valeurs des colonnes, dans l'ordre de première apparition.
pub fn split(
    src: &Path,
    headers: &[String],
    cols: &[usize],
    remove_columns: bool,
    max_groups: usize,
    scratch: &Scratch,
) -> Res<Vec<(Row, Written)>> {
    if cols.is_empty() {
        return Err("Choisissez au moins une colonne de découpage".into());
    }
    let kept: Vec<usize> = (0..headers.len()).filter(|i| !(remove_columns && cols.contains(i))).collect();
    let out_headers: Vec<String> = kept.iter().map(|&i| headers[i].clone()).collect();
    let key = |r: &Row| -> Row { cols.iter().map(|&i| r[i].clone()).collect() };

    // 1er passage : inventaire des groupes.
    let mut index: HashMap<Row, usize> = HashMap::new();
    let mut keys: Vec<Row> = Vec::new();
    for row in rows(src)? {
        let k = key(&row?);
        if !index.contains_key(&k) {
            if keys.len() >= max_groups {
                return Err(format!(
                    "Le découpage produirait plus de {max_groups} fichiers : choisissez une colonne avec moins de valeurs distinctes"
                ));
            }
            index.insert(k.clone(), keys.len());
            keys.push(k);
        }
    }

    // Passages suivants : au plus OPEN_FILES fichiers ouverts à la fois.
    let mut results: Vec<(Row, Written)> = Vec::with_capacity(keys.len());
    for batch_start in (0..keys.len()).step_by(OPEN_FILES) {
        let batch_end = (batch_start + OPEN_FILES).min(keys.len());
        let mut sinks: Vec<Sink> = (batch_start..batch_end)
            .map(|_| Sink::create(scratch.path("csv"), out_headers.clone()))
            .collect::<Res<_>>()?;
        for row in rows(src)? {
            let row = row?;
            let g = index[&key(&row)];
            if (batch_start..batch_end).contains(&g) {
                let out: Row = kept.iter().map(|&i| row[i].clone()).collect();
                sinks[g - batch_start].write(&out)?;
            }
        }
        for (i, s) in sinks.into_iter().enumerate() {
            results.push((keys[batch_start + i].clone(), s.finish()?));
        }
    }
    Ok(results)
}

// ---------------------------------------------------------------------------
// Jointure
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JoinKind {
    #[default]
    Inner,
    Left,
    Right,
    Full,
    LeftAnti,
    RightAnti,
    LeftSemi,
}

pub struct JoinOptions {
    pub kind: JoinKind,
    pub case_insensitive: bool,
    pub trim: bool,
    pub right_suffix: String,
}

/// Jointure par hachage : le fichier de gauche est lu en flux, celui de droite
/// est indexé en mémoire (placer le plus petit fichier à droite).
pub fn join(left: &Input, right: &Input, lk: &[String], rk: &[String], opts: &JoinOptions, scratch: &Scratch) -> Res<Written> {
    let lk = col_indices(left.headers, lk)?;
    let rk = col_indices(right.headers, rk)?;
    if lk.is_empty() || lk.len() != rk.len() {
        return Err("Indiquez autant de colonnes de jointure à gauche qu'à droite (au moins une)".into());
    }
    let norm = |v: &str| -> String {
        let v = if opts.trim { v.trim() } else { v };
        if opts.case_insensitive { v.to_lowercase() } else { v.to_string() }
    };
    let key = |row: &Row, cols: &[usize]| -> Row { cols.iter().map(|&i| norm(&row[i])).collect() };

    match opts.kind {
        JoinKind::LeftAnti | JoinKind::LeftSemi => {
            let mut right_keys = HashSet::new();
            for r in rows(right.path)? {
                right_keys.insert(key(&r?, &rk));
            }
            let want = opts.kind == JoinKind::LeftSemi;
            let mut sink = Sink::create(scratch.path("csv"), left.headers.to_vec())?;
            for l in rows(left.path)? {
                let l = l?;
                if right_keys.contains(&key(&l, &lk)) == want {
                    sink.write(&l)?;
                }
            }
            return sink.finish();
        }
        JoinKind::RightAnti => {
            let mut left_keys = HashSet::new();
            for l in rows(left.path)? {
                left_keys.insert(key(&l?, &lk));
            }
            let mut sink = Sink::create(scratch.path("csv"), right.headers.to_vec())?;
            for r in rows(right.path)? {
                let r = r?;
                if !left_keys.contains(&key(&r, &rk)) {
                    sink.write(&r)?;
                }
            }
            return sink.finish();
        }
        _ => {}
    }

    // En-têtes : toutes les colonnes de gauche, puis celles de droite hors clés.
    let right_cols: Vec<usize> = (0..right.headers.len()).filter(|i| !rk.contains(i)).collect();
    let mut headers = left.headers.to_vec();
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

    let mut right_rows: Vec<Row> = Vec::new();
    let mut index: HashMap<Row, Vec<usize>> = HashMap::new();
    for r in rows(right.path)? {
        let r = r?;
        index.entry(key(&r, &rk)).or_default().push(right_rows.len());
        right_rows.push(r);
    }

    let mut sink = Sink::create(scratch.path("csv"), headers)?;
    let combine = |l: &[String], r: Option<&Row>| -> Row {
        let mut row = l.to_vec();
        row.extend(right_cols.iter().map(|&i| r.map(|r| r[i].clone()).unwrap_or_default()));
        row
    };
    let mut matched = vec![false; right_rows.len()];
    for l in rows(left.path)? {
        let l = l?;
        match index.get(&key(&l, &lk)) {
            Some(idx) => {
                for &ri in idx {
                    matched[ri] = true;
                    sink.write(&combine(&l, Some(&right_rows[ri])))?;
                }
            }
            None if matches!(opts.kind, JoinKind::Left | JoinKind::Full) => sink.write(&combine(&l, None))?,
            None => {}
        }
    }
    if matches!(opts.kind, JoinKind::Right | JoinKind::Full) {
        for (ri, r) in right_rows.iter().enumerate() {
            if !matched[ri] {
                // Ligne de droite seule : les clés de gauche reçoivent les valeurs de droite.
                let mut l = vec![String::new(); left.headers.len()];
                for (&li, &rki) in lk.iter().zip(&rk) {
                    l[li] = r[rki].clone();
                }
                sink.write(&combine(&l, Some(r)))?;
            }
        }
    }
    sink.finish()
}

// ---------------------------------------------------------------------------
// Comparaison de deux versions
// ---------------------------------------------------------------------------

pub struct CompareOptions {
    /// Colonnes comparées ; vide = toutes les colonnes communes hors clés.
    pub columns: Vec<String>,
    pub include_unchanged: bool,
    pub case_insensitive: bool,
    pub trim: bool,
}

#[derive(Serialize, Default, Debug)]
pub struct CompareSummary {
    pub added: u64,
    pub removed: u64,
    pub modified: u64,
    pub unchanged: u64,
    pub duplicate_keys: u64,
    pub columns_added: Vec<String>,
    pub columns_removed: Vec<String>,
    /// Nombre de modifications par colonne.
    pub changes_by_column: Vec<(String, u64)>,
}

/// Compare deux versions d'un fichier ligne à ligne via une clé. Les deux fichiers
/// sont triés sur la clé (tri externe) puis parcourus en parallèle : la mémoire
/// utilisée ne dépend pas de la taille des fichiers.
pub fn compare(old: &Input, new: &Input, keys: &[String], opts: &CompareOptions, scratch: &Scratch) -> Res<(Written, CompareSummary)> {
    if keys.is_empty() {
        return Err("Choisissez au moins une colonne identifiant les lignes".into());
    }
    let ok = col_indices(old.headers, keys).map_err(|e| format!("Ancienne version : {e}"))?;
    let nk = col_indices(new.headers, keys).map_err(|e| format!("Nouvelle version : {e}"))?;
    let compared: Vec<String> = if opts.columns.is_empty() {
        new.headers.iter().filter(|c| old.headers.contains(c) && !keys.contains(c)).cloned().collect()
    } else {
        opts.columns.clone()
    };
    let oc = col_indices(old.headers, &compared).map_err(|e| format!("Ancienne version : {e}"))?;
    let nc = col_indices(new.headers, &compared).map_err(|e| format!("Nouvelle version : {e}"))?;

    let mut summary = CompareSummary {
        columns_added: new.headers.iter().filter(|c| !old.headers.contains(c)).cloned().collect(),
        columns_removed: old.headers.iter().filter(|c| !new.headers.contains(c)).cloned().collect(),
        ..Default::default()
    };
    let mut changes = vec![0u64; compared.len()];

    // Colonnes du résultat.
    let union: Vec<String> = new.headers.iter().chain(summary.columns_removed.iter()).cloned().collect();
    let from_new: Vec<Option<usize>> = union.iter().map(|c| new.headers.iter().position(|h| h == c)).collect();
    let from_old: Vec<Option<usize>> = union.iter().map(|c| old.headers.iter().position(|h| h == c)).collect();
    let mut out_headers = vec!["statut".to_string(), "colonnes_modifiees".to_string()];
    out_headers.extend(union.iter().cloned());
    out_headers.extend(compared.iter().map(|c| format!("{c} (avant)")));
    let mut sink = Sink::create(scratch.path("csv"), unique_headers(out_headers))?;

    let sort_keys: Vec<SortKey> =
        keys.iter().map(|k| SortKey { column: k.clone(), descending: false, mode: SortMode::Raw }).collect();
    let old_sorted = sort_file(old.path, old.headers, &sort_keys, scratch)?;
    let new_sorted = match sort_file(new.path, new.headers, &sort_keys, scratch) {
        Ok(w) => w,
        Err(e) => {
            old_sorted.discard();
            return Err(e);
        }
    };

    let norm = |v: &str| -> String {
        let v = if opts.trim { v.trim() } else { v };
        if opts.case_insensitive { v.to_lowercase() } else { v.to_string() }
    };
    let n_compared = compared.len();
    let emit = |sink: &mut Sink, status: &str, o: Option<&Row>, n: Option<&Row>, changed: &[usize]| -> Res<()> {
        let mut row = vec![status.to_string(), changed.iter().map(|&i| compared[i].as_str()).collect::<Vec<_>>().join(", ")];
        for (fnw, fold) in from_new.iter().zip(&from_old) {
            let v = match (n, fnw, o, fold) {
                (Some(n), Some(i), _, _) => n[*i].clone(),
                (_, _, Some(o), Some(i)) => o[*i].clone(),
                _ => String::new(),
            };
            row.push(v);
        }
        for i in 0..n_compared {
            row.push(if changed.contains(&i) { o.map(|o| o[oc[i]].clone()).unwrap_or_default() } else { String::new() });
        }
        sink.write(&row)
    };

    let result: Res<()> = (|| {
        let mut oi = rows(&old_sorted.path)?;
        let mut ni = rows(&new_sorted.path)?;
        let mut o = oi.next().transpose()?;
        let mut n = ni.next().transpose()?;
        let (mut last_o, mut last_n): (Option<Row>, Option<Row>) = (None, None);
        let okey = |r: &Row| -> Row { ok.iter().map(|&i| r[i].clone()).collect() };
        let nkey = |r: &Row| -> Row { nk.iter().map(|&i| r[i].clone()).collect() };
        loop {
            let ord = match (&o, &n) {
                (None, None) => break,
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (Some(a), Some(b)) => okey(a).cmp(&nkey(b)),
            };
            if ord != Ordering::Greater {
                let k = okey(o.as_ref().unwrap());
                if last_o.as_ref() == Some(&k) {
                    summary.duplicate_keys += 1;
                }
                last_o = Some(k);
            }
            if ord != Ordering::Less {
                let k = nkey(n.as_ref().unwrap());
                if last_n.as_ref() == Some(&k) {
                    summary.duplicate_keys += 1;
                }
                last_n = Some(k);
            }
            match ord {
                Ordering::Less => {
                    summary.removed += 1;
                    emit(&mut sink, "supprimée", o.as_ref(), None, &[])?;
                    o = oi.next().transpose()?;
                }
                Ordering::Greater => {
                    summary.added += 1;
                    emit(&mut sink, "ajoutée", None, n.as_ref(), &[])?;
                    n = ni.next().transpose()?;
                }
                Ordering::Equal => {
                    let (a, b) = (o.as_ref().unwrap(), n.as_ref().unwrap());
                    let changed: Vec<usize> = (0..n_compared).filter(|&i| norm(&a[oc[i]]) != norm(&b[nc[i]])).collect();
                    if changed.is_empty() {
                        summary.unchanged += 1;
                        if opts.include_unchanged {
                            emit(&mut sink, "identique", o.as_ref(), n.as_ref(), &[])?;
                        }
                    } else {
                        summary.modified += 1;
                        changed.iter().for_each(|&i| changes[i] += 1);
                        emit(&mut sink, "modifiée", o.as_ref(), n.as_ref(), &changed)?;
                    }
                    o = oi.next().transpose()?;
                    n = ni.next().transpose()?;
                }
            }
        }
        Ok(())
    })();
    old_sorted.discard();
    new_sorted.discard();
    result?;
    summary.changes_by_column =
        compared.iter().cloned().zip(changes).filter(|(_, n)| *n > 0).collect();
    Ok((sink.finish()?, summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csvio::tests::tmpdir;

    fn src(dir: &Path, name: &str, headers: &[&str], data: &[&[&str]]) -> Written {
        let mut s = Sink::create(dir.join(name), headers.iter().map(|s| s.to_string()).collect()).unwrap();
        for r in data {
            s.write(r).unwrap();
        }
        s.finish().unwrap()
    }
    fn input<'a>(name: &'a str, w: &'a Written) -> Input<'a> {
        Input { name, path: &w.path, headers: &w.headers }
    }
    fn all(w: &Written) -> Vec<Row> {
        rows(&w.path).unwrap().map(Result::unwrap).collect()
    }
    fn sc(d: &Path) -> Scratch {
        Scratch { dir: d.to_path_buf(), sort_budget: 1 << 20 }
    }

    #[test]
    fn fusions() {
        let d = tmpdir("merge");
        let a = src(&d, "a", &["id", "nom"], &[&["1", "A"]]);
        let b = src(&d, "b", &["nom", "ville"], &[&["B", "Lyon"], &["B", "Lyon"]]);
        let opts = MergeOptions { mode: MergeMode::Union, source_column: Some("source"), dedupe: true };
        let m = merge(&[input("a.csv", &a), input("b.csv", &b)], &opts, &sc(&d)).unwrap();
        assert_eq!(m.headers, vec!["id", "nom", "ville", "source"]);
        assert_eq!(all(&m), vec![vec!["1", "A", "", "a.csv"], vec!["", "B", "Lyon", "b.csv"]]);
        let strict = MergeOptions { mode: MergeMode::Strict, source_column: None, dedupe: false };
        assert!(merge(&[input("a", &a), input("b", &b)], &strict, &sc(&d)).is_err());
        let inter = MergeOptions { mode: MergeMode::Intersection, source_column: None, dedupe: false };
        assert_eq!(merge(&[input("a", &a), input("b", &b)], &inter, &sc(&d)).unwrap().headers, vec!["nom"]);
    }

    #[test]
    fn decoupage() {
        let d = tmpdir("split");
        let a = src(&d, "a", &["pays", "ville", "n"], &[&["FR", "Paris", "1"], &["FR", "Lyon", "2"], &["FR", "Paris", "3"]]);
        let g = split(&a.path, &a.headers, &[0, 1], true, 100, &sc(&d)).unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].0, vec!["FR", "Paris"]);
        assert_eq!(g[0].1.headers, vec!["n"]);
        assert_eq!(all(&g[0].1), vec![vec!["1"], vec!["3"]]);
        assert!(split(&a.path, &a.headers, &[2], false, 2, &sc(&d)).is_err());
        // Plus de groupes que de fichiers ouverts simultanément.
        let data: Vec<Vec<String>> = (0..450).map(|i| vec![i.to_string()]).collect();
        let refs: Vec<Vec<&str>> = data.iter().map(|r| r.iter().map(String::as_str).collect()).collect();
        let refs: Vec<&[&str]> = refs.iter().map(|r| r.as_slice()).collect();
        let big = src(&d, "big", &["k"], &refs);
        let g = split(&big.path, &big.headers, &[0], false, 1000, &sc(&d)).unwrap();
        assert_eq!(g.len(), 450);
        assert_eq!(all(&g[449].1), vec![vec!["449"]]);
    }

    #[test]
    fn jointures() {
        let d = tmpdir("join");
        let l = src(&d, "l", &["id", "nom"], &[&["1", "A"], &["2", "B"]]);
        let r = src(&d, "r", &["code", "nom"], &[&["1", "X"], &["3", "Z"]]);
        let run = |kind| {
            let o = JoinOptions { kind, case_insensitive: false, trim: false, right_suffix: "_d".into() };
            let w = join(&input("l", &l), &input("r", &r), &["id".into()], &["code".into()], &o, &sc(&d)).unwrap();
            (w.headers.clone(), all(&w))
        };
        let (h, rows) = run(JoinKind::Inner);
        assert_eq!(h, vec!["id", "nom", "nom_d"]);
        assert_eq!(rows, vec![vec!["1", "A", "X"]]);
        assert_eq!(run(JoinKind::Left).1[1], vec!["2", "B", ""]);
        assert_eq!(run(JoinKind::Full).1[2], vec!["3", "", "Z"]);
        assert_eq!(run(JoinKind::Right).1.len(), 2);
        assert_eq!(run(JoinKind::LeftAnti).1, vec![vec!["2", "B"]]);
        assert_eq!(run(JoinKind::RightAnti).1, vec![vec!["3", "Z"]]);
        assert_eq!(run(JoinKind::LeftSemi).1, vec![vec!["1", "A"]]);
    }

    #[test]
    fn comparaison() {
        let d = tmpdir("compare");
        let old = src(&d, "old", &["id", "nom", "prix", "obsolete"], &[
            &["1", "Café", "2,5", "x"], &["2", "Thé", "3", "y"], &["3", "Eau", "1", "z"],
        ]);
        let new = src(&d, "new", &["id", "nom", "prix", "stock"], &[
            &["3", "eau", "1", "5"], &["1", "Café", "2,8", "9"], &["4", "Jus", "4", "1"],
        ]);
        let opts = CompareOptions { columns: vec![], include_unchanged: true, case_insensitive: true, trim: false };
        let (w, s) = compare(&input("old", &old), &input("new", &new), &["id".into()], &opts, &sc(&d)).unwrap();
        assert_eq!((s.added, s.removed, s.modified, s.unchanged), (1, 1, 1, 1));
        assert_eq!(s.columns_added, vec!["stock"]);
        assert_eq!(s.columns_removed, vec!["obsolete"]);
        assert_eq!(s.changes_by_column, vec![("prix".to_string(), 1)]);
        assert_eq!(w.headers, vec!["statut", "colonnes_modifiees", "id", "nom", "prix", "stock", "obsolete", "nom (avant)", "prix (avant)"]);
        let r = all(&w);
        assert_eq!(r[0], vec!["modifiée", "prix", "1", "Café", "2,8", "9", "x", "", "2,5"]);
        assert_eq!(r[1], vec!["supprimée", "", "2", "Thé", "3", "", "y", "", ""]);
        assert_eq!(r[2][0], "identique");
        assert_eq!(r[3][0], "ajoutée");
    }
}
