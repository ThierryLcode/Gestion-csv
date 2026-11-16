//! Profil d'un fichier : type, remplissage, valeurs distinctes, extrêmes et
//! valeurs fréquentes de chaque colonne, calculés en un seul passage.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::csvio::{reader, io_err, Res};
use crate::text::{parse_date_auto, parse_number, DateFormat, DateTime};

/// Au-delà de ce nombre de valeurs distinctes, le décompte devient approximatif.
const DISTINCT_CAP: usize = 50_000;

#[derive(Serialize)]
pub struct ColumnProfile {
    pub name: String,
    /// entier, décimal, date, booléen, texte ou vide
    pub kind: String,
    pub filled: u64,
    pub empty: u64,
    pub distinct: u64,
    /// Vrai si le nombre de valeurs distinctes dépasse la limite de décompte.
    pub distinct_capped: bool,
    pub min: Option<String>,
    pub max: Option<String>,
    pub mean: Option<f64>,
    pub min_length: u64,
    pub max_length: u64,
    /// Proportion de valeurs non vides compatibles avec le type détecté (0 à 1).
    pub type_ratio: f64,
    pub top: Vec<(String, u64)>,
}

#[derive(Serialize)]
pub struct Profile {
    pub rows: u64,
    pub columns: Vec<ColumnProfile>,
}

#[derive(Default)]
struct Acc {
    filled: u64,
    ints: u64,
    nums: u64,
    dates: u64,
    bools: u64,
    sum: f64,
    min_num: Option<f64>,
    max_num: Option<f64>,
    min_date: Option<DateTime>,
    max_date: Option<DateTime>,
    min_text: Option<String>,
    max_text: Option<String>,
    min_len: Option<u64>,
    max_len: u64,
    counts: HashMap<String, u64>,
    capped: bool,
}

fn is_bool(v: &str) -> bool {
    matches!(v.to_lowercase().as_str(), "vrai" | "faux" | "true" | "false" | "oui" | "non" | "yes" | "no")
}

impl Acc {
    fn add(&mut self, raw: &str) {
        let v = raw.trim();
        if v.is_empty() {
            return;
        }
        self.filled += 1;
        let len = v.chars().count() as u64;
        self.min_len = Some(self.min_len.map_or(len, |m| m.min(len)));
        self.max_len = self.max_len.max(len);
        if let Some(n) = parse_number(v) {
            self.nums += 1;
            if n.fract() == 0.0 && !v.contains([',', '.']) {
                self.ints += 1;
            }
            self.sum += n;
            self.min_num = Some(self.min_num.map_or(n, |m| m.min(n)));
            self.max_num = Some(self.max_num.map_or(n, |m| m.max(n)));
        } else if let Some(d) = parse_date_auto(v) {
            self.dates += 1;
            self.min_date = Some(self.min_date.map_or(d, |m| m.min(d)));
            self.max_date = Some(self.max_date.map_or(d, |m| m.max(d)));
        } else if is_bool(v) {
            self.bools += 1;
        }
        if self.min_text.as_deref().is_none_or(|m| v < m) {
            self.min_text = Some(v.to_string());
        }
        if self.max_text.as_deref().is_none_or(|m| v > m) {
            self.max_text = Some(v.to_string());
        }
        if let Some(c) = self.counts.get_mut(v) {
            *c += 1;
        } else if self.counts.len() < DISTINCT_CAP {
            self.counts.insert(v.to_string(), 1);
        } else {
            self.capped = true;
        }
    }

