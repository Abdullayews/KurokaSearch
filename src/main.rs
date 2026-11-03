// SPDX-License-Identifier: AGPL-3.0-or-later
//! KurokaSearch web server: routing, static assets, image proxy, autocomplete.
//! Zero external template/framework code — routing is a plain match on the path.

mod engine;
mod engines;
mod render;

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Method, Response, Server};

fn main() {
    let addr = std::env::var("KUROKA_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let server = Arc::new(Server::http(&addr)
        .unwrap_or_else(|e| { eprintln!("kuroka: cannot bind {addr}: {e}"); std::process::exit(1); }));
    eprintln!("kuroka {} listening on http://{addr}", engine::VERSION);

    let workers = 8;
    let mut handles = Vec::new();
    for _ in 0..workers {
        let srv = server.clone();
        handles.push(std::thread::spawn(move || loop {
            let req = match srv.recv() { Ok(r) => r, Err(_) => break };
            handle(req);
        }));
    }
    for h in handles { let _ = h.join(); }
}

fn handle(req: tiny_http::Request) {
    let url = req.url().to_string();
    let (path, query) = match url.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (url.clone(), String::new()),
    };
    let path = percent_decode(&path);

    if req.method() != &Method::Get && req.method() != &Method::Head {
        let _ = respond(req, 405, html_ct(), render::error_page(405, "Method not allowed").into_bytes(), None);
        return;
    }

    let params = parse_query(&query);
    let get = |k: &str| params.iter().rev().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap_or_default();

    match path.as_str() {
        "/" => html(req, 200, render::home_page()),
        "/healthz" => text(req, 200, "ok"),
        // settings live in a popup on home/results now
        "/preferences" => { let _ = redirect(req, "/"); }
        "/search" => {
            let q = get("q").trim().to_string();
            if q.is_empty() {
                // empty query -> back to the home page
                let _ = redirect(req, "/");
            } else {
                let p = engine::SearchParams {
                    query: q,
                    category: engine::Category::from_str(&get("c")),
                    page: get("page").parse::<u32>().ok().filter(|p| (1..=50).contains(p)).unwrap_or(1),
                    engines: get("e").split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                    safesearch: get("ss").parse::<u8>().unwrap_or(0).min(2),
                    language: { let l = get("lang"); if l.is_empty() { "auto".into() } else { l } },
                    time: get("time"),
                    size: get("size"),
                    layout: get("layout"),
                };
                let out = engine::run_search(&p);
                html(req, 200, render::search_page(&out, &p))
            }
        }
        "/ac" => {
            let q = get("q");
            let list = autocomplete(&q);
            let body = serde_json::to_string(&list).unwrap_or_else(|_| "[]".into());
            json(req, 200, body)
        }
        "/img" => {
            let u = get("u");
            if !is_proxyable_url(&u) {
                text(req, 400, "bad url");
            } else {
                match fetch_image_cached(&u) {
                    Ok((ctype, bytes)) => {
                        let _ = respond(req, 200, Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes()).unwrap(),
                            bytes, Some("no-store, max-age=0"));
                    }
                    Err(e) => text(req, 502, &engine::clip(&e, 120)),
                }
            }
        }
        p if p.starts_with("/static/") => serve_static(req, p),
        _ => html(req, 404, render::error_page(404, "This page does not exist.")),
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn html_ct() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()
}

fn respond(req: tiny_http::Request, code: u16, ct: Header, body: Vec<u8>, cache: Option<&str>)
    -> std::io::Result<()> {
    let is_head = req.method() == &Method::Head;
    let payload = if is_head { Vec::new() } else { body };
    let mut resp = Response::from_data(payload).with_status_code(code);
    resp.add_header(ct);
    resp.add_header(Header::from_bytes(&b"X-Content-Type-Options"[..], &b"nosniff"[..]).unwrap());
    resp.add_header(Header::from_bytes(&b"Referrer-Policy"[..], &b"no-referrer"[..]).unwrap());
    // Cache policy: ONLY the mascot art is long-cached. Everything else —
    // pages, proxies, styles, scripts — is never stored anywhere.
    let c = cache.unwrap_or("no-store, max-age=0");
    resp.add_header(Header::from_bytes(&b"Cache-Control"[..], c.as_bytes()).unwrap());
    req.respond(resp)
}

fn redirect(req: tiny_http::Request, to: &str) -> std::io::Result<()> {
    let resp = Response::from_data(Vec::new()).with_status_code(302)
        .with_header(Header::from_bytes(&b"Location"[..], to.as_bytes()).unwrap());
    req.respond(resp)
}

