//! Routes HTTP : interface Web statique et API JSON.

use std::io::Write as _;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::csvdata::{self, ReadOptions, Table, WriteOptions};
use crate::ops::{self, Combine, Condition, JoinKind, MergeMode};
use crate::store::{AppState, Entry, FileInfo};

const MAX_SPLIT_GROUPS: usize = 2000;

// ---------------------------------------------------------------------------
// Gestion des erreurs
// ---------------------------------------------------------------------------

pub struct AppError(StatusCode, String);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<String> for AppError {
    fn from(msg: String) -> Self {
        AppError(StatusCode::BAD_REQUEST, msg)
    }
}

fn bad(msg: impl Into<String>) -> AppError {
    AppError(StatusCode::BAD_REQUEST, msg.into())
}

fn not_found(id: &str) -> AppError {
    AppError(StatusCode::NOT_FOUND, format!("Fichier introuvable : {id}"))
}

type ApiResult<T> = Result<T, AppError>;

/// Exécute un traitement potentiellement long hors du thread asynchrone.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("Erreur interne : {e}")))?
        .map_err(AppError::from)
}

fn entry(st: &AppState, id: &str) -> ApiResult<Entry> {
    st.get(id).ok_or_else(|| not_found(id))
}

// ---------------------------------------------------------------------------
// Routeur
// ---------------------------------------------------------------------------

pub fn router(state: AppState, max_body: usize) -> Router {
    Router::new()
        .route("/", get(|| async { Html(include_str!("../static/index.html")) }))
        .route("/app.js", get(|| async { static_file("text/javascript", include_str!("../static/app.js")) }))
        .route("/style.css", get(|| async { static_file("text/css", include_str!("../static/style.css")) }))
        .route("/api/files", get(list_files).post(upload).delete(clear_files))
        .route("/api/files/{id}", get(file_info).patch(rename_file).delete(delete_file))
        .route("/api/files/{id}/rows", get(file_rows))
        .route("/api/files/{id}/values", get(column_values))
        .route("/api/files/{id}/download", get(download))
        .route("/api/zip", get(download_zip))
        .route("/api/merge", post(merge))
        .route("/api/split", post(split))
        .route("/api/join", post(join))
        .route("/api/filter", post(filter))
        .layer(DefaultBodyLimit::max(max_body))
        .with_state(state)
}

fn static_file(content_type: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

// ---------------------------------------------------------------------------
// Gestion des fichiers
// ---------------------------------------------------------------------------

async fn list_files(State(st): State<AppState>) -> Json<Vec<FileInfo>> {
    Json(st.list().iter().map(FileInfo::from).collect())
}

async fn file_info(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<FileInfo>> {
    Ok(Json(FileInfo::from(&entry(&st, &id)?)))
}

async fn delete_file(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    if st.remove(&id) { Ok(StatusCode::NO_CONTENT) } else { Err(not_found(&id)) }
}

async fn clear_files(State(st): State<AppState>) -> StatusCode {
    st.clear();
    StatusCode::NO_CONTENT
}

#[derive(Deserialize)]
struct RenameReq {
    name: String,
}

async fn rename_file(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RenameReq>,
) -> ApiResult<Json<FileInfo>> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(bad("Le nom ne peut pas être vide"));
    }
    let e = st.rename(&id, name.to_string()).ok_or_else(|| not_found(&id))?;
    Ok(Json(FileInfo::from(&e)))
}

