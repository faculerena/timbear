use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use chrono::{DateTime, Utc};
use futures::stream::Stream;
use sqlx::SqlitePool;
use std::convert::Infallible;
use tokio::sync::broadcast;
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::BroadcastStream;
use axum_extra::extract::cookie::{Cookie, CookieJar};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use time;

use crate::models::{
    CreateTimbaResponse, Stats, Timba, TimbaCreatedEvent, TimbaInfo, TimbaType, VoteCount,
    VoteRequest, VoteResponse,
};

pub async fn root() -> StatusCode {
    debug!("timbeando");
    StatusCode::OK
}

pub async fn get_stats(
    State((pool, _tx)): State<(SqlitePool, broadcast::Sender<TimbaCreatedEvent>)>,
) -> Result<Json<Stats>, StatusCode> {
    debug!("obteniendo estadisticas");

    let total_timbas = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM timbas")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    let total_votes = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM votes")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    let oldest_timba = sqlx::query_scalar::<_, String>(
        "SELECT time_of_creation FROM timbas ORDER BY id ASC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);

    let newest_timba = sqlx::query_scalar::<_, String>(
        "SELECT time_of_creation FROM timbas ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);

    info!("stats: {} timbas, {} votos", total_timbas, total_votes);

    Ok(Json(Stats {
        total_timbas,
        total_votes,
        oldest_timba,
        newest_timba,
    }))
}

pub async fn get_latest_timbas(
    State((pool, _tx)): State<(SqlitePool, broadcast::Sender<TimbaCreatedEvent>)>,
) -> Result<Json<Vec<TimbaCreatedEvent>>, StatusCode> {
    debug!("obteniendo las últimas timbas");

    let result = sqlx::query_as::<_, (i64, String, String)>(
        r#"
        SELECT id, name, time_of_creation
        FROM timbas
        ORDER BY id DESC
        LIMIT 10
        "#,
    )
    .fetch_all(&pool)
    .await;

    match result {
        Ok(rows) => {
            let timbas: Vec<TimbaCreatedEvent> = rows
                .into_iter()
                .map(|(id, name, time_of_creation)| TimbaCreatedEvent {
                    id,
                    name,
                    time_of_creation,
                })
                .collect();

            debug!("encontradas {} timbas recientes", timbas.len());
            Ok(Json(timbas))
        }
        Err(e) => {
            error!("db error al obtener últimas timbas: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn stream_timbas(
    State(tx): State<broadcast::Sender<TimbaCreatedEvent>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    debug!("nueva conexión al stream de timbas");

    let rx = tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => {
            debug!("enviando evento de timba: {:?}", event);
            Some(Ok(Event::default().json_data(event).unwrap()))
        }
        Err(e) => {
            warn!("error en el stream: {:?}", e);
            None
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

pub async fn create_timba(
    State((pool, tx)): State<(SqlitePool, broadcast::Sender<TimbaCreatedEvent>)>,
    Path(name): Path<String>,
    Json(payload): Json<Timba>,
) -> Result<Json<CreateTimbaResponse>, StatusCode> {
    debug!("nueva timba: {}", name);
    debug!("  opciones: {:?}", payload.fields);

    match generate_timba(&payload, &name, &pool).await {
        Ok(id) => {
            info!("timba '{}' creada, ID: {}", name, id);

            let event = TimbaCreatedEvent {
                id,
                name: name.clone(),
                time_of_creation: chrono::Utc::now().to_rfc3339(),
            };
            let _ = tx.send(event);

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

    let fields = match &timba.ty {
        Scale(lower, upper) => {
            if lower >= upper {
                warn!(
                    "error en timba Scale: lower ({}) >= upper ({})",
                    lower, upper
                );
                return Err(StatusCode::BAD_REQUEST);
            }
            (*lower..=*upper)
                .map(|i| i.to_string())
                .collect::<Vec<String>>()
        }
        _ => timba.fields.clone(),
    };

    let validation_result = match timba.ty {
        Simple => {
            if fields.len() < 2 {
                warn!(
                    "error en timba simple: necesita al menos 2 fields, recibió {}",
                    fields.len()
                );
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            }
        }
        Multiple => {
            if fields.len() < 2 {
                warn!(
                    "error en timba multiple: necesita al menos 2 fields, recibió {}",
                    fields.len()
                );
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            }
        }
        YN => {
            if fields.len() != 2 {
                warn!("error en timba YN: necesita 2, recibió {}", fields.len());
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            }
        }
        Scale(_, _) => StatusCode::OK, // Ya validado arriba
    };

    if validation_result != StatusCode::OK {
        return Err(validation_result);
    }

    let time_of_creation = chrono::Utc::now().to_rfc3339();
    let fields_json = serde_json::to_string(&fields).unwrap_or_default();
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
    State((pool, _tx)): State<(SqlitePool, broadcast::Sender<TimbaCreatedEvent>)>,
    Path(id): Path<i64>,
    jar: CookieJar,
    Json(vote_request): Json<VoteRequest>,
) -> Result<(CookieJar, Json<VoteResponse>), StatusCode> {
    debug!("voto: poll_id={}, choices={:?}", id, vote_request.choices);

    if vote_request.choices.is_empty() {
        return Ok((jar, Json(VoteResponse {
            success: false,
            message: "Debes proporcionar al menos una opcion".to_string(),
        })));
    }

    // Obtener o crear voter_id desde cookie
    let (voter_id, jar) = match jar.get("voter_id") {
        Some(cookie) => (cookie.value().to_string(), jar),
        None => {
            let new_voter_id = Uuid::new_v4().to_string();
            let cookie = Cookie::build(("voter_id", new_voter_id.clone()))
                .max_age(time::Duration::days(365))
                .path("/")
                .build();
            debug!("nuevo voter_id creado: {}", new_voter_id);
            (new_voter_id, jar.add(cookie))
        }
    };

    debug!("voter_id: {}", voter_id);

    let timba_result = sqlx::query_as::<_, (String, String, Option<String>)>(
        r#"
        SELECT fields, type, deadline
        FROM timbas
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await;

    let (fields_json, timba_type, deadline) = match timba_result {
        Ok(Some((fields, ty, dl))) => (fields, ty, dl),
        Ok(None) => {
            warn!("fallo en el voto: la timba {} no existe", id);
            return Ok((jar, Json(VoteResponse {
                success: false,
                message: "Timba no encontrada".to_string(),
            })));
        }
        Err(e) => {
            error!("db error al buscar timba {}: {}", id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Verificar deadline
    if let Some(deadline_str) = deadline {
        match DateTime::parse_from_rfc3339(&deadline_str) {
            Ok(deadline_dt) => {
                let now = Utc::now();
                if now > deadline_dt {
                    warn!("intento de voto en timba cerrada {}: deadline era {}", id, deadline_str);
                    return Ok((jar, Json(VoteResponse {
                        success: false,
                        message: "Esta timba ya cerró. El deadline fue superado.".to_string(),
                    })));
                }
            }
            Err(e) => {
                warn!("formato de deadline invalido para timba {}: {}", id, e);
            }
        }
    }

    // Verificar si ya votó (deduplicación)
    let already_voted = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM votes
        WHERE timba_id = ?1 AND voter_identifier = ?2
        "#,
    )
    .bind(id)
    .bind(&voter_id)
    .fetch_one(&pool)
    .await
    .unwrap_or(0);

    if already_voted > 0 {
        warn!("intento de voto duplicado: timba={}, voter_id={}", id, voter_id);
        return Ok((jar, Json(VoteResponse {
            success: false,
            message: "Ya votaste en esta timba".to_string(),
        })));
    }

    let fields: Vec<String> = serde_json::from_str(&fields_json).unwrap_or_default();

    if vote_request.choices.len() > 1 && timba_type != "Multiple" {
        return Ok((jar, Json(VoteResponse {
            success: false,
            message: format!(
                "Esta timba es de tipo '{}' y solo acepta un voto. Usa 'Multiple' para votos multiples.",
                timba_type
            ),
        })));
    }

    let mut invalid_choices = Vec::new();
    for choice in &vote_request.choices {
        if !fields.contains(choice) {
            invalid_choices.push(choice.clone());
        }
    }

    if !invalid_choices.is_empty() {
        warn!(
            "falló el voto: votos invalidos {:?} para la timba {}. ({:?})",
            invalid_choices, id, fields
        );
        return Ok((jar, Json(VoteResponse {
            success: false,
            message: format!(
                "Votos invalidos: {}. Votos validos: {}",
                invalid_choices.join(", "),
                fields.join(", ")
            ),
        })));
    }

    let vote_time = chrono::Utc::now().to_rfc3339();

    for choice in &vote_request.choices {
        let result = sqlx::query(
            r#"
            INSERT INTO votes (timba_id, vote_choice, voter_identifier, vote_time)
            VALUES (?1, ?2, ?3, ?4)
            "#,
        )
        .bind(id)
        .bind(choice)
        .bind(&voter_id)
        .bind(&vote_time)
        .execute(&pool)
        .await;

        if let Err(e) = result {
            error!("db error al guardar voto: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    info!(
        "votos recibidos: timba={}, votos={:?}",
        id, vote_request.choices
    );
    Ok((jar, Json(VoteResponse {
        success: true,
        message: format!("Votos aceptados: {}", vote_request.choices.join(", ")),
    })))
}

pub async fn get_timba(
    State((pool, _tx)): State<(SqlitePool, broadcast::Sender<TimbaCreatedEvent>)>,
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
