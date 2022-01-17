#![feature(async_closure)]
use rand::prelude::SliceRandom;
use regex::Regex;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Document, Element, HtmlAnchorElement, HtmlButtonElement, HtmlIFrameElement, HtmlInputElement,
    Request, RequestInit, RequestMode, Response,
};

#[derive(Deserialize)]
pub struct Scores {
    pub scores: Vec<Score>,
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
struct Playlists {
    items: Vec<Playlist>,
}

#[derive(Deserialize)]
pub struct Playlist {
    pub id: String,
    pub playlist_id: String,
    pub name: String,
    pub user_id: String,
    pub tracks: Vec<String>,
}

struct State {
    playlist: Option<String>,
    auth: String,
    home: Option<Element>,
    queued_scores: Vec<Score>,
}

// Called by our JS entry point to run the example
#[wasm_bindgen(start)]
pub async fn run() -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let state = Rc::new(RefCell::new(State {
        playlist: None,
        auth: String::new(),
        home: None,
        queued_scores: Vec::new(),
    }));
    let state_ref = Rc::clone(&state);
    let username = document
        .get_element_by_id("username")
        .ok_or_else(|| JsValue::from("username element missing"))?
        .dyn_into::<HtmlInputElement>()?;
    let password = document
        .get_element_by_id("password")
        .ok_or_else(|| JsValue::from("password element missing"))?
        .dyn_into::<HtmlInputElement>()?;
    let a = Closure::wrap(Box::new(move || {
        let state = Rc::clone(&state_ref);
        state.borrow_mut().auth =
            base64::encode(format!("{}:{}", username.value(), password.value()));
        wasm_bindgen_futures::spawn_local(async {
            let window = web_sys::window().expect("no global `window` exists");
            let document = window.document().expect("should have a document on window");
            let request = query(
                "https://branlandapp.com/api/login",
                "POST",
                &state.borrow().auth,
            )
            .unwrap();
            let resp_value = JsFuture::from(window.fetch_with_request(&request))
                .await
                .unwrap();
            let resp: Response = resp_value.dyn_into().unwrap();
            if resp.status() == 401 {
                window
                    .alert_with_message("Invalid username or password")
                    .expect("alert");
            } else {
                // TODO: clean up login elements
                document
                    .get_element_by_id("login")
                    .ok_or_else(|| JsValue::from("login element missing"))
                    .unwrap()
                    .dyn_into::<HtmlButtonElement>()
                    .unwrap()
                    .set_onclick(None);
                generate_home_page(state).await;
            }
        })
    }) as Box<dyn FnMut()>);
    document
        .get_element_by_id("login")
        .ok_or_else(|| JsValue::from("login element missing"))?
        .dyn_into::<HtmlButtonElement>()?
        .set_onclick(Some(a.as_ref().unchecked_ref()));
    a.forget();
    Ok(())
}

