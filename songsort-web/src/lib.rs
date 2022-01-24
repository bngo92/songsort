use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Playlist {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PlaylistItems {
    pub items: Vec<Item>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Item {
    pub track: Track,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub album: Album,
    pub artists: Vec<Artist>,
    pub preview_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Album {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Artist {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct User {
    pub id: String,
}