fn html(req: tiny_http::Request, code: u16, body: String) {
    let _ = respond(req, code, html_ct(), body.into_bytes(), None);
}

fn text(req: tiny_http::Request, code: u16, body: &str) {
    let ct = Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..]).unwrap();
    let _ = respond(req, code, ct, body.as_bytes().to_vec(), None);
}

fn json(req: tiny_http::Request, code: u16, body: String) {
    let ct = Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=utf-8"[..]).unwrap();
    let _ = respond(req, code, ct, body.into_bytes(), None);
}

/// Decode a percent-encoded path (no '+' handling).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| -> Option<u8> {
                match c { b'0'..=b'9' => Some(c - b'0'), b'a'..=b'f' => Some(c - b'a' + 10),
                    b'A'..=b'F' => Some(c - b'A' + 10), _ => None }
            };
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse an `a=1&b=2` query string into pairs (both sides decoded, '+' = space).
fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&').filter(|s| !s.is_empty()).map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (percent_decode(&k.replace('+', " ")), percent_decode(&v.replace('+', " ")))
    }).collect()
}

// ---------------------------------------------------------------------------
// static files
// ---------------------------------------------------------------------------

fn static_root() -> PathBuf {
    if let Ok(p) = std::env::var("KUROKA_STATIC") { return PathBuf::from(p); }
    if Path::new("static").is_dir() { return PathBuf::from("static"); }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("static");
            if p.is_dir() { return p; }
        }
    }
    PathBuf::from("static")
}

fn serve_static(req: tiny_http::Request, url_path: &str) {
    let rel = url_path.trim_start_matches("/static/");
    let rel = rel.replace('\\', "/");
    let path = Path::new(&rel);
    // traversal guard: only normal components
    if path.components().any(|c| !matches!(c, Component::Normal(_))) {
        text(req, 400, "bad path");
        return;
    }
    let full = static_root().join(path);
    let mime = match full.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "webmanifest" => "application/manifest+json",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    };
    match std::fs::read(&full) {
        Ok(bytes) => {
            let ct = Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap();
            // Only the mascot art stays in the browser cache; every other
            // asset (css, js, banner, icons…) is served no-store.
            let cache = if rel.starts_with("brand/mascot") {
                "public, max-age=31536000, immutable"
            } else {
                "no-store, max-age=0"
            };
            let _ = respond(req, 200, ct, bytes, Some(cache));
        }
        Err(_) => text(req, 404, "not found"),
    }
}

// ---------------------------------------------------------------------------
// image proxy with a small disk cache (keeps upstream CDNs friendly)
// ---------------------------------------------------------------------------

fn cache_dir() -> PathBuf {
    if let Ok(p) = std::env::var("KUROKA_CACHE") { return PathBuf::from(p); }
    if Path::new("cache/img").is_dir() { return PathBuf::from("cache/img"); }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("cache/img");
            if p.is_dir() { return p; }
        }
    }
    PathBuf::from("cache/img")
}

/// FNV-1a 64-bit — compact cache key.
fn hash64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() { h ^= *b as u64; h = h.wrapping_mul(0x100000001b3); }
    h
}

const CACHE_TTL: u64 = 24 * 3600;

/// 2-slot gate — upstream image CDNs 403 a datacenter IP on parallel bursts.
fn with_img_permit<R>(f: impl FnOnce() -> R) -> R {
    static GATE: std::sync::OnceLock<(std::sync::Mutex<u32>, std::sync::Condvar)> = std::sync::OnceLock::new();
    let gate = GATE.get_or_init(|| (std::sync::Mutex::new(2u32), std::sync::Condvar::new()));
    let mut permits = gate.0.lock().unwrap_or_else(|p| p.into_inner());
    while *permits == 0 {
        permits = gate.1.wait(permits).unwrap_or_else(|p| p.into_inner());
    }
    *permits -= 1;
    drop(permits);
    let out = f();
    let mut permits = gate.0.lock().unwrap_or_else(|p| p.into_inner());
    *permits += 1;
    gate.1.notify_one();
    out
}

/// One gate per upstream host: requests to the same CDN are strictly serial.
fn host_gate(host: &str) -> &'static std::sync::Mutex<()> {
    static GATES: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, &'static std::sync::Mutex<()>>>> =
        std::sync::OnceLock::new();
    let map = GATES.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut m = map.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(g) = m.get(host) { return g; }
    let g: &'static std::sync::Mutex<()> = Box::leak(Box::new(std::sync::Mutex::new(())));
    m.insert(host.to_string(), g);
    g
}

