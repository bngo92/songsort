use azure_data_cosmos::prelude::CosmosEntity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Scores {
    pub scores: Vec<Score>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Score {
    pub id: String,
    pub track_id: String,
    pub track: String,
    pub album: String,
    pub artists: Vec<String>,
    pub user_id: String,
    pub score: i32,
    pub wins: i32,
    pub losses: i32,
}

impl<'a> CosmosEntity<'a> for Score {
    type Entity = &'a str;

    fn partition_key(&'a self) -> Self::Entity {
        self.user_id.as_ref()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Playlists {
    pub items: Vec<Playlist>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Playlist {
    pub id: String,
    pub playlist_id: String,
    pub name: String,
    pub user_id: String,
    pub tracks: Vec<String>,
}

impl<'a> CosmosEntity<'a> for Playlist {
    type Entity = &'a str;

    fn partition_key(&'a self) -> Self::Entity {
        self.user_id.as_ref()
    }
}
