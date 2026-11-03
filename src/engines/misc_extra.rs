// SPDX-License-Identifier: AGPL-3.0-or-later
//! Extra files / music / social / maps engines:
//! The Pirate Bay (apibay), Mixcloud, Audius, Lemmy, Photon.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

// ---------------------------------------------------------------------------
// The Pirate Bay — the official apibay.org JSON API.
// ---------------------------------------------------------------------------

pub fn apibay_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!("https://apibay.org/q.php?q={}&page={}", urlencode(&p.query), page - 1);
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(12))?;
    if body.trim_start().starts_with('{') {
        // error object like {"err": "No results"}
        return Ok(vec![]);
    }
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(arr) = v.as_array() {
        for it in arr {
            let name = it.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let hash = it.get("info_hash").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if name.is_empty() || hash.len() < 16 { continue; }
            let seeders = it.get("seeders").and_then(|x| x.as_str()).unwrap_or("0").parse::<i64>().unwrap_or(0);
            let leechers = it.get("leechers").and_then(|x| x.as_str()).unwrap_or("0").parse::<i64>().unwrap_or(0);
            let size = it.get("size").and_then(|x| x.as_str()).unwrap_or("0").parse::<i64>().unwrap_or(0);
            let user = it.get("username").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let added = it.get("added").and_then(|x| x.as_str()).unwrap_or("").parse::<i64>().unwrap_or(0);
            let magnet = format!("magnet:?xt=urn:btih:{hash}&dn={}", urlencode(&name));
            let mut r = SearchResult::web(name, magnet, String::new());
            r.source = "thepiratebay.org".into();
            r.meta.push(("size".into(), human_bytes(size)));
            r.meta.push(("seeders".into(), seeders.to_string()));
            r.meta.push(("leechers".into(), leechers.to_string()));
            if !user.is_empty() { r.meta.push(("by".into(), user)); }
            if added > 0 {
                r.published_epoch = Some(added);
                r.published = Some(epoch_text(added));
            }
            r.meta.push(("engine".into(), "pirate bay".into()));
            out.push(r);
            if out.len() >= 20 { break; }
        }
    }
    Ok(out)
}

fn epoch_text(e: i64) -> String {
    civil_from_days(e.div_euclid(86400))
}

