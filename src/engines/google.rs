// SPDX-License-Identifier: AGPL-3.0-or-later
//! Google Web search (All tab).
//!
//! Google answers datacenter sessions with a "please enable JavaScript"
//! shell, so this engine is built to degrade gracefully and never slow the
//! page down: a detected JS-wall / captcha is remembered in-process for
//! ten minutes and every later search fails in 0 ms instead of re-probing.
//! The moment Google serves real HTML — behind KUROKA_PROXY (residential
//! exit), from another network, or whenever their wall lifts — the parser
//! picks it up again. It understands both result formats Google hands to
//! non-JS clients:
//!   * classic basic HTML:  <a href="/url?q=…"> + <h3>/BNeawe title blocks
//!   * modern server-rendered page: <a href="https://…">…<h3>…</h3></a>
//! Privacy stays intact: Google sees only this server, never the visitor.

use crate::engine::*;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// epoch seconds until which Google is treated as unreachable
fn blocked_until() -> &'static Mutex<u64> {
    static B: OnceLock<Mutex<u64>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(0))
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// how long a detected wall suppresses further probes
const WALL_TTL: u64 = 600;

fn hl_of(lang: &str) -> &'static str {
    match lang {
        "az" => "az", "tr" => "tr", "ru" => "ru", "de" => "de", "fr" => "fr",
        "es" => "es", "ar" => "ar", "ja" => "ja", "zh" => "zh-CN",
        _ => "en",
    }
}

fn tbs_of(time: &str) -> Option<&'static str> {
    match time {
        "d" => Some("d"), "w" => Some("w"), "m" => Some("m"), "y" => Some("y"),
        _ => None,
    }
}

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    // cached wall — fail instantly instead of burning a request
    {
        let b = blocked_until().lock().unwrap_or_else(|e| e.into_inner());
        if *b > now_epoch() {
            return Err("js-walled (cached)".into());
        }
    }

    let start = (p.page.max(1).saturating_sub(1)) * 10;
    let mut url = format!(
        "https://www.google.com/search?q={}&hl={}&num=10&start={start}",
        urlencode(&p.query), hl_of(&p.language));
    if p.safesearch > 0 { url.push_str("&safe=active"); }
    if let Some(t) = tbs_of(&p.time) { url.push_str("&tbs=qdr:"); url.push_str(t); }

    let body = http_get_browser(&url, "https://www.google.com/", &[], Duration::from_secs(9))?;

    // wall detection — remember it so the next search does not re-probe
    if body.contains("/httpservice/retry/enablejs")
        || body.contains("Our systems have detected")
        || body.contains("unusual traffic from your computer network")
        || body.contains("/sorry/index")
    {
        let mut b = blocked_until().lock().unwrap_or_else(|e| e.into_inner());
        *b = now_epoch() + WALL_TTL;
        return Err("js-walled".into());
    }

    let mut out = parse_basic(&body);
    if out.is_empty() { out = parse_modern(&body); }
    for r in out.iter_mut() {
        r.meta.push(("engine".into(), "google".into()));
    }
    Ok(out)
}

/// plain text of the first `class="NEEDLE…"` div after `at`, tags stripped
fn class_text_after(window: &str, needles: &[&str]) -> Option<String> {
    let lower = window.to_ascii_lowercase();
    for needle in needles {
        let mut probe = 0usize;
        while let Some(rel) = lower[probe..].find(&format!("class=\"{needle}")) {
            let open = probe + rel;
            let tag_end = lower[open..].find('>').map(|e| open + e)?;
            let close = lower[tag_end..].find("</div>").map(|e| tag_end + e)?;
            let text = strip_tags(&window[tag_end + 1..close]);
            let text = decode_entities(text.trim());
            if !text.is_empty() { return Some(text); }
            probe = close + 6;
        }
    }
    None
}

