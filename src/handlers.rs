use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use sqlx::SqlitePool;
use tracing::{debug, error, info, warn};

use crate::models::{CreateTimbaResponse, Timba, TimbaInfo, TimbaType, VoteCount, VoteResponse};

pub async fn root() -> StatusCode {
    debug!("timbeando");
    StatusCode::OK
}

pub async fn create_timba(
    State(pool): State<SqlitePool>,
    Path(name): Path<String>,
    Json(payload): Json<Timba>,
) -> Result<Json<CreateTimbaResponse>, StatusCode> {
    debug!("nueva timba: {}", name);
    debug!("  opciones: {:?}", payload.fields);

    match generate_timba(&payload, &name, &pool).await {
        Ok(id) => {
            info!("timba '{}' creada, ID: {}", name, id);
            Ok(Json(CreateTimbaResponse { id }))
        }
        Err(status) => {
            warn!("fallo en la creacion de timba '{}': {:?}", name, status);
            Err(status)
        }
    }
}

async fn generate_timba(timba: &Timba, name: &str, pool: &SqlitePool) -> Result<i64, StatusCode> {
    use TimbaType::*;

    let validation_result = match timba.ty {
        Simple => {
            if timba.fields.len() < 2 {
                warn!(
                    "error en timba simple: necesita al menos 2 fields, recibió {}",
                    timba.fields.len()
                );
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            }
        }
        Multiple => {
            if timba.fields.len() < 2 {
                warn!(
                    "error en timba multiple: necesita al menos 2 fields, recibió {}",
                    timba.fields.len()
                );
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            }
        }
        YN => {
            if timba.fields.len() != 2 {
                warn!(
                    "error en timba YN: necesita 2, recibió {}",
                    timba.fields.len()
                );
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            }
        }
        Scale(lower, upper) => {
            if lower >= upper {
                warn!(
                    "error en timba Scale: lower ({}) >= upper ({})",
                    lower, upper
                );
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            }
        }
    };

    if validation_result != StatusCode::OK {
        return Err(validation_result);
    }

    let time_of_creation = chrono::Utc::now().to_rfc3339();
    let fields_json = serde_json::to_string(&timba.fields).unwrap_or_default();
    let type_str = timba.ty.to_string();

    debug!("guardando '{}' en la db", name);

    let result = sqlx::query(
        r#"
        INSERT INTO timbas (name, time_of_creation, deadline, fields, type)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(name)
    .bind(&time_of_creation)
    .bind(&timba.deadline)
    .bind(&fields_json)
    .bind(&type_str)
    .execute(pool)
    .await;

    match result {
        Ok(res) => {
            let id = res.last_insert_rowid();
            debug!("timba insertada, ID: {}", id);
            Ok(id)
        }
        Err(e) => {
            error!("timbaste demasiado fuerte: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn vote(
    State(pool): State<SqlitePool>,
    Path((id, choice)): Path<(i64, String)>,
) -> Result<Json<VoteResponse>, StatusCode> {
    debug!("voto: poll_id={}, choice={}", id, choice);

    let timba_result = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT fields
        FROM timbas
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await;

    let fields_json = match timba_result {
        Ok(Some((fields,))) => fields,
        Ok(None) => {
            warn!("fallo en el voto: la timba {} no existe", id);
            return Ok(Json(VoteResponse {
                success: false,
                message: "Timba no encontrada".to_string(),
            }));
        }
        Err(e) => {
            error!("db error al buscar timba {}: {}", id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let fields: Vec<String> = serde_json::from_str(&fields_json).unwrap_or_default();
    if !fields.contains(&choice) {
        warn!(
            "falló el voto: voto '{}' invalido para la timba {}. ({:?})",
            choice, id, fields
        );
        return Ok(Json(VoteResponse {
            success: false,
            message: format!(
                "Voto invalido '{}'. Votos validos: {}",
                choice,
                fields.join(", ")
            ),
        }));
    }

    let vote_time = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        INSERT INTO votes (timba_id, vote_choice, voter_identifier, vote_time)
        VALUES (?1, ?2, ?3, ?4)
        "#,
    )
    .bind(id)
    .bind(&choice)
    .bind(Option::<String>::None)
    .bind(&vote_time)
    .execute(&pool)
    .await;

    match result {
        Ok(_) => {
            info!("voto recibido: timba={}, voto={}", id, choice);
            Ok(Json(VoteResponse {
                success: true,
                message: format!("Voto '{}' aceptado", choice),
            }))
        }
        Err(e) => {
            error!("db error al guardar voto: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_timba(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Json<TimbaInfo>, StatusCode> {
    debug!("buscando timba: {}", id);

    let result = sqlx::query_as::<_, (i64, String, String, Option<String>, String, String)>(
        r#"
        SELECT id, name, time_of_creation, deadline, fields, type
        FROM timbas
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await;

    let (timba_id, name, time_of_creation, deadline, fields_json, ty) = match result {
        Ok(Some(data)) => data,
        Ok(None) => {
            warn!("timba {} no encontrada", id);
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            error!("db error al buscar timba {}: {}", id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let fields: Vec<String> = serde_json::from_str(&fields_json).unwrap_or_default();

    let vote_counts = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT vote_choice, COUNT(*) as count
        FROM votes
        WHERE timba_id = ?1
        GROUP BY vote_choice
        "#,
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();

    let mut votes = Vec::new();
    let mut total_votes = 0i64;

    for (choice, count) in vote_counts {
        total_votes += count;
        votes.push(VoteCount { choice, count });
    }

    for field in &fields {
        if !votes.iter().any(|v| &v.choice == field) {
            votes.push(VoteCount {
                choice: field.clone(),
                count: 0,
            });
        }
    }

    debug!(
        "timba '{}' (ID: {}) encontrada, tiene {} votos",
        name, id, total_votes
    );

    Ok(Json(TimbaInfo {
        id: timba_id,
        name,
        time_of_creation,
        deadline,
        fields,
        ty,
        votes,
        total_votes,
    }))
}
