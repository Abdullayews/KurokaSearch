// SPDX-License-Identifier: AGPL-3.0-or-later
//! Crossref — scholarly literature metadata. "Science" engine.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let offset = (page - 1) * 10;
    let url = format!(
        "https://api.crossref.org/works?query={}&rows=10&offset={offset}&select=title,abstract,DOI,URL,container-title,issued,author",
        urlencode(&p.query)
    );
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(12))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    let items = v.pointer("/message/items").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    for it in items {
        let title = it.get("title").and_then(|x| x.as_array())
            .and_then(|a| a.first()).and_then(|t| t.as_str()).unwrap_or("").to_string();
        let link = it.get("URL").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if title.is_empty() || link.is_empty() { continue; }
        let abstract_ = it.get("abstract").and_then(|x| x.as_str()).map(|s| strip_tags(s)).unwrap_or_default();
        let journal = it.get("container-title").and_then(|x| x.as_array())
            .and_then(|a| a.first()).and_then(|t| t.as_str()).unwrap_or("").to_string();
        let year = it.pointer("/issued/date-parts/0/0").and_then(|x| x.as_u64());
        let authors = it.get("author").and_then(|x| x.as_array()).map(|a| {
            a.iter().take(3).filter_map(|au| au.get("family").and_then(|f| f.as_str())).collect::<Vec<_>>().join(", ")
        }).unwrap_or_default();
        let mut r = SearchResult::new("web", clip(&title, 170), link, clip(&abstract_, 320));
        if !journal.is_empty() { r.meta.push(("journal".into(), clip(&journal, 60))); }
        if !authors.is_empty() { r.meta.push(("authors".into(), clip(&authors, 60))); }
        if let Some(y) = year { r.published = Some(y.to_string()); }
        r.meta.push(("engine".into(), "crossref".into()));
        out.push(r);
    }
    Ok(out)
}
