//! Lecture, écriture et détection de format des fichiers CSV.

use std::collections::HashSet;

/// Table CSV chargée en mémoire : une ligne d'en-têtes et des lignes de données
/// ayant toutes exactement `headers.len()` cellules.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Table { headers, rows }
    }

    pub fn col_index(&self, name: &str) -> Result<usize, String> {
        self.headers
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| format!("Colonne introuvable : « {name} »"))
    }

    pub fn col_indices(&self, names: &[String]) -> Result<Vec<usize>, String> {
        names.iter().map(|n| self.col_index(n)).collect()
    }
}

/// Options de lecture d'un fichier CSV.
#[derive(Clone, Debug)]
pub struct ReadOptions {
    pub delimiter: u8,
    pub quote: u8,
    pub has_headers: bool,
    pub trim: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        ReadOptions { delimiter: b',', quote: b'"', has_headers: true, trim: false }
    }
}

/// Options d'écriture d'un fichier CSV.
#[derive(Clone, Debug)]
pub struct WriteOptions {
    pub delimiter: u8,
    pub quote_all: bool,
    pub crlf: bool,
    /// `utf-8`, `utf-8-bom` ou tout libellé reconnu par encoding_rs (ex. `windows-1252`).
    pub encoding: String,
    pub include_headers: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions {
            delimiter: b',',
            quote_all: false,
            crlf: false,
            encoding: "utf-8".into(),
            include_headers: true,
        }
    }
}

/// Interprète un séparateur saisi par l'utilisateur.
/// Renvoie `None` pour « auto » (détection automatique).
pub fn parse_delimiter(s: &str) -> Result<Option<u8>, String> {
    let lower = s.trim_matches(|c| c == '\r' || c == '\n').to_lowercase();
    match lower.as_str() {
        "" | "auto" => Ok(None),
        "tab" | "\\t" | "\t" | "tabulation" => Ok(Some(b'\t')),
        "space" | "espace" | " " => Ok(Some(b' ')),
        _ => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii() && c != '"' && c != '\n' && c != '\r' => {
                    Ok(Some(c as u8))
                }
                _ => Err(format!(
                    "Séparateur invalide « {s} » : un seul caractère ASCII est attendu (ex. , ; | ou tab)"
                )),
            }
        }
    }
}

/// Décode des octets en texte. `encoding` vaut `auto` ou un libellé d'encodage
/// (`utf-8`, `windows-1252`, `iso-8859-15`, `utf-16le`…).
/// Renvoie le texte et le nom de l'encodage effectivement utilisé.
pub fn decode(bytes: &[u8], encoding: &str) -> Result<(String, String), String> {
    let label = encoding.trim().to_lowercase();
    if label.is_empty() || label == "auto" {
        if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
            let (text, _) = enc.decode_without_bom_handling(&bytes[bom_len..]);
            return Ok((text.into_owned(), format!("{} (BOM)", enc.name())));
        }
        return match std::str::from_utf8(bytes) {
            Ok(s) => Ok((s.to_string(), "UTF-8".into())),
            Err(_) => {
                // Cas fréquent des CSV exportés par Excel sous Windows.
                let (text, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
                Ok((text.into_owned(), "windows-1252".into()))
            }
        };
    }
    let enc = encoding_rs::Encoding::for_label(label.as_bytes())
        .ok_or_else(|| format!("Encodage inconnu : « {encoding} »"))?;
    let (text, used, _) = enc.decode(bytes);
    Ok((text.into_owned(), used.name().to_string()))
}

/// Détecte le séparateur le plus probable à partir d'un échantillon du texte.
pub fn detect_delimiter(text: &str) -> u8 {
    const CANDIDATES: [u8; 5] = *b",;\t|:";
    const SAMPLE: usize = 64 * 1024;

    let truncated = text.len() > SAMPLE;
    let sample = if truncated {
        let mut end = SAMPLE;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        &text[..end]
    } else {
        text
    };

    let mut best = (b',', 0.0_f64);
    for &cand in &CANDIDATES {
        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(cand)
            .has_headers(false)
            .flexible(true)
            .from_reader(sample.as_bytes());
        let mut counts: Vec<usize> =
            rdr.records().take(100).filter_map(Result::ok).map(|r| r.len()).collect();
        if truncated && counts.len() > 1 {
            counts.pop(); // dernière ligne potentiellement coupée
        }
        let Some(&first) = counts.first() else { continue };
        if first < 2 {
            continue;
        }
        let consistent =
            counts.iter().filter(|&&n| n == first).count() as f64 / counts.len() as f64;
        let mut score = consistent * consistent * (first.min(1000) as f64);
        if cand == b':' {
            score *= 0.5; // fréquent dans les heures : à n'utiliser qu'en dernier recours
        }
        if score > best.1 {
            best = (cand, score);
        }
    }
    best.0
}

/// Analyse un texte CSV en table.
pub fn parse(text: &str, opts: &ReadOptions) -> Result<Table, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(opts.delimiter)
        .quote(opts.quote)
        .has_headers(false)
        .flexible(true)
        .trim(if opts.trim { csv::Trim::All } else { csv::Trim::None })
        .from_reader(text.as_bytes());

    let mut records = rdr.records();
    let mut headers: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let to_vec = |r: csv::StringRecord| r.iter().map(str::to_string).collect::<Vec<_>>();

    if opts.has_headers
        && let Some(first) = records.next() {
            headers = to_vec(first.map_err(|e| format!("Erreur de lecture CSV : {e}"))?);
        }
    for rec in records {
        let rec = rec.map_err(|e| format!("Erreur de lecture CSV : {e}"))?;
        // Ignore les lignes entièrement vides (ex. lignes « ;;; » en fin de fichier Excel).
        if rec.iter().all(|c| c.is_empty()) {
            continue;
        }
        rows.push(to_vec(rec));
    }

    let width = rows.iter().map(Vec::len).max().unwrap_or(0).max(headers.len());
    while headers.len() < width {
        headers.push(String::new());
    }
    for row in &mut rows {
        row.resize(width, String::new());
    }
    Ok(Table::new(unique_headers(headers), rows))
}