    fn finish(self, name: String, rows: u64) -> ColumnProfile {
        let f = self.filled.max(1) as f64;
        // Type majoritaire : au moins 95 % des valeurs non vides.
        let (kind, ratio) = if self.filled == 0 {
            ("vide", 1.0)
        } else if self.ints as f64 / f >= 0.95 {
            ("entier", self.ints as f64 / f)
        } else if self.nums as f64 / f >= 0.95 {
            ("décimal", self.nums as f64 / f)
        } else if self.dates as f64 / f >= 0.95 {
            ("date", self.dates as f64 / f)
        } else if self.bools as f64 / f >= 0.95 {
            ("booléen", self.bools as f64 / f)
        } else {
            ("texte", 1.0)
        };
        let fmt_num = |n: f64| crate::text::format_number(n, ',');
        let iso = DateFormat::parse("AAAA-MM-JJ").unwrap();
        let (min, max, mean) = match kind {
            "entier" | "décimal" => (self.min_num.map(fmt_num), self.max_num.map(fmt_num), (self.nums > 0).then(|| self.sum / self.nums as f64)),
            "date" => (self.min_date.map(|d| iso.write(&d)), self.max_date.map(|d| iso.write(&d)), None),
            _ => (self.min_text, self.max_text, None),
        };
        let mut top: Vec<(String, u64)> = self.counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        top.truncate(10);
        ColumnProfile {
            name,
            kind: kind.into(),
            filled: self.filled,
            empty: rows - self.filled,
            distinct: self.counts.len() as u64,
            distinct_capped: self.capped,
            min,
            max,
            mean,
            min_length: self.min_len.unwrap_or(0),
            max_length: self.max_len,
            type_ratio: ratio,
            top,
        }
    }
}

pub fn profile(path: &Path, headers: &[String]) -> Res<Profile> {
    let mut accs: Vec<Acc> = headers.iter().map(|_| Acc::default()).collect();
    let mut rdr = reader(path)?;
    let mut rec = csv::StringRecord::new();
    let mut n = 0u64;
    while rdr.read_record(&mut rec).map_err(io_err("Erreur de lecture"))? {
        n += 1;
        for (acc, v) in accs.iter_mut().zip(rec.iter()) {
            acc.add(v);
        }
    }
    Ok(Profile {
        rows: n,
        columns: accs.into_iter().zip(headers).map(|(a, h)| a.finish(h.clone(), n)).collect(),
    })
}

/// Valeurs avec effectifs, nombre de valeurs distinctes, plafond atteint.
pub type Distinct = (Vec<(String, u64)>, usize, bool);

/// Valeurs distinctes d'une colonne avec leurs effectifs (les plus fréquentes d'abord).
/// Renvoie aussi le nombre total de valeurs distinctes (plafonné) et si le plafond est atteint.
pub fn distinct_values(path: &Path, col: usize, cap: usize) -> Res<Distinct> {
    let mut counts: HashMap<String, u64> = HashMap::new();
    let mut capped = false;
    let mut rdr = reader(path)?;
    let mut rec = csv::StringRecord::new();
    while rdr.read_record(&mut rec).map_err(io_err("Erreur de lecture"))? {
        let v = rec.get(col).unwrap_or("");
        if let Some(c) = counts.get_mut(v) {
            *c += 1;
        } else if counts.len() < cap {
            counts.insert(v.to_string(), 1);
        } else {
            capped = true;
        }
    }
    let distinct = counts.len();
    let mut v: Vec<(String, u64)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok((v, distinct, capped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csvio::{tests::tmpdir, Sink};

    #[test]
    fn profil() {
        let d = tmpdir("profil");
        let mut s = Sink::create(d.join("p.csv"), vec!["id".into(), "prix".into(), "date".into(), "ville".into()]).unwrap();
        for r in [["1", "2,5", "05/01/2024", "Paris"], ["2", "10", "2023-12-31", "Paris"], ["3", "", "01/02/2024", "Lyon"]] {
            s.write(&r).unwrap();
        }
        let w = s.finish().unwrap();
        let p = profile(&w.path, &w.headers).unwrap();
        assert_eq!(p.rows, 3);
        let c = |i: usize| &p.columns[i];
        assert_eq!(c(0).kind, "entier");
        assert_eq!(c(1).kind, "décimal");
        assert_eq!(c(1).empty, 1);
        assert_eq!(c(1).max.as_deref(), Some("10"));
        assert_eq!(c(1).mean, Some(6.25));
        assert_eq!(c(2).kind, "date");
        assert_eq!(c(2).min.as_deref(), Some("2023-12-31"));
        assert_eq!(c(3).kind, "texte");
        assert_eq!(c(3).distinct, 2);
        assert_eq!(c(3).top[0], ("Paris".to_string(), 2));
        let (v, n, capped) = distinct_values(&w.path, 3, 1).unwrap();
        assert_eq!((v.len(), n, capped), (1, 1, true));
    }
}
