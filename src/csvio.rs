//! Lecture et écriture de fichiers CSV en flux, sans charger le fichier en mémoire.
//!
//! Chaque fichier de l'espace de travail est stocké sur disque dans un format
//! normalisé : UTF-8, séparateur virgule, une ligne d'en-têtes, et toutes les
//! lignes ont exactement autant de cellules que d'en-têtes.

use std::collections::HashSet;
use std::fmt::Display;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use encoding_rs::Encoding;

pub type Res<T> = Result<T, String>;
pub type Row = Vec<String>;

pub fn io_err<E: Display>(ctx: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{ctx} : {e}")
}

/// Répertoire de travail pour les fichiers produits et temporaires.
#[derive(Clone, Debug)]
pub struct Scratch {
    pub dir: PathBuf,
    /// Mémoire approximative (octets) utilisée par bloc lors des tris.
    pub sort_budget: usize,
}

impl Scratch {
    pub fn path(&self, ext: &str) -> PathBuf {
        self.dir.join(format!("{}.{ext}", uuid::Uuid::new_v4().simple()))
    }
}

// ---------------------------------------------------------------------------
// Écriture normalisée
// ---------------------------------------------------------------------------

/// Résultat d'une écriture : un fichier normalisé prêt à rejoindre l'espace de travail.
#[derive(Debug)]
pub struct Written {
    pub path: PathBuf,
    pub headers: Vec<String>,
    pub rows: u64,
    pub size: u64,
}

