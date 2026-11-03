// SPDX-License-Identifier: AGPL-3.0-or-later
//! Extra image/video engines: Bing Images, YouTube, PeerTube (SepiaSearch).

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Bing Images — the async (infinite scroll) endpoint. Each tile carries a
// JSON blob in the `m` attribute with the full-size url + source page.
// ---------------------------------------------------------------------------

pub fn bing_images(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let first = (p.page.max(1) - 1) * 35 + 1;
    let mut qft = String::new();
    match p.size.as_str() {
        "small" => qft.push_str("+filterui:imagesize-small"),
        "medium" => qft.push_str("+filterui:imagesize-medium"),
        "large" => qft.push_str("+filterui:imagesize-large"),
        "wallpaper" => qft.push_str("+filterui:imagesize-wallpaper"),
        _ => {}
    }
    match p.layout.as_str() {
        "wide" => qft.push_str("+filterui:aspect-wide"),
        "tall" => qft.push_str("+filterui:aspect-tall"),
        "square" => qft.push_str("+filterui:aspect-square"),
        _ => {}
    }
    let url = format!(
        "https://www.bing.com/images/async?q={}&first={first}&count=35&mmr=1{qft}",
        urlencode(&p.query));
    let body = http_get_browser(&url, "https://www.bing.com/", &[], Duration::from_secs(12))?;
    let mut out = Vec::new();
    for tile in body.split(r#"class="iusc""#).skip(1) {
        let Some(mraw) = attr_m(tile) else { continue };
        let m = decode_entities(&mraw);
        let v: Value = match serde_json::from_str(&m) { Ok(v) => v, Err(_) => continue };
        let murl = v.get("murl").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let purl = v.get("purl").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let title = v.get("t").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if murl.is_empty() || purl.is_empty() { continue; }
        let display_title = if title.is_empty() { host_of(&purl) } else { title };
        let mut r = SearchResult::new("image", display_title, purl.clone(), clip(&purl, 200));
        r.img = Some(murl.clone());
        r.thumb = Some(murl);
        r.meta.push(("engine".into(), "bing images".into()));
        out.push(r);
        if out.len() >= 35 { break; }
    }
    Ok(out)
}

