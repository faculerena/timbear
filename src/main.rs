mod db;
mod handlers;
mod models;

use axum::{
    Router,
    routing::{get, post},
};
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

    let app = Router::new()
        .route("/", get(handlers::root))
        .route("/new/{name}", post(handlers::create_timba))
        .route("/timba/{id}", get(handlers::get_timba))
        .route("/vote/{id}/{choice}", post(handlers::vote))
        .with_state(pool);

    info!("ruteo timbeado");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    info!("timbeando gratis http://0.0.0.0:3000");

    axum::serve(listener, app).await.unwrap();
}
