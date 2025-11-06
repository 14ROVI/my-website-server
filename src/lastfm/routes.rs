use std::collections::HashMap;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::web::{Data, Path};
use actix_web::{get, web};
use tokio::sync::Mutex;

use crate::lastfm::model::{LastFMAPI, LastFmApiHit};

#[get("/")]
async fn get_recent_songs(state: Data<Mutex<LastFMAPI>>) -> String {
    get_recent_songs_inner(state, "I4ROVI".to_string()).await
}

#[get("/{username}")]
async fn get_users_recent_songs(state: Data<Mutex<LastFMAPI>>, path: Path<String>) -> String {
    let username = path.into_inner();

    get_recent_songs_inner(state, username).await
}

async fn get_recent_songs_inner(state: Data<Mutex<LastFMAPI>>, username: String) -> String {
    let mut state = state.lock().await;
    let last_hit = state.user_cache.get(&username);

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

pub fn config(cfg: &mut web::ServiceConfig) {
    let last_fm_api_key = env::var("LAST_FM_API_KEY")
        .expect("Can't find LAST_FM_API_KEY")
        .to_string();

    cfg.app_data(Data::new(Mutex::new(LastFMAPI {
        key: last_fm_api_key,
        user_cache: HashMap::default(),
    })))
    .service(get_recent_songs)
    .service(get_users_recent_songs);
}