/// Import d'un ou plusieurs fichiers (multipart). Champs reconnus :
/// `file` (répétable), `delimiter`, `encoding`, `has_headers`, `trim`, `quote`.
async fn upload(State(st): State<AppState>, mut mp: Multipart) -> ApiResult<Json<Vec<FileInfo>>> {
    let mut delimiter = String::from("auto");
    let mut encoding = String::from("auto");
    let mut quote = String::from("\"");
    let mut has_headers = true;
    let mut trim = false;
    let mut files: Vec<(String, axum::body::Bytes)> = Vec::new();

    let mp_err = |e: axum::extract::multipart::MultipartError| bad(format!("Erreur lors de l'envoi : {e}"));
    while let Some(field) = mp.next_field().await.map_err(mp_err)? {
        let field_name = field.name().unwrap_or_default().to_string();
        match field_name.as_str() {
            "file" => {
                let fname = field.file_name().unwrap_or("fichier.csv").to_string();
                files.push((fname, field.bytes().await.map_err(mp_err)?));
            }
            "delimiter" => delimiter = field.text().await.map_err(mp_err)?,
            "encoding" => encoding = field.text().await.map_err(mp_err)?,
            "quote" => quote = field.text().await.map_err(mp_err)?,
            "has_headers" => has_headers = is_true(&field.text().await.map_err(mp_err)?),
            "trim" => trim = is_true(&field.text().await.map_err(mp_err)?),
            _ => {}
        }
    }
    if files.is_empty() {
        return Err(bad("Aucun fichier reçu"));
    }
    let forced = csvdata::parse_delimiter(&delimiter)?;
    let quote = match quote.as_bytes() {
        [q] => *q,
        [] => b'"',
        _ => return Err(bad("Le caractère de citation doit être un seul caractère")),
    };

    let mut out = Vec::new();
    for (fname, data) in files {
        let enc = encoding.clone();
        let (table, delim, enc_name) = blocking(move || {
            let (text, enc_name) = csvdata::decode(&data, &enc)?;
            let delim = forced.unwrap_or_else(|| csvdata::detect_delimiter(&text));
            let opts = ReadOptions { delimiter: delim, quote, has_headers, trim };
            Ok((csvdata::parse(&text, &opts)?, delim, enc_name))
        })
        .await
        .map_err(|e| bad(format!("{fname} : {}", e.1)))?;
        let e = st.insert(fname, "import", delim, enc_name, table);
        out.push(FileInfo::from(&e));
    }
    Ok(Json(out))
}

fn is_true(s: &str) -> bool {
    matches!(s.trim().to_lowercase().as_str(), "1" | "true" | "on" | "yes" | "oui")
}