/// classic basic-HTML results: <a href="/url?q=REAL_URL&amp;sa=…">
fn parse_basic(body: &str) -> Vec<SearchResult> {
    let mut out: Vec<SearchResult> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut probe = 0usize;
    while let Some(rel) = body[probe..].find("href=\"/url?q=") {
        let start = probe + rel + "href=\"/url?q=".len();
        let end = body[start..].find('"').map(|e| start + e).unwrap_or(body.len());
        let raw = &body[start..end];
        let u = decode_entities(raw.split('&').next().unwrap_or(""));
        probe = end + 1;
        if !u.starts_with("https://") && !u.starts_with("http://") { continue; }
        if u.contains("google.com/") || u.contains("gstatic.com") || u.contains("googleusercontent") { continue; }
        let key = normalize_probe(&u);
        if !seen.insert(key) { continue; }
        // title + description live in the markup that follows the link
        let window = &body[end..(end + 2200).min(body.len())];
        let title = class_text_after(window, &["BNeawe vvjwJb", "ezO2md", "OSrXXb"])
            .or_else(|| h3_after(window));
        let desc = class_text_after(window, &["BNeawe s3b9Wb", "BNeawe tAd8D", "twGe", "T3zJd"]);
        let title = match title { Some(t) => t, None => continue };
        if title.is_empty() { continue; }
        let mut r = SearchResult::web(clip(&title, 130), u, clip(&desc.unwrap_or_default(), 300));
        r.meta.push(("engine".into(), "google".into()));
        out.push(r);
        if out.len() >= 20 { break; }
    }
    out
}

/// modern server-rendered page: <a … href="https://…">…<h3>title</h3>…</a>
fn parse_modern(body: &str) -> Vec<SearchResult> {
    let mut out: Vec<SearchResult> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut probe = 0usize;
    while let Some(rel) = body[probe..].find("<h3") {
        let h3 = probe + rel;
        // walk back to the opening <a> of this heading
        let back = body[h3.saturating_sub(600)..h3].rfind("<a ").map(|i| h3.saturating_sub(600) + i);
        probe = body[h3..].find("</h3>").map(|e| h3 + e + 5).unwrap_or(body.len());
        let Some(a_at) = back else { continue };
        let tag_end = body[a_at..].find('>').map(|e| a_at + e).unwrap_or(body.len());
        let a_tag = &body[a_at..tag_end];
        let Some(u) = attr_of(a_tag, "href") else { continue };
        if !u.starts_with("https://") && !u.starts_with("http://") { continue; }
        if u.contains("google.") || u.contains("gstatic.com") || u.contains("googleusercontent") { continue; }
        let key = normalize_probe(&u);
        if !seen.insert(key) { continue; }
        let title_open = body[h3..].find('>').map(|e| h3 + e + 1).unwrap_or(h3);
        let title_close = body[title_open..].find("</h3>").map(|e| title_open + e).unwrap_or(title_open);
        let title = decode_entities(strip_tags(&body[title_open..title_close]).trim());
        if title.is_empty() { continue; }
        // description: the nearest VwiC3b block after the heading
        let window = &body[title_close..(title_close + 2600).min(body.len())];
        let desc = class_text_after(window, &["VwiC3b", "x54gtf", "STJUJd"])
            .unwrap_or_default();
        let mut r = SearchResult::web(clip(&title, 130), u, clip(&desc, 300));
        r.meta.push(("engine".into(), "google".into()));
        out.push(r);
        if out.len() >= 20 { break; }
    }
    out
}

fn h3_after(window: &str) -> Option<String> {
    let i = window.find("<h3")?;
    let open = window[i..].find('>').map(|e| i + e + 1)?;
    let close = window[open..].find("</h3>")?;
    Some(decode_entities(strip_tags(&window[open..close]).trim()))
}

/// attribute value of a lowercase-checked name inside one tag
fn attr_of(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let pat = format!("{name}=\"");
    let i = lower.find(&pat)?;
    let start = i + pat.len();
    let end = tag[start..].find('"').map(|e| start + e)?;
    Some(decode_entities(&tag[start..end]))
}

fn normalize_probe(u: &str) -> String {
    let s = u.trim().to_lowercase();
    let s = s.split('?').next().unwrap_or(&s);
    let s = s.strip_suffix('/').unwrap_or(s);
    s.to_string()
}
