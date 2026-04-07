use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::web::{Data, Json};
use actix_web::{get, web};

use rss::{Channel, Item};
use select::document::Document;
use select::predicate::Name;
use tokio::sync::Mutex;

use super::model::{FilmData, LetterboxdScrape};

#[get("/")]
async fn get_films(state: Data<Mutex<LetterboxdScrape>>) -> Json<Vec<FilmData>> {
    let mut state = state.lock().await;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    if now - state.last_hit_at < 5 * 60 {
        return Json(state.last_response.clone());
    }

    let films = get_letterboxd_films().await.unwrap_or_default();

    state.last_hit_at = now;
    state.last_response = films.clone();

    Json(films)
}

async fn get_letterboxd_films() -> Result<Vec<FilmData>, Box<dyn std::error::Error>> {
    let content = reqwest::get("https://letterboxd.com/14rovi/rss/")
        .await?
        .bytes()
        .await?;

    let channel = Channel::read_from(&content[..])?;

    let films = channel
        .items
        .iter()
        .flat_map(|item| parse_letterboxd_poster(item))
        .collect::<Vec<_>>();

    return Ok(films);
}

fn parse_letterboxd_poster(item: &Item) -> Option<FilmData> {
    let name = item
        .extensions
        .get("letterboxd")?
        .get("filmTitle")?
        .first()?
        .value()?
        .to_string();

    let watched_at = item.pub_date()?.to_string();

    let rating = (item
        .extensions
        .get("letterboxd")?
        .get("memberRating")?
        .first()?
        .value()?
        .parse::<f32>()
        .ok()?
        * 2.0) as u32;

    let poster_url = Document::from(item.description()?)
        .find(Name("img"))
        .next()
        .and_then(|n| n.attr("src"))
        .map(|s| s.to_string())?;

    Some(FilmData {
        name,
        rating,
        watched_at,
        poster_url,
    })
}

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.app_data(Data::new(Mutex::new(LetterboxdScrape {
        last_hit_at: u64::default(),
        last_response: Vec::default(),
    })))
    .service(get_films);
}
