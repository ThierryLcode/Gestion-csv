//! Routes HTTP : interface Web statique et API JSON.

use std::io::{BufWriter, Write as _};
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::AsyncWriteExt;

use crate::csvio::{self, ImportOptions, Res, WriteOptions, Written};
use crate::multi::{self, CompareOptions, CompareSummary, Input, JoinKind, MergeMode};
use crate::profile::{self, Profile};
use crate::sql::SqlTable;
use crate::steps::{self, Step};
use crate::store::{AppState, Entry, FileInfo, NewFile, Recipe, INDEX_STEP};

const MAX_SPLIT_GROUPS: usize = 5000;

// ---------------------------------------------------------------------------
// Erreurs
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

fn not_found(what: &str, id: &str) -> AppError {
    AppError(StatusCode::NOT_FOUND, format!("{what} introuvable : {id}"))
}

type ApiResult<T> = Result<T, AppError>;

/// Exécute un traitement long hors du thread asynchrone.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("Erreur interne : {e}")))?
        .map_err(AppError::from)
}

fn entry(st: &AppState, id: &str) -> ApiResult<Arc<Entry>> {
    st.get(id).ok_or_else(|| not_found("Fichier", id))
}

fn info(e: &Entry) -> Json<FileInfo> {
    Json(FileInfo::from(e))
}

// ---------------------------------------------------------------------------
// Routeur
// ---------------------------------------------------------------------------

pub fn router(state: AppState, max_body: Option<usize>) -> Router {
    let limit = match max_body {
        Some(n) => DefaultBodyLimit::max(n),
        None => DefaultBodyLimit::disable(),
    };
    Router::new()
        .route("/", get(|| async { Html(include_str!("../static/index.html")) }))
        .route("/app.js", get(|| async { static_file("text/javascript", include_str!("../static/app.js")) }))
        .route("/style.css", get(|| async { static_file("text/css", include_str!("../static/style.css")) }))
        .route("/api/files", get(list_files).post(upload).delete(clear_files))
        .route("/api/files/{id}", get(file_info).patch(rename_file).delete(delete_file))
        .route("/api/files/{id}/rows", get(file_rows))
        .route("/api/files/{id}/values", get(column_values))
        .route("/api/files/{id}/profile", get(file_profile))
        .route("/api/files/{id}/download", get(download))
        .route("/api/zip", get(download_zip))
        .route("/api/step", post(apply_step))
        .route("/api/merge", post(merge))
        .route("/api/split", post(split))
        .route("/api/join", post(join))
        .route("/api/compare", post(compare))
        .route("/api/sql", post(sql_query))
        .route("/api/recipes", get(list_recipes).post(create_recipe))
        .route("/api/recipes/{id}", get(get_recipe).delete(delete_recipe))
        .route("/api/recipes/{id}/apply", post(apply_recipe))
        .layer(limit)
        .with_state(state)
}

