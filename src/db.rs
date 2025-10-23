use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tracing::{debug, error, info};

pub async fn create_pool() -> Result<SqlitePool, sqlx::Error> {
    debug!("creando db");

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite:timbas.db")
        .await
        .map_err(|e| {
            error!("fallo al conectarse a la db: {}", e);
            e
        })?;

    debug!("db conectada");

    initialize_database(&pool).await?;

    info!("db lista");

    Ok(pool)
}

async fn initialize_database(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    debug!("iniciando schema");

    debug!("creando tabla timbas");
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS timbas (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            time_of_creation TEXT NOT NULL,
            deadline TEXT,
            fields TEXT NOT NULL,
            type TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        error!("fallo creando tabla timbas: {}", e);
        e
    })?;

    debug!("creando tabla votes");
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS votes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timba_id INTEGER NOT NULL,
            vote_choice TEXT NOT NULL,
            voter_identifier TEXT,
            vote_time TEXT NOT NULL,
            FOREIGN KEY (timba_id) REFERENCES timbas(id)
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        error!("fallo creando tabla votes: {}", e);
        e
    })?;

    debug!("schema listo");

    Ok(())
}
