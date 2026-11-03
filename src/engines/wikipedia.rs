// SPDX-License-Identifier: AGPL-3.0-or-later
//! Wikipedia — Action API search (`list=search`). The REST endpoint ignores
//! its `offset` parameter, so real pagination goes through the classic
//! Action API with `sroffset`. "All" engine.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let limit = 6;
    let offset = (p.page.saturating_sub(1)) * limit;
    let lang = wiki_lang(&p.language);
    let url = format!(
        "https://{lang}.wikipedia.org/w/api.php?action=query&format=json\
&list=search&srsearch={}&srlimit={limit}&sroffset={offset}&srprop=snippet|timestamp",
        urlencode(&p.query)
    );
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(arr) = v.pointer("/query/search").and_then(|x| x.as_array()) {
        for pg in arr {
            let title = pg.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if title.is_empty() { continue; }
            let snippet = pg.get("snippet").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let when = pg.get("timestamp").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let link = format!("https://{lang}.wikipedia.org/wiki/{}", urlencode(&title.replace(' ', "_")));
            let mut r = SearchResult::web(title, link, clip(&clean_text(&snippet), 320));
            let epoch = parse_iso8601(&when);
            if epoch.is_some() {
                r.published = Some(when.clone());
                r.published_epoch = epoch;
            }
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
