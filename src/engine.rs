// SPDX-License-Identifier: AGPL-3.0-or-later
//! KurokaSearch engine layer: models, registry, fan-out runner, HTTP helpers.

use serde::Serialize;
use std::thread;
use std::time::{Duration, Instant};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
/// Descriptive UA for open APIs that ask for one (Wikimedia & friends).
pub const CONTACT_UA: &str = "KurokaSearch/3.0 (https://github.com/Abdullayews/KurokaSearch; metasearch; contact: 08abdullayevv@gmail.com)";

/// Rotating browser fingerprints — (user-agent, sec-ch-ua, platform).
/// Upstream bot checks fingerprint UA vs. client hints, so the pairs must
/// stay consistent; `http_get_browser` walks them per request.
pub const UA_POOL: &[(&str, &str, &str)] = &[
    ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
     "\"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"", "\"Windows\""),
    ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0",
     "\"Chromium\";v=\"130\", \"Not_A Brand\";v=\"24\", \"Microsoft Edge\";v=\"130\"", "\"Windows\""),
    ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
     "\"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"", "\"macOS\""),
    ("Mozilla/5.0 (X11; Linux x86_64; rv=133.0) Gecko/20100101 Firefox/133.0", "", "\"Linux\""),
    ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Safari/605.1.15", "", "\"macOS\""),
];

/// Round-robin fingerprint index — spreads requests across the pool so no
/// single UA gets hammered (and a flagged one rotates out quickly).
pub fn next_ua() -> usize {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % UA_POOL.len()
}

// ---------------------------------------------------------------------------
// models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub title: String,
    pub url: String,
    pub desc: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub img: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    /// epoch seconds, when the engine supplied a parseable date
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_epoch: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// small key/value rows the card renders under the title
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub meta: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// map coordinates
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lon: Option<f64>,
}

impl SearchResult {
    pub fn web(title: String, url: String, desc: String) -> Self {
        let source = host_of(&url);
        SearchResult { kind: "web", title, url, desc, source, thumb: None, img: None,
            width: None, height: None, duration: None, published: None, published_epoch: None,
            publisher: None, meta: vec![], preview: None, lat: None, lon: None }
    }
    pub fn new(kind: &'static str, title: String, url: String, desc: String) -> Self {
        let source = host_of(&url);
        SearchResult { kind, title, url, desc, source, thumb: None, img: None,
            width: None, height: None, duration: None, published: None, published_epoch: None,
            publisher: None, meta: vec![], preview: None, lat: None, lon: None }
    }
}

pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = rest.split(['/']).next().unwrap_or(rest);
    host.trim_start_matches("www.").to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category { All, Images, Videos, News, Maps, Music, It, Science, Files, Social, Onion }

impl Category {
    pub fn from_str(s: &str) -> Category {
        match s {
            "images" => Category::Images,
            "videos" => Category::Videos,
            "news" => Category::News,
            "maps" => Category::Maps,
            "music" => Category::Music,
            "it" => Category::It,
            "science" => Category::Science,
            "files" => Category::Files,
            "social" => Category::Social,
            "onion" => Category::Onion,
            _ => Category::All,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self { Category::All => "all", Category::Images => "images", Category::Videos => "videos",
            Category::News => "news", Category::Maps => "maps", Category::Music => "music",
            Category::It => "it", Category::Science => "science", Category::Files => "files",
            Category::Social => "social", Category::Onion => "onion" }
    }
    pub fn label(&self) -> &'static str {
        match self { Category::All => "All", Category::Images => "Images", Category::Videos => "Videos",
            Category::News => "News", Category::Maps => "Maps", Category::Music => "Music",
            Category::It => "IT", Category::Science => "Science", Category::Files => "Files",
            Category::Social => "Social Media", Category::Onion => "Onion" }
    }
}

#[derive(Debug, Clone)]
pub struct SearchParams {
    pub query: String,
    pub category: Category,
    pub page: u32,
    /// engine ids; empty = category defaults
    pub engines: Vec<String>,
    /// 0 = off, 1 = moderate, 2 = strict
    pub safesearch: u8,
    /// "auto" | "en" | "az" | "tr" | "ru" | "de" | "fr" | "es" | "ar" | "ja" | "zh"
    pub language: String,
    /// "" | "d" | "w" | "m" | "y"
    pub time: String,
    /// image size filter: "any|small|medium|large|wallpaper"
    pub size: String,
    /// image layout filter: "any|wide|tall|square"
    pub layout: String,
}

