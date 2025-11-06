use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rocket::futures::lock::Mutex;

use rocket::{get, routes, Route, State};

use crate::lastfm::model::{LastFMAPI, LastFmApiHit};

#[get("/")]
async fn get_recent_songs(state: &State<Arc<Mutex<LastFMAPI>>>) -> String {
    get_users_recent_songs(state, "I4ROVI").await
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

pub fn routes() -> Vec<Route> {
    routes![get_recent_songs, get_users_recent_songs]
}
