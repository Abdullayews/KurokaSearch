// SPDX-License-Identifier: AGPL-3.0-or-later
//! SolidTorrents — torrent search (solidtorrents.to).
//! Files category.

use crate::engine::*;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!("https://solidtorrents.to/search?q={}&page={page}", urlencode(&p.query));
    let body = http_get_browser(&url, "https://solidtorrents.to/", &[], Duration::from_secs(12))?;
    let mut out = Vec::new();
    for card in body.split("torrent-result-card").skip(1) {
        let Some(href) = find_href(card, "/torrent/") else { continue };
        let Some(title) = anchor_text(card, &format!("\"{href}\"")) else { continue };
        if title.is_empty() { continue; }
        let link = format!("https://solidtorrents.to{href}");
        let mut cat = String::new();
        let mut size = String::new();
        let mut seed = String::new();
        let mut leech = String::new();
        if let Some(i) = card.find("torrent-result-meta") {
            let seg = &card[i..];
            let vals = meta_spans(seg);
            if vals.len() >= 1 { cat = vals[0].clone(); }
            if vals.len() >= 2 { size = vals[1].clone(); }
            if vals.len() >= 4 { seed = vals[3].clone(); }
            if vals.len() >= 5 { leech = vals[4].clone(); }
        }
        if p.safesearch == 2 && cat.to_lowercase().contains("xxx") { continue; }
        let mut r = SearchResult::web(title, link, format!("{cat} · {size}"));
        r.source = "solidtorrents.to".into();
        if !seed.is_empty() { r.meta.push(("seeders".into(), seed)); }
        if !leech.is_empty() { r.meta.push(("leechers".into(), leech)); }
        out.push(r);
        if out.len() >= 12 { break; }
    }
    Ok(out)
}

/// Text of every <span>…</span> up to the first swarm-stats block.
fn meta_spans(seg: &str) -> Vec<String> {
    let cut = seg.find("Swarm Stats").unwrap_or(seg.len().min(4000));
    let seg = &seg[..cut];
    let mut vals = Vec::new();
    let mut rest = seg;
    while let Some(i) = rest.find('<') {
        let after = &rest[i..];
        if let Some(sp) = after.strip_prefix("<span") {
            if let Some(gt) = sp.find('>') {
                let close = rest[i + 5 + gt + 1..].find("</span>")
                    .map(|e| i + 5 + gt + 1 + e).unwrap_or(rest.len());
                let text = strip_tags(&rest[i + 5 + gt + 1..close]).trim().to_string();
                if !text.is_empty() { vals.push(text); }
                rest = &rest[(close + 7).min(rest.len())..];
                continue;
            }
        }
        rest = &rest[i + 1..];
    }
    vals
}

fn find_href(chunk: &str, prefix: &str) -> Option<String> {
    let mut rest = chunk;
    while let Some(i) = rest.find("href=\"") {
        let after = &rest[i + 6..];
        let j = after.find('"')?;
        let href = &after[..j];
        if href.starts_with(prefix) { return Some(href.to_string()); }
        rest = after;
    }
    None
}

/// Text of the first anchor whose href equals `href_lit` (quoted).
fn anchor_text(chunk: &str, href_lit: &str) -> Option<String> {
    let pat = format!("href={href_lit}");
    let i = chunk.find(&pat)?;
    let gt = chunk[i..].find('>')? + i;
    let end = chunk[gt + 1..].find("</a>").map(|e| gt + 1 + e).unwrap_or(chunk.len());
    Some(strip_tags(&chunk[gt + 1..end]).trim().to_string())
}
