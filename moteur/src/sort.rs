//! Tri externe : trie des fichiers plus gros que la mémoire en triant des blocs
//! puis en les fusionnant.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::csvio::{rows, Res, Row, Scratch, Sink, Written};
use crate::text::{fold, natural_cmp, parse_date_auto, parse_number, DateTime};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SortMode {
    /// Alphabétique, sans tenir compte de la casse ni des accents.
    #[default]
    Text,
    /// Numérique (« 1 234,5 » accepté) ; les valeurs non numériques sont placées à la fin.
    Number,
    /// Naturel : « fichier2 » avant « fichier10 ».
    Natural,
    /// Chronologique (formats JJ/MM/AAAA, AAAA-MM-JJ…).
    Date,
    /// Octet par octet (usage interne).
    Raw,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SortKey {
    pub column: String,
    #[serde(default)]
    pub descending: bool,
    #[serde(default)]
    pub mode: SortMode,
}

enum K {
    Missing,
    Num(f64),
    Text(String),
    Nat(String),
    Date(DateTime),
    Raw(String),
}

pub struct Comparator {
    cols: Vec<(usize, SortMode, bool)>,
}

impl Comparator {
    pub fn new(headers: &[String], keys: &[SortKey]) -> Res<Self> {
        if keys.is_empty() {
            return Err("Choisissez au moins une colonne de tri".into());
        }
        let cols = keys
            .iter()
            .map(|k| {
                let i = headers
                    .iter()
                    .position(|h| *h == k.column)
                    .ok_or_else(|| format!("Colonne introuvable : « {} »", k.column))?;
                Ok((i, k.mode, k.descending))
            })
            .collect::<Res<_>>()?;
        Ok(Comparator { cols })
    }

    fn keys(&self, row: &[String]) -> Vec<K> {
        self.cols
            .iter()
            .map(|&(i, mode, _)| {
                let v = row.get(i).map(String::as_str).unwrap_or("");
                if mode == SortMode::Raw {
                    return K::Raw(v.to_string());
                }
                if v.trim().is_empty() {
                    return K::Missing;
                }
                match mode {
                    SortMode::Text => K::Text(fold(v)),
                    SortMode::Natural => K::Nat(fold(v)),
                    SortMode::Number => parse_number(v).map(K::Num).unwrap_or(K::Missing),
                    SortMode::Date => parse_date_auto(v).map(K::Date).unwrap_or(K::Missing),
                    SortMode::Raw => unreachable!(),
                }
            })
            .collect()
    }

    fn cmp(&self, a: &(Vec<K>, Row), b: &(Vec<K>, Row)) -> Ordering {
        for (n, (ka, kb)) in a.0.iter().zip(&b.0).enumerate() {
            let ord = match (ka, kb) {
                (K::Missing, K::Missing) => Ordering::Equal,
                // Les valeurs vides ou invalides restent en fin de liste, quel que soit le sens.
                (K::Missing, _) => return Ordering::Greater,
                (_, K::Missing) => return Ordering::Less,
                (K::Num(x), K::Num(y)) => x.total_cmp(y),
                (K::Date(x), K::Date(y)) => x.cmp(y),
                (K::Nat(x), K::Nat(y)) => natural_cmp(x, y),
                (K::Text(x), K::Text(y)) | (K::Raw(x), K::Raw(y)) => x.cmp(y),
                _ => Ordering::Equal,
            };
            let ord = if self.cols[n].2 { ord.reverse() } else { ord };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        // Départage alphabétique exact pour un ordre déterministe (Text / Nat).
        for &(i, mode, desc) in &self.cols {
            if matches!(mode, SortMode::Text | SortMode::Natural) {
                let ord = a.1[i].cmp(&b.1[i]);
                if ord != Ordering::Equal {
                    return if desc { ord.reverse() } else { ord };
                }
            }
        }
        Ordering::Equal
    }
}

fn write_chunk(path: PathBuf, headers: &[String], chunk: &mut Vec<(Vec<K>, Row)>, cmp: &Comparator) -> Res<Written> {
    chunk.sort_by(|a, b| cmp.cmp(a, b)); // tri stable
    let mut sink = Sink::create(path, headers.to_vec())?;
    for (_, row) in chunk.drain(..) {
        sink.write(&row)?;
    }
    sink.finish()
}

/// Trie un fichier normalisé. `budget` = mémoire approximative (octets) par bloc.
pub fn sort_file(src: &Path, headers: &[String], keys: &[SortKey], scratch: &Scratch) -> Res<Written> {
    let cmp = Comparator::new(headers, keys)?;
    let mut chunks: Vec<Written> = Vec::new();
    let mut buf: Vec<(Vec<K>, Row)> = Vec::new();
    let mut bytes = 0usize;
    let cleanup = |chunks: Vec<Written>| chunks.into_iter().for_each(Written::discard);

    for row in rows(src)? {
        let row = match row {
            Ok(r) => r,
            Err(e) => {
                cleanup(chunks);
                return Err(e);
            }
        };
        bytes += row.iter().map(|c| c.len() * 2 + 48).sum::<usize>() + 64;
        buf.push((cmp.keys(&row), row));
        if bytes >= scratch.sort_budget {
            chunks.push(write_chunk(scratch.path("tri"), headers, &mut buf, &cmp)?);
            bytes = 0;
        }
    }
    if chunks.is_empty() {
        return write_chunk(scratch.path("csv"), headers, &mut buf, &cmp);
    }
    if !buf.is_empty() {
        chunks.push(write_chunk(scratch.path("tri"), headers, &mut buf, &cmp)?);
    }

    // Fusion des blocs : à chaque étape, la plus petite tête de bloc (à égalité, le premier bloc).
    let result = (|| {
        let mut iters = chunks.iter().map(|c| rows(&c.path)).collect::<Res<Vec<_>>>()?;
        let mut heads: Vec<Option<(Vec<K>, Row)>> = Vec::with_capacity(iters.len());
        for it in iters.iter_mut() {
            heads.push(it.next().transpose()?.map(|r| (cmp.keys(&r), r)));
        }
        let mut sink = Sink::create(scratch.path("csv"), headers.to_vec())?;
        loop {
            let mut best: Option<usize> = None;
            for (i, h) in heads.iter().enumerate() {
                if let Some(h) = h {
                    match best {
                        Some(b) if cmp.cmp(h, heads[b].as_ref().unwrap()) != Ordering::Less => {}
                        _ => best = Some(i),
                    }
                }
            }
            let Some(b) = best else { break };
            let (_, row) = heads[b].take().unwrap();
            sink.write(&row)?;
            heads[b] = iters[b].next().transpose()?.map(|r| (cmp.keys(&r), r));
        }
        sink.finish()
    })();
    cleanup(chunks);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csvio::tests::tmpdir;

    fn write(dir: &Path, headers: &[&str], data: &[&[&str]]) -> Written {
        let mut s = Sink::create(dir.join("src.csv"), headers.iter().map(|s| s.to_string()).collect()).unwrap();
        for r in data {
            s.write(r).unwrap();
        }
        s.finish().unwrap()
    }

    fn col(w: &Written, i: usize) -> Vec<String> {
        rows(&w.path).unwrap().map(|r| r.unwrap()[i].clone()).collect()
    }

    #[test]
    fn tri_multi_cles_et_externe() {
        let d = tmpdir("tri");
        let data: Vec<Vec<String>> = (0..500).map(|i| vec![format!("g{}", i % 3), format!("{}", (i * 37) % 101)]).collect();
        let refs: Vec<Vec<&str>> = data.iter().map(|r| r.iter().map(String::as_str).collect()).collect();
        let refs2: Vec<&[&str]> = refs.iter().map(|r| r.as_slice()).collect();
        let src = write(&d, &["g", "n"], &refs2);
        let keys = vec![
            SortKey { column: "g".into(), descending: true, mode: SortMode::Text },
            SortKey { column: "n".into(), descending: false, mode: SortMode::Number },
        ];
        // Petit budget pour forcer plusieurs blocs.
        let small = Scratch { dir: d.clone(), sort_budget: 2000 };
        let big = Scratch { dir: d.clone(), sort_budget: 1 << 30 };
        let a = sort_file(&src.path, &src.headers, &keys, &small).unwrap();
        let b = sort_file(&src.path, &src.headers, &keys, &big).unwrap();
        assert_eq!(a.rows, 500);
        assert_eq!(col(&a, 0), col(&b, 0));
        assert_eq!(col(&a, 1), col(&b, 1));
        assert_eq!(col(&a, 0)[0], "g2");
        let first_nums: Vec<f64> = col(&a, 1)[..5].iter().map(|s| s.parse().unwrap()).collect();
        assert!(first_nums.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn tri_texte_naturel_dates_vides_en_fin() {
        let d = tmpdir("tri2");
        let src = write(&d, &["v"], &[&["éclair"], &[""], &["Zèbre"], &["abricot"]]);
        let sc = Scratch { dir: d.clone(), sort_budget: 1 << 20 };
        let k = |mode, descending| vec![SortKey { column: "v".into(), descending, mode }];
        assert_eq!(col(&sort_file(&src.path, &src.headers, &k(SortMode::Text, false), &sc).unwrap(), 0), vec!["abricot", "éclair", "Zèbre", ""]);
        assert_eq!(col(&sort_file(&src.path, &src.headers, &k(SortMode::Text, true), &sc).unwrap(), 0), vec!["Zèbre", "éclair", "abricot", ""]);
        let src = write(&d, &["v"], &[&["f10"], &["f2"], &["f1"]]);
        assert_eq!(col(&sort_file(&src.path, &src.headers, &k(SortMode::Natural, false), &sc).unwrap(), 0), vec!["f1", "f2", "f10"]);
        let src = write(&d, &["v"], &[&["02/01/2024"], &["31/12/2023"], &["x"]]);
        assert_eq!(col(&sort_file(&src.path, &src.headers, &k(SortMode::Date, false), &sc).unwrap(), 0), vec!["31/12/2023", "02/01/2024", "x"]);
    }
}
