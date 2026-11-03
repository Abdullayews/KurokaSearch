// SPDX-License-Identifier: AGPL-3.0-or-later
//! Extra IT engines: Hacker News (Algolia), npm registry, crates.io.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Hacker News — Algolia's public search API.
// ---------------------------------------------------------------------------

pub fn hn_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1) - 1;
    let url = format!("https://hn.algolia.com/api/v1/search?query={}&page={page}&hitsPerPage=15&tags=(story,comment)",
        urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(hits) = v.get("hits").and_then(|x| x.as_array()) {
        for h in hits {
            let title = h.get("title").and_then(|x| x.as_str())
                .or_else(|| h.get("story_title").and_then(|x| x.as_str()))
                .unwrap_or("").to_string();
            if title.is_empty() { continue; }
            let link = h.get("url").and_then(|x| x.as_str())
                .filter(|u| u.starts_with("http"))
                .map(|u| u.to_string())
                .unwrap_or_else(|| format!("https://news.ycombinator.com/item?id={}",
                    h.get("objectID").and_then(|x| x.as_str()).unwrap_or("")));
            let points = h.get("points").and_then(|x| x.as_i64()).unwrap_or(0);
            let comments = h.get("num_comments").and_then(|x| x.as_i64()).unwrap_or(0);
            let author = h.get("author").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let created = h.get("created_at").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let text = h.get("story_text").and_then(|x| x.as_str())
                .or_else(|| h.get("comment_text").and_then(|x| x.as_str()))
                .unwrap_or("");
            let epoch = parse_iso8601(&created);
            if !time_accepts(&p.time, epoch) { continue; }
            let mut r = SearchResult::web(title, link, clip(&strip_tags(text), 260));
            r.source = "news.ycombinator.com".into();
            r.published = Some(created);
            r.published_epoch = epoch;
            r.meta.push(("points".into(), points.to_string()));
            r.meta.push(("comments".into(), comments.to_string()));
            if !author.is_empty() { r.meta.push(("by".into(), author)); }
            r.meta.push(("engine".into(), "hacker news".into()));
            out.push(r);
            if out.len() >= 15 { break; }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// npm — the registry search endpoint (no key needed).
// ---------------------------------------------------------------------------

pub fn npm_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let from = (p.page.max(1) - 1) * 20;
    let url = format!("https://registry.npmjs.org/-/v1/search?text={}&size=20&from={from}", urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(objs) = v.get("objects").and_then(|x| x.as_array()) {
        for o in objs {
            let pkg = o.get("package").ok_or("bad shape")?;
            let name = pkg.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if name.is_empty() { continue; }
            let desc = pkg.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let version = pkg.get("version").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let link = pkg.get("links").and_then(|l| l.get("npm")).and_then(|x| x.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("https://www.npmjs.com/package/{name}"));
            let publisher = pkg.get("publisher").and_then(|l| l.get("username")).and_then(|x| x.as_str()).unwrap_or("");
            let date = pkg.get("date").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let downloads = o.get("downloads").map(|d| format!("{}/mo", monthly(&d))).unwrap_or_default();
            let mut r = SearchResult::web(name.clone(), link, clip(&desc, 240));
            r.source = "npmjs.com".into();
            if !version.is_empty() { r.meta.push(("version".into(), version)); }
            if !downloads.is_empty() { r.meta.push(("downloads".into(), downloads)); }
            if !publisher.is_empty() { r.meta.push(("by".into(), publisher.into())); }
            if !date.is_empty() { r.published = Some(date.clone()); r.published_epoch = parse_iso8601(&date); }
            r.meta.push(("engine".into(), "npm".into()));
            out.push(r);
            if out.len() >= 20 { break; }
        }
    }
    Ok(out)
}

fn monthly(v: &Value) -> String {
    // {"monthly": 270, "weekly": 51}
    let m = v.get("monthly").and_then(|x| x.as_i64()).unwrap_or(0);
    if m >= 1_000_000 { format!("{:.1}M", m as f64 / 1_000_000.0) }
    else if m >= 1_000 { format!("{:.1}k", m as f64 / 1_000.0) }
    else { m.to_string() }
}

// ---------------------------------------------------------------------------
// MDN Web Docs — the public search API (locale-aware, no key).
// ---------------------------------------------------------------------------

pub fn mdn_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1); // the API rejects page 0
    let locale = match p.language.as_str() { "zh" => "zh-CN", "ja" => "ja", "de" => "de", "fr" => "fr", "es" => "es", "it" => "it", "ru" => "ru", "pt" => "pt", "ko" => "ko", _ => "en-US" };
    let url = format!("https://developer.mozilla.org/api/v1/search?q={}&locale={}&page={}&limit=20&highlight=false",
        urlencode(&p.query), locale, page);
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(docs) = v.get("documents").and_then(|x| x.as_array()) {
        for d in docs {
            let path = d.get("mdn_url").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let title = d.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if path.is_empty() || title.is_empty() { continue; }
            let summary = d.get("summary").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let mut r = SearchResult::web(title, format!("https://developer.mozilla.org{path}"), clip(&summary, 260));
            r.source = "developer.mozilla.org".into();
            r.meta.push(("engine".into(), "mdn".into()));
            out.push(r);
            if out.len() >= 20 { break; }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Docker Hub — public registry search API.
// ---------------------------------------------------------------------------

pub fn docker_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!("https://hub.docker.com/v2/search/repositories/?query={}&page={page}&page_size=20",
        urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(objs) = v.get("results").and_then(|x| x.as_array()) {
        for o in objs {
            let name = o.get("repo_name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if name.is_empty() { continue; }
            let desc = o.get("short_description").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let stars = o.get("star_count").and_then(|x| x.as_i64()).unwrap_or(0);
            let pulls = o.get("pull_count").and_then(|x| x.as_i64()).unwrap_or(0);
            let official = o.get("is_official").and_then(|x| x.as_bool()).unwrap_or(false);
            // official images live at hub.docker.com/_/name, user repos at /r/owner/name
            let link = if official {
                let base = name.split('/').next_back().unwrap_or(&name);
                format!("https://hub.docker.com/_/{base}")
            } else {
                format!("https://hub.docker.com/r/{name}")
            };
            let mut r = SearchResult::web(name.clone(), link, clip(&desc, 240));
            r.source = "hub.docker.com".into();
            r.meta.push(("stars".into(), stars.to_string()));
            if pulls > 0 { r.meta.push(("pulls".into(), human_count(pulls))); }
            if official { r.meta.push(("official".into(), "yes".into())); }
            r.meta.push(("engine".into(), "docker hub".into()));
            out.push(r);
        }
    }
    Ok(out)
}

fn human_count(n: i64) -> String {
    if n >= 1_000_000_000 { format!("{:.1}B", n as f64 / 1_000_000_000.0) }
    else if n >= 1_000_000 { format!("{:.1}M", n as f64 / 1_000_000.0) }
    else if n >= 1_000 { format!("{:.1}k", n as f64 / 1_000.0) }
    else { n.to_string() }
}

// ---------------------------------------------------------------------------
// Hugging Face — public models API.
// ---------------------------------------------------------------------------

pub fn hf_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let skip = (p.page.max(1) - 1) * 20;
    let url = format!("https://huggingface.co/api/models?search={}&limit=20&skip={skip}&sort=downloads&direction=-1&full=false",
        urlencode(&p.query));
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(objs) = v.as_array() {
        for o in objs {
            let id = o.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if id.is_empty() { continue; }
            let downloads = o.get("downloads").and_then(|x| x.as_i64()).unwrap_or(0);
            let likes = o.get("likes").and_then(|x| x.as_i64()).unwrap_or(0);
            let pipeline = o.get("pipeline_tag").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let created = o.get("createdAt").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let tags: Vec<&str> = o.get("tags").and_then(|x| x.as_array()).map(|a|
                a.iter().filter_map(|t| t.as_str()).collect()).unwrap_or_default();
            let license = tags.iter().find(|t| t.starts_with("license:"))
                .map(|t| t.trim_start_matches("license:").to_string()).unwrap_or_default();
            let mut desc = String::new();
            if !pipeline.is_empty() { desc.push_str(&format!("{} model", pipeline)); }
            if !license.is_empty() {
                if !desc.is_empty() { desc.push_str(" · "); }
                desc.push_str(&format!("license {license}"));
            }
            if desc.is_empty() { desc.push_str(&format!("{downloads} downloads")); }
            let mut r = SearchResult::web(id.clone(), format!("https://huggingface.co/{id}"), clip(&desc, 240));
            r.source = "huggingface.co".into();
            r.meta.push(("downloads".into(), human_count(downloads)));
            r.meta.push(("likes".into(), likes.to_string()));
            if !pipeline.is_empty() { r.meta.push(("task".into(), pipeline)); }
            if !created.is_empty() { r.published = Some(created.clone()); r.published_epoch = parse_iso8601(&created); }
            r.meta.push(("engine".into(), "huggingface".into()));
            out.push(r);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// GitLab — public projects API (companion to the GitHub engine).
// ---------------------------------------------------------------------------

pub fn gitlab_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!("https://gitlab.com/api/v4/projects?search={}&order_by=last_activity_at&sort=desc&per_page=15&page={page}",
        urlencode(&p.query));
    let body = http_get(&url, USER_AGENT, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(objs) = v.as_array() {
        for o in objs {
            let path = o.get("path_with_namespace").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let name = o.get("name").and_then(|x| x.as_str())
                .unwrap_or(&path).to_string();
            if path.is_empty() { continue; }
            let desc = o.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let stars = o.get("star_count").and_then(|x| x.as_i64()).unwrap_or(0);
            let updated = o.get("last_activity_at").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let epoch = parse_iso8601(&updated);
            if !time_accepts(&p.time, epoch) { continue; }
            let mut r = SearchResult::web(name, format!("https://gitlab.com/{path}"), clip(&desc, 240));
            r.source = "gitlab.com".into();
            r.meta.push(("stars".into(), stars.to_string()));
            if !updated.is_empty() { r.published = Some(updated.clone()); r.published_epoch = epoch; }
            r.meta.push(("engine".into(), "gitlab".into()));
            out.push(r);
            if out.len() >= 15 { break; }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// crates.io — Rust package index. A descriptive UA is required.
// ---------------------------------------------------------------------------

pub fn crates_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!("https://crates.io/api/v1/crates?q={}&per_page=15&page={page}", urlencode(&p.query));
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(crates) = v.get("crates").and_then(|x| x.as_array()) {
        for c in crates {
            let name = c.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if name.is_empty() { continue; }
            let desc = c.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let version = c.get("max_stable_version").or_else(|| c.get("newest_version"))
                .and_then(|x| x.as_str()).unwrap_or("").to_string();
            let downloads = c.get("downloads").and_then(|x| x.as_i64()).unwrap_or(0);
            let updated = c.get("updated_at").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let mut r = SearchResult::web(name.clone(), format!("https://crates.io/crates/{name}"), clip(&desc, 240));
            r.source = "crates.io".into();
            if !version.is_empty() { r.meta.push(("version".into(), version)); }
            r.meta.push(("downloads".into(), downloads.to_string()));
            if !updated.is_empty() { r.published = Some(updated.clone()); r.published_epoch = parse_iso8601(&updated); }
            r.meta.push(("engine".into(), "crates.io".into()));
            out.push(r);
            if out.len() >= 15 { break; }
        }
    }
    Ok(out)
}
