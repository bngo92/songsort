#![feature(let_else)]
use azure_core::Context;
use azure_cosmos::prelude::{
    AuthorizationToken, CollectionClient, ConsistencyLevel, CosmosClient, CosmosEntity,
    CosmosOptions, CreateDocumentOptions, DatabaseClient, DeleteDocumentOptions,
    GetDocumentOptions, GetDocumentResponse, Query, ReplaceDocumentOptions,
};
use hyper::header::HeaderValue;
use hyper::http::response::Builder;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode, Uri};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize)]
struct Token {
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Debug, Serialize)]
struct Scores {
    scores: Vec<Score>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Score {
    id: String,
    track_id: String,
    track: String,
    album: String,
    artists: Vec<String>,
    user_id: String,
    score: i32,
    wins: i32,
    losses: i32,
}

impl<'a> CosmosEntity<'a> for Score {
    type Entity = &'a str;

    fn partition_key(&'a self) -> Self::Entity {
        self.user_id.as_ref()
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Playlists {
    items: Vec<Playlist>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Playlist {
    id: String,
    playlist_id: String,
    name: String,
    user_id: String,
    tracks: Vec<String>,
}

impl<'a> CosmosEntity<'a> for Playlist {
    type Entity = &'a str;

    fn partition_key(&'a self) -> Self::Entity {
        self.user_id.as_ref()
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct User {
    id: String,
    user_id: String,
    auth: String,
    access_token: String,
    refresh_token: String,
}

impl<'a> CosmosEntity<'a> for User {
    type Entity = &'a str;

    fn partition_key(&'a self) -> Self::Entity {
        self.user_id.as_ref()
    }
}

async fn handle(
    db: CosmosClient,
    req: Request<Body>,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
) -> Result<Response<Body>, Infallible> {
    Ok(match route(db, req, session).await {
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

async fn route(
    db: CosmosClient,
    req: Request<Body>,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
) -> Result<Response<Body>, Error> {
    let db = db.into_database_client("songsort");
    eprintln!("{}", req.uri().path());
    if let Some(path) = req.uri().path().strip_prefix("/api/") {
        let path: Vec<_> = path.split('/').collect();
        if req.method() == Method::OPTIONS {
            return get_response_builder()
                .header(
                    "Access-Control-Allow-Headers",
                    HeaderValue::from_static("Authorization"),
                )
                .header(
                    "Access-Control-Allow-Methods",
                    HeaderValue::from_static("GET,POST,DELETE"),
                )
                .status(StatusCode::OK)
                .body(Body::empty())
                .map_err(Error::from);
        }
        let Some(auth) = req.headers().get("Authorization") else {
            return get_response_builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::empty())
                .map_err(Error::from)};
        let Some((_, auth)) = auth.to_str().expect("auth to be ASCII").split_once(' ') else {
            return get_response_builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .map_err(Error::from);
        };
        let user_id = if auth == std::env::var("AUTH").expect("AUTH is missing") {
            req.headers()["x-real-ip"]
                .to_str()
                .expect("x-real-ip to be ASCII")
                .to_owned()
        } else if let Ok(user_id) = login(db.clone(), &session, auth, {
            let uri: Uri = req.headers()["Referer"]
                .to_str()
                .expect("Referer to be ASCII")
                .parse()
                .expect("referer URI");
            &format!(
                "{}://{}",
                uri.scheme().expect("scheme"),
                uri.authority().expect("authority")
            )
        })
        .await
        {
            user_id
        } else {
            return get_response_builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::empty())
                .map_err(Error::from);
        };
        match (&path[..], req.method()) {
            (["login"], &Method::POST) => get_response_builder()
                .header(
                    "Access-Control-Allow-Headers",
                    HeaderValue::from_static("Authorization"),
                )
                .status(StatusCode::OK)
                .body(Body::empty())
                .map_err(Error::from),
            (["playlists"], &Method::GET) => get_playlists(db, session, user_id).await,
            (["playlists", playlist_id], &Method::POST) => {
                import_playlist(db, session, user_id, playlist_id).await
            }
            (["playlists", id], &Method::DELETE) => delete_playlist(db, session, user_id, id).await,
            (["playlists", id, "scores"], &Method::GET) => {
                get_playlist_scores(db, session, user_id, id).await
            }
            (["elo"], &Method::POST) => elo(db, session, user_id, req.uri().query()).await,
            (["scores"], &Method::GET) => get_scores(db, session, user_id).await,
            (_, _) => get_response_builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .body(Body::empty())
                .map_err(Error::from),
        }
    } else {
        get_response_builder()
            .header(
                "Access-Control-Allow-Headers",
                HeaderValue::from_static("Authorization"),
            )
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .map_err(Error::from)
    }
}

async fn login(
    db: DatabaseClient,
    session: &Arc<RwLock<Option<ConsistencyLevel>>>,
    auth: &str,
    origin: &str,
) -> Result<String, Error> {
    let db = db.into_collection_client("users");
    let query = format!("SELECT * FROM c WHERE c.auth = \"{}\"", auth);
    let query = Query::new(&query);
    let session_copy = session.read().unwrap().clone();
    let (resp, session) = if let Some(session) = session_copy {
        (
            db.query_documents()
                .query_cross_partition(true)
                .parallelize_cross_partition_query(true)
                .consistency_level(session.clone())
                .execute(&query)
                .await?,
            session,
        )
    } else {
        let resp = db
            .query_documents()
            .query_cross_partition(true)
            .parallelize_cross_partition_query(true)
            .execute(&query)
            .await?;
        let token = ConsistencyLevel::Session(resp.session_token.clone());
        *session.write().unwrap() = Some(token.clone());
        (resp, token)
    };
    if let Some(user) = resp
        .into_documents()?
        .results
        .into_iter()
        .map(|r| -> User { r.result })
        .next()
    {
        return Ok(user.id);
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
                .body(Body::from(format!(
                    "grant_type=authorization_code&code={}&redirect_uri={}",
                    auth, origin
                )))?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    let token: Token = serde_json::from_slice(&got)?;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = "https://api.spotify.com/v1/me".parse().unwrap();
    let resp = client
        .request(
            Request::builder()
                .uri(uri)
                .header("Authorization", format!("Bearer {}", token.access_token))
                .body(Body::empty())?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    let user: songsort_web::User = serde_json::from_slice(&got)?;

    let user = User {
        id: Uuid::new_v4().to_hyphenated().to_string(),
        user_id: user.id,
        auth: auth.to_owned(),
        access_token: token.access_token.clone(),
        refresh_token: token
            .refresh_token
            .expect("Spotify should return refresh token"),
    };
    db.create_document(
        Context::new(),
        &user,
        CreateDocumentOptions::new().consistency_level(session),
    )
    .await?;
    Ok(user.id)
}

async fn get_playlists(
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
    user_id: String,
) -> Result<Response<Body>, Error> {
    let db = db.into_collection_client("playlists");
    let query = format!("SELECT * FROM c WHERE c.user_id = \"{}\"", user_id);
    let query = Query::new(&query);
    let session_copy = session.read().unwrap().clone();
    let resp = if let Some(session) = session_copy {
        db.query_documents()
            .consistency_level(session)
            .execute(&query)
            .await?
    } else {
        let resp = db.query_documents().execute(&query).await?;
        *session.write().unwrap() = Some(ConsistencyLevel::Session(resp.session_token.clone()));
        resp
    };
    let playlists = Playlists {
        items: resp
            .into_documents()?
            .results
            .into_iter()
            .map(|r| r.result)
            .collect(),
    };
    get_response_builder()
        .body(Body::from(serde_json::to_string(&playlists)?))
        .map_err(Error::from)
}

async fn delete_playlist(
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
    user_id: String,
    id: &str,
) -> Result<Response<Body>, Error> {
    let session_copy = session.read().unwrap().clone();
    if let Some(session) = session_copy {
        db.into_collection_client("playlists")
            .into_document_client(id, &user_id)?
            .delete_document(
                Context::new(),
                DeleteDocumentOptions::new().consistency_level(session),
            )
            .await?;
    } else {
        let resp = db
            .into_collection_client("playlists")
            .into_document_client(id, &user_id)?
            .delete_document(Context::new(), DeleteDocumentOptions::new())
            .await?;
        *session.write().unwrap() = Some(ConsistencyLevel::Session(resp.session_token));
    }
    get_response_builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .map_err(Error::from)
}

async fn elo(
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
    user_id: String,
    query: Option<&str>,
) -> Result<Response<Body>, Error> {
    if let Some((win, lose)) = query.and_then(|s| s.split_once('&')) {
        let client = db.clone().into_collection_client("scores");
        let scores =
            get_score_docs(client.clone(), &session, user_id.clone(), &[win, lose]).await?;
        let mut iter = scores.into_iter();
        if let (Some(win_score), Some(lose_score)) = (iter.next(), iter.next()) {
            let (mut win_score, mut lose_score) = if win_score.track_id == win {
                (win_score, lose_score)
            } else {
                (lose_score, win_score)
            };
            let expected_win =
                1. / (1. + 10f64.powf((lose_score.score - win_score.score) as f64 / 400.));
            let expected_lose =
                1. / (1. + 10f64.powf((win_score.score - lose_score.score) as f64 / 400.));
            let win_diff = (32. * (1. - expected_win)) as i32;
            let lose_diff = (32. * expected_lose) as i32;
            win_score.score += win_diff;
            lose_score.score -= lose_diff;
            win_score.wins += 1;
            lose_score.losses += 1;
            let client1 = client
                .clone()
                .into_document_client(win_score.id.clone(), &win_score.user_id)?;
            let client2 =
                client.into_document_client(lose_score.id.clone(), &lose_score.user_id)?;
            let session = session
                .read()
                .unwrap()
                .clone()
                .expect("session should be set by get_score_docs");
            futures::future::try_join(
                client1.replace_document(
                    Context::new(),
                    &win_score,
                    ReplaceDocumentOptions::new().consistency_level(session.clone()),
                ),
                client2.replace_document(
                    Context::new(),
                    &lose_score,
                    ReplaceDocumentOptions::new().consistency_level(session),
                ),
            )
            .await?;
            get_response_builder()
                .status(StatusCode::OK)
                .body(Body::empty())
                .map_err(Error::from)
        } else {
            get_response_builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .map_err(Error::from)
        }
    } else {
        get_response_builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .map_err(Error::from)
    }
}

async fn get_score_docs(
    db: CollectionClient,
    session: &Arc<RwLock<Option<ConsistencyLevel>>>,
    user_id: String,
    track_ids: &[&str],
) -> Result<Vec<Score>, Error> {
    let query = format!(
        "SELECT * FROM c WHERE c.user_id = \"{}\" AND c.track_id IN ({})",
        user_id,
        track_ids
            .iter()
            .map(|t| format!("\"{}\"", t))
            .collect::<Vec<_>>()
            .join(",")
    );
    let query = Query::new(&query);
    let session_copy = session.read().unwrap().clone();
    let resp = if let Some(session) = session_copy {
        db.query_documents()
            .consistency_level(session)
            .execute(&query)
            .await?
    } else {
        let resp = db.query_documents().execute(&query).await?;
        *session.write().unwrap() = Some(ConsistencyLevel::Session(resp.session_token.clone()));
        resp
    };
    Ok(resp
        .into_documents()?
        .results
        .into_iter()
        .map(|r| r.result)
        .collect())
}

async fn get_playlist_scores(
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
    user_id: String,
    id: &str,
) -> Result<Response<Body>, Error> {
    let client = db
        .clone()
        .into_collection_client("playlists")
        .into_document_client(id, &user_id)?;
    let playlist = if let GetDocumentResponse::Found(playlist) = client
        .get_document::<Playlist>(Context::new(), GetDocumentOptions::new())
        .await?
    {
        playlist.document.document
    } else {
        todo!()
    };

    let db = db.into_collection_client("scores");
    let query = format!(
        "SELECT * FROM c WHERE c.user_id = \"{}\" AND c.track_id IN ({})",
        user_id,
        playlist
            .tracks
            .iter()
            .map(|t| format!("\"{}\"", t))
            .collect::<Vec<_>>()
            .join(",")
    );
    let query = Query::new(&query);
    let session_copy = session.read().unwrap().clone();
    let resp = if let Some(session) = session_copy {
        db.query_documents()
            .consistency_level(session)
            .execute(&query)
            .await?
    } else {
        let resp = db.query_documents().execute(&query).await?;
        *session.write().unwrap() = Some(ConsistencyLevel::Session(resp.session_token.clone()));
        resp
    };
    let scores = Scores {
        scores: resp
            .into_documents()?
            .results
            .into_iter()
            .map(|r| r.result)
            .collect(),
    };
    get_response_builder()
        .body(Body::from(serde_json::to_string(&scores)?))
        .map_err(Error::from)
}

async fn get_scores(
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
    user_id: String,
) -> Result<Response<Body>, Error> {
    let db = db.into_collection_client("scores");
    let query = format!("SELECT * FROM c WHERE c.user_id = \"{}\"", user_id);
    let session_copy = session.read().unwrap().clone();
    let resp = if let Some(session) = session_copy {
        db.query_documents()
            .consistency_level(session)
            .execute(&query)
            .await?
    } else {
        let resp = db.query_documents().execute(&query).await?;
        *session.write().unwrap() = Some(ConsistencyLevel::Session(resp.session_token.clone()));
        resp
    };
    let scores = Scores {
        scores: resp
            .into_documents()?
            .results
            .into_iter()
            .map(|r| r.result)
            .collect(),
    };
    get_response_builder()
        .body(Body::from(serde_json::to_string(&scores)?))
        .map_err(Error::from)
}

async fn import_playlist(
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
    user_id: String,
    playlist_id: &str,
) -> Result<Response<Body>, Error> {
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
    let uri: Uri = format!("https://api.spotify.com/v1/playlists/{}", playlist_id)
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
    let playlist: songsort_web::Playlist = serde_json::from_slice(&got)?;

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
    let playlist_items: songsort_web::PlaylistItems = serde_json::from_slice(&got)?;
    let playlist = Playlist {
        id: Uuid::new_v4().to_hyphenated().to_string(),
        user_id: user_id.clone(),
        playlist_id: playlist_id.to_owned(),
        name: playlist.name,
        tracks: playlist_items
            .items
            .iter()
            .map(|i| i.track.id.clone())
            .collect(),
    };

    let playlist_client = db.clone().into_collection_client("playlists");
    let session_copy = session.read().unwrap().clone();
    let session = if let Some(session) = session_copy {
        playlist_client
            .create_document(
                Context::new(),
                &playlist,
                CreateDocumentOptions::new().consistency_level(session.clone()),
            )
            .await?;
        session
    } else {
        let resp = playlist_client
            .create_document(Context::new(), &playlist, CreateDocumentOptions::new())
            .await?;
        let session_copy = ConsistencyLevel::Session(resp.session_token);
        *session.write().unwrap() = Some(session_copy.clone());
        session_copy
    };
    let score_client = db.into_collection_client("scores");
    for i in &playlist_items.items {
        let score = Score {
            id: Uuid::new_v4().to_hyphenated().to_string(),
            track_id: i.track.id.clone(),
            track: i.track.name.clone(),
            album: i.track.album.name.clone(),
            artists: i.track.artists.iter().map(|a| a.name.clone()).collect(),
            user_id: user_id.clone(),
            score: 1500,
            wins: 0,
            losses: 0,
        };
        score_client
            .create_document(
                Context::new(),
                &score,
                CreateDocumentOptions::new().consistency_level(session.clone()),
            )
            .await
            .map(|_| ())
            .or_else(|e| {
                if let azure_cosmos::Error::Core(azure_core::Error::PolicyError(ref e)) = e {
                    if let Some(azure_core::HttpError::ErrorStatusCode {
                        status: StatusCode::CONFLICT,
                        ..
                    }) = e.downcast_ref::<azure_core::HttpError>()
                    {
                        return Ok(());
                    }
                }
                Err(e)
            })?;
    }
    get_response_builder()
        .status(StatusCode::CREATED)
        .body(Body::empty())
        .map_err(Error::from)
}

#[tokio::main]
async fn main() {
    // We'll bind to 127.0.0.1:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // A `Service` is needed for every connection, so this
    // creates one from our `hello_world` function.
    let master_key =
        std::env::var("COSMOS_MASTER_KEY").expect("Set env variable COSMOS_MASTER_KEY first!");
    let account = std::env::var("COSMOS_ACCOUNT").expect("Set env variable COSMOS_ACCOUNT first!");
    let authorization_token =
        AuthorizationToken::primary_from_base64(&master_key).expect("cosmos config");
    let client = CosmosClient::new(
        account.clone(),
        authorization_token,
        CosmosOptions::default(),
    );
    let session = Arc::new(RwLock::new(None));
    let make_svc = make_service_fn(move |_conn| {
        let client_ref = client.clone();
        let session = Arc::clone(&session);
        async {
            // service_fn converts our function into a `Service`
            Ok::<_, Infallible>(service_fn(move |r| {
                handle(client_ref.clone(), r, Arc::clone(&session))
            }))
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
    CosmosError(azure_cosmos::Error),
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

impl From<azure_cosmos::Error> for Error {
    fn from(e: azure_cosmos::Error) -> Error {
        Error::CosmosError(e)
    }
}