// ---------------------------------------------------------------------------
// engine registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct EngineInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub default_on: bool,
    /// false = engine has no real paging, pagination stops after page 1
    pub paged: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineOutcome {
    pub id: String,
    pub name: String,
    pub count: usize,
    pub time_ms: u64,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct SearchOutcome {
    pub query: String,
    pub category: String,
    pub page: u32,
    pub results: Vec<SearchResult>,
    pub engines: Vec<EngineOutcome>,
    pub total_ms: u64,
    pub deduped: usize,
}

pub const REGISTRY: &[EngineInfo] = &[
    // all
    EngineInfo { id: "bing", name: "Bing Web", category: "all", default_on: true, paged: true },
    EngineInfo { id: "ddg", name: "DuckDuckGo", category: "all", default_on: true, paged: true },
    EngineInfo { id: "wikipedia", name: "Wikipedia", category: "all", default_on: true, paged: true },
    EngineInfo { id: "wikidata", name: "Wikidata", category: "all", default_on: true, paged: true },
    EngineInfo { id: "wiby", name: "Wiby", category: "all", default_on: true, paged: true },
    EngineInfo { id: "marginalia", name: "Marginalia", category: "all", default_on: false, paged: false },
    // images
    EngineInfo { id: "bing_images", name: "Bing Images", category: "images", default_on: true, paged: true },
    EngineInfo { id: "ddg_images", name: "DuckDuckGo", category: "images", default_on: false, paged: true },
    EngineInfo { id: "openverse", name: "Openverse", category: "images", default_on: true, paged: true },
    EngineInfo { id: "commons", name: "Wikimedia Commons", category: "images", default_on: true, paged: true },
    // videos
    EngineInfo { id: "youtube", name: "YouTube", category: "videos", default_on: true, paged: false },
    EngineInfo { id: "ddg_videos", name: "DuckDuckGo", category: "videos", default_on: true, paged: true },
    EngineInfo { id: "sepiasearch", name: "PeerTube", category: "videos", default_on: true, paged: true },
    EngineInfo { id: "dailymotion", name: "Dailymotion", category: "videos", default_on: true, paged: true },
    EngineInfo { id: "odysee", name: "Odysee", category: "videos", default_on: true, paged: true },
    EngineInfo { id: "archiveorg", name: "Internet Archive", category: "videos", default_on: true, paged: true },
    // news
    EngineInfo { id: "gnews", name: "Google News", category: "news", default_on: true, paged: true },
    EngineInfo { id: "bing_news", name: "Bing News", category: "news", default_on: true, paged: true },
    EngineInfo { id: "ddg_news", name: "DuckDuckGo News", category: "news", default_on: true, paged: true },
    EngineInfo { id: "yahoo_news", name: "Yahoo News", category: "news", default_on: true, paged: true },
    // maps
    EngineInfo { id: "nominatim", name: "OpenStreetMap", category: "maps", default_on: true, paged: false },
    EngineInfo { id: "photon", name: "Photon", category: "maps", default_on: true, paged: false },
    // music
    EngineInfo { id: "itunes", name: "Apple Music", category: "music", default_on: true, paged: false },
    EngineInfo { id: "deezer", name: "Deezer", category: "music", default_on: false, paged: true },
    EngineInfo { id: "musicbrainz", name: "MusicBrainz", category: "music", default_on: true, paged: true },
    EngineInfo { id: "mixcloud", name: "Mixcloud", category: "music", default_on: true, paged: true },
    EngineInfo { id: "audius", name: "Audius", category: "music", default_on: true, paged: true },
    EngineInfo { id: "archive_audio", name: "Archive Audio", category: "music", default_on: true, paged: true },
    // it
    EngineInfo { id: "stackexchange", name: "Stack Overflow", category: "it", default_on: true, paged: true },
    EngineInfo { id: "github", name: "GitHub", category: "it", default_on: true, paged: true },
    EngineInfo { id: "gitlab", name: "GitLab", category: "it", default_on: true, paged: true },
    EngineInfo { id: "hn", name: "Hacker News", category: "it", default_on: true, paged: true },
    EngineInfo { id: "npm", name: "npm", category: "it", default_on: true, paged: true },
    EngineInfo { id: "crates", name: "crates.io", category: "it", default_on: true, paged: true },
    EngineInfo { id: "mdn", name: "MDN Web Docs", category: "it", default_on: true, paged: true },
    EngineInfo { id: "docker", name: "Docker Hub", category: "it", default_on: true, paged: true },
    EngineInfo { id: "huggingface", name: "Hugging Face", category: "it", default_on: true, paged: true },
    // science
    EngineInfo { id: "arxiv", name: "arXiv", category: "science", default_on: true, paged: true },
    EngineInfo { id: "crossref", name: "Crossref", category: "science", default_on: true, paged: true },
    EngineInfo { id: "pubmed", name: "PubMed", category: "science", default_on: true, paged: false },
    EngineInfo { id: "europepmc", name: "Europe PMC", category: "science", default_on: true, paged: true },
    EngineInfo { id: "zenodo", name: "Zenodo", category: "science", default_on: true, paged: true },
    EngineInfo { id: "doaj", name: "DOAJ", category: "science", default_on: true, paged: true },
    EngineInfo { id: "eric", name: "ERIC", category: "science", default_on: true, paged: true },
    // files
    EngineInfo { id: "bing_files", name: "Documents (PDF)", category: "files", default_on: false, paged: true },
    EngineInfo { id: "annas", name: "Anna's Archive", category: "files", default_on: true, paged: true },
    EngineInfo { id: "libgen", name: "LibGen", category: "files", default_on: true, paged: false },
    EngineInfo { id: "nyaa", name: "Nyaa", category: "files", default_on: true, paged: true },
    EngineInfo { id: "knaben", name: "Knaben", category: "files", default_on: true, paged: true },
    EngineInfo { id: "solid", name: "SolidTorrents", category: "files", default_on: true, paged: true },
    EngineInfo { id: "tdl", name: "TorrentDownload", category: "files", default_on: true, paged: false },
    EngineInfo { id: "apibay", name: "The Pirate Bay", category: "files", default_on: true, paged: true },
    EngineInfo { id: "gutenberg", name: "Project Gutenberg", category: "files", default_on: true, paged: true },
    EngineInfo { id: "standardebooks", name: "Standard Ebooks", category: "files", default_on: true, paged: true },
    // social
    EngineInfo { id: "bing_social", name: "Social Networks", category: "social", default_on: true, paged: true },
    EngineInfo { id: "reddit", name: "Reddit", category: "social", default_on: false, paged: true },
    EngineInfo { id: "lemmy", name: "Lemmy", category: "social", default_on: true, paged: true },
    // onion
    EngineInfo { id: "ahmia", name: "Ahmia", category: "onion", default_on: true, paged: true },
];

pub fn engines_for(params: &SearchParams) -> Vec<&'static EngineInfo> {
    let cat = params.category.as_str();
    if params.engines.is_empty() {
        REGISTRY.iter().filter(|e| e.category == cat && e.default_on).collect()
    } else {
        params.engines.iter()
            .filter_map(|id| REGISTRY.iter().find(|e| &e.id == id && e.category == cat))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// fan-out runner (threads, per-engine timing, dedupe + round-robin interleave)
// ---------------------------------------------------------------------------

pub fn run_search(params: &SearchParams) -> SearchOutcome {
    let started = Instant::now();
    let engines = engines_for(params);
    let mut outcomes: Vec<EngineOutcome> = Vec::new();
    let mut buckets: Vec<Vec<SearchResult>> = Vec::new();

    type Ret = Result<Vec<SearchResult>, String>;
    let handles: Vec<thread::JoinHandle<(String, String, u64, Ret)>> = engines.iter().map(|e| {
        let p = params.clone();
        let id = e.id.to_string();
        let name = e.name.to_string();
        thread::spawn(move || {
            let t0 = Instant::now();
            let r = crate::engines::run(&id, &p);
            let ms = t0.elapsed().as_millis() as u64;
            (id, name, ms, r)
        })
    }).collect();

    for h in handles {
        let (id, name, ms, res) = h.join().unwrap_or((
            "?".into(), "?".into(), 0, Err("engine thread panicked".into())));
        match res {
            Ok(rs) => {
                outcomes.push(EngineOutcome { id, name, count: rs.len(), time_ms: ms, ok: true, error: None });
                buckets.push(rs);
            }
            Err(e) => {
                outcomes.push(EngineOutcome { id, name, count: 0, time_ms: ms, ok: false,
                    error: Some(clip(&e, 120)) });
            }
        }
    }

    // round-robin interleave for engine diversity, dedupe by URL
    let mut merged: Vec<SearchResult> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut deduped = 0usize;
    loop {
        let mut pushed = false;
        for b in buckets.iter_mut() {
            if let Some(r) = b.first().cloned() {
                b.remove(0);
                let key = normalize_url(&r.url);
                if key.is_empty() { continue; }
                if seen.insert(key) { merged.push(r); pushed = true; deduped += 1; }
            }
            if !b.is_empty() { pushed = true; }
        }
        if !pushed { break; }
    }

    SearchOutcome {
        query: params.query.clone(),
        category: params.category.as_str().to_string(),
        page: params.page,
        results: merged,
        engines: outcomes,
        total_ms: started.elapsed().as_millis() as u64,
        deduped,
    }
}

fn normalize_url(u: &str) -> String {
    let s = u.trim().to_lowercase();
    // magnets: dedupe by info-hash (tracker lists differ between engines)
    if s.starts_with("magnet:") {
        if let Some(pos) = s.find("urn:btih:") {
            let hash: String = s[pos + 9..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            if !hash.is_empty() { return format!("btih:{hash}"); }
        }
    }
    let s = s.split('?').next().unwrap_or(&s);
    let s = s.strip_suffix('/').unwrap_or(s);
    s.to_string()
}

// ---------------------------------------------------------------------------
// HTTP helpers (one shared client; HTTP/2 via ALPN keeps CDNs happy).
// The client keeps a cookie jar (many bot checks are really cookie checks)
// and honors KUROKA_PROXY (http/https/socks5) so any engine can be routed
// through a residential exit when a datacenter IP gets filtered.
// ---------------------------------------------------------------------------

fn http_client() -> &'static reqwest::blocking::Client {
    static CLIENT: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        let mut b = reqwest::blocking::Client::builder()
            .gzip(true)
            .brotli(true)
            .cookie_store(true);
        if let Ok(px) = std::env::var("KUROKA_PROXY") {
            let px = px.trim().to_string();
            if !px.is_empty() {
                if let Ok(p) = reqwest::Proxy::all(&px) { b = b.proxy(p); }
            }
        }
        b.build().unwrap_or_default()
    })
}

