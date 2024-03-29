use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Playlists {
    pub items: Vec<Playlist>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PlaylistItems {
    pub items: Vec<Item>,
    pub next: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Item {
    pub track: Track,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AlbumItems {
    pub items: Vec<AlbumTrack>,
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
pub struct AlbumTrack {
    pub id: String,
    pub name: String,
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
