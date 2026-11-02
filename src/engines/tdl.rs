// SPDX-License-Identifier: AGPL-3.0-or-later
//! TorrentDownload — torrent search (torrentdownload.info).
//! Files category. Links go to the torrent detail page (magnet lives there).

use crate::engine::*;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let url = format!("https://www.torrentdownload.info/search?q={}", urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "text/html")], Duration::from_secs(12))?;
    if !body.contains("tt-name") { return Ok(vec![]); }
    let mut out = Vec::new();
    for row in body.split("<div class=\"tt-name\">").skip(1) {
        let Some((href, title)) = first_link_title(row) else { continue };
        if title.is_empty() || href.starts_with("/search") { continue; }
        let link = format!("https://www.torrentdownload.info{href}");
        let tail = row.split("</tr>").next().unwrap_or(row);
        let age = cell_after(tail, "tdnormal", 0);
        let size = cell_after(tail, "tdnormal", 1);
        let seed = cell_after(tail, "tdseed", 0);
        let leech = cell_after(tail, "tdleech", 0);
        let cat = extract_span_class(tail, "smallish")
            .map(|c| c.trim_start_matches("» ").trim().to_string())
            .unwrap_or_default();
        if p.safesearch == 2 && cat.to_lowercase().contains("xxx") { continue; }
        let mut r = SearchResult::web(title, link, format!("{cat} · {size}"));
        r.source = "torrentdownload.info".into();
        if !age.is_empty() { r.meta.push(("age".into(), age)); }
        if !seed.is_empty() { r.meta.push(("seeders".into(), seed)); }
        if !leech.is_empty() { r.meta.push(("leechers".into(), leech)); }
        out.push(r);
        if out.len() >= 12 { break; }
    }
    Ok(out)
}

/// First <a href="…">TITLE</a> of the chunk.
fn first_link_title(row: &str) -> Option<(String, String)> {
    let i = row.find("<a href=\"")?;
    let href_start = i + 9;
    let href_end = row[href_start..].find('"')? + href_start;
    let href = row[href_start..href_end].to_string();
    let gt = row[href_end..].find('>')? + href_end;
    let end = row[gt + 1..].find("</a>").map(|e| gt + 1 + e).unwrap_or(row.len());
    let title = strip_tags(&row[gt + 1..end]).trim().to_string();
    Some((href, title))
}

/// Nth <td class="cls">…</td> text.
fn cell_after(row: &str, cls: &str, want: usize) -> String {
    let pat = format!("class=\"{cls}\"");
    let mut seen = 0;
    let mut rest = row;
    while let Some(i) = rest.find(&pat) {
        let gt = rest[i..].find('>').map(|e| i + e + 1).unwrap_or(i + pat.len());
        let end = rest[gt..].find("</td>").map(|e| gt + e).unwrap_or(rest.len());
        if seen == want {
            return strip_tags(&rest[gt..end]).split_whitespace().collect::<Vec<_>>().join(" ");
        }
        seen += 1;
        rest = &rest[end + 5..];
    }
    String::new()
}

/// Text of <span class="cls">…</span>.
fn extract_span_class(row: &str, cls: &str) -> Option<String> {
    let pat = format!("class=\"{cls}\"");
    let i = row.find(&pat)?;
    let gt = row[i..].find('>')? + i;
    let end = row[gt + 1..].find("</span>").map(|e| gt + 1 + e).unwrap_or(row.len());
    Some(strip_tags(&row[gt + 1..end]))
}
