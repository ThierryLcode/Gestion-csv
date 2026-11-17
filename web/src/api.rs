//! Routes HTTP : interface Web statique et API JSON au-dessus du moteur.

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

use moteur::csvio::{self, ImportOptions, Res};
use moteur::multi::CompareSummary;
use moteur::profile::Profile;
use moteur::service::{
    csv_filename, CompareRequest, ExportOptions, JoinRequest, MergeRequest, Page, RecipeRequest, SplitRequest, Values,
};
use moteur::steps::Step;
use moteur::{AppState, Entry, FileInfo, Recipe};

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
        let status = if msg.starts_with("Fichier introuvable") || msg.starts_with("Recette introuvable") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::BAD_REQUEST
        };
        AppError(status, msg)
    }
}

fn bad(msg: impl Into<String>) -> AppError {
    AppError(StatusCode::BAD_REQUEST, msg.into())
}

type ApiResult<T> = Result<T, AppError>;

/// Exécute un traitement long hors du thread asynchrone.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("Erreur interne : {e}")))?
        .map_err(AppError::from)
}

/// Exécute une méthode du moteur hors du thread asynchrone.
async fn run<T: Send + 'static>(st: &AppState, f: impl FnOnce(&AppState) -> Res<T> + Send + 'static) -> ApiResult<T> {
    let st = st.clone();
    blocking(move || f(&st)).await
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
    let e = st.entry(&id)?;
    Ok(info(&e))
}

async fn delete_file(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let msg = format!("Fichier introuvable : {id}");
    if run(&st, move |st| Ok(st.remove(&id))).await? { Ok(StatusCode::NO_CONTENT) } else { Err(msg.into()) }
}