fn civil_from_days(z: i64) -> String {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

// ---------------------------------------------------------------------------
// Mixcloud — public search API.
// ---------------------------------------------------------------------------

pub fn mixcloud_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let offset = (p.page.max(1) - 1) * 15;
    let url = format!("https://api.mixcloud.com/search/?q={}&type=cloudcast&limit=15&offset={offset}",
        urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(arr) = v.get("data").and_then(|x| x.as_array()) {
        for it in arr {
            let name = it.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let link = it.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if name.is_empty() || link.is_empty() { continue; }
            let user = it.pointer("/user/name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let secs = it.get("audio_length").and_then(|x| x.as_i64()).unwrap_or(0);
            let created = it.get("created_time").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let thumb = it.pointer("/pictures/medium").and_then(|x| x.as_str()).map(|s| s.to_string())
                .or_else(|| it.pointer("/pictures/thumbnail").and_then(|x| x.as_str()).map(|s| s.to_string()));
            let mut r = SearchResult::new("music", name, link, user.clone());
            r.duration = Some(dur_text(secs));
            r.published = Some(created.clone());
            r.published_epoch = parse_iso8601(&created);
            r.publisher = Some(user.clone());
            r.thumb = thumb;
            if !user.is_empty() { r.meta.push(("by".into(), user)); }
            r.meta.push(("engine".into(), "mixcloud".into()));
            out.push(r);
            if out.len() >= 15 { break; }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Audius — decentralized music. Host discovery, then track search.
// ---------------------------------------------------------------------------

pub fn audius_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let host = audius_host()?;
    let url = format!("{host}/v1/tracks/search?query={}&app_name=KurokaSearch&limit=15",
        urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(12))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(arr) = v.get("data").and_then(|x| x.as_array()) {
        for it in arr {
            let title = it.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let id = it.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if title.is_empty() || id.is_empty() { continue; }
            let permalink = it.get("permalink").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let link = format!("https://audius.co{permalink}");
            let user = it.pointer("/user/name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let genre = it.get("genre").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let ms = it.get("duration").and_then(|x| x.as_i64()).unwrap_or(0);
            let release = it.get("release_date").and_then(|x| x.as_str())
                .or_else(|| it.get("created_at").and_then(|x| x.as_str())).unwrap_or("").to_string();
            let artwork = it.pointer("/artwork/480x480").and_then(|x| x.as_str())
                .or_else(|| it.pointer("/artwork/150x150").and_then(|x| x.as_str())).map(|s| s.to_string());
            let play = it.get("play_count").and_then(|x| x.as_i64()).unwrap_or(0);
            let mut r = SearchResult::new("music", title, link, user.clone());
            r.duration = Some(dur_text(ms / 1000));
            if !release.is_empty() { r.published = Some(release.clone()); }
            r.publisher = Some(user.clone());
            r.thumb = artwork;
            r.preview = Some(format!("{host}/v1/tracks/{id}/stream?app_name=KurokaSearch"));
            if !genre.is_empty() { r.meta.push(("genre".into(), genre)); }
            if play > 0 { r.meta.push(("plays".into(), play.to_string())); }
            if !user.is_empty() { r.meta.push(("by".into(), user)); }
            r.meta.push(("engine".into(), "audius".into()));
            out.push(r);
            if out.len() >= 15 { break; }
        }
    }
    Ok(out)
}

fn audius_host() -> Result<String, String> {
    let body = http_get("https://api.audius.co", USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(8))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("hosts: {e}"))?;
    v.get("data").and_then(|x| x.as_array())
        .and_then(|a| a.first())
        .and_then(|h| h.as_str())
        .map(|s| s.trim_end_matches('/').to_string())
        .ok_or_else(|| "no audius host".into())
}

// ---------------------------------------------------------------------------
// Lemmy — federated link aggregator (lemmy.ml public API).
// ---------------------------------------------------------------------------

pub fn lemmy_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!(
        "https://lemmy.ml/api/v3/search?q={}&type_=Posts&sort=TopAll&limit=15&page={page}",
        urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(12))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(posts) = v.get("posts").and_then(|x| x.as_array()) {
        for pv in posts {
            let post = pv.get("post").cloned().unwrap_or(Value::Null);
            let name = post.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if name.is_empty() { continue; }
            let pid = post.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
            let ext_url = post.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let link = if ext_url.starts_with("http") { ext_url }
                else if pid > 0 { format!("https://lemmy.ml/post/{pid}") } else { continue };
            let body_text = post.get("body").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let published = post.get("published").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let creator = pv.pointer("/creator/name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let community = pv.pointer("/community/name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let score = pv.pointer("/counts/score").and_then(|x| x.as_i64()).unwrap_or(0);
            let comments = pv.pointer("/counts/comments").and_then(|x| x.as_i64()).unwrap_or(0);
            let thumb = post.get("thumbnail_url").and_then(|x| x.as_str()).map(|s| s.to_string());
            let epoch = parse_iso8601(&published);
            if !time_accepts(&p.time, epoch) { continue; }
            let mut r = SearchResult::web(name, link, clip(&strip_tags(&body_text), 260));
            r.source = "lemmy.ml".into();
            r.published = Some(published.clone());
            r.published_epoch = epoch;
            r.thumb = thumb;
            if !community.is_empty() { r.meta.push(("community".into(), community)); }
            if !creator.is_empty() { r.meta.push(("by".into(), creator)); }
            r.meta.push(("score".into(), score.to_string()));
            r.meta.push(("comments".into(), comments.to_string()));
            r.meta.push(("engine".into(), "lemmy".into()));
            out.push(r);
            if out.len() >= 15 { break; }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Photon — Komoot's OSM geocoder (maps category).
// ---------------------------------------------------------------------------

pub fn photon_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let lang = match p.language.as_str() { "auto" | "" => "en", other => other };
    let url = format!("https://photon.komoot.io/api/?q={}&limit=15&lang={}", urlencode(&p.query), lang);
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(feats) = v.get("features").and_then(|x| x.as_array()) {
        for f in feats {
            let props = f.get("properties").cloned().unwrap_or(Value::Null);
            let name = props.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if name.is_empty() { continue; }
            let osm_id = props.get("osm_id").and_then(|x| x.as_i64()).unwrap_or(0);
            let osm_type = props.get("osm_type").and_then(|x| x.as_str()).unwrap_or("");
            let tpath = match osm_type { "R" => "relation", "W" => "way", "N" => "node", _ => "" };
            if osm_id <= 0 || tpath.is_empty() { continue; }
            let link = format!("https://www.openstreetmap.org/{tpath}/{osm_id}");
            let mut place = vec![props.get("city").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                props.get("state").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                props.get("country").and_then(|x| x.as_str()).unwrap_or("").to_string()];
            place.retain(|s| !s.is_empty());
            let osm_key = props.get("osm_key").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let osm_val = props.get("osm_value").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let (lat, lon) = f.pointer("/geometry/coordinates")
                .and_then(|c| c.as_array())
                .map(|c| (c.get(1).and_then(|x| x.as_f64()), c.get(0).and_then(|x| x.as_f64())))
                .unwrap_or((None, None));
            let mut r = SearchResult::new("map", name, link, place.join(", "));
            if !osm_key.is_empty() { r.meta.push(("type".into(), format!("{osm_key}: {osm_val}"))); }
            r.lat = lat;
            r.lon = lon;
            r.meta.push(("engine".into(), "photon".into()));
            out.push(r);
            if out.len() >= 15 { break; }
        }
    }
    Ok(out)
}
