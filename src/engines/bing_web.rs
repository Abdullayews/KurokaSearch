// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bing Web via the public RSS endpoint. Primary "All" engine.

use crate::engine::*;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let first = (p.page.saturating_sub(1)) * 10 + 1;
    let mut url = format!(
        "https://www.bing.com/search?q={}&format=rss&count=10&first={first}",
        urlencode(&p.query)
    );
    if let Some(mkt) = bing_mkt(&p.language) {
        url.push_str("&mkt=");
        url.push_str(mkt);
    }
    if p.safesearch == 2 {
        url.push_str("&adlt=strict");
    }
    let body = http_get_browser(&url, "https://www.bing.com/", &[], Duration::from_secs(10))?;
    let mut out = Vec::new();
    for item in body.split("<item>").skip(1) {
        let title = decode_entities(&tag_text(item, "title").unwrap_or_default());
        let link = tag_text(item, "link").unwrap_or_default();
        let desc = strip_tags(&tag_text(item, "description").unwrap_or_default());
        let pubdate = tag_text(item, "pubDate").unwrap_or_default();
        if title.is_empty() || link.is_empty() { continue; }
        let epoch = parse_rfc2822(&pubdate);
        if !time_accepts(&p.time, epoch) { continue; }
        let mut r = SearchResult::web(title, link, clip(&desc, 340));
        if !pubdate.is_empty() { r.published = Some(pubdate.clone()); }
        r.published_epoch = epoch;
        r.meta.push(("engine".into(), "bing".into()));
        out.push(r);
        if out.len() >= 12 { break; }
    }
    Ok(out)
}
