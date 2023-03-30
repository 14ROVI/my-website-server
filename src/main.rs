#[macro_use] extern crate rocket;
#[macro_use] extern crate dotenv_codegen;
extern crate dotenv;
use rocket::futures::lock::Mutex;
use rocket_db_pools::{Database, Connection};
use rocket_db_pools::sqlx;
use rocket::serde::{Deserialize, Serialize, json::Json};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use rocket::fs::NamedFile;
use std::path::Path;
use rocket::http::Status;
use rocket::fs::TempFile;
use std::io::Cursor;
use image::io::Reader as ImageReader;
use rocket::http::Header;
use rocket::{Request, Response, State};
use rocket::fairing::{Fairing, Info, Kind};
use dotenv::dotenv;



#[derive(Database)]
#[database("db")]
struct DB(sqlx::SqlitePool);


#[derive(Deserialize, Serialize)]
struct StickyNote {
    id: i64,
    content: String,
    created_at: i64,
    x: i64,
    y: i64
}


fn get_sys_time() -> Option<u32> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|t| u32::try_from(t.as_secs()).ok())
}

#[get("/")]
async fn get_all_notes(mut db: Connection<DB>) -> Json<Vec<StickyNote>> {
    sqlx::query_as!(StickyNote, "SELECT id, content, created_at, x, y FROM notes")
        .fetch_all(&mut *db)
        .await
        .map_or_else(
            |_| Json(Vec::new()), 
            Json
        )
}

#[get("/<id>")]
async fn get_note(mut db: Connection<DB>, id: u32) -> Option<Json<StickyNote>> {
    sqlx::query_as!(StickyNote, "SELECT id, content, created_at, x, y FROM notes WHERE id = ?", id)
        .fetch_one(&mut *db)
        .await
        .ok()
        .map(Json)
}

#[post("/?<content>&<x>&<y>")]
async fn create_note(mut db: Connection<DB>, content: &str, x: u32, y: u32) -> Status {
    let sys_time = get_sys_time();

    if let Some(sys_time) = sys_time {
        sqlx::query("INSERT INTO notes (content, created_at, x, y) VALUES (?, ?, ?, ?)")
        .bind(content)
        .bind(sys_time)
        .bind(x)
        .bind(y)
        .execute(&mut *db)
        .await
        .map_or_else(
            |_| Status::InternalServerError, 
            |_| Status::Created
        )
    } else {
        Status::InternalServerError
    }
}

#[patch("/<id>?<content>&<x>&<y>")]
async fn update_note(mut db: Connection<DB>, id: u32, content: &str, x: u32, y: u32) -> Status {
    sqlx::query("UPDATE notes SET content = ?, x = ?, y = ? WHERE id = ?")
        .bind(content)
        .bind(x)
        .bind(y)
        .bind(id)
        .execute(&mut *db)
        .await
        .map_or_else(
            |_| Status::InternalServerError, 
            |_| Status::Ok
        )
}

#[delete("/<id>")]
async fn delete_note(mut db: Connection<DB>, id: u32) -> Status {
    sqlx::query("DELETE FROM notes WHERE id = ?")
        .bind(id)
        .execute(&mut *db)
        .await
        .map_or_else(
            |_| Status::InternalServerError, 
            |_| Status::Ok
        )
}


#[get("/")]
async fn get_paint() -> Option<NamedFile> {
    NamedFile::open(Path::new("paint.png")).await.ok()
}

#[patch("/", data = "<upload>")]
async fn update_paint(upload: TempFile<'_>) -> Status {
    let img = match upload {
        TempFile::File { .. } => {
            ImageReader::open(upload.path().unwrap())
                .ok()
                .and_then(|i| i.with_guessed_format().ok())
                .and_then(|i| i.decode().ok())
        }
        TempFile::Buffered { content } => {
            ImageReader::new(Cursor::new(content.as_bytes()))
                .with_guessed_format()
                .ok()
                .and_then(|i| i.decode().ok())
        }
    };

    if let Some(img) = img {
        if img.height() == 1080 && img.width() == 1920 {
            img.save_with_format("paint.png", image::ImageFormat::Png)
                .map_or_else(
                    |_| Status::InternalServerError, 
                    |_| Status::Created
                )
        } else {
            Status::BadRequest
        }
    } else {
        Status::BadRequest
    }
}


#[get("/")]
async fn get_recent_songs(state: &State<Arc<Mutex<LastFMAPI>>>) -> String {
    let mut state = state.lock().await;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    if now - state.last_hit < 5 {
        return state.last_response.clone();
    }

    let url = format!(
        "http://ws.audioscrobbler.com/2.0/?method=user.getrecenttracks&user=I4ROVI&api_key={}&format=json",
        &state.key
    );

    if let Ok(req) = reqwest::get(url).await {
        if let Ok(text) = req.text().await {
            state.last_response = text.clone();
            state.last_hit = now;
            return text;
        }
    }

    String::default()
}


pub struct CORS;

#[rocket::async_trait]
impl Fairing for CORS {
    fn info(&self) -> Info {
        Info {
            name: "Add CORS headers to responses",
            kind: Kind::Response
        }
    }

    async fn on_response<'r>(&self, _request: &'r Request<'_>, response: &mut Response<'r>) {
        response.set_header(Header::new("Access-Control-Allow-Origin", "*"));
        response.set_header(Header::new("Access-Control-Allow-Methods", "POST, GET, PATCH, DELETE, OPTIONS"));
        response.set_header(Header::new("Access-Control-Allow-Headers", "*"));
        response.set_header(Header::new("Access-Control-Allow-Credentials", "true"));
    }
}

#[options("/<_..>")]
fn all_options() {
    /* Intentionally left empty */
}


struct LastFMAPI {
    key: String,
    secret: String,
    last_hit: u64,
    last_response: String,
}


#[launch]
fn rocket() -> _ {
    dotenv().expect("Couldn't load .env");

    rocket::build()
        .attach(DB::init())
        .attach(CORS)
        .manage(Arc::new(Mutex::new(LastFMAPI {
            key: dotenv!("LAST_FM_API_KEY").to_string(),
            secret: dotenv!("LAST_FM_SHARED_SECRET").to_string(),
            last_hit: 0,
            last_response: String::default()
        })))
        .mount("/", routes![all_options])
        .mount("/notes", routes![
            get_all_notes,
            get_note,
            create_note,
            update_note,
            delete_note
        ])
        .mount("/paint", routes![
            get_paint,
            update_paint,
        ])
        .mount("/lastfm", routes![
            get_recent_songs
        ])
}