/// Raw fetch: status + body in one go (shared by every variant below).
fn fetch(url: &str, ua: &str, extra: &[(&str, &str)], timeout: Duration)
    -> Result<(u16, String), String> {
    let mut rb = http_client().get(url).timeout(timeout).header("User-Agent", ua);
    for (k, v) in extra { rb = rb.header(*k, *v); }
    match rb.send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            resp.text().map(|t| (status, t)).map_err(|e| format!("read: {e}"))
        }
        Err(e) => Err(format!("{e}")),
    }
}

pub fn http_get(url: &str, ua: &str, extra: &[(&str, &str)], timeout: Duration) -> Result<String, String> {
    // every request carries a browser-grade Accept-Language unless the
    // engine already set one — pure-JSON APIs ignore it, bot walls like it
    let has_al = extra.iter().any(|(k, _)| k.eq_ignore_ascii_case("accept-language"));
    let mut headers: Vec<(&str, &str)> = Vec::with_capacity(extra.len() + 1);
    if !has_al { headers.push(("Accept-Language", "en-US,en;q=0.9")); }
    for (k, v) in extra { headers.push((k, v)); }
    let (status, body) = fetch(url, ua, &headers, timeout)?;
    if !(200..300).contains(&status) { return Err(format!("http {status}")); }
    Ok(body)
}