async fn generate_home_page(state: Rc<RefCell<State>>) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let request = query(
        "https://branlandapp.com/api/scores",
        "GET",
        &state.borrow().auth,
    )
    .unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .unwrap();
    let resp: Response = resp_value.dyn_into().unwrap();
    let json = JsFuture::from(resp.json()?).await?;
    let scores: Scores = json.into_serde().unwrap();
    let mut artists = HashMap::new();
    for s in &scores.scores {
        artists
            .entry(s.artists.join(", "))
            .or_insert_with(Vec::new)
            .push(s.score);
    }
    let mut artists: Vec<_> = artists
        .iter()
        .map(|(k, v)| (k.as_str(), v.iter().sum::<i32>() / v.len() as i32))
        .collect();
    artists.sort_by_key(|(_, v)| -*v);
    let mut albums = HashMap::new();
    for s in &scores.scores {
        albums
            .entry(s.album.clone())
            .or_insert_with(Vec::new)
            .push(s.score);
    }
    let mut albums: Vec<_> = albums
        .iter()
        .map(|(k, v)| (k.as_str(), v.iter().sum::<i32>() / v.len() as i32))
        .collect();
    albums.sort_by_key(|(_, v)| -*v);
    let main = document
        .get_element_by_id("main")
        .ok_or_else(|| JsValue::from("main element missing"))?;
    let home = document.create_element("div")?;
    home.set_id("home");
    let header = document.create_element("h1")?;
    header.set_text_content(Some("Playlists"));
    home.append_child(&header)?;
    let form = document.create_element("form")?;
    let playlists = document.create_element("div")?;
    playlists.set_id("playlists");
    form.append_child(&playlists)?;
    home.append_child(&form)?;
    main.append_child(&home)?;
    load_playlists(Rc::clone(&state)).await?;
    let row = document.create_element("div")?;
    row.set_class_name("row");
    let input = document
        .create_element("input")?
        .dyn_into::<HtmlInputElement>()?;
    input.set_type("text");
    input.set_id("input");
    input.set_value("https://open.spotify.com/playlist/5jPjYAdQO0MgzHdwSmYPNZ?si=05d659645f2d4781");
    input.set_class_name("col-7");
    row.append_child(&input)?;
    let import = document
        .create_element("button")?
        .dyn_into::<HtmlButtonElement>()?;
    import.set_type("button");
    import.set_class_name("col-1 offset-2 btn btn-success");
    import.set_text_content(Some("Import"));
    row.append_child(&import)?;
    let header = document.create_element("h1")?;
    header.set_text_content(Some("Stats"));
    home.append_child(&header)?;
    form.append_child(&row)?;
    let row = document.create_element("div")?;
    row.set_class_name("row");
    let left = document.create_element("div")?;
    left.set_class_name("col-6");
    let header = document.create_element("h2")?;
    header.set_text_content(Some("Artists"));
    left.append_child(&header)?;
    let table = document.create_element("table")?;
    table.set_class_name("table table-striped");
    let head = document.create_element("thead")?;
    let tr = document.create_element("tr")?;
    tr.append_child(create_th(&document, "col-1", "#")?.as_ref())?;
    tr.append_child(create_th(&document, "col-8", "Artist")?.as_ref())?;
    tr.append_child(create_th(&document, "", "Score")?.as_ref())?;
    head.append_child(&tr)?;
    table.append_child(&head)?;
    let body = document.create_element("tbody")?;
    for (i, (artist, score)) in artists.iter().enumerate() {
        let row = document.create_element("tr")?;
        let num = document.create_element("th")?;
        num.set_text_content(Some(&(i + 1).to_string()));
        row.append_child(&num)?;
        let track = document.create_element("td")?;
        track.set_text_content(Some(artist));
        row.append_child(&track)?;
        let score_element = document.create_element("td")?;
        score_element.set_text_content(Some(&score.to_string()));
        row.append_child(&score_element)?;
        body.append_child(&row)?;
    }
    table.append_child(&body)?;
    left.append_child(&table)?;
    row.append_child(&left)?;
    home.append_child(&row)?;
    let right = document.create_element("div")?;
    right.set_class_name("col-6");
    let header = document.create_element("h2")?;
    header.set_text_content(Some("Albums"));
    right.append_child(&header)?;
    let table = document.create_element("table")?;
    table.set_class_name("table table-striped");
    let head = document.create_element("thead")?;
    let tr = document.create_element("tr")?;
    tr.append_child(create_th(&document, "col-1", "#")?.as_ref())?;
    tr.append_child(create_th(&document, "col-8", "Album")?.as_ref())?;
    tr.append_child(create_th(&document, "", "Score")?.as_ref())?;
    head.append_child(&tr)?;
    table.append_child(&head)?;
    let body = document.create_element("tbody")?;
    for (i, (album, score)) in albums.iter().enumerate() {
        let row = document.create_element("tr")?;
        let num = document.create_element("th")?;
        num.set_text_content(Some(&(i + 1).to_string()));
        row.append_child(&num)?;
        let track = document.create_element("td")?;
        track.set_text_content(Some(album));
        row.append_child(&track)?;
        let score_element = document.create_element("td")?;
        score_element.set_text_content(Some(&score.to_string()));
        row.append_child(&score_element)?;
        body.append_child(&row)?;
    }
    table.append_child(&body)?;
    right.append_child(&table)?;
    row.append_child(&right)?;
    home.append_child(&row)?;
    let a = Closure::wrap(Box::new(move || {
        let state = Rc::clone(&state);
        wasm_bindgen_futures::spawn_local(async move {
            let window = web_sys::window().expect("no global `window` exists");
            let document = window.document().expect("should have a document on window");
            let input = document
                .get_element_by_id("input")
                .unwrap()
                .dyn_into::<HtmlInputElement>()
                .unwrap();
            let mut input = input.value();
            let re = Regex::new(r"https://open.spotify.com/playlist/([[:alnum:]]*)").unwrap();
            if let Some(id) = re.captures_iter(&input).next() {
                input = id[1].to_owned()
            }
            let url = format!("https://branlandapp.com/api/playlists/{}", input);
            let request = query(&url, "POST", &state.borrow().auth).unwrap();
            let resp_value = JsFuture::from(window.fetch_with_request(&request))
                .await
                .unwrap();
            let resp: Response = resp_value.dyn_into().unwrap();
            if resp.status() == 201 {
                load_playlists(state).await;
            }
            // TODO: error handling
        })
    }) as Box<dyn FnMut()>);
    import.set_onclick(Some(a.as_ref().unchecked_ref()));
    a.forget();
    Ok(())
}

