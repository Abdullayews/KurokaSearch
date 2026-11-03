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
            // API responses are cacheable — private so queries never sit on
            // shared proxies, only in the visitor's own browser
            let _ = respond(req, 200, json_ct(), body.into_bytes(), Some("private, max-age=86400"));
        }
        "/img" => {
            let u = get("u");
            if !is_proxyable_url(&u) {
                text(req, 400, "bad url");
            } else {
                match fetch_image_cached(&u) {
                    Ok((ctype, bytes)) => {
                        let _ = respond(req, 200, Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes()).unwrap(),
                            bytes, Some("private, max-age=86400"));
                    }
                    Err(e) => text(req, 502, &engine::clip(&e, 120)),
                }
            }
        }
        "/favicon" => {
            let d = get("d");
            match favicon_host(&d) {
                None => text(req, 400, "bad host"),
                Some(host) => match fetch_favicon_cached(&host) {
                    Ok((ctype, bytes)) => {
                        let _ = respond(req, 200, Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes()).unwrap(),
                            bytes, Some("public, max-age=604800"));
                    }
                    // a miss is negatively cached server-side and briefly
                    // cacheable by the browser, so dead icons stop re-firing
                    Err(_) => text_cache(req, 404, "no icon", "public, max-age=1800"),
                },
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

fn text_cache(req: tiny_http::Request, code: u16, body: &str, cache: &str) {
    let ct = Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..]).unwrap();
    let _ = respond(req, code, ct, body.as_bytes().to_vec(), Some(cache));
}

fn json_ct() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=utf-8"[..]).unwrap()
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
        "webp" => "image/webp",
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