fn static_file(content_type: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

// ---------------------------------------------------------------------------
// Noms
// ---------------------------------------------------------------------------

fn stem(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
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

fn output_name(custom: Option<String>, default: String) -> String {
    match custom {
        Some(n) if !n.trim().is_empty() => csv_filename(n.trim()),
        _ => default,
    }
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

/// En-tête Content-Disposition compatible avec les noms accentués.
fn attachment(filename: &str) -> String {
    let ascii: String =
        filename.chars().map(|c| if c.is_ascii_alphanumeric() || "._- ".contains(c) { c } else { '_' }).collect();
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

// ---------------------------------------------------------------------------
// Fichiers
// ---------------------------------------------------------------------------

async fn list_files(State(st): State<AppState>) -> Json<Vec<FileInfo>> {
    Json(st.list().iter().map(|e| FileInfo::from(&**e)).collect())
}

async fn file_info(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<FileInfo>> {
    let e = entry(&st, &id)?;
    Ok(info(&e))
}

async fn delete_file(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let st2 = st.clone();
    if blocking(move || Ok(st2.remove(&id))).await? { Ok(StatusCode::NO_CONTENT) } else { Err(not_found("Fichier", "")) }
}

async fn clear_files(State(st): State<AppState>) -> ApiResult<StatusCode> {
    blocking(move || {
        st.clear();
        Ok(())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
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
    let e = st.rename(&id, name.to_string()).ok_or_else(|| not_found("Fichier", &id))?;
    Ok(info(&e))
}

fn is_true(s: &str) -> bool {
    matches!(s.trim().to_lowercase().as_str(), "1" | "true" | "on" | "yes" | "oui")
}

/// Import d'un ou plusieurs fichiers (multipart). Le contenu est écrit sur disque
/// au fil de la réception, puis converti en fichier normalisé.
/// Champs : `file` (répétable), `delimiter`, `encoding`, `has_headers`, `trim`, `quote`.
async fn upload(State(st): State<AppState>, mut mp: Multipart) -> ApiResult<Json<Vec<FileInfo>>> {
    let mut opts = ImportOptions::default();
    let mut delimiter = String::from("auto");
    let mut raws: Vec<(String, PathBuf)> = Vec::new();

    let cleanup = |raws: &[(String, PathBuf)]| raws.iter().for_each(|(_, p)| drop(std::fs::remove_file(p)));
    let mp_err = |e: axum::extract::multipart::MultipartError| bad(format!("Erreur lors de l'envoi : {e}"));

    let result: ApiResult<()> = async {
        while let Some(mut field) = mp.next_field().await.map_err(mp_err)? {
            match field.name().unwrap_or_default() {
                "file" => {
                    let fname = field.file_name().unwrap_or("fichier.csv").to_string();
                    let path = st.scratch.path("brut");
                    raws.push((fname, path.clone()));
                    let mut file = tokio::fs::File::create(&path).await.map_err(|e| bad(e.to_string()))?;
                    while let Some(chunk) = field.chunk().await.map_err(mp_err)? {
                        file.write_all(&chunk).await.map_err(|e| bad(format!("Écriture impossible : {e}")))?;
                    }
                    file.flush().await.map_err(|e| bad(e.to_string()))?;
                }
                "delimiter" => delimiter = field.text().await.map_err(mp_err)?,
                "encoding" => opts.encoding = field.text().await.map_err(mp_err)?,
                "quote" => {
                    let q = field.text().await.map_err(mp_err)?;
                    opts.quote = match q.as_bytes() {
                        [q] => *q,
                        [] => b'"',
                        _ => return Err(bad("Le caractère de citation doit être un seul caractère")),
                    }
                }
                "has_headers" => opts.has_headers = is_true(&field.text().await.map_err(mp_err)?),
                "trim" => opts.trim = is_true(&field.text().await.map_err(mp_err)?),
                _ => {}
            }
        }
        Ok(())
    }
    .await;
    if let Err(e) = result {
        cleanup(&raws);
        return Err(e);
    }
    if raws.is_empty() {
        return Err(bad("Aucun fichier reçu"));
    }
    opts.delimiter = match csvio::parse_delimiter(&delimiter) {
        Ok(d) => d,
        Err(e) => {
            cleanup(&raws);
            return Err(bad(e));
        }
    };

    let mut out = Vec::new();
    for (i, (fname, raw)) in raws.iter().enumerate() {
        let (o, raw2, dest) = (opts.clone(), raw.clone(), st.scratch.path("csv"));
        let res = blocking(move || {
            let r = csvio::import(&raw2, dest.clone(), &o);
            let _ = std::fs::remove_file(&raw2);
            if r.is_err() {
                let _ = std::fs::remove_file(&dest);
            }
            r
        })
        .await;
        let imported = match res {
            Ok(im) => im,
            Err(e) => {
                cleanup(&raws[i + 1..]);
                return Err(bad(format!("{fname} : {}", e.1)));
            }
        };
        let e = st.add(
            imported.written,
            NewFile { name: fname.clone(), origin: "import".into(), delimiter: imported.delimiter, encoding: imported.encoding, steps: vec![] },
        );
        out.push(FileInfo::from(&*e));
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
struct RowsQuery {
    #[serde(default)]
    offset: u64,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct RowsResp {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    offset: u64,
    total: u64,
}

async fn file_rows(State(st): State<AppState>, Path(id): Path<String>, Query(q): Query<RowsQuery>) -> ApiResult<Json<RowsResp>> {
    let e = entry(&st, &id)?;
    let limit = q.limit.unwrap_or(100).min(5000);
    let offset = q.offset.min(e.rows);
    blocking(move || {
        let idx = e.index()?;
        let block = (offset / INDEX_STEP) as usize;
        let rows = match idx.get(block) {
            Some(pos) => csvio::read_page(&e.path, pos, offset % INDEX_STEP, limit)?,
            None => Vec::new(),
        };
        Ok(Json(RowsResp { headers: e.headers.clone(), rows, offset, total: e.rows }))
    })
    .await
}

#[derive(Deserialize)]
struct ValuesQuery {
    column: String,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct ValuesResp {
    values: Vec<(String, u64)>,
    distinct: usize,
    capped: bool,
}

async fn column_values(State(st): State<AppState>, Path(id): Path<String>, Query(q): Query<ValuesQuery>) -> ApiResult<Json<ValuesResp>> {
    let e = entry(&st, &id)?;
    let col = steps::col_index(&e.headers, &q.column)?;
    let limit = q.limit.unwrap_or(1000);
    let (mut values, distinct, capped) = blocking(move || profile::distinct_values(&e.path, col, 1_000_000)).await?;
    values.truncate(limit);
    Ok(Json(ValuesResp { values, distinct, capped }))
}

async fn file_profile(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Profile>> {
    let e = entry(&st, &id)?;
    Ok(Json(blocking(move || profile::profile(&e.path, &e.headers)).await?))
}

// ---------------------------------------------------------------------------
// Téléchargements (en flux)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct ExportQuery {
    delimiter: Option<String>,
    encoding: Option<String>,
    quote: Option<String>,
    line_ending: Option<String>,
    headers: Option<bool>,
}

impl ExportQuery {
    fn options(&self, source_delimiter: u8) -> Res<WriteOptions> {
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

/// Envoie un fichier disque en flux ; s'il est temporaire, il est supprimé dès son ouverture.
async fn stream_file(path: PathBuf, temporary: bool, content_type: String, filename: &str) -> ApiResult<Response> {
    let file = tokio::fs::File::open(&path).await.map_err(|e| bad(format!("Lecture impossible : {e}")))?;
    let len = file.metadata().await.map(|m| m.len()).ok();
    if temporary {
        let _ = tokio::fs::remove_file(&path).await;
    }
    let body = Body::from_stream(tokio_util::io::ReaderStream::with_capacity(file, 256 * 1024));
    let mut resp = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, attachment(filename));
    if let Some(len) = len {
        resp = resp.header(header::CONTENT_LENGTH, len);
    }
    resp.body(body).map_err(|e| bad(e.to_string()))
}

async fn download(State(st): State<AppState>, Path(id): Path<String>, Query(q): Query<ExportQuery>) -> ApiResult<Response> {
    let e = entry(&st, &id)?;
    let opts = q.options(e.delimiter)?;
    let charset = if opts.encoding.to_lowercase().starts_with("utf") { "utf-8".to_string() } else { opts.encoding.clone() };
    let filename = csv_filename(&e.name());
    let content_type = format!("text/csv; charset={charset}");
    if opts.is_native() {
        // Format de stockage identique : envoi direct, sans conversion.
        return stream_file(e.path.clone(), false, content_type, &filename).await;
    }
    let tmp = st.scratch.path("export");
    let tmp2 = tmp.clone();
    blocking(move || {
        let f = std::fs::File::create(&tmp2).map_err(|e| e.to_string())?;
        let mut w = BufWriter::with_capacity(256 * 1024, f);
        let r = csvio::export(&e.path, &opts, &mut w).and_then(|_| w.flush().map_err(|e| e.to_string()));
        if r.is_err() {
            drop(w);
            let _ = std::fs::remove_file(&tmp2);
        }
        r
    })
    .await?;
    stream_file(tmp, true, content_type, &filename).await
}

#[derive(Deserialize)]
struct ZipQuery {
    ids: String,
    name: Option<String>,
    #[serde(flatten)]
    export: ExportQuery,
}

async fn download_zip(State(st): State<AppState>, Query(q): Query<ZipQuery>) -> ApiResult<Response> {
    let entries: Vec<Arc<Entry>> =
        q.ids.split(',').filter(|s| !s.is_empty()).map(|id| entry(&st, id)).collect::<Result<_, _>>()?;
    if entries.is_empty() {
        return Err(bad("Aucun fichier à archiver"));
    }
    let export = q.export;
    let tmp = st.scratch.path("zip");
    let tmp2 = tmp.clone();
    blocking(move || {
        let r = (|| {
            let f = std::fs::File::create(&tmp2).map_err(|e| e.to_string())?;
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
                csvio::export(&e.path, &export.options(e.delimiter)?, &mut zip)?;
            }
            zip.finish().map_err(|e| e.to_string())?.flush().map_err(|e| e.to_string())
        })();
        if r.is_err() {
            let _ = std::fs::remove_file(&tmp2);
        }
        r
    })
    .await?;
    let name = q.name.unwrap_or_else(|| "fichiers.zip".into());
    stream_file(tmp, true, "application/zip".into(), &name).await
}

// ---------------------------------------------------------------------------
// Opérations sur un fichier (étapes de recette)
// ---------------------------------------------------------------------------

fn step_suffix(step: &Step) -> &'static str {
    match step {
        Step::Columns { .. } => "colonnes",
        Step::Sort { .. } => "trie",
        Step::Dedupe { mode, .. } => match mode {
            steps::DedupeMode::Remove => "dedoublonne",
            steps::DedupeMode::Duplicates => "doublons",
            steps::DedupeMode::Removed => "doublons_ecartes",
            steps::DedupeMode::Singles => "uniques",
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

#[derive(Deserialize)]
struct StepReq {
    id: String,
    step: Step,
    name: Option<String>,
}

async fn apply_step(State(st): State<AppState>, Json(req): Json<StepReq>) -> ApiResult<Json<FileInfo>> {
    let e = entry(&st, &req.id)?;
    let (e2, step, scratch) = (e.clone(), req.step.clone(), st.scratch.clone());
    let w = blocking(move || steps::run_step(&e2.path, &e2.headers, &step, &scratch)).await?;
    let mut lineage = e.steps.clone();
    lineage.push(req.step.clone());
    let name = output_name(req.name, format!("{}_{}.csv", stem(&e.name()), step_suffix(&req.step)));
    let out = st.add(w, NewFile { name, origin: step_origin(&req.step).into(), delimiter: e.delimiter, encoding: "UTF-8".into(), steps: lineage });
    Ok(info(&out))
}

// ---------------------------------------------------------------------------
// Opérations multi-fichiers
// ---------------------------------------------------------------------------

fn new_source(name: String, origin: &str, delimiter: u8) -> NewFile {
    NewFile { name, origin: origin.into(), delimiter, encoding: "UTF-8".into(), steps: vec![] }
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
    let entries: Vec<Arc<Entry>> = req.ids.iter().map(|id| entry(&st, id)).collect::<Result<_, _>>()?;
    let default_name = format!("fusion_{}.csv", entries.iter().map(|e| stem(&e.name()).to_string()).collect::<Vec<_>>().join("_"));
    let delimiter = entries.first().map(|e| e.delimiter).unwrap_or(b',');
    let (source_column, mode, dedupe, scratch) = (req.source_column.clone(), req.mode, req.dedupe, st.scratch.clone());
    let w = blocking(move || {
        let names: Vec<String> = entries.iter().map(|e| e.name()).collect();
        let inputs: Vec<Input> =
            entries.iter().zip(&names).map(|(e, n)| Input { name: n, path: &e.path, headers: &e.headers }).collect();
        multi::merge(&inputs, &multi::MergeOptions { mode, source_column: source_column.as_deref(), dedupe }, &scratch)
    })
    .await?;
    Ok(info(&st.add(w, new_source(output_name(req.name, default_name), "fusion", delimiter))))
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

async fn split(State(st): State<AppState>, Json(req): Json<SplitReq>) -> ApiResult<Json<SplitResp>> {
    let e = entry(&st, &req.id)?;
    let cols = steps::col_indices(&e.headers, &req.columns)?;
    let (e2, remove, scratch) = (e.clone(), req.remove_columns, st.scratch.clone());
    let groups = blocking(move || multi::split(&e2.path, &e2.headers, &cols, remove, MAX_SPLIT_GROUPS, &scratch)).await?;
    let base = stem(&e.name()).to_string();
    let files = groups
        .into_iter()
        .map(|(key, w): (Vec<String>, Written)| {
            let suffix: Vec<String> = key.iter().map(|v| sanitize_part(v)).collect();
            let name = format!("{base}_{}.csv", suffix.join("_"));
            FileInfo::from(&*st.add(w, new_source(name, "découpage", e.delimiter)))
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
    let opts = multi::JoinOptions {
        kind: req.kind,
        case_insensitive: req.case_insensitive,
        trim: req.trim,
        right_suffix: req.right_suffix.filter(|s| !s.is_empty()).unwrap_or_else(|| "_2".into()),
    };
    let default_name = format!("jointure_{}_{}.csv", stem(&l.name()), stem(&r.name()));
    let delimiter = l.delimiter;
    let (lk, rk, scratch) = (req.left_columns, req.right_columns, st.scratch.clone());
    let w = blocking(move || {
        let (ln, rn) = (l.name(), r.name());
        let li = Input { name: &ln, path: &l.path, headers: &l.headers };
        let ri = Input { name: &rn, path: &r.path, headers: &r.headers };
        multi::join(&li, &ri, &lk, &rk, &opts, &scratch)
    })
    .await?;
    Ok(info(&st.add(w, new_source(output_name(req.name, default_name), "jointure", delimiter))))
}

#[derive(Deserialize)]
struct CompareReq {
    old_id: String,
    new_id: String,
    keys: Vec<String>,
    #[serde(default)]
    columns: Vec<String>,
    #[serde(default)]
    include_unchanged: bool,
    #[serde(default)]
    case_insensitive: bool,
    #[serde(default)]
    trim: bool,
    name: Option<String>,
}

#[derive(Serialize)]
struct CompareResp {
    file: FileInfo,
    summary: CompareSummary,
}

async fn compare(State(st): State<AppState>, Json(req): Json<CompareReq>) -> ApiResult<Json<CompareResp>> {
    let o = entry(&st, &req.old_id)?;
    let n = entry(&st, &req.new_id)?;
    let default_name = format!("comparaison_{}_{}.csv", stem(&o.name()), stem(&n.name()));
    let delimiter = n.delimiter;
    let opts = CompareOptions {
        columns: req.columns,
        include_unchanged: req.include_unchanged,
        case_insensitive: req.case_insensitive,
        trim: req.trim,
    };
    let (keys, scratch) = (req.keys, st.scratch.clone());
    let (w, summary) = blocking(move || {
        let (on, nn) = (o.name(), n.name());
        let oi = Input { name: &on, path: &o.path, headers: &o.headers };
        let ni = Input { name: &nn, path: &n.path, headers: &n.headers };
        multi::compare(&oi, &ni, &keys, &opts, &scratch)
    })
    .await?;
    let e = st.add(w, new_source(output_name(req.name, default_name), "comparaison", delimiter));
    Ok(Json(CompareResp { file: FileInfo::from(&*e), summary }))
}

// ---------------------------------------------------------------------------
// SQL
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SqlReq {
    query: String,
    name: Option<String>,
}

async fn sql_query(State(st): State<AppState>, Json(req): Json<SqlReq>) -> ApiResult<Json<FileInfo>> {
    let files = st.list();
    let out = st.scratch.path("csv");
    let st2 = st.clone();
    let query = req.query.clone();
    let w = blocking(move || {
        let tables: Vec<SqlTable> = files
            .iter()
            .map(|e| SqlTable { name: &e.sql_name, id: &e.id, path: &e.path, headers: &e.headers })
            .collect();
        let mut sql = st2.sql.lock().map_err(|_| "Moteur SQL indisponible".to_string())?;
        let r = sql.query(&query, &tables, out.clone());
        if r.is_err() {
            let _ = std::fs::remove_file(&out);
        }
        r
    })
    .await?;
    Ok(info(&st.add(w, new_source(output_name(req.name, "resultat_sql.csv".into()), "sql", b','))))
}

// ---------------------------------------------------------------------------
// Recettes
// ---------------------------------------------------------------------------

async fn list_recipes(State(st): State<AppState>) -> Json<Vec<Recipe>> {
    Json(st.recipes())
}

async fn get_recipe(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Recipe>> {
    st.recipe(&id).map(Json).ok_or_else(|| not_found("Recette", &id))
}

#[derive(Deserialize)]
struct RecipeReq {
    name: String,
    #[serde(default)]
    description: String,
    /// Étapes explicites (import d'une recette)…
    steps: Option<Vec<Step>>,
    /// …ou fichier dont on reprend l'historique des étapes.
    from_file: Option<String>,
}

async fn create_recipe(State(st): State<AppState>, Json(req): Json<RecipeReq>) -> ApiResult<Json<Recipe>> {
    let steps = match (req.steps, req.from_file) {
        (Some(s), _) => s,
        (None, Some(id)) => entry(&st, &id)?.steps.clone(),
        (None, None) => return Err(bad("Indiquez les étapes ou le fichier d'origine")),
    };
    let r = Recipe { id: String::new(), name: req.name, description: req.description, steps, created_at: 0 };
    let st2 = st.clone();
    Ok(Json(blocking(move || st2.add_recipe(r)).await?))
}

async fn delete_recipe(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let st2 = st.clone();
    let id2 = id.clone();
    if blocking(move || st2.remove_recipe(&id2)).await? { Ok(StatusCode::NO_CONTENT) } else { Err(not_found("Recette", &id)) }
}

#[derive(Deserialize)]
struct ApplyReq {
    ids: Vec<String>,
}

async fn apply_recipe(State(st): State<AppState>, Path(id): Path<String>, Json(req): Json<ApplyReq>) -> ApiResult<Json<Vec<FileInfo>>> {
    let recipe = st.recipe(&id).ok_or_else(|| not_found("Recette", &id))?;
    if req.ids.is_empty() {
        return Err(bad("Choisissez au moins un fichier"));
    }
    let entries: Vec<Arc<Entry>> = req.ids.iter().map(|i| entry(&st, i)).collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for e in entries {
        let (e2, steps, scratch) = (e.clone(), recipe.steps.clone(), st.scratch.clone());
        let w = blocking(move || steps::run_steps(&e2.path, &e2.headers, &steps, &scratch))
            .await
            .map_err(|err| bad(format!("{} : {}", e.name(), err.1)))?;
        let mut lineage = e.steps.clone();
        lineage.extend(recipe.steps.iter().cloned());
        let name = format!("{}_{}.csv", stem(&e.name()), sanitize_part(&recipe.name));
        let added = st.add(w, NewFile { name, origin: "recette".into(), delimiter: e.delimiter, encoding: "UTF-8".into(), steps: lineage });
        out.push(FileInfo::from(&*added));
    }
    Ok(Json(out))
}
