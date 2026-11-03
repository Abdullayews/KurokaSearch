// SPDX-License-Identifier: AGPL-3.0-or-later
//! Extra science engines: PubMed, Europe PMC, Zenodo, DOAJ, ERIC.

use crate::engine::*;
use serde_json::Value;
use std::time::Duration;

// ---------------------------------------------------------------------------
// PubMed — two-step E-utilities flow (esearch → esummary), no key.
// ---------------------------------------------------------------------------

pub fn pubmed_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let retstart = (p.page.max(1) - 1) * 15;
    let esearch = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pubmed&term={}&retmode=json&retmax=15&retstart={retstart}",
        urlencode(&p.query));
    let body = http_get(&esearch, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let ids: Vec<String> = v.pointer("/esearchresult/idlist")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    if ids.is_empty() { return Ok(vec![]); }
    let esum = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id={}&retmode=json",
        ids.join(","));
    let body2 = http_get(&esum, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(10))?;
    let v2: Value = serde_json::from_str(&body2).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    for id in &ids {
        let art = match v2.pointer(&format!("/result/{id}")) { Some(a) => a, None => continue };
        let title = art.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if title.is_empty() { continue; }
        let journal = art.get("source").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let pubdate = art.get("pubdate").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let authors = art.get("authors").and_then(|x| x.as_array()).map(|a| {
            a.iter().take(3).filter_map(|x| x.get("name").and_then(|n| n.as_str())).collect::<Vec<_>>().join(", ")
        }).unwrap_or_default();
        let doi = art.get("elocationid").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let mut r = SearchResult::web(title, format!("https://pubmed.ncbi.nlm.nih.gov/{id}/"), clip(&authors, 200));
        r.source = "pubmed.ncbi.nlm.nih.gov".into();
        if !journal.is_empty() { r.meta.push(("journal".into(), journal)); }
        if !doi.is_empty() { r.meta.push(("doi".into(), doi)); }
        if !pubdate.is_empty() { r.published = Some(pubdate.clone()); }
        r.meta.push(("engine".into(), "pubmed".into()));
        out.push(r);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Europe PMC — open life-science literature.
// ---------------------------------------------------------------------------

pub fn europepmc_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!(
        "https://www.ebi.ac.uk/europepmc/webservices/rest/search?query={}&format=json&pageSize=15&page={page}",
        urlencode(&p.query));
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(12))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(arr) = v.pointer("/resultList/result").and_then(|x| x.as_array()) {
        for it in arr {
            let title = it.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if title.is_empty() { continue; }
            let doi = it.get("doi").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let id = it.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let src = it.get("source").and_then(|x| x.as_str()).unwrap_or("MED").to_string();
            let link = if !doi.is_empty() {
                format!("https://doi.org/{doi}")
            } else if !id.is_empty() {
                format!("https://europepmc.org/article/{src}/{id}")
            } else { continue };
            let authors = it.get("authorString").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let journal = it.get("journalTitle").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let year = it.get("pubYear").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let mut r = SearchResult::web(title, link, clip(&authors, 200));
            r.source = "europepmc.org".into();
            if !journal.is_empty() { r.meta.push(("journal".into(), journal)); }
            if !doi.is_empty() { r.meta.push(("doi".into(), doi)); }
            if !year.is_empty() { r.published = Some(year.clone()); }
            r.meta.push(("engine".into(), "europepmc".into()));
            out.push(r);
            if out.len() >= 15 { break; }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Zenodo — open research repository (CERN).
// ---------------------------------------------------------------------------

pub fn zenodo_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!("https://zenodo.org/api/records?q={}&size=15&page={page}", urlencode(&p.query));
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(12))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(arr) = v.pointer("/hits/hits").and_then(|x| x.as_array()) {
        for it in arr {
            let meta = it.get("metadata").cloned().unwrap_or(Value::Null);
            let title = meta.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if title.is_empty() { continue; }
            let id = it.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
            let doi = meta.get("doi").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let link = if !doi.is_empty() { format!("https://doi.org/{doi}") }
                else if id > 0 { format!("https://zenodo.org/records/{id}") } else { continue };
            let desc = meta.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let rtype = meta.get("resource_type").and_then(|x| x.get("type")).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let created = it.get("created").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let mut r = SearchResult::web(title, link, clip(&strip_tags(&desc), 280));
            r.source = "zenodo.org".into();
            if !rtype.is_empty() { r.meta.push(("type".into(), rtype)); }
            if !doi.is_empty() { r.meta.push(("doi".into(), doi)); }
            if !created.is_empty() { r.published = Some(created.clone()); r.published_epoch = parse_iso8601(&created); }
            r.meta.push(("engine".into(), "zenodo".into()));
            out.push(r);
            if out.len() >= 15 { break; }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// DOAJ — Directory of Open Access Journals.
// ---------------------------------------------------------------------------

pub fn doaj_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let page = p.page.max(1);
    let url = format!("https://doaj.org/api/v2/search/articles/{}?pageSize=15&page={page}", urlencode(&p.query));
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(12))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(arr) = v.get("results").and_then(|x| x.as_array()) {
        for it in arr {
            let bib = it.get("bibjson").cloned().unwrap_or(Value::Null);
            let title = bib.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if title.is_empty() { continue; }
            let link = bib.get("link").and_then(|l| l.as_array()).and_then(|l| l.first())
                .and_then(|l| l.get("url")).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let doi = bib.get("identifier").and_then(|ids| ids.as_array())
                .and_then(|ids| ids.iter().find(|i| i.get("type").and_then(|t| t.as_str()) == Some("doi")))
                .and_then(|i| i.get("id")).and_then(|x| x.as_str()).unwrap_or("").to_string();
            if link.is_empty() && doi.is_empty() { continue; }
            let url = if !link.is_empty() { link } else { format!("https://doi.org/{doi}") };
            let abstract_ = bib.get("abstract").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let journal = bib.get("journal").and_then(|j| j.get("title")).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let year = bib.get("year").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let mut r = SearchResult::web(title, url, clip(&strip_tags(&abstract_), 280));
            r.source = "doaj.org".into();
            if !journal.is_empty() { r.meta.push(("journal".into(), journal)); }
            if !doi.is_empty() { r.meta.push(("doi".into(), doi)); }
            if !year.is_empty() { r.published = Some(year.clone()); }
            r.meta.push(("engine".into(), "doaj".into()));
            out.push(r);
            if out.len() >= 15 { break; }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// ERIC — education research (US Dept. of Education).
// ---------------------------------------------------------------------------

pub fn eric_search(p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    let start = (p.page.max(1) - 1) * 15;
    let url = format!(
        "https://api.ies.ed.gov/eric/?search={}&format=json&fields=title,author,description,url,datepublished,id&max=15&start={start}",
        urlencode(&p.query));
    let body = http_get(&url, CONTACT_UA, &[("Accept", "application/json")], Duration::from_secs(12))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
    let mut out = Vec::new();
    if let Some(docs) = v.pointer("/response/docs").and_then(|x| x.as_array()) {
        for d in docs {
            let title = json_str(d.get("title"));
            if title.is_empty() { continue; }
            let id = json_str(d.get("id"));
            let link = json_str(d.get("url"));
            let url = if link.starts_with("http") { link }
                else if !id.is_empty() { format!("https://eric.ed.gov/?id={id}") } else { continue };
            let desc = json_str(d.get("description"));
            let authors = d.get("author").and_then(|x| x.as_array()).map(|a| {
                a.iter().take(3).filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")
            }).unwrap_or_default();
            let date = d.get("datepublished").and_then(|x| x.as_array())
                .and_then(|a| a.first()).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let mut r = SearchResult::web(title, url, clip(&desc, 280));
            r.source = "eric.ed.gov".into();
            if !authors.is_empty() { r.meta.push(("authors".into(), authors)); }
            if !date.is_empty() { r.published = Some(date.clone()); }
            r.meta.push(("engine".into(), "eric".into()));
            out.push(r);
            if out.len() >= 15 { break; }
        }
    }
    Ok(out)
}

/// field can be a string or a one-element array
fn json_str(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a.first().and_then(|x| x.as_str()).unwrap_or("").to_string(),
        _ => String::new(),
    }
}
