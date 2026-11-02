// SPDX-License-Identifier: AGPL-3.0-or-later
//! Anna's Archive — books & papers search. Files category.
//! The project's mirrors rotate often, so every request walks the list.

use crate::engine::*;
use std::time::Duration;

const MIRRORS: &[&str] = &[
    "annas-archive.li",
    "annas-archive.org",
    "annas-archive.se",
    "annas-archive.to",
    "annas-archive.ng",
];

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let mut last_err = String::new();
    for host in MIRRORS {
        let mut url = format!("https://{host}/search?q={}", urlencode(&p.query));
        if page > 1 { url.push_str(&format!("&page={page}")); }
        match http_get(&url, USER_AGENT, &[("Accept", "text/html")], Duration::from_secs(10)) {
            Ok(body) => {
                if !body.contains("/md5/") {
                    // parked / challenge page — try the next mirror
                    last_err = format!("{host}: no results markup");
                    continue;
                }
                return Ok(parse(&body, host));
            }
            Err(e) => { last_err = format!("{host}: {e}"); }
        }
    }
    Err(format!("all mirrors unreachable ({})", clip(&last_err, 90)))
}

fn parse(body: &str, host: &str) -> Vec<SearchResult> {
    let mut out = Vec::new();
    for chunk in body.split("href=\"/md5/").skip(1) {
        let hash: String = chunk.chars().take_while(|c| c.is_ascii_hexdigit()).take(32).collect();
        if hash.len() != 32 { continue; }
        let seg = chunk.split("</a>").next().unwrap_or(chunk);
        let title = match h3_text(seg) {
            Some(t) => t,
            None => strip_tags(seg).trim().to_string(),
        };
        let title = decode_entities(title.trim());
        if title.is_empty() { continue; }
        let text = strip_tags(seg);
        let desc = clip(&text, 260);
        let link = format!("https://{host}/md5/{hash}");
        let mut r = SearchResult::web(title, link, desc);
        r.source = host.into();
        if let Some(fmt) = find_format(&text) { r.meta.push(("format".into(), fmt)); }
        out.push(r);
        if out.len() >= 15 { break; }
    }
    out
}

fn h3_text(seg: &str) -> Option<String> {
    let i = seg.find("<h3")?;
    let gt = seg[i..].find('>')? + i;
    let end = seg[gt + 1..].find("</h3>").map(|e| gt + 1 + e).unwrap_or(seg.len());
    Some(strip_tags(&seg[gt + 1..end]).trim().to_string())
}

/// Looks for the ".pdf, 5.2MB" style annotation Anna's rows carry.
fn find_format(text: &str) -> Option<String> {
    let mut it = text.char_indices();
    while let Some((i, c)) = it.next() {
        if c != '.' { continue; }
        let rest = &text[i + 1..];
        let mut chars = rest.chars();
        if let Some(first) = chars.next() {
            if first.is_ascii_alphabetic() {
                let ext: String = rest.chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>();
                if (2..=5).contains(&ext.len()) {
                    let after = rest.trim_start_matches(|c: char| c.is_ascii_alphanumeric());
                    if after.starts_with(',') {
                        return Some(ext.to_uppercase());
                    }
                }
            }
        }
    }
    None
}
