use std::time::{SystemTime, UNIX_EPOCH};

use rocket::{
    delete, get, http::Status, patch, post, response::status, routes, serde::json::Json, Route,
};

use crate::{notes::model::StickyNote, DbPool};

fn get_sys_time() -> Option<u32> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|t| u32::try_from(t.as_secs()).ok())
}

#[get("/")]
async fn get_active_notes(pool: &DbPool) -> Json<Vec<StickyNote>> {
    sqlx::query_as!(
        StickyNote,
        "SELECT id, content, created_at, x, y FROM notes WHERE deleted = FALSE"
    )
    .fetch_all(&**pool)
    .await
    .map_or_else(|_| Json(Vec::new()), Json)
}

#[get("/deleted")]
async fn get_deleted_notes(pool: &DbPool) -> Json<Vec<StickyNote>> {
    sqlx::query_as!(
        StickyNote,
        "SELECT id, content, created_at, x, y FROM notes WHERE deleted = TRUE"
    )
    .fetch_all(&**pool)
    .await
    .map_or_else(|_| Json(Vec::new()), Json)
}

#[get("/<id>")]
async fn get_note(pool: &DbPool, id: u32) -> Option<Json<StickyNote>> {
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

#[post("/?<content>&<x>&<y>")]
async fn create_note(
    pool: &DbPool,
    content: &str,
    x: u32,
    y: u32,
) -> Result<Json<StickyNote>, status::Custom<&'static str>> {
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
        .map(Json)
        .map_err(|_| status::Custom(Status::InternalServerError, "error saving sticky note"))
    } else {
        Err(status::Custom(
            Status::InternalServerError,
            "error getting system time",
        ))
    }
}

#[patch("/<id>?<content>&<x>&<y>")]
async fn update_note(pool: &DbPool, id: u32, content: &str, x: u32, y: u32) -> Status {
    sqlx::query("UPDATE notes SET content = ?, x = ?, y = ? WHERE id = ?")
        .bind(content)
        .bind(x)
        .bind(y)
        .bind(id)
        .execute(&**pool)
        .await
        .map_or_else(|_| Status::InternalServerError, |_| Status::Ok)
}

#[delete("/<id>")]
async fn delete_note(pool: &DbPool, id: u32) -> Status {
    sqlx::query("UPDATE notes SET deleted = TRUE WHERE id = ?")
        .bind(id)
        .execute(&**pool)
        .await
        .map_or_else(|_| Status::InternalServerError, |_| Status::Ok)
}

pub fn routes() -> Vec<Route> {
    routes![
        get_active_notes,
        get_deleted_notes,
        get_note,
        create_note,
        update_note,
        delete_note
    ]
}
