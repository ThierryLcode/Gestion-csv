mod api;
mod csvdata;
mod ops;
mod store;

use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let max_upload_mb: usize =
        std::env::var("MAX_UPLOAD_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(512);

    let addr: SocketAddr = format!("{host}:{port}").parse().expect("Adresse HOST:PORT invalide");
    let app = api::router(store::AppState::default(), max_upload_mb * 1024 * 1024);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Impossible d'écouter sur {addr} : {e}"));

    println!("Gestion CSV disponible sur http://{addr}");
    axum::serve(listener, app).await.expect("Erreur du serveur");
}
