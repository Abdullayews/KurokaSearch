// SPDX-License-Identifier: AGPL-3.0-or-later
//! Openverse — openly licensed images (CC / public domain). "Images" engine.

use crate::engine::*;
use serde_json::Value;
use std::thread;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let mut url = format!(
        "https://api.openverse.org/v1/images/?q={}&page_size=24&page={page}",
        urlencode(&p.query)
    );
    // safesearch off lets mature content through, default filters it
    if p.safesearch == 0 {
        url.push_str("&mature=true");
    }
    // Openverse's edge is burst-flaky from datacenters — one spaced retry
    // turns most first-packet losses into hits
    let mut body = None;
    for attempt in 0..2 {
        match http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(10)) {
            Ok(b) => { body = Some(b); break; }
            Err(_) if attempt == 0 => { thread::sleep(Duration::from_millis(600)); continue; }
            Err(e) => return Err(e),
        }
    }
    let body = body.ok_or_else(|| "openverse unreachable".to_string())?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(results) = v.get("results").and_then(|x| x.as_array()) {
        for it in results {
            let title = it.get("title").and_then(|x| x.as_str()).unwrap_or("Untitled").to_string();
            let landing = it.get("foreign_landing_url").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let img = it.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let thumb = it.get("thumbnail").and_then(|x| x.as_str()).unwrap_or(&img).to_string();
            if img.is_empty() || landing.is_empty() { continue; }
            let w = it.get("width").and_then(|x| x.as_u64()).map(|x| x as u32);
            let h = it.get("height").and_then(|x| x.as_u64()).map(|x| x as u32);
            if !matches_size(w, h, &p.size) || !matches_layout(w, h, &p.layout) { continue; }
            let source = it.get("source").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let license = it.get("license").and_then(|x| x.as_str()).unwrap_or("").to_uppercase();
            let mut r = SearchResult::new("image", clip(&title, 90), landing, String::new());
            r.img = Some(img);
            r.thumb = Some(thumb);
            r.width = w;
            r.height = h;
            if !source.is_empty() { r.source = source; }
            if !license.is_empty() { r.meta.push(("license".into(), license)); }
            r.meta.push(("engine".into(), "openverse".into()));
            out.push(r);
        }
    }
    Ok(out)
}
