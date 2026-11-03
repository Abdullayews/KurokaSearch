// SPDX-License-Identifier: AGPL-3.0-or-later
//! Extra news engines: Yahoo News (HTML scrape). DDG news lives in web_extra.

use crate::engine::*;
use std::time::Duration;

/// Yahoo News search. Result links are `r.search.yahoo.com/…/RU=<real>/RK=…`
/// redirects; the real target is the percent-encoded RU= segment.
pub fn yahoo_news(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let b = (p.page.max(1) - 1) * 10 + 1;
    let mut url = format!("https://news.search.yahoo.com/search?p={}&b={b}", urlencode(&p.query));
    if p.time == "d" { url.push_str("&age=1d"); }
    else if p.time == "w" { url.push_str("&age=7d"); }
    else if p.time == "m" { url.push_str("&age=1m"); }
    let body = http_get_browser(&url, "https://news.yahoo.com/", &[], Duration::from_secs(12))?;
    if !body.contains("s-title") { return Ok(vec![]); }
    let mut out = Vec::new();
    // every article sits in its own <li class="ov-a…"> block
    for li in body.split(r#"<li class="ov-a"#).skip(1) {
        let li = li.split("</li>").next().unwrap_or(li);
        let title_seg = li.split(r#"class="s-title""#).nth(1)
            .and_then(|s| s.split("</h4>").next().map(|x| x.to_string()))
            .unwrap_or_default();
        let Some(i) = title_seg.find("href=\"") else { continue };
        let href = attr_after(&title_seg[i..], "href=\"").unwrap_or_default();
        let link = yahoo_unwrap(&href);
        if link.is_empty() { continue; }
        let title = decode_entities(&strip_tags(&title_seg).trim());
        if title.is_empty() { continue; }
        let source = html_seg(li, r#"class="s-source""#, "</span>")
            .map(|s| strip_tags(&s).trim().to_string())
            .unwrap_or_default();
        let time_raw = html_seg(li, r#"class="s-time""#, "</span>")
            .map(|s| strip_tags(&s).trim().trim_end_matches('·').trim().to_string())
            .unwrap_or_default();
        let desc = html_seg(li, r#"class="s-desc""#, "</p>")
            .map(|s| decode_entities(&strip_tags(&s).trim()))
            .unwrap_or_default();
        let thumb = html_seg(li, r#"class="thmb""#, ">")
            .and_then(|s| attr_after(&s, "src=\""));
        let epoch = parse_relative(&time_raw);
        if !time_accepts(&p.time, epoch) { continue; }
        let mut r = SearchResult::new("news", title, link, clip(&desc, 280));
        if !source.is_empty() {
            r.source = source.split('·').next().unwrap_or(&source).trim().to_string();
        }
        r.published = Some(time_raw);
        r.published_epoch = epoch;
        r.thumb = thumb;
        r.meta.push(("engine".into(), "yahoo news".into()));
        out.push(r);
        if out.len() >= 15 { break; }
    }
    Ok(out)
}

fn html_seg<'a>(li: &'a str, open: &str, close: &str) -> Option<String> {
    let i = li.find(open)?;
    let rest = &li[i + open.len()..];
    let end = rest.find(close)?;
    Some(rest[..end].to_string())
}

fn yahoo_unwrap(href: &str) -> String {
    if let Some(i) = href.find("/RU=") {
        let rest = &href[i + 4..];
        let enc: String = rest.chars().take_while(|c| *c != '/').collect();
        let dec = urldecode(&enc);
        if dec.starts_with("http") { return dec; }
    }
    if href.starts_with("http") && !href.contains("r.search.yahoo.com") { href.to_string() } else { String::new() }
}

fn attr_after(chunk: &str, prefix: &str) -> Option<String> {
    let i = chunk.find(prefix)?;
    let rest = &chunk[i + prefix.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
