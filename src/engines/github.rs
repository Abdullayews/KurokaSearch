// SPDX-License-Identifier: AGPL-3.0-or-later
//! GitHub repository search. "IT" engine.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!(
        "https://api.github.com/search/repositories?q={}&sort=best-match&per_page=8&page={page}",
        urlencode(&p.query)
    );
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/vnd.github+json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(items) = v.get("items").and_then(|x| x.as_array()) {
        for it in items {
            let full = it.get("full_name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let link = it.get("html_url").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if full.is_empty() || link.is_empty() { continue; }
            let desc = it.get("description").and_then(|x| x.as_str()).unwrap_or("");
            let stars = it.get("stargazers_count").and_then(|x| x.as_u64()).unwrap_or(0);
            let lang = it.get("language").and_then(|x| x.as_str()).unwrap_or("");
            let mut r = SearchResult::new("web", full, link, clip(desc, 260));
            r.meta.push(("stars".into(), format!("{stars}★")));
            if !lang.is_empty() { r.meta.push(("lang".into(), lang.to_string())); }
            r.meta.push(("engine".into(), "github".into()));
            out.push(r);
        }
    }
    Ok(out)
}