/// Browser-grade GET for HTML scrapers: full navigation header set
/// (client hints, Sec-Fetch-*, Referer), rotating fingerprints, shared
/// cookie jar, and one UA-rotating retry when a bot wall answers
/// 403/429/503. This is the "always open" layer — engines call it instead
/// of `http_get` whenever the upstream is an HTML page that may probe.
pub fn http_get_browser(url: &str, referer: &str, extra: &[(&str, &str)], timeout: Duration)
    -> Result<String, String> {
    let mut last_err = String::new();
    for attempt in 0..2 {
        let idx = next_ua();
        let (ua, chua, platform) = UA_POOL[idx];
        let mut headers: Vec<(&str, &str)> = vec![
            ("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8"),
            ("Accept-Language", "en-US,en;q=0.9"),
            ("Upgrade-Insecure-Requests", "1"),
            ("Sec-Fetch-Dest", "document"),
            ("Sec-Fetch-Mode", "navigate"),
            ("Sec-Fetch-Site", "cross-site"),
            ("Sec-Fetch-User", "?1"),
        ];
        if !chua.is_empty() {
            headers.push(("Sec-CH-UA", chua));
            headers.push(("Sec-CH-UA-Mobile", "?0"));
            headers.push(("Sec-CH-UA-Platform", platform));
        }
        if !referer.is_empty() { headers.push(("Referer", referer)); }
        for (k, v) in extra {
            // caller-supplied headers win over the defaults
            headers.retain(|(h, _)| !h.eq_ignore_ascii_case(k));
            headers.push((k, v));
        }
        match fetch(url, ua, &headers, timeout) {
            Ok((status, body)) if (200..300).contains(&status) => return Ok(body),
            Ok((status, _)) => {
                last_err = format!("http {status}");
                if matches!(status, 403 | 429 | 503) && attempt == 0 {
                    // bot wall — swap fingerprint and try once more
                    thread::sleep(Duration::from_millis(400 + idx as u64 * 90));
                    continue;
                }
                return Err(last_err);
            }
            Err(e) => {
                last_err = e;
                if attempt == 0 { thread::sleep(Duration::from_millis(250)); continue; }
                return Err(last_err);
            }
        }
    }
    Err(last_err)
}

