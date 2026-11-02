// SPDX-License-Identifier: AGPL-3.0-or-later
//! MusicBrainz — open music database. "Music" engine.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let offset = (page - 1) * 16;
    let url = format!(
        "https://musicbrainz.org/ws/2/recording?query={}&fmt=json&limit=16&offset={offset}",
        urlencode(&p.query)
    );
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(12))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(list) = v.pointer("/recordings").and_then(|x| x.as_array()) {
        for it in list {
            let title = it.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let id = it.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if title.is_empty() || id.is_empty() { continue; }
            let link = format!("https://musicbrainz.org/recording/{id}");
            let artist = it.get("artist-credit").and_then(|x| x.as_array())
                .and_then(|a| a.first())
                .and_then(|a| a.get("name")).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let mut r = SearchResult::new("music", clip(&title, 90), link, clip(&artist, 120));
            if let Some(ms) = it.get("length").and_then(|x| x.as_u64()) {
                r.duration = Some(crate::engines::dailymotion::fmt_dur(ms / 1000));
            }
            let releases = it.get("releases").and_then(|x| x.as_array())
                .and_then(|a| a.first());
            if let Some(rel) = releases {
                if let Some(rn) = rel.get("title").and_then(|x| x.as_str()) {
                    r.meta.push(("release".into(), clip(rn, 48)));
                }
                if let Some(date) = rel.get("date").and_then(|x| x.as_str()) {
                    r.published = Some(date.to_string());
                }
            }
            r.meta.push(("engine".into(), "musicbrainz".into()));
            out.push(r);
        }
    }
    Ok(out)
}
