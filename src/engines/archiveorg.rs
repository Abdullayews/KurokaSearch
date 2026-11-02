// SPDX-License-Identifier: AGPL-3.0-or-later
//! Internet Archive — movies search. "Videos" engine.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let rows = 16u32;
    let start = (page - 1) * rows;
    let url = format!(
        "https://archive.org/advancedsearch.php?q={}+AND+mediatype%3Amovies&fl%5B%5D=identifier&fl%5B%5D=title&fl%5B%5D=description&fl%5B%5D=downloads&rows={rows}&start={start}&output=json",
        urlencode(&p.query)
    );
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(12))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    let docs = v.pointer("/response/docs").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    for d in docs {
        let id = d.get("identifier").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let title = d.get("title").and_then(|x| x.as_str())
            .unwrap_or(&id).to_string();
        if id.is_empty() { continue; }
        let link = format!("https://archive.org/details/{id}");
        let thumb_raw = format!("https://archive.org/services/img/{id}");
        let desc = d.get("description").and_then(|x| x.as_str()).map(|s| clip(&strip_tags(s), 260))
            .unwrap_or_default();
        let downloads = d.get("downloads").and_then(|x| x.as_u64());
        let mut r = SearchResult::new("video", clip(&title, 110), link, desc);
        r.thumb = Some(thumb_raw);
        r.publisher = Some("archive.org".into());
        if let Some(dl) = downloads {
            if dl > 0 { r.meta.push(("downloads".into(), dl.to_string())); }
        }
        r.meta.push(("engine".into(), "archive.org".into()));
        out.push(r);
    }
    Ok(out)
}
