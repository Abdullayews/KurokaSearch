// SPDX-License-Identifier: AGPL-3.0-or-later
//! Nyaa — anime & media torrents (nyaa.si). Files category.
//! Result links are magnets for one-click hand-off to a torrent client.

use crate::engine::*;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!("https://nyaa.si/?q={}&p={page}", urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "text/html")], Duration::from_secs(12))?;
    if !body.contains("torrent-list") { return Ok(vec![]); }
    let mut out = Vec::new();
    // data rows carry a class (<tr class="default">…); the plain <tr> is the header
    for row in body.split("<tr class=").skip(1) {
        let end = row.find("</tr>").unwrap_or(row.len());
        let row = &row[..end];
        if !row.contains("/view/") { continue; }
        let Some((view_href, title)) = view_link(row) else { continue };
        let Some(magnet) = magnet_href(row) else { continue };
        if title.is_empty() { continue; }
        // cell texts in order; the date cell is located via its data-timestamp attr
        let texts: Vec<String> = row.split("<td").skip(1).map(|c| {
            let body = c.splitn(2, '>').nth(1).unwrap_or("");
            strip_tags(body.split("</td>").next().unwrap_or(body)).trim().to_string()
        }).collect();
        let date_idx = count_before(row, "data-timestamp");
        let mut size = String::new();
        let mut seed = String::new();
        let mut leech = String::new();
        if date_idx != usize::MAX {
            if date_idx >= 1 && date_idx - 1 < texts.len() { size = texts[date_idx - 1].clone(); }
            if date_idx + 1 < texts.len() { seed = texts[date_idx + 1].clone(); }
            if date_idx + 2 < texts.len() { leech = texts[date_idx + 2].clone(); }
        }
        let epoch = extract_attr_after(row, "data-timestamp=\"")
            .and_then(|v| v.parse::<i64>().ok());
        let cat = category_of(row);
        if p.safesearch >= 1 && cat.contains("Porn") { continue; }
        if !time_accepts(&p.time, epoch) { continue; }
        let page_url = format!("https://nyaa.si{view_href}");
        let mut r = SearchResult::web(title, magnet, format!("{cat} · {size}"));
        r.source = "nyaa.si".into();
        if !seed.is_empty() { r.meta.push(("seeders".into(), seed)); }
        if !leech.is_empty() { r.meta.push(("leechers".into(), leech)); }
        r.meta.push(("page".into(), page_url));
        r.published_epoch = epoch;
        out.push(r);
        if out.len() >= 12 { break; }
    }
    Ok(out)
}

/// How many "<td" openers precede the marker (gives the marker's cell index).
fn count_before(row: &str, marker: &str) -> usize {
    let pos = match row.find(marker) { Some(p) => p, None => return usize::MAX };
    row[..pos].matches("<td").count()
}

fn view_link(row: &str) -> Option<(String, String)> {
    let i = row.find("href=\"/view/")?;
    let end = row[i + 6..].find('"')? + i + 6;
    let href = row[i + 6..end].to_string();
    if href.ends_with("#comments") { return None; }
    let gt = row[end..].find('>')? + end;
    let close = row[gt + 1..].find("</a>").map(|e| gt + 1 + e).unwrap_or(row.len());
    let title = row[gt + 1..close].replace("<b>", " ").replace("</b>", " ");
    let title = decode_entities(strip_tags(&title).trim());
    Some((href, title))
}

fn magnet_href(row: &str) -> Option<String> {
    let i = row.find("href=\"magnet:?")?;
    let rest = &row[i + 6..];
    let j = rest.find('"')?;
    Some(rest[..j].replace("&amp;", "&"))
}

/// Category: the /?c=… anchor carries the label in its title attribute.
fn category_of(row: &str) -> String {
    let i = match row.find("href=\"/?c=") {
        Some(i) => i,
        None => return "Torrent".into(),
    };
    if let Some(t) = extract_attr_after(&row[i..], "title=\"") {
        if !t.is_empty() { return t; }
    }
    "Torrent".into()
}

/// Value of the first `name="…"` attribute found at or after `start`.
fn extract_attr_after(seg: &str, pat: &str) -> Option<String> {
    let i = seg.find(pat)?;
    let rest = &seg[i + pat.len()..];
    let j = rest.find('"')?;
    Some(rest[..j].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> SearchParams {
        SearchParams { query: "one piece".into(), category: crate::engine::Category::Files,
            page: 1, engines: vec![], safesearch: 0, language: "auto".into(),
            time: String::new(), size: String::new(), layout: String::new() }
    }

    #[test]
    #[ignore]
    fn live_nyaa() {
        match search(&params()) {
            Ok(rs) => println!("nyaa: {} results, first: {:?}", rs.len(),
                rs.first().map(|r| (r.title.clone(), r.url.chars().take(50).collect::<String>()))),
            Err(e) => println!("nyaa ERR: {e}"),
        }
    }
}