/// 6-slot gate — upstream image CDNs 403 a datacenter IP on parallel bursts.
/// (Per-host serialization below already keeps each single CDN calm; the
/// global cap just spreads the load across hosts.)
fn with_img_permit<R>(f: impl FnOnce() -> R) -> R {
    static GATE: std::sync::OnceLock<(std::sync::Mutex<u32>, std::sync::Condvar)> = std::sync::OnceLock::new();
    let gate = GATE.get_or_init(|| (std::sync::Mutex::new(6u32), std::sync::Condvar::new()));
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
        // Wikimedia CDNs 403/429 generic browser UAs from datacenter IPs but
        // welcome a self-identifying one — pick the UA per host up front
        let ua = if host.ends_with("wikimedia.org") || host.ends_with("wikipedia.org")
            || host.ends_with("wikimediausercontent.org")
        {
            engine::CONTACT_UA
        } else {
            engine::USER_AGENT
        };
        let res = engine::http_get_binary(u, ua, headers, Duration::from_secs(10));
        // CDNs occasionally 403 a burst; one spaced retry — second attempt
        // switches to the descriptive UA (some hosts prefer it)
        if res.is_ok() { res } else {
            thread::sleep(Duration::from_millis(350));
            let retry_ua = if ua == engine::USER_AGENT { engine::CONTACT_UA } else { engine::USER_AGENT };
            engine::http_get_binary(u, retry_ua, headers, Duration::from_secs(10))
        }
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
// favicon proxy — a four-step fallback chain so virtually every result row
// gets its real icon. Positive hits are disk-cached for two weeks; misses
// are negatively cached for six hours so unknown hosts never hammer the
// chain on every page load. The letter avatar in the markup is the last
// resort when the whole chain fails.
// ---------------------------------------------------------------------------

/// Validate the `d` parameter into a plain public hostname.
fn favicon_host(d: &str) -> Option<String> {
    let host = d.trim().to_ascii_lowercase();
    let host = host.split('/').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    let host = host.trim_start_matches("www.").to_string();
    if host.len() < 4 || host.len() > 100 { return None; }
    if !host.contains('.') { return None; }
    if !host.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' )) { return None; }
    if matches!(host.as_str(), "localhost" | "0.0.0.0" | "ip6-localhost")
        || host.starts_with("127.") || host.starts_with("10.")
        || host.starts_with("192.168.") || host.starts_with("169.254.")
        || host.starts_with("172.16.") || host.ends_with(".local")
    {
        return None;
    }
    Some(host)
}

/// how long a positively cached icon stays fresh
const FAV_TTL: u64 = 14 * 86400;
/// how long a cached miss blocks re-probing
const FAV_NEG_TTL: u64 = 6 * 3600;

/// favicons are tiny — they get their own wider gate (they must never queue
/// behind full-size image proxy traffic)
fn with_fav_permit<R>(f: impl FnOnce() -> R) -> R {
    static GATE: std::sync::OnceLock<(std::sync::Mutex<u32>, std::sync::Condvar)> = std::sync::OnceLock::new();
    let gate = GATE.get_or_init(|| (std::sync::Mutex::new(8u32), std::sync::Condvar::new()));
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

/// magic-byte sniff — a "200" from an icon service can still be an HTML
/// error page; only real image bytes count
fn looks_like_image(b: &[u8]) -> bool {
    b.len() >= 8
        && (b.starts_with(&[0x89, b'P', b'N', b'G'])          // png
            || b.starts_with(&[0xFF, 0xD8, 0xFF])             // jpeg
            || b.starts_with(b"GIF8")                         // gif
            || b.starts_with(&[0x00, 0x00, 0x01, 0x00])       // ico
            || (b.starts_with(b"RIFF") && b.len() > 11 && &b[8..12] == b"WEBP") // webp
            || b.starts_with(b"<svg")                         // svg (text)
            || b.starts_with(b"<?xml"))                       // svg with prolog
}

fn file_age_secs(p: &std::path::Path) -> Option<u64> {
    std::fs::metadata(p).ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
}

/// Fetch `host`'s icon through the fallback chain:
///   1. DuckDuckGo's icon service (broad coverage, fast)
///   2. Google's favicon service (near-total coverage, generic globe for
///      unknown hosts — still an icon)
///   3. the site's own /favicon.ico
///   4. <link rel="icon"> parsed out of the site's HTML
fn fetch_favicon_cached(host: &str) -> Result<(String, Vec<u8>), String> {
    let dir = cache_dir();
    let key = format!("fav-{:016x}", hash64(host));
    let body_p = dir.join(format!("{key}.body"));
    let meta_p = dir.join(format!("{key}.meta"));

    // cached positive?
    if let (Ok(meta), Ok(body)) = (std::fs::read_to_string(&meta_p), std::fs::read(&body_p)) {
        let fresh = file_age_secs(&body_p).map(|a| a < FAV_TTL).unwrap_or(false);
        if meta.starts_with("image/") && !body.is_empty() && fresh && looks_like_image(&body) {
            return Ok((meta, body));
        }
        // cached miss, still recent enough?
        if meta == "none" {
            if file_age_secs(&body_p).map(|a| a < FAV_NEG_TTL).unwrap_or(false) {
                return Err("cached miss".into());
            }
        }
    }

    let fetched = with_fav_permit(|| {
        let accept: &[(&str, &str)] = &[("Accept", "image/png,image/webp,image/*,*/*;q=0.8")];
        // 1 + 2: the two big icon services
        for url in [
            format!("https://icons.duckduckgo.com/ip3/{host}.ico"),
            format!("https://www.google.com/s2/favicons?domain={host}&sz=64"),
        ] {
            if let Ok((ct, b)) = engine::http_get_binary(&url, engine::USER_AGENT, accept, Duration::from_secs(6)) {
                if ct.starts_with("image/") && !b.is_empty() && looks_like_image(&b) {
                    return Ok((ct, b));
                }
            }
        }
        // 3: the site's own favicon.ico
        if let Ok((ct, b)) = engine::http_get_binary(&format!("https://{host}/favicon.ico"),
            engine::USER_AGENT, accept, Duration::from_secs(6))
        {
            if ct.starts_with("image/") && !b.is_empty() && looks_like_image(&b) {
                return Ok((ct, b));
            }
        }
        // 4: <link rel="icon"> out of the site's HTML
        if let Some(u) = icon_from_html(host) {
            if let Ok((ct, b)) = engine::http_get_binary(&u, engine::USER_AGENT, accept, Duration::from_secs(6)) {
                if ct.starts_with("image/") && !b.is_empty() && looks_like_image(&b) {
                    return Ok((ct, b));
                }
            }
        }
        Err("no icon found".to_string())
    });

    let _ = std::fs::create_dir_all(&dir);
    match fetched {
        Ok((ct, b)) => {
            let _ = std::fs::write(&meta_p, &ct);
            let _ = std::fs::write(&body_p, &b);
            Ok((ct, b))
        }
        Err(e) => {
            // negative marker: meta "none", mtime = now
            let _ = std::fs::write(&meta_p, "none");
            let _ = std::fs::write(&body_p, b"");
            // rewriting the file refreshes its mtime — that IS the timer
            Err(e)
        }
    }
}

/// Extract a `<link rel="…icon…" href="…">` URL from the site's landing
/// page and absolutize it against `https://host`.
fn icon_from_html(host: &str) -> Option<String> {
    let body = engine::http_get_browser(&format!("https://{host}/"), "",
        &[("Accept", "text/html,application/xhtml+xml")], Duration::from_secs(6)).ok()?;
    let lower = body.to_ascii_lowercase();
    let mut probe = 0usize;
    while let Some(rel) = lower[probe..].find("<link") {
        let start = probe + rel;
        let end = lower[start..].find('>').map(|e| start + e).unwrap_or(lower.len());
        let tag = &lower[start..end];
        if tag.contains("rel=\"icon") || tag.contains("rel=\"shortcut icon")
            || tag.contains("rel='icon") || tag.contains("rel='shortcut icon")
            || tag.contains("rel=\"apple-touch-icon")
        {
            if let Some(h) = attr_href(&body[start..end]) {
                let u = if h.starts_with("http") {
                    h
                } else if h.starts_with("//") {
                    format!("https:{h}")
                } else if let Some(stripped) = h.strip_prefix('/') {
                    format!("https://{host}/{stripped}")
                } else {
                    format!("https://{host}/{h}")
                };
                return Some(u);
            }
        }
        probe = end + 1;
        if probe >= lower.len() { break; }
    }
    None
}

/// href="…" value of the first <link> tag in `chunk` (case-tolerant).
fn attr_href(chunk: &str) -> Option<String> {
    let lower = chunk.to_ascii_lowercase();
    let i = lower.find("href")?;
    let rest = chunk[i + 4..].trim_start();
    let rest = rest.strip_prefix('=')?;
    let rest = rest.trim_start();
    let q = rest.chars().next()?;
    if q != '"' && q != '\'' { return None; }
    let end = rest[1..].find(q)?;
    Some(html_unescape(&rest[1..1 + end]))
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&").replace("&quot;", "\"")
        .replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">")
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
