// SPDX-License-Identifier: AGPL-3.0-or-later
//! Wikimedia Commons — free media. "Images" engine.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let limit = 20u32;
    let offset = (p.page.saturating_sub(1)) * limit;
    let url = format!(
        "https://commons.wikimedia.org/w/api.php?action=query&generator=search\
&gsrsearch={}&gsrnamespace=6&gsrlimit={limit}&gsroffset={offset}\
&prop=imageinfo&iiprop=url%7Csize%7Cextmetadata&iiurlwidth=640&format=json",
        urlencode(&p.query)
    );
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(12))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    let pages = v.pointer("/query/pages").and_then(|x| x.as_object()).cloned().unwrap_or_default();
    let mut list: Vec<&Value> = pages.values().collect();
    list.sort_by_key(|pg| pg.get("index").and_then(|x| x.as_u64()).unwrap_or(999));
    for pg in list {
        let title = pg.get("title").and_then(|x| x.as_str()).unwrap_or("").trim_start_matches("File:").to_string();
        let ii = match pg.get("imageinfo").and_then(|x| x.as_array()).and_then(|a| a.first()) {
            Some(x) => x,
            None => continue,
        };
        let page_url = ii.get("descriptionurl").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let full = ii.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let thumb = match ii.get("thumburl").and_then(|x| x.as_str()) {
            Some(t) => t.split('?').next().unwrap_or(t).to_string(),
            None => match synth_thumb(&full) {
                Some(t) => t,
                None => continue,
            },
        };
        // drop API-appended utm markers — the CDN serves an HTML page for them
        let full = full.split('?').next().unwrap_or(&full).to_string();
        let thumb = thumb.split('?').next().unwrap_or(&thumb).to_string();
        if page_url.is_empty() || full.is_empty() { continue; }
        let w = ii.get("width").and_then(|x| x.as_u64()).map(|x| x as u32);
        let h = ii.get("height").and_then(|x| x.as_u64()).map(|x| x as u32);
        if !matches_size(w, h, &p.size) || !matches_layout(w, h, &p.layout) { continue; }
        let mut r = SearchResult::new("image", clip(&title, 90), page_url, String::new());
        r.img = Some(full);
        r.thumb = Some(thumb);
        r.width = w;
        r.height = h;
        let license = v_license(ii);
        if let Some(l) = license { r.meta.push(("license".into(), l)); }
        r.meta.push(("engine".into(), "commons".into()));
        out.push(r);
    }
    Ok(out)
}

fn v_license(ii: &Value) -> Option<String> {
    let s = ii.pointer("/extmetadata/LicenseShortName/value")?.as_str()?.to_string();
    Some(clip(&s, 24))
}

/// upload.wikimedia.org/wikipedia/commons/a/ab/File.jpg
///   -> thumb.wikimedia.org/wikipedia/commons/thumb/a/ab/File.jpg/640px-File.jpg
fn synth_thumb(full: &str) -> Option<String> {
    let rest = full.strip_prefix("https://upload.wikimedia.org/wikipedia/")?;
    let mut parts = rest.split('/');
    let proj = parts.next()?;
    let p1 = parts.next()?;
    let p2 = parts.next()?;
    let name = parts.next()?;
    if parts.next().is_some() || proj.is_empty() || p1.len() != 1 || p2.len() != 2 || name.is_empty() {
        return None;
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if !matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp") { return None; }
    Some(format!("https://thumb.wikimedia.org/wikipedia/{proj}/thumb/{p1}/{p2}/{}/640px-{name}",
        urlencode(name)))
}
