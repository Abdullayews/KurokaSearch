// SPDX-License-Identifier: AGPL-3.0-or-later
//! Files-tab book engines: Project Gutenberg + Standard Ebooks — curated,
//! public-domain, server-rendered catalogs (no keys, no bot walls).

use crate::engine::*;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Project Gutenberg — the grand public-domain library. 25 per page.
// ---------------------------------------------------------------------------

pub fn gutenberg_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let start = (p.page.max(1) - 1) * 25 + 1;
    let url = format!("https://www.gutenberg.org/ebooks/search/?query={}&start_index={start}",
        urlencode(&p.query));
    let body = http_get_browser(&url, "https://www.gutenberg.org/", &[], Duration::from_secs(12))?;
    if !body.contains("booklink") { return Ok(vec![]); }
    let mut out = Vec::new();
    for block in body.split(r#"<li class="booklink">"#).skip(1) {
        let href = block.split("href=\"").nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap_or("")
            .to_string();
        if !href.starts_with("/ebooks/") { continue; }
        let title = span_of(block, "title");
        if title.is_empty() { continue; }
        let author = span_of(block, "subtitle");
        let extra = span_of(block, "extra");
        let thumb = block.split("src=\"").nth(1)
            .and_then(|s| s.split('"').next())
            .filter(|s| s.starts_with("/cache/"))
            .map(|s| format!("https://www.gutenberg.org{s}"));
        let mut r = SearchResult::web(title, format!("https://www.gutenberg.org{href}"),
            clip(&author, 200));
        r.source = "gutenberg.org".into();
        r.thumb = thumb;
        if !extra.is_empty() { r.meta.push(("downloads".into(), extra)); }
        if !author.is_empty() { r.meta.push(("author".into(), author)); }
        r.meta.push(("engine".into(), "gutenberg".into()));
        out.push(r);
        if out.len() >= 25 { break; }
    }
    Ok(out)
}

/// text of `<span class="CLS">…</span>`
fn span_of(block: &str, cls: &str) -> String {
    let marker = format!(r#"class="{cls}""#);
    let Some(i) = block.find(&marker) else { return String::new() };
    let rest = &block[i + marker.len()..];
    let Some(gt) = rest.find('>') else { return String::new() };
    let Some(end) = rest[gt + 1..].find("</span>") else { return String::new() };
    decode_entities(&strip_tags(&rest[gt + 1..gt + 1 + end])).trim().to_string()
}

// ---------------------------------------------------------------------------
// Standard Ebooks — lovingly typeset public-domain classics.
// ---------------------------------------------------------------------------

pub fn standardebooks_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!("https://standardebooks.org/ebooks?query={}&view=grid&per-page=12&page={page}",
        urlencode(&p.query));
    let body = http_get_browser(&url, "https://standardebooks.org/", &[], Duration::from_secs(12))?;
    if !body.contains("schema:Book") { return Ok(vec![]); }
    let mut out = Vec::new();
    for block in body.split("<li typeof=\"schema:Book\"").skip(1) {
        let href = block.split("about=\"").nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap_or("")
            .to_string();
        if !href.starts_with("/ebooks/") { continue; }
        // first schema:name = book title, the one inside class="author" = author
        let title = schema_name(block);
        if title.is_empty() { continue; }
        let author = block.find("class=\"author\"")
            .map(|i| schema_name(&block[i..]))
            .unwrap_or_default();
        let thumb = block.split("src=\"").nth(1)
            .and_then(|s| s.split('"').next())
            .filter(|s| s.starts_with("/images/covers/"))
            .map(|s| format!("https://standardebooks.org{s}"));
        let mut r = SearchResult::web(title, format!("https://standardebooks.org{href}"),
            clip(&author, 200));
        r.source = "standardebooks.org".into();
        r.thumb = thumb;
        if !author.is_empty() { r.meta.push(("author".into(), author)); }
        r.meta.push(("engine".into(), "standard ebooks".into()));
        out.push(r);
        if out.len() >= 12 { break; }
    }
    Ok(out)
}

/// text of the first `<span property="schema:name">…</span>` in `block`
fn schema_name(block: &str) -> String {
    let marker = "property=\"schema:name\"";
    let Some(i) = block.find(marker) else { return String::new() };
    let rest = &block[i + marker.len()..];
    let Some(gt) = rest.find('>') else { return String::new() };
    let Some(end) = rest[gt + 1..].find("</span>") else { return String::new() };
    decode_entities(&strip_tags(&rest[gt + 1..gt + 1 + end])).trim().to_string()
}
