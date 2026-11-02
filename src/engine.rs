// SPDX-License-Identifier: AGPL-3.0-or-later
//! KurokaSearch engine layer: models, registry, fan-out runner, HTTP helpers.

use serde::Serialize;
use std::thread;
use std::time::{Duration, Instant};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
/// Descriptive UA for open APIs that ask for one (Wikimedia & friends).
pub const CONTACT_UA: &str = "KurokaSearch/3.0 (https://github.com/Abdullayews/KurokaSearch; metasearch; contact: 08abdullayevv@gmail.com)";

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
pub enum Category { All, Images, Videos, News, Maps, Music, It, Science, Files, Social }

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
            _ => Category::All,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self { Category::All => "all", Category::Images => "images", Category::Videos => "videos",
            Category::News => "news", Category::Maps => "maps", Category::Music => "music",
            Category::It => "it", Category::Science => "science", Category::Files => "files",
            Category::Social => "social" }
    }
    pub fn label(&self) -> &'static str {
        match self { Category::All => "All", Category::Images => "Images", Category::Videos => "Videos",
            Category::News => "News", Category::Maps => "Maps", Category::Music => "Music",
            Category::It => "IT", Category::Science => "Science", Category::Files => "Files",
            Category::Social => "Social Media" }
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
    EngineInfo { id: "wikipedia", name: "Wikipedia", category: "all", default_on: true, paged: true },
    // images
    EngineInfo { id: "openverse", name: "Openverse", category: "images", default_on: true, paged: true },
    EngineInfo { id: "commons", name: "Wikimedia Commons", category: "images", default_on: true, paged: true },
    // videos
    EngineInfo { id: "dailymotion", name: "Dailymotion", category: "videos", default_on: true, paged: true },
    EngineInfo { id: "archiveorg", name: "Internet Archive", category: "videos", default_on: true, paged: true },
    // news
    EngineInfo { id: "gnews", name: "Google News", category: "news", default_on: true, paged: true },
    EngineInfo { id: "bing_news", name: "Bing News", category: "news", default_on: true, paged: true },
    // maps
    EngineInfo { id: "nominatim", name: "OpenStreetMap", category: "maps", default_on: true, paged: false },
    // music
    EngineInfo { id: "itunes", name: "Apple Music", category: "music", default_on: true, paged: false },
    EngineInfo { id: "musicbrainz", name: "MusicBrainz", category: "music", default_on: true, paged: true },
    // it
    EngineInfo { id: "stackexchange", name: "Stack Overflow", category: "it", default_on: true, paged: true },
    EngineInfo { id: "github", name: "GitHub", category: "it", default_on: true, paged: true },
    // science
    EngineInfo { id: "arxiv", name: "arXiv", category: "science", default_on: true, paged: true },
    EngineInfo { id: "crossref", name: "Crossref", category: "science", default_on: true, paged: true },
    // files
    EngineInfo { id: "bing_files", name: "Documents (PDF)", category: "files", default_on: true, paged: true },
    // social
    EngineInfo { id: "bing_social", name: "Social Networks", category: "social", default_on: true, paged: true },
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
    let s = s.split('?').next().unwrap_or(&s);
    let s = s.strip_suffix('/').unwrap_or(s);
    s.to_string()
}

// ---------------------------------------------------------------------------
// HTTP helpers (one shared client; HTTP/2 via ALPN keeps CDNs happy)
// ---------------------------------------------------------------------------

fn http_client() -> &'static reqwest::blocking::Client {
    static CLIENT: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| reqwest::blocking::Client::builder()
        .gzip(true)
        .build()
        .unwrap_or_default())
}

pub fn http_get(url: &str, ua: &str, extra: &[(&str, &str)], timeout: Duration) -> Result<String, String> {
    let mut rb = http_client().get(url).timeout(timeout).header("User-Agent", ua);
    for (k, v) in extra { rb = rb.header(*k, *v); }
    match rb.send() {
        Ok(resp) => {
            let status = resp.status();
            if !status.is_success() { return Err(format!("http {}", status.as_u16())); }
            resp.text().map_err(|e| format!("read: {e}"))
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

pub fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
        .replace("&quot;", "\"").replace("&#x27;", "'").replace("&#39;", "'")
        .replace("&nbsp;", " ").replace("&mdash;", "—").replace("&ndash;", "–")
        .replace("&hellip;", "…").replace("&laquo;", "«").replace("&raquo;", "»")
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
        "zh" => Some("zh-CN"),
        _ => None, // auto
    }
}

/// Google News locale from language setting.
pub fn gnews_locale(lang: &str) -> (&'static str, &'static str, &'static str) {
    match lang {
        "en" => ("en-US", "US", "US:en"),
        "az" => ("az-AZ", "AZ", "AZ:az"),
        "tr" => ("tr-TR", "TR", "TR:tr"),
        "ru" => ("ru-RU", "RU", "RU:ru"),
        "de" => ("de-DE", "DE", "DE:de"),
        "fr" => ("fr-FR", "FR", "FR:fr"),
        "es" => ("es-ES", "ES", "ES:es"),
        "ar" => ("ar", "EG", "EG:ar"),
        "ja" => ("ja-JP", "JP", "JP:ja"),
        "zh" => ("zh-Hans", "CN", "CN:zh-Hans"),
        _ => ("en-US", "US", "US:en"),
    }
}

/// Deterministic hue for a domain letter-avatar.
pub fn avatar_hue(host: &str) -> u16 {
    let mut h: u32 = 5381;
    for b in host.as_bytes() { h = h.wrapping_mul(33).wrapping_add(*b as u32); }
    (h % 360) as u16
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
