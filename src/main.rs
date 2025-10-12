use dotenvy::dotenv;

use itertools::Itertools;

use image::{EncodableLayout, ImageReader};

use rocket::{
    delete,
    fairing::{Fairing, Info, Kind},
    fs::{NamedFile, TempFile},
    futures::lock::Mutex,
    get,
    http::{Header, Status},
    launch, options, patch, post,
    response::{status, Redirect},
    routes,
    serde::{json::Json, Deserialize, Serialize},
    uri, Build, Request, Response, Rocket, State,
};

use sqlx::{Pool, Sqlite, SqlitePool};

use select::document::Document;
use select::predicate::{Attr, Class, Name, Predicate};

use std::{
    collections::HashMap,
    env,
    io::Cursor,
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

type DbPool = State<Pool<Sqlite>>;

#[derive(Deserialize, Serialize)]
struct StickyNote {
    id: i64,
    content: String,
    created_at: i64,
    x: i64,
    y: i64,
}

#[derive(Deserialize, Serialize, Clone)]
struct FilmData {
    name: String,
    poster_url: String,
    rating: u32,
    watched_at: String,
}

struct LastFmApiHit {
    hit_at: u64,
    data: String,
}

struct LastFMAPI {
    key: String,
    user_cache: HashMap<String, LastFmApiHit>,
}

struct LetterboxdScrape {
    last_hit_at: u64,
    last_response: Vec<FilmData>,
}

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

#[get("/")]
async fn get_paint() -> Option<NamedFile> {
    NamedFile::open(Path::new("paint.png")).await.ok()
}

#[patch("/", data = "<upload>")]
async fn update_paint(upload: TempFile<'_>) -> Status {
    let img = match upload {
        TempFile::File { .. } => ImageReader::open(upload.path().unwrap())
            .ok()
            .and_then(|i| i.with_guessed_format().ok())
            .and_then(|i| i.decode().ok()),
        TempFile::Buffered { content } => ImageReader::new(Cursor::new(content.as_bytes()))
            .with_guessed_format()
            .ok()
            .and_then(|i| i.decode().ok()),
    };

    if let Some(img) = img {
        if img.height() == 1080 && img.width() == 1920 {
            img.save_with_format("paint.png", image::ImageFormat::Png)
                .map_or_else(|_| Status::InternalServerError, |_| Status::Created)
        } else {
            Status::BadRequest
        }
    } else {
        Status::BadRequest
    }
}

#[get("/")]
async fn get_recent_songs() -> Redirect {
    Redirect::to(uri!("./I4ROVI"))
}

#[get("/<username>")]
async fn get_users_recent_songs(state: &State<Arc<Mutex<LastFMAPI>>>, username: &str) -> String {
    let mut state = state.lock().await;
    let last_hit = state.user_cache.get(username);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    if let Some(last_hit) = last_hit {
        if now - last_hit.hit_at < 5 {
            return last_hit.data.clone();
        }
    }

    let url = format!(
        "http://ws.audioscrobbler.com/2.0/?method=user.getrecenttracks&user={}&api_key={}&format=json",
        username, &state.key
    );

    if let Ok(req) = reqwest::get(url).await {
        if let Ok(text) = req.text().await {
            state.user_cache.insert(
                username.to_owned(),
                LastFmApiHit {
                    hit_at: now,
                    data: text.clone(),
                },
            );
            return text;
        }
    }

    String::default()
}

#[get("/films")]
async fn get_films(state: &State<Arc<Mutex<LetterboxdScrape>>>) -> Json<Vec<FilmData>> {
    let mut state = state.lock().await;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    if now - state.last_hit_at < 5 * 60 {
        return Json::from(state.last_response.clone());
    }

    let mut films = vec![];

    if let Ok(req) = reqwest::get("https://letterboxd.com/14rovi/films/by/date/size/large/").await {
        if let Ok(text) = req.text().await {
            let document = Document::from(text.as_str());
            for film_node in
                document.find(Name("div").and(Class("poster-grid")).descendant(Name("li")))
            {
                let (Some(name), Some(rating), Some(div_node), Some(watched_at)) = (
                    film_node
                        .find(Name("img"))
                        .next()
                        .and_then(|n| n.attr("alt"))
                        .map(|s| s.to_string()),
                    film_node
                        .find(Name("span").and(Class("rating")))
                        .next()
                        .and_then(|n| n.attr("class"))
                        .and_then(|c| c.split("-").last())
                        .and_then(|r| r.parse().ok()),
                    film_node
                        .find(Attr("data-component-class", "LazyPoster"))
                        .next(),
                    film_node
                        .find(Name("time"))
                        .next()
                        .and_then(|n| n.attr("datetime"))
                        .map(|dt| dt.to_string()),
                ) else {
                    continue;
                };

                let (
                    Some(film_id),
                    Some(film_url_name),
                    Some(film_poster_width),
                    Some(film_poster_height),
                ) = (
                    div_node.attr("data-film-id"),
                    div_node.attr("data-item-slug"),
                    div_node.attr("data-image-width"),
                    div_node.attr("data-image-height"),
                )
                else {
                    continue;
                };

                films.push(FilmData {
                    name,
                    rating,
                    watched_at,
                    poster_url: format!(
                        "https://a.ltrbxd.com/resized/film-poster/{}/{}-{}-0-{}-0-{}-crop.jpg",
                        film_id.chars().join("/"),
                        film_id,
                        film_url_name,
                        film_poster_width,
                        film_poster_height
                    ),
                });
            }
        }
    }

    state.last_hit_at = now;
    state.last_response = films.clone();

    Json::from(films)
}

pub struct CORS;

#[rocket::async_trait]
impl Fairing for CORS {
    fn info(&self) -> Info {
        Info {
            name: "Add CORS headers to responses",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _request: &'r Request<'_>, response: &mut Response<'r>) {
        response.set_header(Header::new("Access-Control-Allow-Origin", "*"));
        response.set_header(Header::new(
            "Access-Control-Allow-Methods",
            "POST, GET, PATCH, DELETE, OPTIONS",
        ));
        response.set_header(Header::new("Access-Control-Allow-Headers", "*"));
        response.set_header(Header::new("Access-Control-Allow-Credentials", "true"));
    }
}

#[options("/<_..>")]
fn all_options() {
    /* Intentionally left empty */
}

#[launch]
async fn rocket() -> Rocket<Build> {
    dotenv().expect("Couldn't load .env");

    let pool = SqlitePool::connect(&env::var("DATABASE_URL").expect("Can't find DATABASE_URL"))
        .await
        .unwrap();

    rocket::build()
        .attach(CORS)
        .manage(pool)
        .manage(Arc::new(Mutex::new(LastFMAPI {
            key: env::var("LAST_FM_API_KEY")
                .expect("Can't find LAST_FM_API_KEY")
                .to_string(),
            user_cache: HashMap::default(),
        })))
        .manage(Arc::new(Mutex::new(LetterboxdScrape {
            last_hit_at: u64::default(),
            last_response: Vec::default(),
        })))
        .mount("/", routes![all_options, get_films])
        .mount(
            "/notes",
            routes![
                get_active_notes,
                get_deleted_notes,
                get_note,
                create_note,
                update_note,
                delete_note
            ],
        )
        .mount("/paint", routes![get_paint, update_paint])
        .mount("/lastfm", routes![get_recent_songs, get_users_recent_songs])
}