/// Garantit des noms de colonnes non vides et uniques.
pub fn unique_headers(headers: Vec<String>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    headers
        .into_iter()
        .enumerate()
        .map(|(i, h)| {
            let base = if h.trim().is_empty() { format!("Colonne {}", i + 1) } else { h };
            let mut name = base.clone();
            let mut n = 2;
            while seen.contains(&name) {
                name = format!("{base}_{n}");
                n += 1;
            }
            seen.insert(name.clone());
            name
        })
        .collect()
}

/// Sérialise une table en octets CSV selon les options données.
pub fn write(table: &Table, opts: &WriteOptions) -> Result<Vec<u8>, String> {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(opts.delimiter)
        .quote_style(if opts.quote_all { csv::QuoteStyle::Always } else { csv::QuoteStyle::Necessary })
        .terminator(if opts.crlf { csv::Terminator::CRLF } else { csv::Terminator::Any(b'\n') })
        .from_writer(Vec::new());
    let err = |e: csv::Error| format!("Erreur d'écriture CSV : {e}");
    if opts.include_headers {
        wtr.write_record(&table.headers).map_err(err)?;
    }
    for row in &table.rows {
        wtr.write_record(row).map_err(err)?;
    }
    let bytes = wtr.into_inner().map_err(|e| format!("Erreur d'écriture CSV : {e}"))?;

    let label = opts.encoding.trim().to_lowercase();
    match label.as_str() {
        "" | "utf-8" | "utf8" => Ok(bytes),
        "utf-8-bom" | "utf8-bom" => {
            let mut out = vec![0xEF, 0xBB, 0xBF];
            out.extend(bytes);
            Ok(out)
        }
        _ => {
            let enc = encoding_rs::Encoding::for_label(label.as_bytes())
                .ok_or_else(|| format!("Encodage inconnu : « {} »", opts.encoding))?;
            let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
            let (out, _, had_errors) = enc.encode(&text);
            if had_errors {
                return Err(format!(
                    "Certains caractères ne peuvent pas être représentés en {} : choisissez UTF-8",
                    enc.name()
                ));
            }
            Ok(out.into_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecte_point_virgule() {
        let t = "nom;prénom;âge\nDupont;Jean;42\nMartin;\"Anne, Marie\";37\n";
        assert_eq!(detect_delimiter(t), b';');
    }

    #[test]
    fn detecte_tabulation_et_virgule() {
        assert_eq!(detect_delimiter("a\tb\tc\n1\t2\t3\n"), b'\t');
        assert_eq!(detect_delimiter("a,b\n1,2\n"), b',');
        assert_eq!(detect_delimiter("a|b|c\n1|2|3\n"), b'|');
    }

    #[test]
    fn parse_complete_les_lignes_courtes_et_en_tetes() {
        let t = "a,,a\n1,2\n3,4,5,6\n";
        let tbl = parse(t, &ReadOptions::default()).unwrap();
        assert_eq!(tbl.headers, vec!["a", "Colonne 2", "a_2", "Colonne 4"]);
        assert_eq!(tbl.rows[0], vec!["1", "2", "", ""]);
        assert_eq!(tbl.rows.len(), 2);
    }

    #[test]
    fn parse_sans_en_tetes() {
        let opts = ReadOptions { has_headers: false, ..Default::default() };
        let tbl = parse("1,2\n3,4\n", &opts).unwrap();
        assert_eq!(tbl.headers, vec!["Colonne 1", "Colonne 2"]);
        assert_eq!(tbl.rows.len(), 2);
    }

    #[test]
    fn decode_latin1_et_bom() {
        let (s, enc) = decode(b"caf\xe9", "auto").unwrap();
        assert_eq!(s, "café");
        assert_eq!(enc, "windows-1252");
        let (s, _) = decode(b"\xEF\xBB\xBFa;b", "auto").unwrap();
        assert_eq!(s, "a;b");
    }

    #[test]
    fn ecriture_aller_retour() {
        let tbl = Table::new(vec!["x".into(), "y".into()], vec![vec!["a;b".into(), "été".into()]]);
        let opts = WriteOptions { delimiter: b';', encoding: "windows-1252".into(), ..Default::default() };
        let bytes = write(&tbl, &opts).unwrap();
        assert_eq!(bytes, b"x;y\n\"a;b\";\xe9t\xe9\n");
        let (text, _) = decode(&bytes, "auto").unwrap();
        let back = parse(&text, &ReadOptions { delimiter: b';', ..Default::default() }).unwrap();
        assert_eq!(back, tbl);
    }

    #[test]
    fn separateurs_utilisateur() {
        assert_eq!(parse_delimiter("auto").unwrap(), None);
        assert_eq!(parse_delimiter("tab").unwrap(), Some(b'\t'));
        assert_eq!(parse_delimiter(";").unwrap(), Some(b';'));
        assert!(parse_delimiter(";;").is_err());
    }
}
