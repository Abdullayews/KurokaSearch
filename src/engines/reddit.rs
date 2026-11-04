// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reddit search (social) — the public JSON endpoint, queried server-side
//! so reddit never sees the visitor. Threads open on their permalink.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let t = match p.time.as_str() {
        "d" => "day", "w" => "week", "m" => "month", "y" => "year", _ => "all",
    };
    let url = format!(
        "https://www.reddit.com/search.json?q={}&limit=25&sort=relevance&t={t}",
        urlencode(&p.query));
    let body = crate::proxypool::get_json_walled(
        &url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    let children = v.get("data").and_then(|d| d.get("children"))
        .and_then(|c| c.as_array());
    for it in children.into_iter().flatten() {
        let d = match it.get("data") { Some(d) => d, None => continue };
        let title = d.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let permalink = d.get("permalink").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if title.is_empty() || permalink.is_empty() { continue; }
        let link = format!("https://www.reddit.com{permalink}");
        let mut desc = d.get("selftext").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if desc.is_empty() {
            if let Some(dom) = d.get("domain").and_then(|x| x.as_str()) {
                desc = format!("links to {dom}");
            }
        }
        let mut r = SearchResult::new("social", clip(&title, 130), link, clip(&desc, 260));
        if let Some(th) = d.get("thumbnail").and_then(|x| x.as_str()) {
            if th.starts_with("http") { r.thumb = Some(th.to_string()); }
        }
        let sub = d.get("subreddit").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if !sub.is_empty() { r.meta.push(("subreddit".into(), clip(&sub, 40))); }
        if let Some(score) = d.get("score").and_then(|x| x.as_i64()) {
            r.meta.push(("score".into(), format!("{score}")));
        }
        if let Some(nc) = d.get("num_comments").and_then(|x| x.as_i64()) {
            r.meta.push(("comments".into(), format!("{nc}")));
        }
        if let Some(ts) = d.get("created_utc").and_then(|x| x.as_f64()) {
            r.published_epoch = Some(ts as i64);
        }
        r.meta.push(("engine".into(), "reddit".into()));
        out.push(r);
        if out.len() >= 25 { break; }
    }
    Ok(out)
}
