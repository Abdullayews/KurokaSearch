// SPDX-License-Identifier: AGPL-3.0-or-later
//! Extra web engines: DuckDuckGo (HTML), Marginalia, Wikidata. "All" category.

use crate::engine::*;
use serde_json::Value;
use std::thread;
use std::time::Duration;

// ---------------------------------------------------------------------------
// DuckDuckGo — the non-JS HTML endpoint. Redirect links carry uddg=<real url>.
// ---------------------------------------------------------------------------

pub fn ddg_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    ddg_html(p, "")
}

pub fn ddg_news(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let df = match p.time.as_str() { "d" => "d", "w" => "w", "m" => "m", "y" => "y", _ => "" };
    ddg_html(p, &format!("&iar=news{}", if df.is_empty() { String::new() } else { format!("&df={df}") }))
}

fn ddg_html(p: &SearchParams, extra: &str) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let s = (page - 1) * 20;
    let url = format!("https://html.duckduckgo.com/html/?q={}{extra}&s={s}", urlencode(&p.query));
    // one retry — the anomaly check is burst-triggered, a second try
    // right after often gets a clean page
    let mut body = String::new();
    let mut last_err: Option<String> = None;
    for attempt in 0..2 {
        match http_get_ddg(&url, USER_AGENT, &[("Accept", "text/html")], Duration::from_secs(10)) {
            Ok(b) if b.contains("result__a") => { body = b; break; }
            Ok(b) => {
                let blocked = b.contains("anomaly") || b.contains("challenge")
                    || b.contains("redirected to the non-JavaScript") || b.len() < 4000;
                if blocked {
                    last_err = Some("rate limited (bot check)".into());
                    if attempt == 0 { thread::sleep(Duration::from_millis(500)); continue; }
                } else {
                    return Ok(vec![]);
                }
            }
            Err(e) => {
                last_err = Some(format!("http {e}"));
                if attempt == 0 { thread::sleep(Duration::from_millis(500)); continue; }
            }
        }
    }
    if body.is_empty() {
        return Err(last_err.unwrap_or_else(|| "no results page".into()));
    }
    let out = ddg_parse(&body);
    if out.is_empty() {
        // result__a existed but nothing parsed — surface why
        let visible = strip_tags(&body);
        let visible = visible.trim();
        return Err(format!("layout changed ({})", clip(visible, 36)));
    }
    Ok(out)
}

/// Walk every `<a … result__a …>` anchor — attribute order agnostic,
/// entity-decodes the redirect href before unwrapping. Snippets are
/// collected from the `result__snippet` anchors in the same walk and
/// zipped by order.
fn ddg_parse(body: &str) -> Vec<SearchResult> {
    let mut titles: Vec<(String, String)> = Vec::new(); // (href, text)
    let mut snippets: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(i) = rest.find("<a") {
        let Some(gt) = rest[i..].find('>') else { break };
        let (tag, after) = (&rest[i..i + gt + 1], &rest[i + gt + 1..]);
        let inner = after.split("</a>").next().unwrap_or("");
        if tag.contains("result__snippet") {
            snippets.push(decode_entities(&strip_tags(inner).trim().to_string()));
        } else if tag.contains("result__a") {
            let href = tag.split("href=\"").nth(1)
                .and_then(|h| h.split('"').next()).unwrap_or("");
            titles.push((decode_entities(href), decode_entities(&strip_tags(inner).trim().to_string())));
        }
        rest = after;
    }
    let mut out = Vec::new();
    for (idx, (href, title)) in titles.into_iter().enumerate() {
        let link = ddg_unwrap(&href);
        if title.is_empty() || link.is_empty() { continue; }
        let snippet = snippets.get(idx).cloned().unwrap_or_default();
        let mut r = SearchResult::web(title, link, clip(&snippet, 320));
        r.meta.push(("engine".into(), "duckduckgo".into()));
        out.push(r);
        if out.len() >= 20 { break; }
    }
    out
}