/// the `m="…"` attribute right after an iusc tile
fn attr_m(tile: &str) -> Option<String> {
    let i = tile.find("m=\"")?;
    let rest = &tile[i + 3..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

// ---------------------------------------------------------------------------
// YouTube — scrape ytInitialData from the results page (CONSENT cookie keeps
// the EU interstitial away). videoRenderer blocks carry everything.
// ---------------------------------------------------------------------------

pub fn youtube_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let url = format!("https://www.youtube.com/results?search_query={}", urlencode(&p.query));
    let body = http_get(&url, USER_AGENT,
        &[("Accept", "text/html"), ("Accept-Language", "en-US,en;q=0.9"),
          ("Cookie", "CONSENT=YES+cb.20210328-17-p0.en+FX+419")],
        Duration::from_secs(15))?;
    let data = extract_yt_initial_data(&body).ok_or("no ytInitialData")?;
    let v: Value = serde_json::from_str(&data).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    collect_video_renderers(&v, &mut out);
    let mut results = Vec::new();
    for vr in out.into_iter().take(20) {
        let id = vr.get("videoId").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if id.is_empty() { continue; }
        let title = text_of(&vr, &["title"]);
        if title.is_empty() { continue; }
        let owner = vr.get("ownerText").and_then(|o| o.get("runs"))
            .and_then(|r| r.get(0)).and_then(|r| r.get("text"))
            .and_then(|x| x.as_str()).unwrap_or("").to_string();
        let dur = vr.get("lengthText").and_then(|x| x.get("simpleText"))
            .and_then(|x| x.as_str()).unwrap_or("").to_string();
        let pub_rel = vr.get("publishedTimeText").and_then(|x| x.get("simpleText"))
            .and_then(|x| x.as_str()).unwrap_or("").to_string();
        let views = vr.get("viewCountText").and_then(|x| x.get("simpleText"))
            .and_then(|x| x.as_str()).unwrap_or("").to_string();
        let thumb = vr.get("thumbnail").and_then(|t| t.get("thumbnails"))
            .and_then(|t| t.as_array())
            .and_then(|t| t.last())
            .and_then(|t| t.get("url")).and_then(|x| x.as_str())
            .map(|s| s.to_string());
        let desc = vr.get("detailedMetadataSnippets").and_then(|d| d.get(0))
            .and_then(|d| d.get("snippetText")).and_then(|d| d.get("runs"))
            .and_then(|d| d.get(0)).and_then(|d| d.get("text"))
            .and_then(|x| x.as_str()).unwrap_or("").to_string();
        let epoch = parse_relative(&pub_rel);
        if !time_accepts(&p.time, epoch) { continue; }
        let mut r = SearchResult::new("video", title, format!("https://www.youtube.com/watch?v={id}"), clip(&desc, 240));
        r.duration = Some(dur);
        r.published = Some(pub_rel);
        r.published_epoch = epoch;
        r.publisher = Some(owner.clone());
        r.thumb = thumb;
        if !views.is_empty() { r.meta.push(("views".into(), views)); }
        if !owner.is_empty() { r.meta.push(("by".into(), owner)); }
        results.push(r);
    }
    Ok(results)
}

fn extract_yt_initial_data(body: &str) -> Option<String> {
    for marker in ["var ytInitialData = ", "ytInitialData = "] {
        if let Some(i) = body.find(marker) {
            let rest = body[i + marker.len()..].trim_start();
            if !rest.starts_with('{') { continue; }
            // balanced-brace scan (strings can contain braces)
            let bytes = rest.as_bytes();
            let mut depth = 0i32;
            let mut in_str = false;
            let mut esc = false;
            for (n, &b) in bytes.iter().enumerate() {
                if in_str {
                    if esc { esc = false; }
                    else if b == b'\\' { esc = true; }
                    else if b == b'"' { in_str = false; }
                    continue;
                }
                match b {
                    b'"' => in_str = true,
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 { return Some(rest[..=n].to_string()); }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// recursively collect every object that has a "videoRenderer" key
fn collect_video_renderers(v: &Value, out: &mut Vec<Value>) {
    match v {
        Value::Object(map) => {
            if let Some(vr) = map.get("videoRenderer") {
                out.push(vr.clone());
            }
            for x in map.values() { collect_video_renderers(x, out); }
        }
        Value::Array(arr) => {
            for x in arr { collect_video_renderers(x, out); }
        }
        _ => {}
    }
}

fn text_of(v: &Value, path: &[&str]) -> String {
    let mut cur = v;
    for key in path {
        cur = match cur.get(key) { Some(x) => x, None => return String::new() };
    }
    if let Some(runs) = cur.get("runs").and_then(|x| x.as_array()) {
        return runs.iter().filter_map(|r| r.get("text").and_then(|t| t.as_str())).collect::<String>();
    }
    cur.get("simpleText").and_then(|x| x.as_str()).unwrap_or("").to_string()
}

// ---------------------------------------------------------------------------
// PeerTube — SepiaSearch is the official index of public instances.
// ---------------------------------------------------------------------------

pub fn sepiasearch(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let start = (p.page.max(1) - 1) * 12;
    let nsfw = if p.safesearch >= 1 { "false" } else { "both" };
    let url = format!(
        "https://sepiasearch.org/api/v1/search/videos?search={}&count=12&start={start}&sort=-match&nsfw={nsfw}",
        urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(arr) = v.get("data").and_then(|x| x.as_array()) {
        for it in arr {
            let name = it.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let uuid = it.get("uuid").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if name.is_empty() || uuid.is_empty() { continue; }
            let link = format!("https://sepiasearch.org/videos/watch/{uuid}");
            let channel = it.get("channel").and_then(|c| c.get("displayName")).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let dur = it.get("duration").and_then(|x| x.as_i64()).unwrap_or(0);
            let published = it.get("publishedAt").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let epoch = parse_iso8601(&published);
            if !time_accepts(&p.time, epoch) { continue; }
            let thumb = it.get("thumbnailPath").and_then(|x| x.as_str())
                .map(|path| format!("https://sepiasearch.org{path}"));
            let desc = it.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let mut r = SearchResult::new("video", name, link, clip(&strip_tags(&desc), 240));
            r.duration = Some(dur_text(dur));
            r.published = Some(published);
            r.published_epoch = epoch;
            r.publisher = Some(channel.clone());
            r.thumb = thumb;
            if !channel.is_empty() { r.meta.push(("by".into(), channel)); }
            out.push(r);
            if out.len() >= 12 { break; }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Odysee — the Lighthouse search index (LBRY blockchain, decentralized).
// claimType=stream keeps channels out; `include` adds titles + thumbs.
// ---------------------------------------------------------------------------

pub fn odysee_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let from = (p.page.max(1) - 1) * 20;
    let nsfw = if p.safesearch >= 1 { "false" } else { "true" };
    let url = format!(
        "https://lighthouse.odysee.com/search?s={}&size=20&from={from}&claimType=stream&nsfw={nsfw}&include=channel,title,thumbnail_url",
        urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let arr = v.as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for it in arr {
        let name = it.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let claim = it.get("claimId").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if name.is_empty() || claim.is_empty() || name.starts_with('@') { continue; }
        let title = it.get("title").and_then(|x| x.as_str())
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .unwrap_or_else(|| name.replace(['-', '_'], " "));
        let channel = it.get("channel").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let channel_id = it.get("channel_claim_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let link = if !channel.is_empty() && !channel_id.is_empty() {
            format!("https://odysee.com/{channel}:{channel_id}/{name}:{claim}")
        } else {
            format!("https://odysee.com/{name}:{claim}")
        };
        let thumb = it.get("thumbnail_url").and_then(|x| x.as_str())
            .filter(|u| u.starts_with("http")).map(|u| u.to_string());
        let mut r = SearchResult::new("video", clip(&title, 110), link, String::new());
        r.publisher = if channel.is_empty() { Some("odysee.com".into()) } else { Some(channel.clone()) };
        r.thumb = thumb;
        if !channel.is_empty() { r.meta.push(("by".into(), channel)); }
        r.meta.push(("engine".into(), "odysee".into()));
        out.push(r);
        if out.len() >= 20 { break; }
    }
    Ok(out)
}