async fn clear_files(State(st): State<AppState>) -> ApiResult<StatusCode> {
    run(&st, |st| {
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

async fn rename_file(State(st): State<AppState>, Path(id): Path<String>, Json(req): Json<RenameReq>) -> ApiResult<Json<FileInfo>> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(bad("Le nom ne peut pas être vide"));
    }
    let e = st.rename(&id, name.to_string()).ok_or_else(|| format!("Fichier introuvable : {id}"))?;
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
        let (o, raw2, name) = (opts.clone(), raw.clone(), fname.clone());
        let res = run(&st, move |st| {
            let r = st.import_path(&raw2, &name, &o);
            let _ = std::fs::remove_file(&raw2);
            r
        })
        .await;
        match res {
            Ok(e) => out.push(FileInfo::from(&*e)),
            Err(e) => {
                cleanup(&raws[i + 1..]);
                return Err(bad(format!("{fname} : {}", e.1)));
            }
        }
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
struct RowsQuery {
    #[serde(default)]
    offset: u64,
    limit: Option<usize>,
}

async fn file_rows(State(st): State<AppState>, Path(id): Path<String>, Query(q): Query<RowsQuery>) -> ApiResult<Json<Page>> {
    let limit = q.limit.unwrap_or(100).min(5000);
    Ok(Json(run(&st, move |st| st.rows(&id, q.offset, limit)).await?))
}

#[derive(Deserialize)]
struct ValuesQuery {
    column: String,
    limit: Option<usize>,
}

async fn column_values(State(st): State<AppState>, Path(id): Path<String>, Query(q): Query<ValuesQuery>) -> ApiResult<Json<Values>> {
    let limit = q.limit.unwrap_or(1000);
    Ok(Json(run(&st, move |st| st.values(&id, &q.column, limit)).await?))
}

async fn file_profile(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Profile>> {
    Ok(Json(run(&st, move |st| st.profile_of(&id)).await?))
}

// ---------------------------------------------------------------------------
// Téléchargements (en flux)
// ---------------------------------------------------------------------------

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

async fn download(State(st): State<AppState>, Path(id): Path<String>, Query(q): Query<ExportOptions>) -> ApiResult<Response> {
    let e = st.entry(&id)?;
    let wopts = q.to_write_options(e.delimiter)?;
    let charset = if wopts.encoding.to_lowercase().starts_with("utf") { "utf-8".to_string() } else { wopts.encoding.clone() };
    let filename = csv_filename(&e.name());
    let content_type = format!("text/csv; charset={charset}");
    if wopts.is_native() {
        // Format de stockage identique : envoi direct, sans conversion.
        return stream_file(e.path.clone(), false, content_type, &filename).await;
    }
    let tmp = st.scratch.path("export");
    let tmp2 = tmp.clone();
    run(&st, move |st| st.export_file(&id, &q, &tmp2)).await?;
    stream_file(tmp, true, content_type, &filename).await
}

#[derive(Deserialize)]
struct ZipQuery {
    ids: String,
    name: Option<String>,
    #[serde(flatten)]
    export: ExportOptions,
}

async fn download_zip(State(st): State<AppState>, Query(q): Query<ZipQuery>) -> ApiResult<Response> {
    let ids: Vec<String> = q.ids.split(',').filter(|s| !s.is_empty()).map(String::from).collect();
    let tmp = st.scratch.path("zip");
    let (tmp2, export) = (tmp.clone(), q.export);
    run(&st, move |st| st.export_zip(&ids, &export, &tmp2)).await?;
    let name = q.name.unwrap_or_else(|| "fichiers.zip".into());
    stream_file(tmp, true, "application/zip".into(), &name).await
}

// ---------------------------------------------------------------------------
// Opérations
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct StepReq {
    id: String,
    step: Step,
    name: Option<String>,
}

async fn apply_step(State(st): State<AppState>, Json(req): Json<StepReq>) -> ApiResult<Json<FileInfo>> {
    let e = run(&st, move |st| st.apply_step(&req.id, &req.step, req.name)).await?;
    Ok(info(&e))
}

async fn merge(State(st): State<AppState>, Json(req): Json<MergeRequest>) -> ApiResult<Json<FileInfo>> {
    let e = run(&st, move |st| st.merge_files(&req)).await?;
    Ok(info(&e))
}

#[derive(Serialize)]
struct SplitResp {
    files: Vec<FileInfo>,
}

async fn split(State(st): State<AppState>, Json(req): Json<SplitRequest>) -> ApiResult<Json<SplitResp>> {
    let parts = run(&st, move |st| st.split_file(&req)).await?;
    Ok(Json(SplitResp { files: parts.iter().map(|e| FileInfo::from(&**e)).collect() }))
}

async fn join(State(st): State<AppState>, Json(req): Json<JoinRequest>) -> ApiResult<Json<FileInfo>> {
    let e = run(&st, move |st| st.join_files(&req)).await?;
    Ok(info(&e))
}

#[derive(Serialize)]
struct CompareResp {
    file: FileInfo,
    summary: CompareSummary,
}

async fn compare(State(st): State<AppState>, Json(req): Json<CompareRequest>) -> ApiResult<Json<CompareResp>> {
    let (e, summary) = run(&st, move |st| st.compare_files(&req)).await?;
    Ok(Json(CompareResp { file: FileInfo::from(&*e), summary }))
}

#[derive(Deserialize)]
struct SqlReq {
    query: String,
    name: Option<String>,
}

async fn sql_query(State(st): State<AppState>, Json(req): Json<SqlReq>) -> ApiResult<Json<FileInfo>> {
    let e = run(&st, move |st| st.run_sql(&req.query, req.name)).await?;
    Ok(info(&e))
}

// ---------------------------------------------------------------------------
// Recettes
// ---------------------------------------------------------------------------

async fn list_recipes(State(st): State<AppState>) -> Json<Vec<Recipe>> {
    Json(st.recipes())
}

async fn get_recipe(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Recipe>> {
    Ok(Json(st.recipe(&id).ok_or_else(|| format!("Recette introuvable : {id}"))?))
}

async fn create_recipe(State(st): State<AppState>, Json(req): Json<RecipeRequest>) -> ApiResult<Json<Recipe>> {
    Ok(Json(run(&st, move |st| st.create_recipe(&req)).await?))
}

async fn delete_recipe(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let msg = format!("Recette introuvable : {id}");
    if run(&st, move |st| st.remove_recipe(&id)).await? { Ok(StatusCode::NO_CONTENT) } else { Err(msg.into()) }
}

#[derive(Deserialize)]
struct ApplyReq {
    ids: Vec<String>,
}

async fn apply_recipe(State(st): State<AppState>, Path(id): Path<String>, Json(req): Json<ApplyReq>) -> ApiResult<Json<Vec<FileInfo>>> {
    let out = run(&st, move |st| st.apply_recipe(&id, &req.ids)).await?;
    Ok(Json(out.iter().map(|e| FileInfo::from(&**e)).collect()))
}

// Vérifie que l'état reste partageable entre threads (exigé par axum).
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Arc<Entry>>();
};