impl Written {
    pub fn discard(self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Écrit un fichier normalisé ligne par ligne.
pub struct Sink {
    path: PathBuf,
    wtr: csv::Writer<BufWriter<File>>,
    headers: Vec<String>,
    rows: u64,
}

impl Sink {
    pub fn create(path: PathBuf, headers: Vec<String>) -> Res<Sink> {
        let file = File::create(&path).map_err(io_err("Création du fichier impossible"))?;
        let mut wtr = csv::WriterBuilder::new()
            .flexible(true)
            .from_writer(BufWriter::with_capacity(256 * 1024, file));
        wtr.write_record(&headers).map_err(io_err("Erreur d'écriture"))?;
        Ok(Sink { path, wtr, headers, rows: 0 })
    }

    /// Écrit une ligne ; les lignes trop courtes sont complétées par des cellules vides.
    pub fn write<T: AsRef<[u8]>>(&mut self, row: &[T]) -> Res<()> {
        let err = io_err("Erreur d'écriture");
        if row.len() >= self.headers.len() {
            self.wtr.write_record(row).map_err(err)?;
        } else {
            for f in row {
                self.wtr.write_field(f).map_err(&err)?;
            }
            for _ in row.len()..self.headers.len() {
                self.wtr.write_field("").map_err(&err)?;
            }
            self.wtr.write_record(std::iter::empty::<&[u8]>()).map_err(err)?;
        }
        self.rows += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Res<Written> {
        self.wtr.flush().map_err(io_err("Erreur d'écriture"))?;
        drop(self.wtr);
        let size = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        Ok(Written { path: self.path, headers: self.headers, rows: self.rows, size })
    }
}

// ---------------------------------------------------------------------------
// Lecture normalisée
// ---------------------------------------------------------------------------

/// Ouvre un fichier normalisé ; l'itérateur ne renvoie que les lignes de données.
pub fn reader(path: &Path) -> Res<csv::Reader<File>> {
    csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .buffer_capacity(256 * 1024)
        .from_path(path)
        .map_err(io_err("Ouverture du fichier impossible"))
}

/// Itère sur les lignes d'un fichier normalisé.
pub fn rows(path: &Path) -> Res<impl Iterator<Item = Res<Row>>> {
    Ok(reader(path)?.into_records().map(|r| {
        r.map(|rec| rec.iter().map(str::to_string).collect())
            .map_err(io_err("Erreur de lecture"))
    }))
}

/// Positions (en octets) d'une ligne sur `step`, pour la pagination rapide.
pub fn build_index(path: &Path, step: u64) -> Res<Vec<csv::Position>> {
    let mut rdr = reader(path)?;
    rdr.headers().map_err(io_err("Erreur de lecture"))?;
    let mut rec = csv::ByteRecord::new();
    let mut idx = Vec::new();
    let mut n = 0u64;
    loop {
        if n.is_multiple_of(step) {
            idx.push(rdr.position().clone());
        }
        if !rdr.read_byte_record(&mut rec).map_err(io_err("Erreur de lecture"))? {
            break;
        }
        n += 1;
    }
    Ok(idx)
}

/// Lit `limit` lignes à partir de la position donnée.
pub fn read_page(path: &Path, pos: &csv::Position, skip: u64, limit: usize) -> Res<Vec<Row>> {
    let mut rdr = reader(path)?;
    rdr.headers().map_err(io_err("Erreur de lecture"))?;
    rdr.seek(pos.clone()).map_err(io_err("Erreur de lecture"))?;
    let mut out = Vec::with_capacity(limit);
    let mut rec = csv::StringRecord::new();
    let mut skipped = 0;
    while out.len() < limit && rdr.read_record(&mut rec).map_err(io_err("Erreur de lecture"))? {
        if skipped < skip {
            skipped += 1;
            continue;
        }
        out.push(rec.iter().map(str::to_string).collect());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Séparateur et encodage
// ---------------------------------------------------------------------------

/// Interprète un séparateur saisi par l'utilisateur ; `None` = détection automatique.
pub fn parse_delimiter(s: &str) -> Res<Option<u8>> {
    let lower = s.trim_matches(|c| c == '\r' || c == '\n').to_lowercase();
    match lower.as_str() {
        "" | "auto" => Ok(None),
        "tab" | "\\t" | "\t" | "tabulation" => Ok(Some(b'\t')),
        "space" | "espace" | " " => Ok(Some(b' ')),
        _ => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii() && c != '"' && c != '\n' && c != '\r' => Ok(Some(c as u8)),
                _ => Err(format!(
                    "Séparateur invalide « {s} » : un seul caractère ASCII est attendu (ex. , ; | ou tab)"
                )),
            }
        }
    }
}

/// Détecte le séparateur le plus probable à partir d'un échantillon de texte.
pub fn detect_delimiter(sample: &str, truncated: bool) -> u8 {
    const CANDIDATES: [u8; 5] = *b",;\t|:";
    let mut best = (b',', 0.0_f64);
    for &cand in &CANDIDATES {
        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(cand)
            .has_headers(false)
            .flexible(true)
            .from_reader(sample.as_bytes());
        let mut counts: Vec<usize> = rdr.records().take(100).filter_map(Result::ok).map(|r| r.len()).collect();
        if truncated && counts.len() > 1 {
            counts.pop(); // dernière ligne potentiellement coupée
        }
        let Some(&first) = counts.first() else { continue };
        if first < 2 {
            continue;
        }
        let consistent = counts.iter().filter(|&&n| n == first).count() as f64 / counts.len() as f64;
        let mut score = consistent * consistent * (first.min(1000) as f64);
        if cand == b':' {
            score *= 0.5; // fréquent dans les heures : en dernier recours seulement
        }
        if score > best.1 {
            best = (cand, score);
        }
    }
    best.0
}

/// Détermine l'encodage d'un fichier brut. Renvoie l'encodage, la longueur du BOM à
/// ignorer et un libellé lisible.
pub fn detect_encoding(path: &Path, label: &str) -> Res<(&'static Encoding, u64, String)> {
    let mut f = File::open(path).map_err(io_err("Ouverture du fichier impossible"))?;
    let mut head = [0u8; 4];
    let n = read_up_to(&mut f, &mut head).map_err(io_err("Erreur de lecture"))?;
    let bom = Encoding::for_bom(&head[..n]);

    let label = label.trim().to_lowercase();
    if !label.is_empty() && label != "auto" {
        let enc = Encoding::for_label(label.as_bytes()).ok_or_else(|| format!("Encodage inconnu : « {label} »"))?;
        let skip = match bom {
            Some((b, len)) if b == enc => len as u64,
            _ => 0,
        };
        return Ok((enc, skip, enc.name().to_string()));
    }
    if let Some((enc, len)) = bom {
        return Ok((enc, len as u64, format!("{} (BOM)", enc.name())));
    }
    // Vérifie la validité UTF-8 sur tout le fichier, par blocs.
    f.seek(SeekFrom::Start(0)).map_err(io_err("Erreur de lecture"))?;
    let mut rdr = BufReader::with_capacity(1 << 20, f);
    let mut carry: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = rdr.read(&mut buf).map_err(io_err("Erreur de lecture"))?;
        if n == 0 {
            break;
        }
        carry.extend_from_slice(&buf[..n]);
        match std::str::from_utf8(&carry) {
            Ok(_) => carry.clear(),
            Err(e) if e.error_len().is_none() => {
                let keep = carry.split_off(e.valid_up_to());
                carry = keep;
            }
            Err(_) => return Ok((encoding_rs::WINDOWS_1252, 0, "windows-1252".into())),
        }
    }
    if carry.is_empty() {
        Ok((encoding_rs::UTF_8, 0, "UTF-8".into()))
    } else {
        Ok((encoding_rs::WINDOWS_1252, 0, "windows-1252".into()))
    }
}

fn read_up_to(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// Ouvre un fichier brut en le transcodant à la volée en UTF-8.
fn open_decoded(path: &Path, enc: &'static Encoding, skip: u64) -> Res<impl Read> {
    let mut f = File::open(path).map_err(io_err("Ouverture du fichier impossible"))?;
    f.seek(SeekFrom::Start(skip)).map_err(io_err("Erreur de lecture"))?;
    Ok(encoding_rs_io::DecodeReaderBytesBuilder::new()
        .encoding(Some(enc))
        .build(BufReader::with_capacity(1 << 20, f)))
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub delimiter: Option<u8>,
    pub encoding: String,
    pub quote: u8,
    pub has_headers: bool,
    pub trim: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions { delimiter: None, encoding: "auto".into(), quote: b'"', has_headers: true, trim: false }
    }
}

pub struct Imported {
    pub written: Written,
    pub delimiter: u8,
    pub encoding: String,
}

/// Convertit un fichier brut (tout séparateur, tout encodage) en fichier normalisé.
pub fn import(raw: &Path, out: PathBuf, opts: &ImportOptions) -> Res<Imported> {
    let (enc, skip, enc_name) = detect_encoding(raw, &opts.encoding)?;

    let delimiter = match opts.delimiter {
        Some(d) => d,
        None => {
            const SAMPLE: usize = 64 * 1024;
            let mut buf = vec![0u8; SAMPLE];
            let n = read_up_to(&mut open_decoded(raw, enc, skip)?, &mut buf).map_err(io_err("Erreur de lecture"))?;
            buf.truncate(n);
            detect_delimiter(&String::from_utf8_lossy(&buf), n == SAMPLE)
        }
    };

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .quote(opts.quote)
        .has_headers(false)
        .flexible(true)
        .trim(if opts.trim { csv::Trim::All } else { csv::Trim::None })
        .buffer_capacity(256 * 1024)
        .from_reader(open_decoded(raw, enc, skip)?);

    let mut rec = csv::StringRecord::new();
    let read = |rdr: &mut csv::Reader<_>, rec: &mut csv::StringRecord| -> Res<bool> {
        rdr.read_record(rec).map_err(|e| format!("Erreur de lecture CSV : {e}"))
    };

    // Première ligne : en-têtes, ou première ligne de données.
    let mut first_row: Option<Row> = None;
    let headers: Vec<String> = if !read(&mut rdr, &mut rec)? {
        Vec::new()
    } else if opts.has_headers {
        rec.iter().map(str::to_string).collect()
    } else {
        let row: Row = rec.iter().map(str::to_string).collect();
        let h = vec![String::new(); row.len()];
        first_row = Some(row);
        h
    };
    let headers = unique_headers(headers);
    let width = headers.len();

    let mut sink = Sink::create(out, headers)?;
    let mut max_width = width;
    if let Some(row) = first_row {
        sink.write(&row)?;
    }
    while read(&mut rdr, &mut rec)? {
        // Ignore les lignes entièrement vides (ex. « ;;; » en fin de fichier Excel).
        if rec.iter().all(|c| c.is_empty()) {
            continue;
        }
        max_width = max_width.max(rec.len());
        sink.write(&rec.iter().collect::<Vec<_>>())?;
    }
    let mut written = sink.finish()?;

    // Des lignes plus longues que l'en-tête : on élargit l'en-tête et on complète.
    if max_width > width {
        let mut headers = written.headers.clone();
        headers.resize(max_width, String::new());
        let tmp = written.path.with_extension("elargi");
        let mut sink = Sink::create(tmp.clone(), unique_headers(headers))?;
        for row in rows(&written.path)? {
            sink.write(&row?)?;
        }
        let w = sink.finish()?;
        std::fs::rename(&w.path, &written.path).map_err(io_err("Erreur d'écriture"))?;
        written = Written { path: written.path, ..w };
    }
    Ok(Imported { written, delimiter, encoding: enc_name })
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

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct WriteOptions {
    pub delimiter: u8,
    pub quote_all: bool,
    pub crlf: bool,
    /// `utf-8`, `utf-8-bom` ou tout libellé reconnu (ex. `windows-1252`).
    pub encoding: String,
    pub include_headers: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions { delimiter: b',', quote_all: false, crlf: false, encoding: "utf-8".into(), include_headers: true }
    }
}

impl WriteOptions {
    /// Vrai si le fichier normalisé peut être envoyé tel quel.
    pub fn is_native(&self) -> bool {
        self.delimiter == b',' && !self.quote_all && !self.crlf && self.include_headers
            && matches!(self.encoding.to_lowercase().as_str(), "" | "utf-8" | "utf8")
    }
}

/// Transcode à la volée un flux UTF-8 vers un autre encodage.
struct EncodeWriter<W: Write> {
    inner: W,
    encoder: Option<encoding_rs::Encoder>,
    pending: Vec<u8>,
    buf: Vec<u8>,
}

impl<W: Write> EncodeWriter<W> {
    fn encode(&mut self, upto: usize) -> io::Result<()> {
        let Some(encoder) = self.encoder.as_mut() else { return Ok(()) };
        let text = std::str::from_utf8(&self.pending[..upto]).map_err(io::Error::other)?;
        let mut src = text;
        while !src.is_empty() {
            let need = encoder.max_buffer_length_from_utf8_without_replacement(src.len()).unwrap_or(4096).max(16);
            self.buf.resize(need, 0);
            let (res, read, written) = encoder.encode_from_utf8_without_replacement(src, &mut self.buf, false);
            self.inner.write_all(&self.buf[..written])?;
            src = &src[read..];
            if let encoding_rs::EncoderResult::Unmappable(c) = res {
                return Err(io::Error::other(format!(
                    "le caractère « {c} » ne peut pas être représenté en {} : choisissez UTF-8",
                    encoder.encoding().name()
                )));
            }
        }
        self.pending.drain(..upto);
        Ok(())
    }
}

impl<W: Write> Write for EncodeWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.encoder.is_none() {
            return self.inner.write(data);
        }
        self.pending.extend_from_slice(data);
        let upto = match std::str::from_utf8(&self.pending) {
            Ok(_) => self.pending.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(e) => return Err(io::Error::other(e)),
        };
        self.encode(upto)?;
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Exporte un fichier normalisé vers `out` selon les options données.
pub fn export(src: &Path, opts: &WriteOptions, out: impl Write) -> Res<()> {
    let label = opts.encoding.trim().to_lowercase();
    let mut out = out;
    let encoder = match label.as_str() {
        "" | "utf-8" | "utf8" => None,
        "utf-8-bom" | "utf8-bom" => {
            out.write_all(b"\xEF\xBB\xBF").map_err(io_err("Erreur d'écriture"))?;
            None
        }
        _ => {
            let enc = Encoding::for_label(label.as_bytes()).ok_or_else(|| format!("Encodage inconnu : « {label} »"))?;
            (enc != encoding_rs::UTF_8).then(|| enc.new_encoder())
        }
    };
    let enc_writer = EncodeWriter { inner: out, encoder, pending: Vec::new(), buf: Vec::new() };
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(opts.delimiter)
        .flexible(true)
        .quote_style(if opts.quote_all { csv::QuoteStyle::Always } else { csv::QuoteStyle::Necessary })
        .terminator(if opts.crlf { csv::Terminator::CRLF } else { csv::Terminator::Any(b'\n') })
        .buffer_capacity(256 * 1024)
        .from_writer(enc_writer);
    let err = io_err("Erreur d'export");
    let mut rdr = reader(src)?;
    if opts.include_headers {
        wtr.write_byte_record(rdr.byte_headers().map_err(&err)?).map_err(&err)?;
    }
    let mut rec = csv::ByteRecord::new();
    while rdr.read_byte_record(&mut rec).map_err(&err)? {
        wtr.write_byte_record(&rec).map_err(&err)?;
    }
    wtr.flush().map_err(io_err("Erreur d'export"))?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Répertoire temporaire propre à un test.
    pub fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gestion-csv-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn import_bytes(dir: &Path, bytes: &[u8], opts: &ImportOptions) -> Imported {
        let raw = dir.join("brut");
        std::fs::write(&raw, bytes).unwrap();
        import(&raw, dir.join("norm.csv"), opts).unwrap()
    }

    fn all_rows(p: &Path) -> Vec<Row> {
        rows(p).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn detecte_separateurs() {
        assert_eq!(detect_delimiter("nom;prénom\nDupont;\"Jean, Paul\"\n", false), b';');
        assert_eq!(detect_delimiter("a\tb\tc\n1\t2\t3\n", false), b'\t');
        assert_eq!(detect_delimiter("a,b\n1,2\n", false), b',');
        assert_eq!(detect_delimiter("a|b|c\n1|2|3\n", false), b'|');
    }

    #[test]
    fn import_latin1_point_virgule_lignes_irregulieres() {
        let d = tmpdir("import");
        let im = import_bytes(&d, b"ville;\xe9t\xe9;ville\nGen\xe8ve;chaud\n;;\nLyon;doux;x;y\n", &ImportOptions::default());
        assert_eq!(im.delimiter, b';');
        assert_eq!(im.encoding, "windows-1252");
        assert_eq!(im.written.headers, vec!["ville", "été", "ville_2", "Colonne 4"]);
        assert_eq!(all_rows(&im.written.path), vec![
            vec!["Genève", "chaud", "", ""],
            vec!["Lyon", "doux", "x", "y"],
        ]);
        assert_eq!(im.written.rows, 2);
    }

    #[test]
    fn import_bom_sans_entetes() {
        let d = tmpdir("bom");
        let opts = ImportOptions { has_headers: false, ..Default::default() };
        let im = import_bytes(&d, b"\xEF\xBB\xBF1,2\n3,4\n", &opts);
        assert_eq!(im.written.headers, vec!["Colonne 1", "Colonne 2"]);
        assert_eq!(im.written.rows, 2);
        assert!(im.encoding.contains("BOM"));
    }

    #[test]
    fn export_encodage_et_separateur() {
        let d = tmpdir("export");
        let im = import_bytes(&d, "x,y\n\"a;b\",été\n".as_bytes(), &ImportOptions::default());
        let mut out = Vec::new();
        let opts = WriteOptions { delimiter: b';', encoding: "windows-1252".into(), ..Default::default() };
        export(&im.written.path, &opts, &mut out).unwrap();
        assert_eq!(out, b"x;y\n\"a;b\";\xe9t\xe9\n");
        let opts = WriteOptions { encoding: "windows-1252".into(), ..Default::default() };
        let tbl = import_bytes(&d, "x\n€ ✓\n".as_bytes(), &ImportOptions::default());
        assert!(export(&tbl.written.path, &opts, Vec::new()).is_err());
    }

    #[test]
    fn pagination() {
        let d = tmpdir("page");
        let mut text = String::from("n\n");
        for i in 0..2500 {
            text.push_str(&format!("{i}\n"));
        }
        let im = import_bytes(&d, text.as_bytes(), &ImportOptions::default());
        let idx = build_index(&im.written.path, 1000).unwrap();
        let page = read_page(&im.written.path, &idx[2], 5, 3).unwrap();
        assert_eq!(page, vec![vec!["2005"], vec!["2006"], vec!["2007"]]);
    }

    #[test]
    fn separateurs_utilisateur() {
        assert_eq!(parse_delimiter("auto").unwrap(), None);
        assert_eq!(parse_delimiter("tab").unwrap(), Some(b'\t'));
        assert_eq!(parse_delimiter(";").unwrap(), Some(b';'));
        assert!(parse_delimiter(";;").is_err());
    }
}
