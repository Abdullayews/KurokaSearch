// SPDX-License-Identifier: AGPL-3.0-or-later
//! Wikipedia — REST search API. "All" engine.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let limit = 6;
    let offset = (p.page.saturating_sub(1)) * limit;
    let lang = wiki_lang(&p.language);
    let url = format!(
        "https://{lang}.wikipedia.org/w/rest.php/v1/search/page?q={}&limit={limit}&offset={offset}",
        urlencode(&p.query)
    );
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(pages) = v.get("pages").and_then(|x| x.as_array()) {
        for pg in pages {
            let title = pg.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let desc = pg.get("excerpt").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let key = pg.get("key").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if title.is_empty() || key.is_empty() { continue; }
            let link = format!("https://{lang}.wikipedia.org/wiki/{}", key.replace(' ', "_"));
            let thumb = pg.get("thumbnail")
                .and_then(|t| t.get("source")).and_then(|s| s.as_str())
                .map(|s| s.to_string());
            let mut r = SearchResult::web(title, link, clip(&strip_tags(&desc), 320));
            r.thumb = thumb;
            r.meta.push(("engine".into(), "wikipedia".into()));
            out.push(r);
        }
    }
    Ok(out)
}

fn wiki_lang(language: &str) -> String {
    match language {
        "auto" | "" => "en".into(),
        other => other.into(),
    }
}
