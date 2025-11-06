use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct StickyNote {
    pub id: i64,
    pub content: String,
    pub created_at: i64,
    pub x: i64,
    pub y: i64,
}
