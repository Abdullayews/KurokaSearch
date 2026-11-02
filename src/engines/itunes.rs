// SPDX-License-Identifier: AGPL-3.0-or-later
//! Apple Music (iTunes Search API) — "Music" engine: artwork + 30s previews.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    if p.page > 1 { return Ok(vec![]); }
    let url = format!(
        "https://itunes.apple.com/search?term={}&media=music&limit=16",
        urlencode(&p.query)
    );
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(results) = v.get("results").and_then(|x| x.as_array()) {
        for it in results {
            let title = it.get("trackName").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let artist = it.get("artistName").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let link = it.get("trackViewUrl").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if title.is_empty() || link.is_empty() { continue; }
            let mut r = SearchResult::new("music", clip(&title, 90), link, clip(&artist, 120));
            if let Some(art) = it.get("artworkUrl100").and_then(|x| x.as_str()) {
                let big = art.replace("100x100", "600x600");
                r.thumb = Some(big);
            }
            if let Some(ms) = it.get("trackTimeMillis").and_then(|x| x.as_u64()) {
                r.duration = Some(crate::engines::dailymotion::fmt_dur(ms / 1000));
            }
            if let Some(pv) = it.get("previewUrl").and_then(|x| x.as_str()) {
                r.preview = Some(pv.to_string());
            }
            if let Some(al) = it.get("collectionName").and_then(|x| x.as_str()) {
                r.meta.push(("album".into(), clip(al, 48)));
            }
            r.meta.push(("engine".into(), "apple".into()));
            out.push(r);
        }
    }
    Ok(out)
}
