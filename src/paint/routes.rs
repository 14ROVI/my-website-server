use super::model::{blank_canvas, Edit, HEIGHT, WIDTH};
use actix_web::{
    get, rt,
    web::{self, Bytes},
    Error, HttpRequest, HttpResponse,
};
use actix_ws::AggregatedMessage;
use futures::StreamExt;
use sqlx::SqlitePool;
use std::time::{Duration, Instant};
use tiny_skia::{IntSize, Pixmap};
use tokio::sync::{broadcast, Mutex};

pub struct PaintState {
    canvas: Mutex<Pixmap>,
    updates: broadcast::Sender<Bytes>,
    pool: SqlitePool,
}

impl PaintState {
    pub async fn load(pool: SqlitePool) -> Result<Self, sqlx::Error> {
        let pixels: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT pixels FROM paint_canvas WHERE id = 1")
                .fetch_optional(&pool)
                .await?;
        let canvas = match pixels {
            Some(pixels) => Pixmap::from_vec(pixels, IntSize::from_wh(WIDTH, HEIGHT).unwrap())
                .ok_or_else(|| sqlx::Error::Protocol("Invalid saved Paint canvas".into()))?,
            None => blank_canvas(),
        };
        let (updates, _) = broadcast::channel(64);
        Ok(Self {
            canvas: Mutex::new(canvas),
            updates,
            pool,
        })
    }

    async fn accept(&self, edit: Edit) -> Result<(), &'static str> {
        if edit.points.len() > 256 || !edit.valid() {
            return Err("Invalid drawing.");
        }
        let mut canvas = self.canvas.lock().await;
        let mut next = canvas.clone();
        edit.apply(&mut next);
        let Some([x, y, width, height]) = changed_region(&canvas, &next) else {
            return Ok(());
        };
        sqlx::query("INSERT INTO paint_canvas (id, pixels) VALUES (1, ?) ON CONFLICT(id) DO UPDATE SET pixels = excluded.pixels")
            .bind(next.data()).execute(&self.pool).await.map_err(|_| "Could not save drawing.")?;
        // Persist and publish in the same order, including across server workers.
        let _ = self.updates.send(frame(&next, x, y, width, height));
        *canvas = next;
        Ok(())
    }
}

fn changed_region(before: &Pixmap, after: &Pixmap) -> Option<[u32; 4]> {
    let (mut left, mut top, mut right, mut bottom) = (WIDTH, HEIGHT, 0, 0);
    let stride = WIDTH as usize * 4;
    for (y, (old, new)) in before
        .data()
        .chunks_exact(stride)
        .zip(after.data().chunks_exact(stride))
        .enumerate()
    {
        if old == new {
            continue;
        }
        let changed = || {
            old.chunks_exact(4)
                .zip(new.chunks_exact(4))
                .map(|(a, b)| a != b)
        };
        let first = changed().position(|diff| diff).unwrap() as u32;
        let last = changed().rev().position(|diff| diff).unwrap() as u32;
        left = left.min(first);
        right = right.max(WIDTH - last);
        top = top.min(y as u32);
        bottom = y as u32 + 1;
    }
    (right > left).then(|| [left, top, right - left, bottom - top])
}

// Each frame contains x, y, width and height (little-endian u16), followed by RGBA pixels.
fn frame(canvas: &Pixmap, x: u32, y: u32, width: u32, height: u32) -> Bytes {
    let mut frame = Vec::with_capacity(8 + (width * height * 4) as usize);
    for value in [x, y, width, height] {
        frame.extend_from_slice(&(value as u16).to_le_bytes());
    }
    for row in y..y + height {
        let start = ((row * WIDTH + x) * 4) as usize;
        frame.extend_from_slice(&canvas.data()[start..start + width as usize * 4]);
    }
    frame.into()
}

#[get("/ws")]
async fn websocket(
    req: HttpRequest,
    state: web::Data<PaintState>,
    stream: web::Payload,
) -> Result<HttpResponse, Error> {
    let origin = req
        .headers()
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !matches!(
        origin,
        "https://rovi.me" | "http://localhost:8080" | "http://127.0.0.1:8080"
    ) {
        return Ok(HttpResponse::Forbidden().finish());
    }
    let (response, mut session, stream) = actix_ws::handle(&req, stream)?;
    let mut stream = stream
        .max_frame_size(128 * 1024)
        .aggregate_continuations()
        .max_continuation_size(128 * 1024);
    rt::spawn(async move {
        // Subscribe and capture the initial state atomically: no join-time update gap.
        let (snapshot, mut updates) = {
            let canvas = state.canvas.lock().await;
            (
                frame(&canvas, 0, 0, WIDTH, HEIGHT),
                state.updates.subscribe(),
            )
        };
        if session.binary(snapshot).await.is_err() {
            return;
        }
        let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
        let mut last_seen = Instant::now();
        let mut rate_window = Instant::now();
        let mut count = 0;
        loop {
            tokio::select! {
                incoming = stream.next() => match incoming {
                    Some(Ok(AggregatedMessage::Text(text))) => {
                        last_seen = Instant::now();
                        if rate_window.elapsed() > Duration::from_secs(1) { rate_window = Instant::now(); count = 0; }
                        count += 1;
                        if count > 80 { break; }
                        let result = match serde_json::from_str::<Edit>(&text) {
                            Ok(edit) => state.accept(edit).await,
                            Err(_) => Err("Invalid drawing message."),
                        };
                        if let Err(message) = result {
                            if session.text(message).await.is_err() { break; }
                        }
                    }
                    Some(Ok(AggregatedMessage::Ping(bytes))) => { last_seen = Instant::now(); if session.pong(&bytes).await.is_err() { break; } }
                    Some(Ok(AggregatedMessage::Pong(_))) => last_seen = Instant::now(),
                    Some(Ok(AggregatedMessage::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                },
                update = updates.recv() => match update {
                    Ok(message) => { if session.binary(message).await.is_err() { break; } }
                    // A slow client must reconnect and resync, never silently skip edits.
                    Err(_) => break,
                },
                _ = heartbeat.tick() => {
                    if last_seen.elapsed() > Duration::from_secs(45) || session.ping(b"paint").await.is_err() { break; }
                }
            }
        }
        let _ = session.close(None).await;
    });
    Ok(response)
}

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(websocket);
}
