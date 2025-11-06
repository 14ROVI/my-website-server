use std::collections::HashMap;

pub struct LastFmApiHit {
    pub hit_at: u64,
    pub data: String,
}

pub struct LastFMAPI {
    pub key: String,
    pub user_cache: HashMap<String, LastFmApiHit>,
}