async fn load_playlists(state: Rc<RefCell<State>>) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let playlists_element = document
        .get_element_by_id("playlists")
        .ok_or_else(|| JsValue::from("playlists element missing"))?;
    let request = query(
        "https://branlandapp.com/api/playlists",
        "GET",
        &state.borrow().auth,
    )?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    let playlists: Playlists = json.into_serde().unwrap();
    while let Some(child) = playlists_element.first_element_child() {
        child.remove();
    }
    if playlists.items.is_empty() {
        let p = document.create_element("p")?;
        p.set_text_content(Some(
            "Import a playlist and choose an option to start sorting songs!",
        ));
        playlists_element.append_child(&p)?;
    } else {
        for p in playlists.items {
            let row = document.create_element("div")?;
            row.set_class_name("row");
            let label = document.create_element("label")?;
            label.set_class_name("col-7 col-form-label");
            let link = document
                .create_element("a")?
                .dyn_into::<HtmlAnchorElement>()?;
            link.set_text_content(Some(&p.name));
            link.set_href(&format!(
                "https://open.spotify.com/playlist/{}",
                p.playlist_id
            ));
            label.append_child(&link)?;
            row.append_child(&label)?;
            let div = document.create_element("div")?;
            div.set_class_name("col-2");
            let select = document.create_element("select")?;
            select.set_class_name("form-select");
            let option = document.create_element("option")?;
            option.set_text_content(Some("Random match"));
            select.append_child(&option)?;
            div.append_child(&select)?;
            row.append_child(&div)?;
            let button = document
                .create_element("button")?
                .dyn_into::<HtmlButtonElement>()?;
            button.set_type("button");
            button.set_class_name("btn btn-success col-1 me-2");
            button.set_text_content(Some("Go"));
            let state_ref = Rc::clone(&state);
            let id = p.id.clone();
            let a = Closure::wrap(Box::new(move || {
                let state = Rc::clone(&state_ref);
                let id = id.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let window = web_sys::window().expect("no global `window` exists");
                    let url = format!("https://branlandapp.com/api/playlists/{}/scores", id);
                    let request = query(&url, "GET", &state.borrow().auth).unwrap();
                    let resp_value = JsFuture::from(window.fetch_with_request(&request))
                        .await
                        .unwrap();
                    let resp: Response = resp_value.dyn_into().unwrap();
                    let json = JsFuture::from(resp.json().unwrap()).await.unwrap();
                    let scores: Scores = json.into_serde().unwrap();
                    generate_random_page(state, scores, id);
                });
            }) as Box<dyn FnMut()>);
            button.set_onclick(Some(a.as_ref().unchecked_ref()));
            a.forget();
            row.append_child(&button)?;
            let button = document
                .create_element("button")?
                .dyn_into::<HtmlButtonElement>()?;
            button.set_type("button");
            button.set_class_name("btn btn-danger col-1");
            button.set_text_content(Some("Delete"));
            let state_ref = Rc::clone(&state);
            let a = Closure::wrap(Box::new(move || {
                let state = Rc::clone(&state_ref);
                let id = p.id.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let window = web_sys::window().expect("no global `window` exists");
                    let url = format!("https://branlandapp.com/api/playlists/{}", id);
                    let request = query(&url, "DELETE", &state.borrow().auth).unwrap();
                    JsFuture::from(window.fetch_with_request(&request))
                        .await
                        .unwrap();
                    load_playlists(state).await;
                })
            }) as Box<dyn FnMut()>);
            button.set_onclick(Some(a.as_ref().unchecked_ref()));
            a.forget();
            row.append_child(&button)?;
            playlists_element.append_child(&row)?;
        }
    }
    Ok(())
}

