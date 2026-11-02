// SPDX-License-Identifier: AGPL-3.0-or-later
//! Knaben — torrent meta-search (indexes many public trackers).
//! Files category. Magnet links open the user's torrent client.

use crate::engine::*;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let q = urlencode(&p.query.replace('/', " "));
    let url = format!("https://knaben.org/search/{q}/0/{}/0", p.page.max(1));
    let body = http_get(&url, USER_AGENT, &[("Accept", "text/html")], Duration::from_secs(12))?;
    if body.contains("Area 51") { return Err("blocked (area 51)".into()); }
    let mut out = Vec::new();
    for row in body.split("<tr class=\"text-nowrap").skip(1) {
        let magnet = match find_magnet(row) { Some(m) => m, None => continue };
        let title = match attr_anchor_title(row) { Some(t) => t, None => continue };
        if title.is_empty() { continue; }
        let cells: Vec<String> = split_cells(row);
        // thead order: category, title, size, date, seeders, peers, source
        let mut size = String::new();
        let mut date = String::new();
        let mut seed = String::new();
        let mut leech = String::new();
        let mut cat = String::new();
        if let Some(c) = cells.first() { cat = strip_tags(c).trim().to_string(); }
        if cells.len() >= 3 { size = strip_tags(&cells[1]).trim().to_string(); }
        if cells.len() >= 4 { date = strip_tags(&cells[2]).trim().to_string(); }
        if cells.len() >= 5 { seed = strip_tags(&cells[3]).trim().to_string(); }
        if cells.len() >= 6 { leech = strip_tags(&cells[4]).trim().to_string(); }
        if p.safesearch == 2 && cat.to_lowercase().contains("xxx") { continue; }
        let epoch = parse_yyyy_mm_dd(&date);
        if !time_accepts(&p.time, epoch) { continue; }
        let mut r = SearchResult::web(title, magnet, format!("{cat} · {size}"));
        r.source = "knaben.org".into();
        if !seed.is_empty() { r.meta.push(("seeders".into(), seed.clone())); }
        if !leech.is_empty() { r.meta.push(("leechers".into(), leech)); }
        if !seed.is_empty() && seed.chars().all(|c| c.is_ascii_digit()) {
            r.meta.push(("popularity".into(), seeds_popularity(&seed)));
        }
        r.published_epoch = epoch;
        if !date.is_empty() { r.published = Some(date); }
        out.push(r);
        if out.len() >= 12 { break; }
    }
    Ok(out)
}

fn seeds_popularity(seed: &str) -> String {
    let n: u64 = seed.parse().unwrap_or(0);
    if n >= 1000 { "★★★".into() } else if n >= 100 { "★★".into() } else if n > 0 { "★".into() } else { "–".into() }
}

/// First magnet:? link href in a row chunk.
fn find_magnet(row: &str) -> Option<String> {
    let i = row.find("href=\"magnet:?")?;
    let rest = &row[i + 6..];
    let j = rest.find('"')?;
    Some(rest[..j].replace("&amp;", "&"))
}

/// Title of the magnet anchor — Knaben puts the name in the title attribute
/// (before the href) and in the anchor text (after the tag closes).
fn attr_anchor_title(row: &str) -> Option<String> {
    let i = row.find("href=\"magnet:?")?;
    // anchor text: the tag's real '>' sits past the href value
    let href_end = row[i + 6..].find('"')? + i + 6;
    let gt = row[href_end..].find('>')? + href_end;
    let close = row[gt + 1..].find("</a>").map(|e| gt + 1 + e).unwrap_or(row.len());
    let text = strip_tags(&row[gt + 1..close]);
    // title attribute lives between the anchor start and the href
    let head = &row[..i];
    let a = head.rfind("<a ")?;
    let title = extract_attr(&head[a..], "title").unwrap_or_default();
    let t = if !title.is_empty() { title } else { text };
    Some(decode_entities(t.trim()))
}

/// Text content of every <td> in the chunk (rough split, nested tags collapse).
fn split_cells(row: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut rest = row;
    // cut off the tail after </tr>
    if let Some(e) = rest.find("</tr>") { rest = &rest[..e]; }
    while let Some(i) = rest.find("<td") {
        let after = &rest[i..];
        let open_end = after.find('>').map(|e| i + e + 1).unwrap_or(i + 4);
        let close = rest[open_end..].find("</td>").map(|e| open_end + e).unwrap_or(rest.len());
        let inner = &rest[open_end..close];
        // only direct content cells, skip the category cell (it holds /browse/ links)
        let is_cat = inner.contains("/browse/");
        if !is_cat {
            cells.push(inner.to_string());
        }
        rest = &rest[close + 5..];
    }
    cells
}

fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let pat = format!("{name}=\"");
    let i = tag.find(&pat)?;
    let rest = &tag[i + pat.len()..];
    let j = rest.find('"')?;
    Some(rest[..j].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> SearchParams {
        SearchParams { query: "ubuntu".into(), category: crate::engine::Category::Files,
            page: 1, engines: vec![], safesearch: 0, language: "auto".into(),
            time: String::new(), size: String::new(), layout: String::new() }
    }

    #[test]
    #[ignore]
    fn live_knaben_debug() {
        let body = std::fs::read_to_string("/tmp/knaben_reqwest.html").unwrap();
        for (n, row) in body.split("<tr class=\"text-nowrap").skip(1).enumerate() {
            let m = find_magnet(row);
            let t = attr_anchor_title(row);
            println!("row {n}: magnet={} title={:?}", m.is_some(), t.map(|x| x.chars().take(40).collect::<String>()));
            if n >= 3 { break; }
        }
    }

    #[test]
    #[ignore]
    fn live_knaben() {
        let url = format!("https://knaben.org/search/{}/0/1/0", urlencode(&"ubuntu".replace('/', " ")));
        let body = http_get(&url, USER_AGENT, &[("Accept", "text/html")], Duration::from_secs(12)).unwrap();
        std::fs::write("/tmp/knaben_reqwest.html", &body).unwrap();
        println!("saved {} bytes; rows marker: {}", body.len(), body.matches("<tr class=\"text-nowrap").count());
        match search(&params()) {
            Ok(rs) => println!("knaben: {} results, first: {:?}", rs.len(),
                rs.first().map(|r| (r.title.clone(), r.url.chars().take(60).collect::<String>()))),
            Err(e) => println!("knaben ERR: {e}"),
        }
    }
}

/// "2024-10-13 …" → epoch seconds (UTC, day precision).
pub fn parse_yyyy_mm_dd(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' { return None; }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let m: i64 = s.get(5..7)?.parse().ok()?;
    let d: i64 = s.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) { return None; }
    fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }
    Some(days_from_civil(y, m, d) * 86400)
}
