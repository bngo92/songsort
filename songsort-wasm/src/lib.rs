#![feature(async_closure)]
use rand::prelude::SliceRandom;
use serde::Deserialize;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Element, HtmlAnchorElement, HtmlButtonElement, HtmlIFrameElement, HtmlInputElement, Request,
    RequestInit, RequestMode, Response,
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

pub struct State {
    pub playlist: String,
    pub score1: String,
    pub score2: String,
    pub auth: String,
}

// Called by our JS entry point to run the example
#[wasm_bindgen(start)]
pub async fn run() -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let state = Rc::new(RefCell::new(State {
        playlist: String::from("5jPjYAdQO0MgzHdwSmYPNZ"),
        score1: String::new(),
        score2: String::new(),
        auth: String::new(),
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
            let request = query("https://branlandapp.com/api/login", &state.borrow().auth).unwrap();
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
                generate_playlists_page(state).await;
            }
        })
    }) as Box<dyn FnMut()>);
    document
        .get_element_by_id("login")
        .ok_or_else(|| JsValue::from("login element missing"))?
        .dyn_into::<HtmlButtonElement>()?
        .set_onclick(Some(a.as_ref().unchecked_ref()));
    a.forget();
    /*let state_ref = Rc::clone(&state);
    let a = Closure::wrap(Box::new(move || {
        let state = Rc::clone(&state_ref);
        let url = format!(
            "https://branlandapp.com/api/{}?{}&{}",
            state.borrow().playlist,
            state.borrow().score1,
            state.borrow().score2
        );
        wasm_bindgen_futures::spawn_local((async || play(state, url).await.unwrap())())
    }) as Box<dyn FnMut()>);
    document
        .get_element_by_id("score1")
        .ok_or_else(||JsValue::from("score1 element missing"))?
        .dyn_into::<HtmlButtonElement>()?
        .set_onclick(Some(a.as_ref().unchecked_ref()));
    a.forget();
    let state_ref = Rc::clone(&state);
    let a = Closure::wrap(Box::new(move || {
        let state = Rc::clone(&state_ref);
        let url = format!(
            "https://branlandapp.com/api/{}?{}&{}",
            state.borrow().playlist,
            state.borrow().score2,
            state.borrow().score1
        );
        wasm_bindgen_futures::spawn_local((async || play(state, url).await.unwrap())())
    }) as Box<dyn FnMut()>);
    document
        .get_element_by_id("score2")
        .ok_or_else(||JsValue::from("score2 element missing"))?
        .dyn_into::<HtmlButtonElement>()?
        .set_onclick(Some(a.as_ref().unchecked_ref()));
    a.forget();*/

    Ok(())
}

async fn generate_playlists_page(state: Rc<RefCell<State>>) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let main = document
        .get_element_by_id("main")
        .ok_or_else(|| JsValue::from("main element missing"))?;
    let header = document.create_element("h1")?;
    header.set_text_content(Some("Playlists"));
    main.append_child(&header)?;
    let form = document.create_element("form")?;
    let playlists = document.create_element("div")?;
    load_playlists(Rc::clone(&state), &playlists).await?;
    form.append_child(&playlists)?;
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
    form.append_child(&row)?;
    main.append_child(&form)?;
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
            let url = format!("https://branlandapp.com/api/{}", input.value());
            let mut opts = RequestInit::new();
            opts.method("POST");
            opts.mode(RequestMode::Cors);
            let request = Request::new_with_str_and_init(&url, &opts).unwrap();
            request
                .headers()
                .set("Authorization", &format!("Basic {}", state.borrow().auth))
                .unwrap();
            let resp_value = JsFuture::from(window.fetch_with_request(&request))
                .await
                .unwrap();
            let resp: Response = resp_value.dyn_into().unwrap();
            if resp.status() == 200 {}
        })
    }) as Box<dyn FnMut()>);
    import.set_onclick(Some(a.as_ref().unchecked_ref()));
    a.forget();
    Ok(())
}

async fn play(state: Rc<RefCell<State>>, url: String) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&url, &state.borrow().auth)?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    let scores: Scores = json.into_serde().unwrap();
    refresh_state(state, scores)?;
    Ok(())
}

async fn load_playlists(
    state: Rc<RefCell<State>>,
    playlists_element: &Element,
) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let request = query(
        "https://branlandapp.com/api/playlists",
        &state.borrow().auth,
    )?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    let playlists: Playlists = json.into_serde().unwrap();
    while let Some(child) = playlists_element.first_element_child() {
        child.remove();
    }
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
        button.set_type("submit");
        button.set_class_name("btn btn-success col-1 me-2");
        button.set_text_content(Some("Go"));
        row.append_child(&button)?;
        let button = document
            .create_element("button")?
            .dyn_into::<HtmlButtonElement>()?;
        button.set_type("button");
        button.set_class_name("btn btn-danger col-1");
        button.set_text_content(Some("Delete"));
        row.append_child(&button)?;
        playlists_element.append_child(&row)?;
    }
    Ok(())
}

fn refresh_state(state: Rc<RefCell<State>>, mut scores: Scores) -> Result<(), JsValue> {
    if scores.scores.is_empty() {
        return Ok(());
    }
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
            record.set_text_content(Some(&format!("{}-{}", scores[0].wins, scores[0].losses)));
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
    let scores: Vec<_> = scores
        .scores
        .choose_multiple(&mut rand::thread_rng(), 2)
        .collect();
    state.borrow_mut().score1 = scores[0].track_id.clone();
    state.borrow_mut().score2 = scores[1].track_id.clone();
    document
        .get_element_by_id("iframe1")
        .ok_or_else(|| JsValue::from("iframe1 element missing"))?
        .dyn_into::<HtmlIFrameElement>()?
        .set_src(&format!(
            "https://open.spotify.com/embed/track/{}?utm_source=generator",
            scores[0].track_id
        ));
    document
        .get_element_by_id("iframe2")
        .ok_or_else(|| JsValue::from("iframe2 element missing"))?
        .dyn_into::<HtmlIFrameElement>()?
        .set_src(&format!(
            "https://open.spotify.com/embed/track/{}?utm_source=generator",
            scores[1].track_id
        ));
    document
        .get_element_by_id("track1")
        .ok_or_else(|| JsValue::from("track1 element missing"))?
        .set_text_content(Some(&scores[0].track));
    document
        .get_element_by_id("track2")
        .ok_or_else(|| JsValue::from("track2 element missing"))?
        .set_text_content(Some(&scores[1].track));
    Ok(())
}

fn query(url: &str, auth: &str) -> Result<Request, JsValue> {
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);
    let request = Request::new_with_str_and_init(url, &opts)?;
    request
        .headers()
        .set("Authorization", &format!("Basic {}", auth))?;
    Ok(request)
}