/// DuckDuckGo's DNS answer points some datacenter networks at an address
/// they cannot reach. The service is spread over several stable IPs, so
/// every DDG request goes through a client pinned to a reachable one and
/// falls back to the default resolver if that pin ever stops working.
fn ddg_client() -> &'static reqwest::blocking::Client {
    static C: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let pin = "40.114.177.156:443".parse().unwrap();
        reqwest::blocking::Client::builder()
            .gzip(true)
            .brotli(true)
            .cookie_store(true)
            .resolve("duckduckgo.com", pin)
            .resolve("html.duckduckgo.com", pin)
            .build()
            .unwrap_or_default()
    })
}

/// DDG-only variant of `http_get`: pinned IP first, plain resolver second.
pub fn http_get_ddg(url: &str, ua: &str, extra: &[(&str, &str)], timeout: Duration) -> Result<String, String> {
    let mut rb = ddg_client().get(url).timeout(timeout).header("User-Agent", ua);
    for (k, v) in extra { rb = rb.header(*k, *v); }
    match rb.send() {
        Ok(resp) => {
            let status = resp.status();
            if !status.is_success() { return Err(format!("http {}", status.as_u16())); }
            match resp.text() {
                Ok(t) if !t.is_empty() => Ok(t),
                Ok(_) => Err("empty body".into()),
                Err(e) => Err(format!("read: {e}")),
            }
        }
        // transport failure on the pinned IP -> retry through normal DNS
        Err(_) => http_get(url, ua, extra, timeout),
    }
}

/// Like `http_get`, but hands back the body together with the status code
/// even for non-2xx answers — some protections reply 403 with an HTML
/// challenge page the caller wants to inspect.
pub fn http_get_any_status(url: &str, ua: &str, extra: &[(&str, &str)], timeout: Duration)
    -> Result<(u16, String), String> {
    let mut rb = http_client().get(url).timeout(timeout).header("User-Agent", ua);
    for (k, v) in extra { rb = rb.header(*k, *v); }
    match rb.send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            resp.text().map(|t| (status, t)).map_err(|e| format!("read: {e}"))
        }
        Err(e) => Err(format!("{e}")),
    }
}