fn generate_random_page(
    state: Rc<RefCell<State>>,
    scores: Scores,
    id: String,
) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    if scores.scores.len() < 2 {
        window
            .alert_with_message("Playlist has less than 2 songs")
            .expect("alert");
        return Ok(());
    }
    state.borrow_mut().playlist = Some(id);
    let navbar = document
        .get_element_by_id("navbar")
        .ok_or_else(|| JsValue::from("navbar element missing"))?;
    let ul = document.create_element("ul")?;
    ul.set_class_name("navbar-nav flex-grow-1");
    let li = document.create_element("li")?;
    li.set_class_name("nav-item");
    let item = document
        .create_element("a")?
        .dyn_into::<HtmlAnchorElement>()?;
    item.set_class_name("nav-link");
    item.set_href("#");
    item.set_text_content(Some("Random match"));
    li.append_child(&item)?;
    ul.append_child(&li)?;
    navbar
        .children()
        .item(0)
        .expect("brand element missing")
        .insert_adjacent_element("afterend", &ul)?;
    let main = document
        .get_element_by_id("main")
        .ok_or_else(|| JsValue::from("main element missing"))?;
    let random = document.create_element("div")?;
    random.set_id("random");
    let header = document.create_element("h1")?;
    header.set_text_content(Some("Random match"));
    random.append_child(&header)?;
    let row = document.create_element("div")?;
    row.set_class_name("row");
    let left = document.create_element("div")?;
    left.set_class_name("col-6");
    let iframe1 = document
        .create_element("iframe")?
        .dyn_into::<HtmlIFrameElement>()?;
    iframe1.set_id("iframe1");
    iframe1.set_width("100%");
    iframe1.set_height("380");
    iframe1.set_frame_border("0");
    left.append_child(&iframe1)?;
    let score1 = document
        .create_element("button")?
        .dyn_into::<HtmlButtonElement>()?;
    score1.set_type("button");
    score1.set_id("score1");
    score1.set_class_name("btn btn-info width");
    let track1 = document.create_element("div")?;
    track1.set_id("track1");
    track1.set_class_name("truncate");
    score1.append_child(&track1)?;
    left.append_child(&score1)?;
    row.append_child(&left)?;
    random.append_child(&row)?;
    let right = document.create_element("div")?;
    right.set_class_name("col-6");
    let iframe2 = document
        .create_element("iframe")?
        .dyn_into::<HtmlIFrameElement>()?;
    iframe2.set_id("iframe2");
    iframe2.set_width("100%");
    iframe2.set_height("380");
    iframe2.set_frame_border("0");
    right.append_child(&iframe2)?;
    let score2 = document
        .create_element("button")?
        .dyn_into::<HtmlButtonElement>()?;
    score2.set_type("button");
    score2.set_id("score2");
    score2.set_class_name("btn btn-warning width");
    let track2 = document.create_element("div")?;
    track2.set_id("track2");
    track2.set_class_name("truncate");
    score2.append_child(&track2)?;
    right.append_child(&score2)?;
    row.append_child(&right)?;
    random.append_child(&row)?;
    let row = document.create_element("div")?;
    row.set_class_name("row");
    let left = document.create_element("div")?;
    left.set_class_name("col-6");
    let table = document.create_element("table")?;
    table.set_class_name("table table-striped");
    let head = document.create_element("thead")?;
    let tr = document.create_element("tr")?;
    tr.append_child(create_th(&document, "col-1", "#")?.as_ref())?;
    tr.append_child(create_th(&document, "col-8", "Track")?.as_ref())?;
    tr.append_child(create_th(&document, "", "Record")?.as_ref())?;
    tr.append_child(create_th(&document, "", "Score")?.as_ref())?;
    head.append_child(&tr)?;
    table.append_child(&head)?;
    let body = document.create_element("tbody")?;
    body.set_id("scores1");
    table.append_child(&body)?;
    left.append_child(&table)?;
    row.append_child(&left)?;
    let right = document.create_element("div")?;
    right.set_class_name("col-6");
    let table = document.create_element("table")?;
    table.set_class_name("table table-striped");
    let head = document.create_element("thead")?;
    let tr = document.create_element("tr")?;
    tr.append_child(create_th(&document, "col-1", "#")?.as_ref())?;
    tr.append_child(create_th(&document, "col-8", "Track")?.as_ref())?;
    tr.append_child(create_th(&document, "", "Record")?.as_ref())?;
    tr.append_child(create_th(&document, "", "Score")?.as_ref())?;
    head.append_child(&tr)?;
    table.append_child(&head)?;
    let body = document.create_element("tbody")?;
    body.set_id("scores2");
    table.append_child(&body)?;
    right.append_child(&table)?;
    row.append_child(&right)?;
    random.append_child(&row)?;
    if let Some(child) = main.first_element_child() {
        child.remove();
        state.borrow_mut().home = Some(child);
    }
    main.append_child(&random)?;
    refresh_scores(state, scores)?;
    Ok(())
}