#[derive(Deserialize)]
struct RowsQuery {
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct RowsResp {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    offset: usize,
    total: usize,
}

async fn file_rows(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<RowsQuery>,
) -> ApiResult<Json<RowsResp>> {
    let e = entry(&st, &id)?;
    let limit = q.limit.unwrap_or(100).min(5000);
    let t = &e.table;
    let start = q.offset.min(t.rows.len());
    let end = (start + limit).min(t.rows.len());
    Ok(Json(RowsResp {
        headers: t.headers.clone(),
        rows: t.rows[start..end].to_vec(),
        offset: start,
        total: t.rows.len(),
    }))
}

#[derive(Deserialize)]
struct ValuesQuery {
    column: String,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct ValuesResp {
    values: Vec<(String, usize)>,
    distinct: usize,
}

async fn column_values(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ValuesQuery>,
) -> ApiResult<Json<ValuesResp>> {
    let e = entry(&st, &id)?;
    let col = e.table.col_index(&q.column)?;
    let limit = q.limit.unwrap_or(1000);
    let mut values = blocking(move || Ok(ops::distinct_values(&e.table, col))).await?;
    let distinct = values.len();
    values.truncate(limit);
    Ok(Json(ValuesResp { values, distinct }))
}

// ---------------------------------------------------------------------------
// Téléchargements
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct ExportQuery {
    /// Séparateur de sortie ; vide ou « source » = celui du fichier.
    delimiter: Option<String>,
    encoding: Option<String>,
    /// `necessary` (défaut) ou `always`.
    quote: Option<String>,
    /// `lf` (défaut) ou `crlf`.
    line_ending: Option<String>,
    headers: Option<bool>,
}

impl ExportQuery {
    fn options(&self, source_delimiter: u8) -> Result<WriteOptions, String> {
        let delimiter = match self.delimiter.as_deref() {
            None | Some("") | Some("source") => source_delimiter,
            Some(d) => csvdata::parse_delimiter(d)?.unwrap_or(source_delimiter),
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

fn csv_filename(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.ends_with(".csv") || lower.ends_with(".tsv") || lower.ends_with(".txt") {
        name.to_string()
    } else {
        format!("{name}.csv")
    }
}

/// En-tête Content-Disposition compatible avec les noms accentués.
fn attachment(filename: &str) -> String {
    let ascii: String = filename
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || "._- ".contains(c) { c } else { '_' })
        .collect();
    let mut encoded = String::new();
    for b in filename.bytes() {
        if b.is_ascii_alphanumeric() || b"._-".contains(&b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

async fn download(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ExportQuery>,
) -> ApiResult<Response> {
    let e = entry(&st, &id)?;
    let opts = q.options(e.delimiter)?;
    let charset = if opts.encoding.starts_with("utf") { "utf-8".to_string() } else { opts.encoding.clone() };
    let table = e.table.clone();
    let bytes = blocking(move || csvdata::write(&table, &opts)).await?;
    Ok((
        [
            (header::CONTENT_TYPE, format!("text/csv; charset={charset}")),
            (header::CONTENT_DISPOSITION, attachment(&csv_filename(&e.name))),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Deserialize)]
struct ZipQuery {
    ids: String,
    name: Option<String>,
    #[serde(flatten)]
    export: ExportQuery,
}

async fn download_zip(State(st): State<AppState>, Query(q): Query<ZipQuery>) -> ApiResult<Response> {
    let entries: Vec<Entry> = q
        .ids
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|id| entry(&st, id))
        .collect::<Result<_, _>>()?;
    if entries.is_empty() {
        return Err(bad("Aucun fichier à archiver"));
    }
    let export = q.export;
    let bytes = blocking(move || {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let mut used = std::collections::HashSet::new();
        for e in &entries {
            let data = csvdata::write(&e.table, &export.options(e.delimiter)?)?;
            let base = csv_filename(&e.name).replace(['/', '\\'], "_");
            let mut fname = base.clone();
            let mut n = 2;
            while !used.insert(fname.clone()) {
                fname = format!("{n}_{base}");
                n += 1;
            }
            zip.start_file(fname, options).map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
        Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
    })
    .await?;
    let name = q.name.unwrap_or_else(|| "fichiers.zip".into());
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_DISPOSITION, attachment(&name)),
        ],
        bytes,
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Opérations
// ---------------------------------------------------------------------------

fn stem(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

fn output_name(custom: Option<String>, default: String) -> String {
    match custom {
        Some(n) if !n.trim().is_empty() => csv_filename(n.trim()),
        _ => default,
    }
}

#[derive(Deserialize)]
struct MergeReq {
    ids: Vec<String>,
    #[serde(default)]
    mode: MergeMode,
    source_column: Option<String>,
    #[serde(default)]
    dedupe: bool,
    name: Option<String>,
}

async fn merge(State(st): State<AppState>, Json(req): Json<MergeReq>) -> ApiResult<Json<FileInfo>> {
    let entries: Vec<Entry> = req.ids.iter().map(|id| entry(&st, id)).collect::<Result<_, _>>()?;
    let default_name = format!(
        "fusion_{}.csv",
        entries.iter().map(|e| stem(&e.name)).collect::<Vec<_>>().join("_")
    );
    let delimiter = entries.first().map(|e| e.delimiter).unwrap_or(b',');
    let source_column = req.source_column.clone();
    let (mode, dedupe) = (req.mode, req.dedupe);
    let table = blocking(move || {
        let inputs: Vec<(&str, &Table)> = entries.iter().map(|e| (e.name.as_str(), &*e.table)).collect();
        let opts = ops::MergeOptions { mode, source_column: source_column.as_deref(), dedupe };
        ops::merge(&inputs, &opts)
    })
    .await?;
    let e = st.insert(output_name(req.name, default_name), "fusion", delimiter, "UTF-8".into(), table);
    Ok(Json(FileInfo::from(&e)))
}

#[derive(Deserialize)]
struct SplitReq {
    id: String,
    columns: Vec<String>,
    #[serde(default)]
    remove_columns: bool,
}

#[derive(Serialize)]
struct SplitResp {
    files: Vec<FileInfo>,
}

fn sanitize_part(v: &str) -> String {
    let s: String = v
        .trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || "-_.".contains(c) { c } else { '_' })
        .take(60)
        .collect();
    if s.trim_matches('_').is_empty() { "vide".into() } else { s }
}

async fn split(State(st): State<AppState>, Json(req): Json<SplitReq>) -> ApiResult<Json<SplitResp>> {
    let e = entry(&st, &req.id)?;
    let cols = e.table.col_indices(&req.columns)?;
    let table = e.table.clone();
    let remove = req.remove_columns;
    let groups = blocking(move || ops::split(&table, &cols, remove, MAX_SPLIT_GROUPS)).await?;
    let base = stem(&e.name).to_string();
    let files = groups
        .into_iter()
        .map(|(key, t)| {
            let suffix: Vec<String> = key.iter().map(|v| sanitize_part(v)).collect();
            let name = format!("{base}_{}.csv", suffix.join("_"));
            FileInfo::from(&st.insert(name, "découpage", e.delimiter, "UTF-8".into(), t))
        })
        .collect();
    Ok(Json(SplitResp { files }))
}

#[derive(Deserialize)]
struct JoinReq {
    left_id: String,
    right_id: String,
    left_columns: Vec<String>,
    right_columns: Vec<String>,
    #[serde(default)]
    kind: JoinKind,
    #[serde(default)]
    case_insensitive: bool,
    #[serde(default)]
    trim: bool,
    right_suffix: Option<String>,
    name: Option<String>,
}

async fn join(State(st): State<AppState>, Json(req): Json<JoinReq>) -> ApiResult<Json<FileInfo>> {
    let l = entry(&st, &req.left_id)?;
    let r = entry(&st, &req.right_id)?;
    let lk = l.table.col_indices(&req.left_columns)?;
    let rk = r.table.col_indices(&req.right_columns)?;
    let opts = ops::JoinOptions {
        kind: req.kind,
        case_insensitive: req.case_insensitive,
        trim: req.trim,
        right_suffix: req.right_suffix.filter(|s| !s.is_empty()).unwrap_or_else(|| "_2".into()),
    };
    let default_name = format!("jointure_{}_{}.csv", stem(&l.name), stem(&r.name));
    let delimiter = l.delimiter;
    let (lt, rt) = (l.table.clone(), r.table.clone());
    let table = blocking(move || ops::join(&lt, &rt, &lk, &rk, &opts)).await?;
    let e = st.insert(output_name(req.name, default_name), "jointure", delimiter, "UTF-8".into(), table);
    Ok(Json(FileInfo::from(&e)))
}

#[derive(Deserialize)]
struct FilterReq {
    id: String,
    conditions: Vec<Condition>,
    #[serde(default)]
    combine: Combine,
    name: Option<String>,
}

async fn filter(State(st): State<AppState>, Json(req): Json<FilterReq>) -> ApiResult<Json<FileInfo>> {
    let e = entry(&st, &req.id)?;
    let table = e.table.clone();
    let (conditions, combine) = (req.conditions, req.combine);
    let result = blocking(move || ops::filter(&table, &conditions, combine)).await?;
    let default_name = format!("{}_filtre.csv", stem(&e.name));
    let out = st.insert(output_name(req.name, default_name), "filtre", e.delimiter, "UTF-8".into(), result);
    Ok(Json(FileInfo::from(&out)))
}