pub fn http_get_binary(url: &str, ua: &str, extra: &[(&str, &str)], timeout: Duration)
    -> Result<(String, Vec<u8>), String> {
    let mut rb = http_client().get(url).timeout(timeout).header("User-Agent", ua);
    for (k, v) in extra { rb = rb.header(*k, *v); }
    match rb.send() {
        Ok(resp) => {
            let ctype = resp.headers().get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream").to_string();
            match resp.bytes() {
                Ok(b) if b.len() <= 15 * 1024 * 1024 => Ok((ctype, b.to_vec())),
                Ok(_) => Err("payload too large".into()),
                Err(e) => Err(format!("read: {e}")),
            }
        }
        Err(e) => Err(format!("{e}")),
    }
}

pub fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_entities(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Decode, strip real tags, decode again — for upstream feeds that carry
/// double-encoded HTML blobs (Google News descriptions embed `&lt;a href=…`).
pub fn clean_text(s: &str) -> String {
    decode_entities(&strip_tags(&decode_entities(s)))
}

pub fn decode_entities(s: &str) -> String {
    // decode to a fixpoint: upstream HTML is sometimes double-encoded
    // (&amp;#039; -> &#039; -> '), so a single pass leaves literal entities
    let mut cur = s.to_string();
    for _ in 0..3 {
        let next = decode_entities_once(&cur);
        if next == cur { break; }
        cur = next;
    }
    cur
}

/// One decoding pass — named entities plus ALL numeric forms
/// (decimal with or without leading zeros, and hex `&#x27;`).
fn decode_entities_once(s: &str) -> String {
    let named: &[(&str, &str)] = &[
        ("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""),
        ("&nbsp;", " "), ("&mdash;", "—"), ("&ndash;", "–"), ("&hellip;", "…"),
        ("&laquo;", "«"), ("&raquo;", "»"), ("&middot;", "·"), ("&rsquo;", "'"),
        ("&lsquo;", "'"), ("&rdquo;", "\""), ("&ldquo;", "\""), ("&apos;", "'"),
    ];
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        // inspect at most 10 bytes, but never cut inside a multi-byte char
        let mut end = tail.len().min(10);
        while end > 0 && !tail.is_char_boundary(end) { end -= 1; }
        let window = &tail[..end];
        let mut matched = false;
        if let Some(semi) = window.find(';') {
            let ent = &tail[..=semi];
            if let Some((_, rep)) = named.iter().find(|(n, _)| *n == ent) {
                out.push_str(rep);
                rest = &tail[ent.len()..];
                matched = true;
            } else if let Some(num) = ent.strip_prefix("&#").and_then(|e| e.strip_suffix(';')) {
                let v = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    num.parse::<u32>().ok() // "039" == 39 — leading zeros are legal
                };
                if let Some(c) = v.and_then(char::from_u32) {
                    out.push(c);
                    rest = &tail[ent.len()..];
                    matched = true;
                }
            }
        }
        if !matched {
            out.push('&');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

pub fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else {
        let cut: String = s.chars().take(max).collect();
        format!("{}…", cut.trim_end())
    }
}

/// Percent-encode for query strings (form encoding).
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// XML/RSS helper: text content of the first <tag ...>...</tag>, CDATA-aware.
pub fn tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = xml.find(&open)?;
    let gt = xml[start..].find('>')? + start;
    let close = format!("</{tag}>");
    let end = xml[gt + 1..].find(&close)? + gt + 1;
    let mut raw = xml[gt + 1..end].to_string();
    if let Some(s) = raw.strip_prefix("<![CDATA[") {
        raw = s.strip_suffix("]]>").unwrap_or(s).to_string();
    }
    Some(raw)
}

/// Parse RFC-2822 dates like "Sat, 31 Oct 2026 00:51:00 GMT" to epoch seconds.
pub fn parse_rfc2822(s: &str) -> Option<i64> {
    let s = s.trim();
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 4 { return None; }
    let mut i = 0;
    if parts[0].ends_with(',') { i = 1; }
    let day: i64 = parts.get(i)?.parse().ok()?;
    let month = match parts.get(i + 1).copied()? {
        "Jan" => 1, "Feb" => 2, "Mar" => 3, "Apr" => 4, "May" => 5, "Jun" => 6,
        "Jul" => 7, "Aug" => 8, "Sep" => 9, "Oct" => 10, "Nov" => 11, "Dec" => 12,
        _ => return None,
    };
    let mut year: i64 = parts.get(i + 2)?.parse().ok()?;
    if year < 100 { year += 2000; }
    let time = parts.get(i + 3)?;
    let tp: Vec<i64> = time.split(':').filter_map(|x| x.parse().ok()).collect();
    if tp.len() < 3 { return None; }
    let (y, m, d) = (year, month as i64, day);
    let days = days_from_civil(y, m, d);
    Some(days * 86400 + tp[0] * 3600 + tp[1] * 60 + tp[2])
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Does the time filter ("d","w","m","y") accept this epoch?
pub fn time_accepts(time: &str, epoch: Option<i64>) -> bool {
    if time.is_empty() { return true; }
    let Some(e) = epoch else { return false; };
    let cutoff = match time {
        "d" => 86400,
        "w" => 7 * 86400,
        "m" => 30 * 86400,
        "y" => 365 * 86400,
        _ => return true,
    };
    now_epoch() - e <= cutoff
}

/// Bing market code from language setting.
pub fn bing_mkt(lang: &str) -> Option<&'static str> {
    match lang {
        "en" => Some("en-US"), "az" => Some("az-Latn-AZ"), "tr" => Some("tr-TR"),
        "ru" => Some("ru-RU"), "de" => Some("de-DE"), "fr" => Some("fr-FR"),
        "es" => Some("es-ES"), "ar" => Some("ar-SA"), "ja" => Some("ja-JP"),
        "zh" => Some("zh-CN"), "it" => Some("it-IT"), "pt" => Some("pt-BR"),
        "nl" => Some("nl-NL"), "pl" => Some("pl-PL"), "ko" => Some("ko-KR"),
        "hi" => Some("hi-IN"), "id" => Some("id-ID"), "vi" => Some("vi-VN"),
        "fa" => Some("fa-IR"), "uk" => Some("uk-UA"), "th" => Some("th-TH"),
        "el" => Some("el-GR"), "he" => Some("he-IL"), "bn" => Some("bn-BD"),
        "ro" => Some("ro-RO"),
        _ => None, // auto
    }
}

/// Google News locale from language setting.
pub fn gnews_locale(lang: &str) -> (&'static str, &'static str, &'static str) {
    match lang {
        "az" => ("az-AZ", "AZ", "AZ:az"),
        "tr" => ("tr-TR", "TR", "TR:tr"),
        "ru" => ("ru-RU", "RU", "RU:ru"),
        "de" => ("de-DE", "DE", "DE:de"),
        "fr" => ("fr-FR", "FR", "FR:fr"),
        "es" => ("es-ES", "ES", "ES:es"),
        "ar" => ("ar", "EG", "EG:ar"),
        "ja" => ("ja-JP", "JP", "JP:ja"),
        "zh" => ("zh-Hans", "CN", "CN:zh-Hans"),
        "it" => ("it-IT", "IT", "IT:it"),
        "pt" => ("pt-BR", "BR", "BR:pt"),
        "nl" => ("nl-NL", "NL", "NL:nl"),
        "pl" => ("pl-PL", "PL", "PL:pl"),
        "ko" => ("ko-KR", "KR", "KR:ko"),
        "hi" => ("hi-IN", "IN", "IN:hi"),
        "id" => ("id-ID", "ID", "ID:id"),
        "vi" => ("vi-VN", "VN", "VN:vi"),
        "fa" => ("fa-IR", "IR", "IR:fa"),
        "uk" => ("uk-UA", "UA", "UA:uk"),
        "th" => ("th-TH", "TH", "TH:th"),
        "el" => ("el-GR", "GR", "GR:el"),
        "he" => ("he-IL", "IL", "IL:he"),
        "bn" => ("bn-BD", "BD", "BD:bn"),
        "ro" => ("ro-RO", "RO", "RO:ro"),
        _ => ("en-US", "US", "US:en"),
    }
}

/// Deterministic hue for a domain letter-avatar.
pub fn avatar_hue(host: &str) -> u16 {
    let mut h: u32 = 5381;
    for b in host.as_bytes() { h = h.wrapping_mul(33).wrapping_add(*b as u32); }
    (h % 360) as u16
}

/// Percent-decode (for redirect-unwrapping: DDG `uddg=`, Yahoo `RU=`).
pub fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = |b: u8| -> Option<u8> {
                    match b {
                        b'0'..=b'9' => Some(b - b'0'),
                        b'a'..=b'f' => Some(b - b'a' + 10),
                        b'A'..=b'F' => Some(b - b'A' + 10),
                        _ => None,
                    }
                };
                match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    (Some(h), Some(l)) => { out.push(h * 16 + l); i += 3; }
                    _ => { out.push(bytes[i]); i += 1; }
                }
            }
            b'+' => { out.push(b' '); i += 1; }
            b => { out.push(b); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Parse ISO-8601 dates like "2026-11-03T14:46:38Z" (UTC) to epoch seconds.
pub fn parse_iso8601(s: &str) -> Option<i64> {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() < 10 { return None; }
    let num = |r: std::ops::Range<usize>| -> Option<i64> { s.get(r)?.parse().ok() };
    let y = num(0..4)?;
    let mo = num(5..7)?;
    let d = num(8..10)?;
    let (h, mi, sec) = if b.len() >= 19 {
        (num(11..13)?, num(14..16)?, num(17..19)?)
    } else { (0, 0, 0) };
    Some(days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + sec)
}

/// Parse relative dates like "2 hours ago", "3 days ago", "1h ago" to epoch.
pub fn parse_relative(s: &str) -> Option<i64> {
    let s = s.trim().to_lowercase();
    let (n_str, unit) = s.split_once([' ', '-'])?;
    let n: i64 = if n_str == "a" || n_str == "an" || n_str == "1" { 1 } else { n_str.parse().ok()? };
    let secs = if unit.starts_with("second") || unit.starts_with("sec") || unit == "s" { 1 }
        else if unit.starts_with("minute") || unit.starts_with("min") || unit == "m" { 60 }
        else if unit.starts_with("hour") || unit.starts_with("hr") || unit == "h" { 3600 }
        else if unit.starts_with("day") || unit == "d" { 86400 }
        else if unit.starts_with("week") || unit.starts_with("wk") || unit == "w" { 7 * 86400 }
        else if unit.starts_with("month") || unit == "mo" { 30 * 86400 }
        else if unit.starts_with("year") || unit.starts_with("yr") || unit == "y" { 365 * 86400 }
        else { return None; };
    Some(now_epoch() - n * secs)
}

/// "1.2 GB" style human size from bytes.
pub fn human_bytes(bytes: i64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0usize;
    while v >= 1024.0 && u < units.len() - 1 { v /= 1024.0; u += 1; }
    if u == 0 { format!("{bytes} B") } else { format!("{v:.1} {}", units[u]) }
}

/// "6:39" / "1:02:03" duration text from seconds.
pub fn dur_text(secs: i64) -> String {
    let secs = secs.max(0);
    let h = secs / 3600; let m = (secs % 3600) / 60; let s = secs % 60;
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// Does an image of w×h match the requested size bucket?
pub fn matches_size(w: Option<u32>, h: Option<u32>, size: &str) -> bool {
    if size.is_empty() || size == "any" { return true; }
    let (Some(w), Some(h)) = (w, h) else { return false; };
    let maxdim = w.max(h);
    match size {
        "small" => maxdim < 300,
        "medium" => maxdim >= 300 && maxdim < 700,
        "large" => maxdim >= 700 && w < 1280,
        "wallpaper" => w >= 1280,
        _ => true,
    }
}

/// Does an image of w×h match the requested layout?
pub fn matches_layout(w: Option<u32>, h: Option<u32>, layout: &str) -> bool {
    if layout.is_empty() || layout == "any" { return true; }
    let (Some(w), Some(h)) = (w, h) else { return false; };
    match layout {
        "wide" => w as f64 >= h as f64 * 1.2,
        "tall" => h as f64 >= w as f64 * 1.2,
        "square" => (w as f64 - h as f64).abs() < (w.max(h) as f64 * 0.2),
        _ => true,
    }
}
