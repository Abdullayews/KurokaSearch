// SPDX-License-Identifier: AGPL-3.0-or-later
//! arXiv — open science preprints. "Science" engine.

use crate::engine::*;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let start = (p.page.saturating_sub(1)) * 10;
    let url = format!(
        "https://export.arxiv.org/api/query?search_query=all:{}&start={start}&max_results=10&sortBy=relevance",
        urlencode(&p.query)
    );
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/atom+xml")], Duration::from_secs(12))?;
    let mut out = Vec::new();
    for item in body.split("<entry>").skip(1) {
        let title = tag_text(item, "title").map(|t| t.split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_default();
        let link = tag_text(item, "id").unwrap_or_default();
        let summary = tag_text(item, "summary").map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let published = tag_text(item, "published").unwrap_or_default();
        if title.is_empty() || link.is_empty() { continue; }
        let mut r = SearchResult::new("web", clip(&title, 160), link, clip(&summary, 380));
        if !published.is_empty() {
            let date = &published[..published.len().min(10)];
            r.published = Some(date.to_string());
        }
        r.meta.push(("engine".into(), "arxiv".into()));
        out.push(r);
        if out.len() >= 10 { break; }
    }
    Ok(out)
}
