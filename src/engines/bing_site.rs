// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bing Web RSS with query modifiers — powers "Files" (filetype:pdf)
//! and "Social Media" (site: filters) categories.

use crate::engine::*;
use std::time::Duration;

const SOCIAL_SITES: &[&str] = &[
    "x.com", "twitter.com", "reddit.com", "instagram.com", "threads.net",
    "facebook.com", "tiktok.com", "tumblr.com", "vk.com", "linkedin.com",
];

pub fn search_files(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let q = format!("{} filetype:pdf", p.query.trim());
    bing_rss(p, &q)
}

pub fn search_social(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let sites = SOCIAL_SITES.iter().map(|s| format!("site:{s}")).collect::<Vec<_>>().join(" OR ");
    let q = format!("({}) {}", sites, p.query.trim());
    bing_rss(p, &q)
}

fn bing_rss(p: &SearchParams, q: &str) -> Result<Vec<SearchResult>, String> {
    let first = (p.page.saturating_sub(1)) * 10 + 1;
    let mut url = format!(
        "https://www.bing.com/search?q={}&format=rss&count=10&first={first}",
        urlencode(q)
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
        let mut r = SearchResult::web(title, link, clip(&desc, 300));
        r.published_epoch = epoch;
        if !pubdate.is_empty() { r.published = Some(pubdate); }
        out.push(r);
        if out.len() >= 12 { break; }
    }
    Ok(out)
}
