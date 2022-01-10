use hyper::header::HeaderValue;
use hyper::http::response::Builder;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode, Uri};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

type Db = Arc<
    Mutex<(
        HashMap<String, Playlist>,
        HashMap<String, HashMap<String, Score>>,
    )>,
>;

#[derive(Debug, Deserialize, Serialize)]
struct Token {
    access_token: String,
}

#[derive(Debug, Serialize)]
struct Scores {
    scores: Vec<Score>,
}

#[derive(Clone, Debug, Serialize)]
struct Score {
    playlist: String,
    track_id: String,
    track: String,
    score: i32,
    preview_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Playlists {
    items: Vec<Playlist>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Playlist {
    name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct PlaylistItems {
    items: Vec<Item>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Item {
    track: Track,
}

#[derive(Debug, Deserialize, Serialize)]
struct Track {
    id: String,
    name: String,
    album: Album,
    artists: Vec<Artist>,
    preview_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Album {
    name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Artist {
    name: String,
}

async fn handle(req: Request<Body>, db: Db) -> Result<Response<Body>, Infallible> {
    Ok(match route(req, db).await {
        Err(e) => {
            eprintln!("server error: {:?}", e);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .expect("empty response builder should work")
        }
        Ok(resp) => resp,
    })
}

async fn route(req: Request<Body>, db: Db) -> Result<Response<Body>, Error> {
    if req.uri().path() == "/login" {
        if req.method() == Method::OPTIONS
            || req.headers().get("Authorization")
                == HeaderValue::from_str(&format!(
                    "Basic {}",
                    std::env::var("AUTH").expect("AUTH is missing")
                ))
                .ok()
                .as_ref()
        {
            get_response_builder()
                .header(
                    "Access-Control-Allow-Headers",
                    HeaderValue::from_static("Authorization"),
                )
                .status(StatusCode::OK)
                .body(Body::empty())
                .map_err(Error::from)
        } else {
            get_response_builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::empty())
                .map_err(Error::from)
        }
    } else if req.uri().path() == "/playlists" {
        get_playlists(db).await
    } else {
        get_playlist(&req, db, &req.uri().path()[1..]).await
    }
}

async fn get_playlists(db: Db) -> Result<Response<Body>, Error> {
    let db = db.lock().unwrap();
    let playlist = Playlists {
        items: db.0.values().cloned().collect(),
    };
    get_response_builder()
        .body(Body::from(serde_json::to_string(&playlist)?))
        .map_err(Error::from)
}

async fn get_playlist(
    req: &Request<Body>,
    db: Db,
    playlist_id: &str,
) -> Result<Response<Body>, Error> {
    let playlist_id = playlist_id.to_owned();
    if let Some(query) = req.uri().query() {
        if let Some((win, lose)) = query.split_once('&') {
            let win = win.to_owned();
            let lose = lose.to_owned();
            let db = &mut db.lock().unwrap().1;
            let win_score = db[&playlist_id][&win].score;
            let lose_score = db[&playlist_id][&lose].score;
            let expected_win = 1. / (1. + 10f64.powf((lose_score - win_score) as f64 / 400.));
            let expected_lose = 1. / (1. + 10f64.powf((win_score - lose_score) as f64 / 400.));
            let win_diff = (32. * (1. - expected_win)) as i32;
            let lose_diff = (32. * expected_lose) as i32;
            db.get_mut(&playlist_id)
                .unwrap()
                .get_mut(&win)
                .unwrap()
                .score += win_diff;
            db.get_mut(&playlist_id)
                .unwrap()
                .get_mut(&lose)
                .unwrap()
                .score -= lose_diff;
        } else {
            return get_response_builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .map_err(Error::from);
        }
    }
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = "https://accounts.spotify.com/api/token".parse().unwrap();
    let resp = client
        .request(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header(
                    "Authorization",
                    &format!(
                        "Basic {}",
                        std::env::var("SPOTIFY_TOKEN").expect("SPOTIFY_TOKEN is missing")
                    ),
                )
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(Body::from("grant_type=client_credentials"))?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    let token: Token = serde_json::from_slice(&got)?;
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = format!(
        "https://api.spotify.com/v1/playlists/{}/tracks",
        playlist_id
    )
    .parse()
    .unwrap();
    let resp = client
        .request(
            Request::builder()
                .uri(uri)
                .header("Authorization", format!("Bearer {}", token.access_token))
                .body(Body::empty())?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    let playlist: PlaylistItems = serde_json::from_slice(&got)?;
    let db = &mut db.lock().unwrap().1;
    let playlist_db = db.entry(playlist_id.clone()).or_insert_with(HashMap::new);
    for i in &playlist.items {
        playlist_db.entry(i.track.id.clone()).or_insert(Score {
            playlist: playlist_id.to_owned(),
            track_id: i.track.id.clone(),
            track: i.track.name.clone(),
            score: 1500,
            preview_url: i.track.preview_url.clone(),
        });
    }
    let scores = Scores {
        scores: playlist_db.values().cloned().collect(),
    };
    get_response_builder()
        .body(Body::from(serde_json::to_string(&scores)?))
        .map_err(Error::from)
}

#[tokio::main]
async fn main() {
    // We'll bind to 127.0.0.1:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // A `Service` is needed for every connection, so this
    // creates one from our `hello_world` function.
    let db = Arc::new(Mutex::new((HashMap::new(), HashMap::new())));
    let make_svc = make_service_fn(move |_conn| {
        let db = Arc::clone(&db);
        async {
            // service_fn converts our function into a `Service`
            Ok::<_, Infallible>(service_fn(move |r| handle(r, Arc::clone(&db))))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);

    // Run this server for... forever!
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

fn get_response_builder() -> Builder {
    Response::builder().header("Access-Control-Allow-Origin", HeaderValue::from_static("*"))
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
enum Error {
    HyperError(hyper::Error),
    RequestError(hyper::http::Error),
    JSONError(serde_json::Error),
}

impl From<hyper::Error> for Error {
    fn from(e: hyper::Error) -> Error {
        Error::HyperError(e)
    }
}

impl From<hyper::http::Error> for Error {
    fn from(e: hyper::http::Error) -> Error {
        Error::RequestError(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::JSONError(e)
    }
}
