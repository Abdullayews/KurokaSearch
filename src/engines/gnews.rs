// SPDX-License-Identifier: AGPL-3.0-or-later
//! Google News via the public RSS feed. Time filter via `when:` operator.

use crate::engine::*;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let first = (p.page.saturating_sub(1)) * 10 + 1;
    let mut q = p.query.clone();
    if !p.time.is_empty() {
        let op = match p.time.as_str() { "d" => "1d", "w" => "7d", "m" => "1m", _ => "1y" };
        q.push_str(&format!(" when:{op}"));
    }
    let (hl, gl, ceid) = gnews_locale(&p.language);
    let mut url = format!(
        "https://news.google.com/rss/search?q={}&hl={hl}&gl={gl}&ceid={ceid}",
        urlencode(&q)
    );
    if p.page > 1 {
        // Google News RSS does not really page; nudge with start is ignored,
        // so deeper pages are served by bing_news only.
        url.push_str(&format!("&start={first}"));
    }
    if p.safesearch == 2 { url.push_str("&safe=active"); }
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/rss+xml")], Duration::from_secs(12))?;
    let mut out = Vec::new();
    for item in body.split("<item>").skip(1) {
        let title = decode_entities(&tag_text(item, "title").unwrap_or_default());
        let link = tag_text(item, "link").unwrap_or_default();
        let pubdate = tag_text(item, "pubDate").unwrap_or_default();
        let source = decode_entities(&tag_text(item, "source").unwrap_or_default());
        let desc = strip_tags(&tag_text(item, "description").unwrap_or_default());
        if title.is_empty() || link.is_empty() { continue; }
        let epoch = parse_rfc2822(&pubdate);
        if !time_accepts(&p.time, epoch) { continue; }
        let mut r = SearchResult::new("news", title, link, clip(&desc, 300));
        if !source.is_empty() { r.source = source; }
        r.published = Some(pubdate);
        r.published_epoch = epoch;
        r.meta.push(("engine".into(), "gnews".into()));
        out.push(r);
        if out.len() >= 20 { break; }
    }
    Ok(out)
}
