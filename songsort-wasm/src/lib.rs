#![feature(async_closure)]
use rand::prelude::SliceRandom;
use regex::Regex;
use songsort::{Playlists, Score, Scores};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Document, Element, HtmlAnchorElement, HtmlButtonElement, HtmlIFrameElement, HtmlInputElement,
    Request, RequestInit, RequestMode, Response, UrlSearchParams, Window,
};

struct State {
    current_page: Page,
    playlist: Option<String>, // TODO: do we still need this?
    auth: String,
    home: Option<Element>,
    random_match: Option<Element>,
    queued_scores: Vec<Score>,
}

#[derive(PartialEq)]
enum Page {
    Login,
    Home,
    RandomMatch(String),
}

// Called by our JS entry point to run the example
#[wasm_bindgen(start)]
pub async fn run() -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let state = Rc::new(RefCell::new(State {
        current_page: Page::Login,
        playlist: None,
        auth: String::new(),
        home: None,
        random_match: None,
        queued_scores: Vec::new(),
    }));
    let state_ref = Rc::clone(&state);
    let a = Closure::wrap(Box::new(move || {
        let state = Rc::clone(&state_ref);
        wasm_bindgen_futures::spawn_local(async {
            switch_pages(state, Page::Home).await.unwrap();
        });
        true
    }) as Box<dyn FnMut() -> bool>);
    document
        .get_element_by_id("brand")
        .ok_or_else(|| JsValue::from("brand element missing"))?
        .dyn_into::<HtmlAnchorElement>()?
        .set_onclick(Some(a.as_ref().unchecked_ref()));
    a.forget();
    let q = window.location().search()?;
    let params = UrlSearchParams::new_with_str(&q)?;
    if let Some(code) = params.get("code") {
        state.borrow_mut().auth = code.clone();
        let request = query("/api/login", "POST", &state.borrow().auth)?;
        let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
        let resp: Response = resp_value.dyn_into()?;
        if resp.status() == 401 {
            window.alert_with_message("Please contact bngo92@gmail.com for support")?;
        } else {
            switch_pages(state, Page::Home).await?;
        }
    } else {
        let a = Closure::wrap(Box::new(move || {
            let window = web_sys::window().expect("no global `window` exists");
            let location = window.location();
            location.set_href(&format!("https://accounts.spotify.com/authorize?client_id=ee3d1b4f8d80477ea48743a511ef3018&redirect_uri={}&response_type=code", location.origin().unwrap().as_str())).unwrap();
        }) as Box<dyn FnMut()>);
        document
            .get_element_by_id("login")
            .ok_or_else(|| JsValue::from("login element missing"))?
            .dyn_into::<HtmlButtonElement>()?
            .set_onclick(Some(a.as_ref().unchecked_ref()));
        a.forget();
        let state_ref = Rc::clone(&state);
        let a = Closure::wrap(Box::new(move || {
            let state = Rc::clone(&state_ref);
            state.borrow_mut().auth = String::from("demo");
            wasm_bindgen_futures::spawn_local(async {
                switch_pages(state, Page::Home).await.unwrap();
            });
        }) as Box<dyn FnMut()>);
        document
            .get_element_by_id("demo")
            .ok_or_else(|| JsValue::from("demo element missing"))?
            .dyn_into::<HtmlButtonElement>()?
            .set_onclick(Some(a.as_ref().unchecked_ref()));
        a.forget();
    };
    Ok(())
}

