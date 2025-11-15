mod db;
mod handlers;
mod models;

use axum::{
    Router,
    routing::{get, post},
};
use tokio::sync::broadcast;
use tower_http::services::ServeDir;
use tower_http::cors::{CorsLayer, Any};
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .init();

    info!("empieza el ritual");

    let pool = db::create_pool().await.expect("Falló la creacion de db");

    info!("db conectada");

    let (tx, _rx) = broadcast::channel::<models::TimbaCreatedEvent>(100);

    info!("canal de broadcast creado");

    // Configurar CORS para permitir requests desde cualquier origen
    // Nota: No se puede usar credentials con allow_origin(Any), asi que las cookies
    // funcionaran solo si el frontend está en el mismo origen o se configura un origen específico
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
        ]);

    let app = Router::new()
        .route("/", get(handlers::root))
        .route("/stats", get(handlers::get_stats))
        .route("/latest", get(handlers::get_latest_timbas))
        .route("/new/{name}", post(handlers::create_timba))
        .route("/timba/{id}", get(handlers::get_timba))
        .route("/vote/{id}", post(handlers::vote))
        .with_state((pool, tx.clone()))
        .route("/stream", get(handlers::stream_timbas))
        .with_state(tx)
        .nest_service("/static", ServeDir::new("static"))
        .layer(cors);

    info!("ruteo timbeado");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    info!("timbeando gratis http://0.0.0.0:3000");

    axum::serve(listener, app).await.unwrap();
}
