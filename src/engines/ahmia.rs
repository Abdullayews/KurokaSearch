// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ahmia — search engine for Tor onion services (ahmia.fi).
//! Powers the Onion category.
//!
//! Ahmia's search form carries a rotating hidden token: the flow is
//! home → extract token → search with it. Without the token the search
//! silently redirects back to the landing page.

use crate::engine::*;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let token = fetch_token()?;
    let page = p.page.max(1);
    let mut url = format!("https://ahmia.fi/search/?q={}", urlencode(&p.query));
    if page > 1 { url.push_str(&format!("&page={page}")); }
    if !token.is_empty() { url.push('&'); url.push_str(&token); }
    let body = http_get_browser(&url, "https://ahmia.fi/", &[], Duration::from_secs(12))?;
    if body.contains("ahmia-home-logo") {
        return Err("ahmia.fi declined this query".into());
    }
    let mut out = Vec::new();
    for item in body.split("<li class=\"result\">").skip(1) {
        let Some(onion) = redirect_onion(item) else { continue };
        let title = h4_text(item);
        if title.is_empty() { continue; }
        let desc = p_text(item);
        let cite = cite_text(item);
        let source = if cite.is_empty() { host_of(&onion) } else { cite };
        let mut r = SearchResult::web(title, onion, desc);
        r.source = source;
        out.push(r);
        if out.len() >= 12 { break; }
    }
    Ok(out)
}

/// name=value of the search form's hidden anti-bot input ("" when absent).
fn fetch_token() -> Result<String, String> {
    let home = http_get_browser("https://ahmia.fi/", "", &[], Duration::from_secs(10))?;
    let Some(i) = home.find("type=\"hidden\"") else { return Ok(String::new()) };
    let seg = &home[i..];
    let name = attr(seg, "name").unwrap_or_default();
    let value = attr(seg, "value").unwrap_or_default();
    if name.is_empty() || value.is_empty() { return Ok(String::new()); }
    Ok(format!("{name}={value}"))
}

/// The onion URL out of /search/redirect?…&redirect_url=http://xxx.onion/…
fn redirect_onion(item: &str) -> Option<String> {
    let i = item.find("redirect_url=")?;
    let rest = &item[i + 13..];
    let end = rest.find(['&', '"']).unwrap_or(rest.len());
    let raw = &rest[..end];
    let url = percent_decode_light(raw);
    if url.contains(".onion") && url.starts_with("http") { Some(url) } else { None }
}

/// Anchor text inside the first <h4> (result title).
fn h4_text(item: &str) -> String {
    let Some(i) = item.find("<h4") else { return String::new() };
    let gt = match item[i..].find('>') { Some(g) => i + g, None => return String::new() };
    let end = item[gt + 1..].find("</h4>").map(|e| gt + 1 + e).unwrap_or(item.len());
    decode_entities(strip_tags(&item[gt + 1..end]).trim())
}

/// Text of the <p> that follows the title (description).
fn p_text(item: &str) -> String {
    let Some(i) = item.find("</h4>") else { return String::new() };
    let rest = &item[i..];
    let Some(a) = rest.find('<') else { return String::new() };
    let gt = match rest[a..].find('>') { Some(g) => a + g, None => return String::new() };
    let end = rest[gt + 1..].find("</p>").map(|e| gt + 1 + e).unwrap_or(rest.len());
    clip(&decode_entities(strip_tags(&rest[gt + 1..end]).trim()), 260)
}

/// Text of <cite> — the bare onion hostname.
fn cite_text(item: &str) -> String {
    let Some(i) = item.find("<cite>") else { return String::new() };
    let end = item[i..].find("</cite>").map(|e| i + e).unwrap_or(item.len());
    strip_tags(&item[i + 6..end]).trim().to_string()
}

fn attr(seg: &str, key: &str) -> Option<String> {
    let pat = format!("{key}=\"");
    let i = seg.find(&pat)?;
    let rest = &seg[i + pat.len()..];
    let j = rest.find('"')?;
    Some(rest[..j].to_string())
}

fn percent_decode_light(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| -> Option<u8> {
                match c {
                    b'0'..=b'9' => Some(c - b'0'),
                    b'a'..=b'f' => Some(c - b'a' + 10),
                    b'A'..=b'F' => Some(c - b'A' + 10),
                    _ => None,
                }
            };
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> SearchParams {
        SearchParams { query: "dark web forum".into(), category: crate::engine::Category::Onion,
            page: 1, engines: vec![], safesearch: 0, language: "auto".into(),
            time: String::new(), size: String::new(), layout: String::new() }
    }

    #[test]
    #[ignore]
    fn live_ahmia() {
        match search(&params()) {
            Ok(rs) => println!("ahmia: {} results, first: {:?}", rs.len(),
                rs.first().map(|r| (r.title.clone(), r.source.clone()))),
            Err(e) => println!("ahmia ERR: {e}"),
        }
    }
}
