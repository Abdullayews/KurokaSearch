// SPDX-License-Identifier: AGPL-3.0-or-later
//! Stack Exchange API — Stack Overflow. "IT" engine.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!(
        "https://api.stackexchange.com/2.3/search/advanced?order=desc&sort=relevance&q={}&site=stackoverflow&pagesize=12&page={page}&filter=default",
        urlencode(&p.query)
    );
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(items) = v.get("items").and_then(|x| x.as_array()) {
        for it in items {
            let title = decode_entities(it.get("title").and_then(|x| x.as_str()).unwrap_or(""));
            let link = it.get("link").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if title.is_empty() || link.is_empty() { continue; }
            let score = it.get("score").and_then(|x| x.as_i64()).unwrap_or(0);
            let answers = it.get("answer_count").and_then(|x| x.as_i64()).unwrap_or(0);
            let answered = it.get("is_answered").and_then(|x| x.as_bool()).unwrap_or(false);
            let tags = it.get("tags").and_then(|x| x.as_array()).map(|a|
                a.iter().take(4).filter_map(|t| t.as_str().map(|s| s.to_string())).collect::<Vec<_>>()
            ).unwrap_or_default();
            let mut r = SearchResult::new("web", clip(&title, 130), link,
                format!("{} answer{}", answers, if answers == 1 { "" } else { "s" }));
            r.meta.push(("score".into(), score.to_string()));
            r.meta.push(("state".into(), if answered { "answered".into() } else { "open".into() }));
            if !tags.is_empty() { r.meta.push(("tags".into(), tags.join(" · "))); }
            r.meta.push(("engine".into(), "stackoverflow".into()));
            out.push(r);
        }
    }
    Ok(out)
}