/// DDG hrefs look like `//duckduckgo.com/l/?uddg=https%3A%2F%2Fsite%2F&rut=…`
fn ddg_unwrap(href: &str) -> String {
    let href = href.trim();
    if let Some(i) = href.find("uddg=") {
        let rest = &href[i + 5..];
        let enc: String = rest.chars().take_while(|c| *c != '&').collect();
        let dec = urldecode(&enc);
        if dec.starts_with("http") { return dec; }
    }
    if href.starts_with("http") { href.to_string() } else { String::new() }
}

/// value of the attribute that opens right before the current position
fn attr_after(chunk: &str, prefix: &str) -> Option<String> {
    let i = chunk.find(prefix)?;
    let rest = &chunk[i + prefix.len()..];
    // find the closing quote
    let q = rest.chars().next()?;
    if q != '"' && q != '\'' { return None; }
    let end = rest[1..].find(q)?;
    Some(rest[1..1 + end].to_string())
}

// ---------------------------------------------------------------------------
// Marginalia — independent index. Two-step handshake (token + 2s wait),
// so it runs only when explicitly enabled.
// ---------------------------------------------------------------------------

pub fn marginalia_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let q = urlencode(&p.query);
    let first = format!("https://old-search.marginalia.nu/search?query={q}");
    let page1 = http_get(&first, USER_AGENT, &[("Accept", "text/html")], Duration::from_secs(10))?;
    let sst = page1.split("sst=").nth(1)
        .map(|rest| rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '=' || *c == 'S').collect::<String>())
        .unwrap_or_default();
    if sst.is_empty() {
        return Err("handshake failed".into());
    }
    thread::sleep(Duration::from_millis(2100));
    let second = format!("https://old-search.marginalia.nu/search?query={q}&sst={sst}");
    let body = http_get(&second, USER_AGENT, &[("Accept", "text/html")], Duration::from_secs(10))?;
    if !body.contains("search-result") { return Ok(vec![]); }
    let mut out = Vec::new();
    for block in body.split(r#"class="card search-result""#).skip(1) {
        let url = block.split(r#"class="url""#).nth(1)
            .and_then(|s| attr_after(s, "href=\""))
            .unwrap_or_default();
        let title = block.split(r#"class="title""#).nth(1)
            .and_then(|s| attr_after(s, "href=\"").map(|_| {
                let seg = s.split("</a>").next().unwrap_or(s);
                decode_entities(&strip_tags(seg).trim())
            }))
            .unwrap_or_default();
        if url.is_empty() || title.is_empty() || !url.starts_with("http") { continue; }
        let desc = block.split(r#"class="description""#).nth(1)
            .map(|s| {
                let seg = s.split("</p>").next().unwrap_or(s);
                decode_entities(&strip_tags(seg).trim())
            })
            .unwrap_or_default();
        let mut r = SearchResult::web(title, url, clip(&desc, 300));
        r.meta.push(("engine".into(), "marginalia".into()));
        out.push(r);
        if out.len() >= 20 { break; }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Wikidata — entity search (labels + descriptions).
// ---------------------------------------------------------------------------

pub fn wikidata_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let lang = wiki_lang(&p.language);
    let url = format!(
        "https://www.wikidata.org/w/api.php?action=wbsearchentities&search={}&language={lang}&uselang={lang}&format=json&limit=10",
        urlencode(&p.query));
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(arr) = v.get("search").and_then(|x| x.as_array()) {
        for it in arr {
            let label = it.get("label").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let desc = it.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let id = it.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if label.is_empty() || id.is_empty() { continue; }
            let link = format!("https://www.wikidata.org/wiki/{id}");
            let title = if desc.is_empty() { label.clone() } else { format!("{label} — {desc}") };
            let mut r = SearchResult::web(title, link, clip(&desc, 240));
            r.meta.push(("entity".into(), id));
            r.meta.push(("engine".into(), "wikidata".into()));
            out.push(r);
        }
    }
    Ok(out)
}

fn wiki_lang(language: &str) -> &str {
    match language { "auto" | "" => "en", other => other }
}