fn url_host(u: &str) -> String {
    u.split_once("://").map(|(_, r)| r).unwrap_or("")
        .split('/').next().unwrap_or("").to_ascii_lowercase()
}

fn fetch_image_cached(u: &str) -> Result<(String, Vec<u8>), String> {
    let dir = cache_dir();
    let key = format!("{:016x}", hash64(u));
    let body_p = dir.join(format!("{key}.body"));
    let meta_p = dir.join(format!("{key}.meta"));

    // fresh cache hit?
    if let (Ok(meta), Ok(body)) = (std::fs::read_to_string(&meta_p), std::fs::read(&body_p)) {
        let fresh = std::fs::metadata(&body_p).ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .map(|age| age.as_secs() < CACHE_TTL)
            .unwrap_or(false);
        if fresh && meta.starts_with("image/") && !body.is_empty() {
            return Ok((meta, body));
        }
    }

    let host = url_host(u);
    let _host_lock = host_gate(&host).lock().unwrap_or_else(|p| p.into_inner());
    let fetched = with_img_permit(|| {
        let headers: &[(&str, &str)] = &[("Accept", "image/avif,image/webp,image/*,*/*;q=0.8")];
        let mut res = engine::http_get_binary(u, engine::USER_AGENT, headers, Duration::from_secs(12));
        // CDNs occasionally 403 a burst; spaced retries often clear it
        for delay in [400u64, 1500] {
            if res.is_ok() { break; }
            thread::sleep(Duration::from_millis(delay));
            res = engine::http_get_binary(u, engine::USER_AGENT, headers, Duration::from_secs(12));
        }
        res
    });
    let (ctype, bytes) = fetched?;
    if !ctype.starts_with("image/") || bytes.is_empty() {
        return Err(format!("not an image ({ctype})"));
    }
    // store (best effort)
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&meta_p, &ctype);
    let _ = std::fs::write(&body_p, &bytes);
    Ok((ctype, bytes))
}

/// Only public http(s) URLs may go through the image proxy.
fn is_proxyable_url(u: &str) -> bool {
    let u = u.trim();
    if !(u.starts_with("https://") || u.starts_with("http://")) { return false; }
    let host = u.split_once("://").map(|(_, r)| r).unwrap_or("");
    let host = host.split('/').next().unwrap_or("").to_ascii_lowercase();
    let host = host.split(':').next().unwrap_or("").to_string();
    if host.is_empty() { return false; }
    !matches!(host.as_str(),
        "localhost" | "127.0.0.1" | "0.0.0.0" | "::1" | "[::1]" | "metadata.google.internal")
        && !host.starts_with("127.") && !host.starts_with("10.") && !host.starts_with("192.168.")
        && !host.starts_with("169.254.") && !host.starts_with("172.16.")
        && !host.starts_with("172.17.") && !host.starts_with("172.18.")
        && !host.starts_with("172.19.") && !host.starts_with("172.2")
        && !host.starts_with("172.30.") && !host.starts_with("172.31.")
}

// ---------------------------------------------------------------------------
// autocomplete (server-side proxy so the provider never sees the user)
// ---------------------------------------------------------------------------

fn autocomplete(q: &str) -> Vec<String> {
    let q = q.trim();
    if q.is_empty() || q.chars().count() > 120 { return vec![]; }
    // Google suggest (client=firefox returns a plain JSON array)
    let g = format!("https://suggestqueries.google.com/complete/search?client=firefox&q={}",
        engine::urlencode(q));
    if let Ok(body) = engine::http_get(&g, engine::USER_AGENT, &[], Duration::from_secs(3)) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(arr) = v.get(1).and_then(|x| x.as_array()) {
                let list: Vec<String> = arr.iter()
                    .filter_map(|s| s.as_str().map(|s| s.to_string()))
                    .take(8).collect();
                if !list.is_empty() { return list; }
            }
        }
    }
    // DuckDuckGo fallback
    let d = format!("https://duckduckgo.com/ac/?q={}&type=list", engine::urlencode(q));
    if let Ok(body) = engine::http_get(&d, engine::USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(3)) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(arr) = v.get(1).and_then(|x| x.as_array()) {
                return arr.iter().filter_map(|s| s.as_str().map(|s| s.to_string()))
                    .take(8).collect();
            }
        }
    }
    vec![]
}
