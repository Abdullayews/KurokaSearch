// SPDX-License-Identifier: AGPL-3.0-or-later
//! Nominatim (OpenStreetMap) — place search. "Maps" engine.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

pub fn search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    if p.page > 1 { return Ok(vec![]); }
    let url = format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=jsonv2&limit=14&addressdetails=1",
        urlencode(&p.query)
    );
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(12))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(list) = v.as_array() {
        for it in list {
            let name = it.get("display_name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let lat = it.get("lat").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok());
            let lon = it.get("lon").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok());
            let (Some(la), Some(lo)) = (lat, lon) else { continue };
            let kind = it.get("type").and_then(|x| x.as_str()).unwrap_or("place").to_string();
            let link = format!("https://www.openstreetmap.org/?mlat={la}&mlon={lo}#map=15/{la}/{lo}");
            // split display name: first part is the title, rest is the address
            let (title, addr) = match name.split_once(',') {
                Some((t, a)) => (t.trim().to_string(), a.trim().to_string()),
                None => (name.clone(), String::new()),
            };
            let mut r = SearchResult::new("map", clip(&title, 90), link, clip(&addr, 200));
            r.lat = Some(la);
            r.lon = Some(lo);
            r.meta.push(("type".into(), kind.replace('_', " ")));
            r.meta.push(("engine".into(), "osm".into()));
            out.push(r);
        }
    }
    Ok(out)
}
