// SPDX-License-Identifier: AGPL-3.0-or-later
//! Deezer search (music) — public catalog API: tracks with album art and
//! 30-second previews that plug straight into the built-in player.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let index = (p.page.max(1) - 1) * 25;
    let url = format!(
        "https://api.deezer.com/search?q={}&limit=25&index={index}",
        urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    // Deezer signals errors as {"error": {...}} with HTTP 200
    if let Some(err) = v.get("error") {
        if !err.is_null() && err.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
            return Err(format!("api: {}", clip(&err.to_string(), 90)));
        }
    }
    let mut out = Vec::new();
    if let Some(arr) = v.get("data").and_then(|x| x.as_array()) {
        for it in arr {
            let title = it.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let link = it.get("link").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if title.is_empty() || link.is_empty() { continue; }
            let artist = it.pointer("/artist/name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let mut r = SearchResult::new("music", clip(&title, 110), link, clip(&artist, 120));
            if let Some(cover) = it.pointer("/album/cover_big").and_then(|x| x.as_str())
                .or_else(|| it.pointer("/album/cover_medium").and_then(|x| x.as_str()))
            {
                r.thumb = Some(cover.to_string());
            }
            if let Some(secs) = it.get("duration").and_then(|x| x.as_i64()) {
                if secs > 0 { r.duration = Some(crate::engines::dailymotion::fmt_dur(secs as u64)); }
            }
            if let Some(pv) = it.get("preview").and_then(|x| x.as_str()) {
                if !pv.is_empty() { r.preview = Some(pv.to_string()); }
            }
            if let Some(al) = it.pointer("/album/title").and_then(|x| x.as_str()) {
                r.meta.push(("album".into(), clip(al, 48)));
            }
            r.meta.push(("engine".into(), "deezer".into()));
            out.push(r);
            if out.len() >= 25 { break; }
        }
    }
    Ok(out)
}