fn refresh_scores(state: Rc<RefCell<State>>, mut scores: Scores) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    scores.scores.sort_by_key(|s| -s.score);
    let scores1 = document
        .get_element_by_id("scores1")
        .ok_or_else(|| JsValue::from("scores element missing"))?;
    while let Some(child) = scores1.first_element_child() {
        child.remove();
    }
    let scores2 = document
        .get_element_by_id("scores2")
        .ok_or_else(|| JsValue::from("scores element missing"))?;
    while let Some(child) = scores2.first_element_child() {
        child.remove();
    }
    for (i, scores) in scores.scores.chunks_mut(2).enumerate() {
        let row = document.create_element("tr")?;
        let num = document.create_element("th")?;
        num.set_text_content(Some(&(2 * i + 1).to_string()));
        row.append_child(&num)?;
        let track = document.create_element("td")?;
        track.set_text_content(Some(&scores[0].track));
        row.append_child(&track)?;
        let record = document.create_element("td")?;
        record.set_text_content(Some(&format!("{}-{}", scores[0].wins, scores[0].losses)));
        row.append_child(&record)?;
        let score_element = document.create_element("td")?;
        score_element.set_text_content(Some(&scores[0].score.to_string()));
        row.append_child(&score_element)?;
        scores1.append_child(&row)?;

        let second_score = scores.get(1);
        let row = document.create_element("tr")?;
        let num = document.create_element("th")?;
        if second_score.is_some() {
            num.set_text_content(Some(&(2 * i + 2).to_string()));
        }
        row.append_child(&num)?;
        let track = document.create_element("td")?;
        track.set_text_content(second_score.map(|s| s.track.as_ref()));
        row.append_child(&track)?;
        let record = document.create_element("td")?;
        if second_score.is_some() {
            record.set_text_content(Some(&format!("{}-{}", scores[1].wins, scores[1].losses)));
        }
        row.append_child(&record)?;
        let score_element = document.create_element("td")?;
        if let Some(score) = second_score {
            score_element.set_text_content(Some(&score.score.to_string()));
        }
        row.append_child(&record)?;
        row.append_child(&score_element)?;
        scores2.append_child(&row)?;
    }
    let queued_scores = &mut state.borrow_mut().queued_scores;
    match queued_scores.len() {
        // Reload the queue if it's empty
        0 => {
            let mut scores = scores.scores;
            scores.shuffle(&mut rand::thread_rng());
            queued_scores.extend(scores);
        }
        // Always queue the last song next before reloading
        1 => {
            let last = queued_scores.pop().unwrap();
            let mut scores = scores.scores;
            scores.shuffle(&mut rand::thread_rng());
            queued_scores.extend(scores);
            queued_scores.push(last);
        }
        _ => {}
    };
    let track1 = queued_scores.pop().unwrap();
    let track2 = queued_scores.pop().unwrap();
    let state_ref = Rc::clone(&state);
    let url = format!("https://branlandapp.com/api/elo?{}&{}", track1.track_id, track2.track_id);
    let a = Closure::wrap(Box::new(move || {
        let state = Rc::clone(&state_ref);
        let url = url.clone();
        wasm_bindgen_futures::spawn_local(async { elo(state, url).await.unwrap() })
    }) as Box<dyn FnMut()>);
    document
        .get_element_by_id("score1")
        .ok_or_else(|| JsValue::from("score1 element missing"))?
        .dyn_into::<HtmlButtonElement>()?
        .set_onclick(Some(a.as_ref().unchecked_ref()));
    a.forget();
    let state_ref = Rc::clone(&state);
    let url = format!("https://branlandapp.com/api/elo?{}&{}", track2.track_id, track1.track_id);
    let a = Closure::wrap(Box::new(move || {
        let state = Rc::clone(&state_ref);
        let url = url.clone();
        wasm_bindgen_futures::spawn_local(async { elo(state, url).await.unwrap() })
    }) as Box<dyn FnMut()>);
    document
        .get_element_by_id("score2")
        .ok_or_else(|| JsValue::from("score2 element missing"))?
        .dyn_into::<HtmlButtonElement>()?
        .set_onclick(Some(a.as_ref().unchecked_ref()));
    a.forget();
    document
        .get_element_by_id("iframe1")
        .ok_or_else(|| JsValue::from("iframe1 element missing"))?
        .dyn_into::<HtmlIFrameElement>()?
        .set_src(&format!(
            "https://open.spotify.com/embed/track/{}?utm_source=generator",
            track1.track_id
        ));
    document
        .get_element_by_id("iframe2")
        .ok_or_else(|| JsValue::from("iframe2 element missing"))?
        .dyn_into::<HtmlIFrameElement>()?
        .set_src(&format!(
            "https://open.spotify.com/embed/track/{}?utm_source=generator",
            track2.track_id
        ));
    document
        .get_element_by_id("track1")
        .ok_or_else(|| JsValue::from("track1 element missing"))?
        .set_text_content(Some(&track1.track));
    document
        .get_element_by_id("track2")
        .ok_or_else(|| JsValue::from("track2 element missing"))?
        .set_text_content(Some(&track2.track));
    Ok(())
}

async fn elo(state: Rc<RefCell<State>>, url: String) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&url, "POST", &state.borrow().auth)?;
    JsFuture::from(window.fetch_with_request(&request)).await?;
    let url = format!(
        "https://branlandapp.com/api/playlists/{}/scores",
        state.borrow().playlist.as_deref().expect("playlist")
    );
    let request = query(&url, "GET", &state.borrow().auth)?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    let scores: Scores = json.into_serde().unwrap();
    refresh_scores(state, scores)?;
    Ok(())
}

fn query(url: &str, method: &str, auth: &str) -> Result<Request, JsValue> {
    let mut opts = RequestInit::new();
    opts.method(method);
    opts.mode(RequestMode::Cors);
    let request = Request::new_with_str_and_init(url, &opts)?;
    request
        .headers()
        .set("Authorization", &format!("Basic {}", auth))?;
    Ok(request)
}

fn create_th(document: &Document, class: &str, text: &str) -> Result<Element, JsValue> {
    let th = document.create_element("th")?;
    th.set_class_name(class);
    th.set_text_content(Some(text));
    Ok(th)
}
