// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dailymotion — open video search API. "Videos" engine.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let mut url = format!(
        "https://api.dailymotion.com/videos?search={}&fields=id,title,url,duration,thumbnail_360_url,owner.screenname&limit=16&page={page}",
        urlencode(&p.query)
    );
    if p.safesearch >= 1 {
        url.push_str("&explicit=false");
    } else {
        url.push_str("&explicit=true");
    }
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(list) = v.get("list").and_then(|x| x.as_array()) {
        for it in list {
            let title = it.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let link = it.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let id = it.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if title.is_empty() || link.is_empty() || id.is_empty() { continue; }
            let dur = it.get("duration").and_then(|x| x.as_u64()).unwrap_or(0);
            let thumb = it.get("thumbnail_360_url").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let owner = it.get("owner.screenname").and_then(|x| x.as_str()).unwrap_or("Dailymotion").to_string();
            let mut r = SearchResult::new("video", clip(&title, 110), link, String::new());
            if !thumb.is_empty() {
                r.thumb = Some(thumb);
            }
            r.duration = Some(fmt_dur(dur));
            r.publisher = Some(owner);
            r.meta.push(("engine".into(), "dailymotion".into()));
            out.push(r);
        }
    }
    Ok(out)
}

pub fn fmt_dur(sec: u64) -> String {
    let h = sec / 3600; let m = (sec % 3600) / 60; let s = sec % 60;
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}
