use serde::{Deserialize, Serialize};

pub struct LetterboxdScrape {
    pub last_hit_at: u64,
    pub last_response: Vec<FilmData>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct FilmData {
    pub name: String,
    pub poster_url: String,
    pub rating: u32,
    pub watched_at: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct LetterboxdPoster {
    pub url: String,
}
