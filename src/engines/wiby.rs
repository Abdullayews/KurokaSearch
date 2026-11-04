// SPDX-License-Identifier: AGPL-3.0-or-later
//! Wiby — an independent index of the classic, lightweight web. Plain HTML,
//! no JS, no tracking: one <blockquote> per result.

use crate::engine::*;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!("https://wiby.me/?q={}&pg={page}", urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "text/html")], Duration::from_secs(10))?;
    if !body.contains("tlink") { return Ok(vec![]); }
    let mut out = Vec::new();
    for block in body.split("<blockquote").skip(1) {
        // <a class="tlink" href="URL">TITLE</a>
        let href = block.split("href=\"").nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap_or("")
            .to_string();
        let title = block.split("</a>").next()
            .map(|seg| decode_entities(&strip_tags(seg)))
            .unwrap_or_default();
        if !href.starts_with("http") || title.is_empty() { continue; }
        // …<br><p class="url">URL</p><p>DESCRIPTION</p>…
        let desc = block.split(r#"<p class="url">"#).nth(1)
            .and_then(|s| s.split("</p>").nth(1))
            .map(|s| {
                let seg = s.split("</p>").next().unwrap_or(s);
                decode_entities(&strip_tags(seg))
            })
            .unwrap_or_default();
        let mut r = SearchResult::web(title, href, clip(&desc, 300));
        r.meta.push(("engine".into(), "wiby".into()));
        out.push(r);
        if out.len() >= 20 { break; }
    }
    Ok(out)
}
