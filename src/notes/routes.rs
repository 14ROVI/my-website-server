use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use actix_web::{
    delete, get, patch, post,
    web::{self, Json},
    HttpResponse, Responder,
};

use crate::{notes::model::StickyNote, DbPool};

fn get_sys_time() -> Option<u32> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|t| u32::try_from(t.as_secs()).ok())
}

#[get("/")]
async fn get_active_notes(pool: DbPool) -> Json<Vec<StickyNote>> {
    sqlx::query_as!(
        StickyNote,
        "SELECT id, content, created_at, x, y FROM notes WHERE deleted = FALSE"
    )
    .fetch_all(&**pool)
    .await
    .map_or_else(|_| Json(Vec::new()), Json)
}

#[get("/deleted")]
async fn get_deleted_notes(pool: DbPool) -> Json<Vec<StickyNote>> {
    sqlx::query_as!(
        StickyNote,
        "SELECT id, content, created_at, x, y FROM notes WHERE deleted = TRUE"
    )
    .fetch_all(&**pool)
    .await
    .map_or_else(|_| Json(Vec::new()), Json)
}

#[get("/{id}")]
async fn get_note(pool: DbPool, path: web::Path<u32>) -> Option<Json<StickyNote>> {
    let id = path.into_inner();

    sqlx::query_as!(
        StickyNote,
        "SELECT id, content, created_at, x, y FROM notes WHERE id = ?",
        id
    )
    .fetch_one(&**pool)
    .await
    .ok()
    .map(Json)
}

#[derive(Deserialize)]
struct NoteFields {
    content: String,
    x: i32,
    y: i32,
}

#[post("/")]
async fn create_note(pool: DbPool, query: web::Query<NoteFields>) -> impl Responder {
    let NoteFields { content, x, y } = query.into_inner();
    let sys_time = get_sys_time();

    if let Some(sys_time) = sys_time {
        sqlx::query_as!(
            StickyNote,
            "INSERT INTO notes (content, created_at, x, y) VALUES (?, ?, ?, ?)
            RETURNING id, content, created_at, x, y",
            content,
            sys_time,
            x,
            y
        )
        .fetch_one(&**pool)
        .await
        .map_or_else(
            |_| HttpResponse::InternalServerError().body("Error saving sticky note."),
            |note| HttpResponse::Ok().json(note),
        )
    } else {
        HttpResponse::InternalServerError().body("Error getting system time.")
    }
}

#[patch("/{id}")]
async fn update_note(
    pool: DbPool,
    path: web::Path<u32>,
    query: web::Query<NoteFields>,
) -> impl Responder {
    let id = path.into_inner();
    let NoteFields { content, x, y } = query.into_inner();

    sqlx::query("UPDATE notes SET content = ?, x = ?, y = ? WHERE id = ?")
        .bind(content)
        .bind(x)
        .bind(y)
        .bind(id)
        .execute(&**pool)
        .await
        .map_or_else(
            |_| HttpResponse::InternalServerError(),
            |_| HttpResponse::Ok(),
        )
}

#[delete("/{id}")]
async fn delete_note(pool: DbPool, path: web::Path<u32>) -> impl Responder {
    let id = path.into_inner();

    sqlx::query("UPDATE notes SET deleted = TRUE WHERE id = ?")
        .bind(id)
        .execute(&**pool)
        .await
        .map_or_else(
            |_| HttpResponse::InternalServerError(),
            |_| HttpResponse::Ok(),
        )
}

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(get_active_notes)
        .service(get_deleted_notes)
        .service(get_note)
        .service(create_note)
        .service(update_note)
        .service(delete_note);
}