async fn switch_pages(state: Rc<RefCell<State>>, next_page: Page) -> Result<(), JsValue> {
    if state.borrow().current_page == next_page {
        return Ok(());
    }
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let main = document
        .get_element_by_id("main")
        .ok_or_else(|| JsValue::from("main element missing"))?;
    let navbar = document
        .get_element_by_id("navbar")
        .ok_or_else(|| JsValue::from("navbar element missing"))?;
    let mut borrowed_state = state.borrow_mut();
    // TODO: Clean up closures and one-off elements
    match borrowed_state.current_page {
        Page::Home => {
            if let Some(child) = main.first_element_child() {
                child.remove();
                borrowed_state.home = Some(child);
            }
        }
        Page::RandomMatch(_) => {
            if let Some(child) = main.first_element_child() {
                child.remove();
                borrowed_state.random_match = Some(child);
            }
            navbar.children().item(1).unwrap().remove();
            borrowed_state.queued_scores.clear();
        }
        Page::Login => {
            if let Some(child) = main.first_element_child() {
                child.remove();
            }
        }
    }
    drop(borrowed_state);
    match next_page {
        Page::Home => {
            let mut borrowed_state = state.borrow_mut();
            let element = if let Some(element) = borrowed_state.home.take() {
                element
            } else {
                web_sys::console::log_1(&JsValue::from("Generating home page"));
                generate_home_page(&state, borrowed_state.auth == "demo").await?
            };
            main.append_child(&element)?;
            borrowed_state.current_page = Page::Home;
            drop(borrowed_state);
            refresh_home_page(state).await?;
        }
        Page::RandomMatch(id) => {
            let mut borrowed_state = state.borrow_mut();
            let element = if let Some(element) = borrowed_state.random_match.take() {
                element
            } else {
                web_sys::console::log_1(&JsValue::from("Generating random match"));
                generate_random_page()?
            };
            main.append_child(&element)?;
            // TODO: Cache navbar element
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
            borrowed_state.current_page = Page::RandomMatch(id.clone());
            borrowed_state.playlist = Some(id.clone());
            drop(borrowed_state);
            let scores = fetch_scores(&window, &state, &id).await?;
            refresh_scores(state, scores)?;
        }
        Page::Login => {
            unreachable!()
        }
    }
    Ok(())
}

async fn generate_home_page(state: &Rc<RefCell<State>>, demo: bool) -> Result<Element, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let home = document.create_element("div")?;
    home.set_id("home");
    let header = document.create_element("h1")?;
    header.set_text_content(Some("Saved Playlists"));
    home.append_child(&header)?;
    let playlists = document.create_element("div")?;
    playlists.set_id("playlists");
    home.append_child(&playlists)?;
    let row = document.create_element("div")?;
    row.set_class_name("row");
    let form = document.create_element("form")?;
    let form_row = document.create_element("div")?;
    form_row.set_class_name("row");
    let input = document
        .create_element("input")?
        .dyn_into::<HtmlInputElement>()?;
    input.set_type("text");
    input.set_id("input");
    input.set_value("https://open.spotify.com/playlist/37i9dQZF1DX49jUV2NfGku?si=379bbc586c78450a");
    input.set_class_name("col-7");
    form_row.append_child(&input)?;
    let import = document
        .create_element("button")?
        .dyn_into::<HtmlButtonElement>()?;
    import.set_type("button");
    import.set_class_name("col-1 offset-2 btn btn-success");
    import.set_text_content(Some("Save"));
    if demo {
        import.set_disabled(true);
    }
    form_row.append_child(&import)?;
    form.append_child(&form_row)?;
    row.append_child(&form)?;
    home.append_child(&row)?;
    let header = document.create_element("h1")?;
    header.set_text_content(Some("My Spotify Playlists"));
    home.append_child(&header)?;
    let spotify = document.create_element("div")?;
    spotify.set_id("spotify");
    home.append_child(&spotify)?;
    let header = document.create_element("h1")?;
    header.set_text_content(Some("Stats"));
    home.append_child(&header)?;
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
    body.set_id("left-stats");
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
    body.set_id("right-stats");
    table.append_child(&body)?;
    right.append_child(&table)?;
    row.append_child(&right)?;
    home.append_child(&row)?;
    let state_ref = Rc::clone(state);
    let a = Closure::wrap(Box::new(move || {
        let state = Rc::clone(&state_ref);
        wasm_bindgen_futures::spawn_local(async move {
            let window = web_sys::window().expect("no global `window` exists");
            let document = window.document().expect("should have a document on window");
            let input = document
                .get_element_by_id("input")
                .unwrap()
                .dyn_into::<HtmlInputElement>()
                .unwrap();
            let mut input = input.value();
            let playlist_re =
                Regex::new(r"https://open.spotify.com/playlist/([[:alnum:]]*)").unwrap();
            let album_re = Regex::new(r"https://open.spotify.com/album/([[:alnum:]]*)").unwrap();
            let url = if let Some(id) = playlist_re.captures_iter(&input).next() {
                input = id[1].to_owned();
                format!("/api/?action=import&playlist={}", input)
            } else if let Some(id) = album_re.captures_iter(&input).next() {
                input = id[1].to_owned();
                format!("/api/?action=import&album={}", input)
            } else {
                input
            };
            let request = query(&url, "POST", &state.borrow().auth).unwrap();
            let resp_value = JsFuture::from(window.fetch_with_request(&request))
                .await
                .unwrap();
            let resp: Response = resp_value.dyn_into().unwrap();
            match resp.status() {
                201 => {
                    load_playlists(state).await.unwrap();
                }
                // TODO: error handling
                _ => {}
            }
        })
    }) as Box<dyn FnMut()>);
    import.set_onclick(Some(a.as_ref().unchecked_ref()));
    a.forget();
    Ok(home)
}

