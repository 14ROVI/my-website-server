mod cors;
mod lastfm;
mod letterboxd;
mod notes;

use dotenvy::dotenv;
use image::{EncodableLayout, ImageReader};
use rocket::{
    fs::{NamedFile, TempFile},
    futures::lock::Mutex,
    get,
    http::Status,
    launch, patch, routes, Build, Rocket, State,
};
use sqlx::{Pool, Sqlite, SqlitePool};
use std::{collections::HashMap, env, io::Cursor, path::Path, sync::Arc};

use crate::{lastfm::LastFMAPI, letterboxd::LetterboxdScrape};

type DbPool = State<Pool<Sqlite>>;

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

#[launch]
async fn rocket() -> Rocket<Build> {
    dotenv().expect("Couldn't load .env");

    let pool = SqlitePool::connect(&env::var("DATABASE_URL").expect("Can't find DATABASE_URL"))
        .await
        .unwrap();

    rocket::build()
        .attach(cors::CORS)
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
        .mount("/", cors::routes())
        .mount("/films", letterboxd::routes())
        .mount("/notes", notes::routes())
        .mount("/paint", routes![get_paint, update_paint])
        .mount("/lastfm", lastfm::routes())
}
