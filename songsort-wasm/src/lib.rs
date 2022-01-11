#![feature(async_closure)]
use rand::prelude::SliceRandom;
use serde::Deserialize;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    HtmlButtonElement, HtmlIFrameElement, HtmlInputElement, Request, RequestInit, RequestMode,
    Response,
};

#[derive(Deserialize)]
struct Scores {
    scores: Vec<Score>,
}

#[derive(Deserialize)]
struct Score {
    id: String,
    track_id: String,
    track: String,
    user_id: String,
    score: i32,
    wins: i32,
    losses: i32,
}

struct State {
    playlist: String,
    score1: String,
    score2: String,
    auth: String,
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
        .ok_or(JsValue::from("username element missing"))?
        .dyn_into::<HtmlInputElement>()?;
    let password = document
        .get_element_by_id("password")
        .ok_or(JsValue::from("password element missing"))?
        .dyn_into::<HtmlInputElement>()?;
    let a = Closure::wrap(Box::new(move || {
        let state = Rc::clone(&state_ref);
        let auth = base64::encode(format!("{}:{}", username.value(), password.value()));
        state.borrow_mut().auth = auth.clone();
        wasm_bindgen_futures::spawn_local((async || {
            let window = web_sys::window().expect("no global `window` exists");
            let url = format!("https://branlandapp.com/api/login");
            let request = query(&url, &state.borrow().auth).unwrap();
            let resp_value = JsFuture::from(window.fetch_with_request(&request))
                .await
                .unwrap();
            let resp: Response = resp_value.dyn_into().unwrap();
            if resp.status() == 401 {
                window.alert_with_message("Invalid username or password");
            } else {
                refresh(state).await;
            }
        })())
    }) as Box<dyn FnMut()>);
    document
        .get_element_by_id("login")
        .ok_or(JsValue::from("login element missing"))?
        .dyn_into::<HtmlButtonElement>()?
        .set_onclick(Some(a.as_ref().unchecked_ref()));
    a.forget();
    let state_ref = Rc::clone(&state);
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
        .ok_or(JsValue::from("score1 element missing"))?
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
        .ok_or(JsValue::from("score2 element missing"))?
        .dyn_into::<HtmlButtonElement>()?
        .set_onclick(Some(a.as_ref().unchecked_ref()));
    a.forget();
    let state_ref = Rc::clone(&state);
    let input = document
        .get_element_by_id("input")
        .ok_or(JsValue::from("input element missing"))?
        .dyn_into::<HtmlInputElement>()?;
    let a = Closure::wrap(Box::new(move || {
        let state = Rc::clone(&state_ref);
        state.borrow_mut().playlist = input.value();
        wasm_bindgen_futures::spawn_local((async move || {
            let window = web_sys::window().expect("no global `window` exists");
            let url = format!("https://branlandapp.com/api/{}", state.borrow().playlist);
            let mut opts = RequestInit::new();
            opts.method("POST");
            opts.mode(RequestMode::Cors);
            let request = Request::new_with_str_and_init(&url, &opts).unwrap();
            request
                .headers()
                .set("Authorization", &format!("Basic {}", state.borrow().auth))
                .unwrap();
            JsFuture::from(window.fetch_with_request(&request))
                .await
                .unwrap();
        })())
    }) as Box<dyn FnMut()>);
    document
        .get_element_by_id("import")
        .ok_or(JsValue::from("import element missing"))?
        .dyn_into::<HtmlButtonElement>()?
        .set_onclick(Some(a.as_ref().unchecked_ref()));
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

async fn refresh(state: Rc<RefCell<State>>) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let url = format!("https://branlandapp.com/api/{}", state.borrow().playlist);
    let request = query(&url, &state.borrow().auth)?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    let scores: Scores = json.into_serde().unwrap();
    refresh_state(state, scores)?;
    Ok(())
}

fn refresh_state(state: Rc<RefCell<State>>, mut scores: Scores) -> Result<(), JsValue> {
    if scores.scores.is_empty() {
        return Ok(());
    }
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    scores.scores.sort_by_key(|s| -s.score);
    let element = document
        .get_element_by_id("scores")
        .ok_or(JsValue::from("scores element missing"))?;
    while let Some(child) = element.first_element_child() {
        child.remove();
    }
    for score in &scores.scores {
        let val = document.create_element("li")?;
        val.set_text_content(Some(&format!("{} {}-{} {}", score.track, score.wins, score.losses, score.score)));
        element.append_child(&val)?;
    }
    let scores: Vec<_> = scores
        .scores
        .choose_multiple(&mut rand::thread_rng(), 2)
        .collect();
    state.borrow_mut().score1 = scores[0].track_id.clone();
    state.borrow_mut().score2 = scores[1].track_id.clone();
    document
        .get_element_by_id("iframe1")
        .ok_or(JsValue::from("iframe1 element missing"))?
        .dyn_into::<HtmlIFrameElement>()?
        .set_src(&format!(
            "https://open.spotify.com/embed/track/{}?utm_source=generator",
            scores[0].track_id
        ));
    document
        .get_element_by_id("iframe2")
        .ok_or(JsValue::from("iframe2 element missing"))?
        .dyn_into::<HtmlIFrameElement>()?
        .set_src(&format!(
            "https://open.spotify.com/embed/track/{}?utm_source=generator",
            scores[1].track_id
        ));
    document
        .get_element_by_id("track1")
        .ok_or(JsValue::from("track1 element missing"))?
        .set_text_content(Some(&scores[0].track));
    document
        .get_element_by_id("track2")
        .ok_or(JsValue::from("track2 element missing"))?
        .set_text_content(Some(&scores[1].track));
    Ok(())
}

fn query(url: &str, auth: &str) -> Result<Request, JsValue> {
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);
    let request = Request::new_with_str_and_init(&url, &opts)?;
    request
        .headers()
        .set("Authorization", &format!("Basic {}", auth))?;
    Ok(request)
}