async fn refresh_home_page(state: Rc<RefCell<State>>) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    load_playlists(Rc::clone(&state)).await?;
    let request = query("/api/scores", "GET", &state.borrow().auth).unwrap();
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
    let body = document
        .get_element_by_id("left-stats")
        .ok_or_else(|| JsValue::from("left-stats element missing"))?;
    while let Some(child) = body.first_element_child() {
        child.remove();
    }
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
    let body = document
        .get_element_by_id("right-stats")
        .ok_or_else(|| JsValue::from("right-stats element missing"))?;
    while let Some(child) = body.first_element_child() {
        child.remove();
    }
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
    Ok(())
}

async fn load_playlists(state: Rc<RefCell<State>>) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let playlists_element = document
        .get_element_by_id("playlists")
        .ok_or_else(|| JsValue::from("playlists element missing"))?;
    let request = query("/api/playlists", "GET", &state.borrow().auth)?;
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
            label.set_class_name("col-7");
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
                    let scores = fetch_scores(&window, &state, &id).await.unwrap();
                    if scores.scores.len() < 2 {
                        window
                            .alert_with_message("Playlist has less than 2 songs")
                            .expect("alert");
                    } else {
                        switch_pages(state, Page::RandomMatch(id)).await.unwrap();
                    }
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
            button.set_text_content(Some("Unsave"));
            if state.borrow().auth == "demo" {
                button.set_disabled(true);
            }
            let state_ref = Rc::clone(&state);
            let a = Closure::wrap(Box::new(move || {
                let state = Rc::clone(&state_ref);
                let id = p.id.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let window = web_sys::window().expect("no global `window` exists");
                    let url = format!("/api/playlists/{}", id);
                    let request = query(&url, "DELETE", &state.borrow().auth).unwrap();
                    let resp_value = JsFuture::from(window.fetch_with_request(&request))
                        .await
                        .unwrap();
                    let resp: Response = resp_value.dyn_into().unwrap();
                    match resp.status() {
                        204 => {
                            load_playlists(state).await.unwrap();
                        }
                        // TODO: error handling
                        _ => {}
                    }
                })
            }) as Box<dyn FnMut()>);
            button.set_onclick(Some(a.as_ref().unchecked_ref()));
            a.forget();
            row.append_child(&button)?;
            playlists_element.append_child(&row)?;
        }
    }
    let playlists_element = document
        .get_element_by_id("spotify")
        .ok_or_else(|| JsValue::from("spotify element missing"))?;
    let request = query("/api/spotify/playlists", "GET", &state.borrow().auth).unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .unwrap();
    let resp: Response = resp_value.dyn_into().unwrap();
    if resp.status() == 405 {
        while let Some(child) = playlists_element.first_element_child() {
            child.remove();
        }
        let p = document.create_element("p")?;
        p.set_text_content(Some("Not supported in demo"));
        playlists_element.append_child(&p)?;
        return Ok(());
    }
    let json = JsFuture::from(resp.json()?).await?;
    let playlists: Playlists = json.into_serde().unwrap();
    while let Some(child) = playlists_element.first_element_child() {
        child.remove();
    }
    for p in playlists.items {
        let row = document.create_element("div")?;
        row.set_class_name("row");
        let label = document.create_element("label")?;
        label.set_class_name("col-9");
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
        let button = document
            .create_element("button")?
            .dyn_into::<HtmlButtonElement>()?;
        button.set_type("button");
        button.set_class_name("btn btn-success col-1");
        button.set_text_content(Some("Save"));
        let state_ref = Rc::clone(&state);
        let a = Closure::wrap(Box::new(move || {
            let state = Rc::clone(&state_ref);
            let id = p.playlist_id.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let window = web_sys::window().expect("no global `window` exists");
                let url = format!("/api/playlists/{}", id);
                let request = query(&url, "POST", &state.borrow().auth).unwrap();
                let resp_value = JsFuture::from(window.fetch_with_request(&request))
                    .await
                    .unwrap();
                let resp: Response = resp_value.dyn_into().unwrap();
                match resp.status() {
                    201 => {
                        load_playlists(state).await.unwrap();
                    }
                    // TODO: error handling
                    _ => {}
                }
            })
        }) as Box<dyn FnMut()>);
        button.set_onclick(Some(a.as_ref().unchecked_ref()));
        a.forget();
        row.append_child(&button)?;
        playlists_element.append_child(&row)?;
    }
    Ok(())
}

