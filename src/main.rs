mod api;
mod csvio;
mod multi;
mod profile;
mod sort;
mod sql;
mod steps;
mod store;
mod text;

use std::net::SocketAddr;
use std::path::PathBuf;

fn env_num(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[tokio::main]
async fn main() {
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = env_num("PORT", 8080);
    let data_dir = PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "donnees".into()));
    // 0 (par défaut) = pas de limite de taille d'envoi.
    let max_upload_mb = env_num("MAX_UPLOAD_MB", 0);
    let sort_memory_mb = env_num("SORT_MEMORY_MB", 256).max(16);

    let state = store::AppState::open(&data_dir, sort_memory_mb * 1024 * 1024).unwrap_or_else(|e| {
        eprintln!("Erreur : {e}");
        std::process::exit(1);
    });
    let addr: SocketAddr = format!("{host}:{port}").parse().expect("Adresse HOST:PORT invalide");
    let app = api::router(state, (max_upload_mb > 0).then(|| max_upload_mb * 1024 * 1024));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Impossible d'écouter sur {addr} : {e}"));

    println!("Gestion CSV disponible sur http://{addr}");
    println!("Données : {}", data_dir.display());
    axum::serve(listener, app).await.expect("Erreur du serveur");
}
