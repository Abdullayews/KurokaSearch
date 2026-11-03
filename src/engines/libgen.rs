// SPDX-License-Identifier: AGPL-3.0-or-later
//! Library Genesis — book & article shadow library (libgen.vg mirror).
//! Files category.

use crate::engine::*;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let url = format!("https://libgen.vg/index.php?req={}", urlencode(&p.query));
    let body = http_get_browser(&url, "https://libgen.vg/", &[], Duration::from_secs(12))?;
    if !body.contains("tablelibgen") { return Ok(vec![]); }
    let mut out = Vec::new();
    for row in body.split("<tr>").skip(1) {
        let end = row.find("</tr>").unwrap_or(row.len());
        let row = &row[..end];
        let Some((href, title)) = edition_link(row) else { continue };
        if title.is_empty() { continue; }
        let link = format!("https://libgen.vg{href}");
        let cells: Vec<String> = row.split("<td").skip(1).map(|c| {
            let body = c.splitn(2, '>').nth(1).unwrap_or("");
            body.split("</td>").next().unwrap_or(body).to_string()
        }).collect();
        // cells: [0] series+edition+isbn, [1] author, [2] publisher, [3] year,
        //         [4] language, [5] pages, [6] size link, [7] ext, [8] mirrors
        let author = cells.get(1).map(|c| strip_tags(c).trim().to_string()).unwrap_or_default();
        let year = cells.get(3).map(|c| strip_tags(c).trim().to_string()).unwrap_or_default();
        let lang = cells.get(4).map(|c| strip_tags(c).trim().to_string()).unwrap_or_default();
        let size = cells.get(6).map(|c| strip_tags(c).trim().to_string()).unwrap_or_default();
        let ext = cells.get(7).map(|c| strip_tags(c).trim().to_string()).unwrap_or_default();
        let series = cells.first()
            .map(|c| c.split("</b>").next().unwrap_or(""))
            .map(|b| strip_tags(&b.replace("<b>", "").replace("</b>", "")).trim().to_string())
            .unwrap_or_default();
        let full_title = if series.is_empty() || title.to_lowercase().contains(&series.to_lowercase()) {
            title.clone()
        } else {
            format!("{series} — {title}")
        };
        let desc = [author, year, lang.to_uppercase(), format!("{ext}, {size}")]
            .iter().filter(|s| !s.is_empty()).cloned()
            .collect::<Vec<_>>().join(" · ");
        let mut r = SearchResult::web(full_title, link, desc);
        r.source = "libgen.vg".into();
        if !ext.is_empty() { r.meta.push(("format".into(), ext.to_uppercase())); }
        if !size.is_empty() { r.meta.push(("size".into(), size)); }
        if !lang.is_empty() { r.meta.push(("language".into(), lang)); }
        out.push(r);
        if out.len() >= 15 { break; }
    }
    Ok(out)
}

/// edition.php?id=… anchor and its text. The anchors carry tooltip title
/// attributes containing '>' characters, so the tag end is found past the
/// href value instead of by naive '>' scanning.
fn edition_link(row: &str) -> Option<(String, String)> {
    let i = row.find("href=\"edition.php")?;
    let href_start = i + 6;
    let href_end = row[href_start..].find('"')? + href_start;
    let href = row[href_start..href_end].to_string();
    let gt = row[href_end..].find('>')? + href_end;
    let close = row[gt + 1..].find("</a>").map(|e| gt + 1 + e).unwrap_or(row.len());
    let text = strip_tags(&row[gt + 1..close]);
    let text = decode_entities(text.trim());
    if text.is_empty() || text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((href, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> SearchParams {
        SearchParams { query: "python".into(), category: crate::engine::Category::Files,
            page: 1, engines: vec![], safesearch: 0, language: "auto".into(),
            time: String::new(), size: String::new(), layout: String::new() }
    }

    #[test]
    #[ignore]
    fn live_libgen() {
        match search(&params()) {
            Ok(rs) => println!("libgen: {} results, first: {:?}", rs.len(),
                rs.first().map(|r| (r.title.clone(), r.desc.clone()))),
            Err(e) => println!("libgen ERR: {e}"),
        }
    }
}