fn generate_random_page() -> Result<Element, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
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
    Ok(random)
}

fn refresh_scores(state: Rc<RefCell<State>>, mut scores: Scores) -> Result<(), JsValue> {
    async fn elo(state: Rc<RefCell<State>>, url: String) -> Result<(), JsValue> {
        let window = web_sys::window().expect("no global `window` exists");
        let request = query(&url, "POST", &state.borrow().auth)?;
        JsFuture::from(window.fetch_with_request(&request)).await?;
        let scores =
            fetch_scores(&window, &state, state.borrow().playlist.as_ref().unwrap()).await?;
        refresh_scores(state, scores)?;
        Ok(())
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
    let mut iter = (1..).zip(scores.scores.iter());
    while let Some((i, score)) = iter.next() {
        let row = document.create_element("tr")?;
        let num = document.create_element("th")?;
        num.set_text_content(Some(&i.to_string()));
        row.append_child(&num)?;
        let track = document.create_element("td")?;
        track.set_text_content(Some(&score.track));
        row.append_child(&track)?;
        let record = document.create_element("td")?;
        record.set_text_content(Some(&format!("{}-{}", score.wins, score.losses)));
        row.append_child(&record)?;
        let score_element = document.create_element("td")?;
        score_element.set_text_content(Some(&score.score.to_string()));
        row.append_child(&score_element)?;
        scores1.append_child(&row)?;

        if let Some((i, score)) = iter.next() {
            let row = document.create_element("tr")?;
            let num = document.create_element("th")?;
            num.set_text_content(Some(&i.to_string()));
            row.append_child(&num)?;
            let track = document.create_element("td")?;
            track.set_text_content(Some(&score.track));
            row.append_child(&track)?;
            let record = document.create_element("td")?;
            record.set_text_content(Some(&format!("{}-{}", score.wins, score.losses)));
            row.append_child(&record)?;
            let score_element = document.create_element("td")?;
            score_element.set_text_content(Some(&score.score.to_string()));
            row.append_child(&score_element)?;
            scores2.append_child(&row)?;
        }
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
    let url = format!("/api/elo?{}&{}", track1.track_id, track2.track_id);
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
    let url = format!("/api/elo?{}&{}", track2.track_id, track1.track_id);
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

async fn fetch_scores(
    window: &Window,
    state: &Rc<RefCell<State>>,
    id: &str,
) -> Result<Scores, JsValue> {
    let url = format!("/api/playlists/{}/scores", id);
    let request = query(&url, "GET", &state.borrow().auth)?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(json.into_serde().unwrap())
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
