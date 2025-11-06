use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rocket::futures::future::join_all;
use rocket::futures::lock::Mutex;
use rocket::serde::json::Json;
use rocket::{get, routes, Route, State};

use select::predicate::{Attr, Class, Name, Predicate};
use select::{document::Document, node::Node};

use super::model::{FilmData, LetterboxdPoster, LetterboxdScrape};

#[get("/")]
async fn get_films(state: &State<Arc<Mutex<LetterboxdScrape>>>) -> Json<Vec<FilmData>> {
    let mut state = state.lock().await;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    if now - state.last_hit_at < 5 * 60 {
        return Json::from(state.last_response.clone());
    }

    let films = get_letterboxd_films().await.unwrap_or_default();

    state.last_hit_at = now;
    state.last_response = films.clone();

    Json::from(films)
}

async fn get_letterboxd_films() -> Result<Vec<FilmData>, Box<dyn std::error::Error>> {
    let mut films = {
        let html = reqwest::get("https://letterboxd.com/14rovi/films/by/date/size/large/")
            .await?
            .text()
            .await?;

        Document::from(html.as_str())
            .find(Name("div").and(Class("poster-grid")).descendant(Name("li")))
            .flat_map(parse_letterboxd_poster)
            .collect::<Vec<_>>()
    };

    let futures = films.iter_mut().map(|film| set_poster_url(film));

    join_all(futures).await;

    return Ok(films);
}

fn parse_letterboxd_poster(film_node: Node) -> Option<FilmData> {
    let name = film_node
        .find(Name("img"))
        .next()
        .and_then(|n| n.attr("alt"))
        .map(|s| s.to_string())?;

    let rating = film_node
        .find(Name("span").and(Class("rating")))
        .next()
        .and_then(|n| n.attr("class"))
        .and_then(|c| c.split("-").last())
        .and_then(|r| r.parse::<u32>().ok())?;

    let watched_at = film_node
        .find(Name("time"))
        .next()
        .and_then(|n| n.attr("datetime"))
        .map(|dt| dt.to_string())?;

    let poster_url = film_node
        .find(Attr("data-component-class", "LazyPoster"))
        .next()
        .and_then(|n| n.attr("data-item-link"))
        .map(|s| s.to_string())?;

    Some(FilmData {
        name,
        rating,
        watched_at,
        poster_url,
    })
}

async fn set_poster_url(film: &mut FilmData) {
    if let Ok(req) = reqwest::get(format!(
        "https://letterboxd.com{}poster/std/150",
        film.poster_url
    ))
    .await
    {
        if let Ok(text) = req.text().await {
            let poster: LetterboxdPoster = serde_json::from_str(&text).unwrap();
            film.poster_url = poster.url;
        }
    }
}

pub fn routes() -> Vec<Route> {
    routes![get_films]
}
