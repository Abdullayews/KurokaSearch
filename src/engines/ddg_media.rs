// SPDX-License-Identifier: AGPL-3.0-or-later
//! DuckDuckGo vertical searches — Images & Videos — through the vqd token
//! flow: a plain GET of the search page hands back a per-query `vqd` token,
//! which unlocks the JSON endpoints i.js (images) and v.js (videos).

use crate::engine::*;
use serde_json::Value;
use std::thread;
use std::time::Duration;

// ---------------------------------------------------------------------------
// vqd handshake
// ---------------------------------------------------------------------------

/// Pull the `vqd` token out of the search page. The token is a dash-y
/// alphanumeric blob that follows `vqd=` (quoted or bare) in the page JS.
fn extract_vqd(body: &str) -> Option<String> {
    let mut rest = body;
    while let Some(i) = rest.find("vqd") {
        rest = &rest[i + 3..];
        let r = rest.trim_start();
        let r = r.strip_prefix('=')?;
        let r = r.trim_start();
        let r = r.strip_prefix('"').or_else(|| r.strip_prefix('\'')).unwrap_or(r);
        let tok: String = r.chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if tok.len() >= 8 && tok.contains('-') {
            return Some(tok);
        }
    }
    None
}

fn fetch_vqd(query: &str) -> Result<String, String> {
    let url = format!("https://duckduckgo.com/?q={}&iax=images&ia=images", urlencode(query));
    // one retry — DDG's edge is burst-flaky towards datacenter IPs
    let mut last: Option<String> = None;
    for attempt in 0..2 {
        let ua = UA_POOL[next_ua()].0;
        match http_get_ddg(&url, ua,
            &[("Accept", "text/html"), ("Accept-Language", "en-US,en;q=0.9"),
              ("Sec-Fetch-Dest", "document"), ("Sec-Fetch-Mode", "navigate"),
              ("Sec-Fetch-Site", "none"), ("Sec-Fetch-User", "?1"),
              ("Upgrade-Insecure-Requests", "1")], Duration::from_secs(10)) {
            Ok(body) => {
                if let Some(vqd) = extract_vqd(&body) { return Ok(vqd); }
                last = Some("vqd token not found".into());
            }
            Err(e) => { last = Some(format!("http {e}")); }
        }
        if attempt == 0 { thread::sleep(Duration::from_millis(450)); }
    }
    Err(last.unwrap_or_else(|| "vqd handshake failed".into()))
}

/// DDG locale code from the language setting (wt-wt = no region).
fn ddg_locale(lang: &str) -> &'static str {
    match lang {
        "en" => "us-en", "tr" => "tr-tr", "ru" => "ru-ru", "de" => "de-de",
        "fr" => "fr-fr", "es" => "es-es", "it" => "it-it", "nl" => "nl-nl",
        "pl" => "pl-pl", "pt" => "pt-br", "ja" => "jp-jp", "zh" => "cn-zh",
        "ko" => "kr-kr", "ar" => "ar-eg", "az" => "az-az",
        _ => "wt-wt",
    }
}

/// Headers the JSON endpoints expect — without the same-origin Referer and
/// XHR marker DuckDuckGo serves an empty set (or nothing at all).
fn xhr_headers(referer: &str) -> [(&'static str, String); 4] {
    [
        ("Accept", "application/json, text/javascript, */*; q=0.01".into()),
        ("Referer", referer.into()),
        ("X-Requested-With", "XMLHttpRequest".into()),
        ("Sec-Fetch-Site", "same-origin".into()),
    ]
}

// ---------------------------------------------------------------------------
// Images — i.js
// ---------------------------------------------------------------------------

pub fn ddg_images(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let vqd = fetch_vqd(&p.query)?;
    let referer = format!("https://duckduckgo.com/?q={}&iax=images&ia=images", urlencode(&p.query));
    let url = format!(
        "https://duckduckgo.com/i.js?l={}&o=json&q={}&vqd={vqd}&f=,,,&p={}",
        ddg_locale(&p.language), urlencode(&p.query), p.page.max(1));
    let headers = xhr_headers(&referer);
    let extra: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let body = http_get_ddg(&url, UA_POOL[next_ua()].0, &extra, Duration::from_secs(12))?;
    if body.contains("\"challenge\"") {
        return Err("bot challenge (rate limited)".into());
    }
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(arr) = v.get("results").and_then(|x| x.as_array()) {
        for it in arr {
            let title = it.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let image = it.get("image").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let page_url = it.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if image.is_empty() || page_url.is_empty() { continue; }
            let w = it.get("width").and_then(|x| x.as_u64()).map(|x| x as u32);
            let h = it.get("height").and_then(|x| x.as_u64()).map(|x| x as u32);
            // honor the Extras size/layout filters the same way Bing does
            if !matches_size(w, h, &p.size) || !matches_layout(w, h, &p.layout) { continue; }
            // the grid shows DDG's lightweight thumbnail when present; the
            // click-through stays the full-size image
            let thumb = it.get("thumbnail").and_then(|x| x.as_str()).filter(|u| u.starts_with("http"))
                .map(|u| u.to_string()).unwrap_or_else(|| image.clone());
            let mut r = SearchResult::new("image", clip(&title, 110), page_url.clone(), clip(&page_url, 200));
            r.img = Some(image);
            r.thumb = Some(thumb);
            r.width = w;
            r.height = h;
            r.meta.push(("engine".into(), "duckduckgo".into()));
            out.push(r);
            if out.len() >= 40 { break; }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Videos — v.js
// ---------------------------------------------------------------------------

pub fn ddg_videos(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let vqd = fetch_vqd(&p.query)?;
    let referer = format!("https://duckduckgo.com/?q={}&iax=videos&ia=videos", urlencode(&p.query));
    let url = format!(
        "https://duckduckgo.com/v.js?l={}&o=json&q={}&vqd={vqd}&f=,,,&p={}",
        ddg_locale(&p.language), urlencode(&p.query), p.page.max(1));
    let headers = xhr_headers(&referer);
    let extra: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let body = http_get_ddg(&url, UA_POOL[next_ua()].0, &extra, Duration::from_secs(12))?;
    if body.contains("\"challenge\"") {
        return Err("bot challenge (rate limited)".into());
    }
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(arr) = v.get("results").and_then(|x| x.as_array()) {
        for it in arr {
            let title = it.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let link = it.get("content").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if title.is_empty() || link.is_empty() { continue; }
            let desc = it.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let publisher = it.get("publisher").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let mut r = SearchResult::new("video", clip(&title, 110), link, clip(&desc, 260));
            if let Some(thumb) = it.get("images").and_then(|x| x.get("medium"))
                .and_then(|x| x.as_str())
            {
                r.thumb = Some(thumb.to_string());
            }
            if let Some(secs) = it.get("duration").and_then(|x| x.as_i64()) {
                if secs > 0 { r.duration = Some(crate::engines::dailymotion::fmt_dur(secs as u64)); }
            }
            let when = it.get("published").and_then(|x| x.as_str()).unwrap_or("");
            if let Some(epoch) = parse_iso8601(when).or_else(|| parse_relative(when)) {
                r.published_epoch = Some(epoch);
                r.published = Some(when.to_string());
            }
            if !publisher.is_empty() { r.publisher = Some(publisher); }
            r.meta.push(("engine".into(), "duckduckgo".into()));
            out.push(r);
            if out.len() >= 30 { break; }
        }
    }
    Ok(out)
}
