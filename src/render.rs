// SPDX-License-Identifier: AGPL-3.0-or-later
//! Server-side HTML rendering: home, results, error pages + settings popup.
//! 100% original markup — no template engine, no upstream code.

use crate::engine::*;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

fn avatar(host: &str) -> String {
    let hue = avatar_hue(host);
    let letter = host.chars().next().map(|c| c.to_ascii_uppercase()).unwrap_or('?');
    // real favicon from the server-side proxy sits ON TOP of the letter;
    // if it cannot load the letter avatar underneath stays visible
    format!(r#"<span class="k-ava" style="--h:{hue}" aria-hidden="true"><img src="/favicon?d={}" alt="" loading="lazy" onerror="this.remove()">{}</span>"#,
        esc(host), esc(&letter.to_string()))
}

fn gear_svg(size: u32) -> String {
    format!(r#"<svg width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M12 15.5A3.5 3.5 0 1 1 15.5 12 3.5 3.5 0 0 1 12 15.5Zm7.4-2.6a7.6 7.6 0 0 0 0-1.8l2-1.6-2-3.4-2.4 1a7.4 7.4 0 0 0-1.6-.9L15 3.5h-4l-.4 2.6a7.4 7.4 0 0 0-1.6 1l-2.4-1-2 3.4 2 1.6a7.6 7.6 0 0 0 0 1.8l-2 1.6 2 3.4 2.4-1a7.4 7.4 0 0 0 1.6.9l.4 2.7h4l.4-2.7a7.4 7.4 0 0 0 1.6-.9l2.4 1 2-3.4Z" fill="currentColor" fill-rule="evenodd" clip-rule="evenodd"/></svg>"#)
}

fn chevron_svg() -> &'static str {
    r#"<svg class="k-chev" width="14" height="14" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M5 9l7 7 7-7" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#
}

fn close_svg() -> &'static str {
    r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/></svg>"#
}

/// Languages: Auto first, then A–Z by English name.
const LANGS: &[(&str, &str)] = &[
    ("auto", "Auto-detect"),
    ("ar", "العربية"), ("az", "Azərbaycanca"), ("bn", "বাংলা"), ("zh", "中文"),
    ("nl", "Nederlands"), ("en", "English"), ("fa", "فارسی"), ("fr", "Français"),
    ("de", "Deutsch"), ("el", "Ελληνικά"), ("he", "עברית"), ("hi", "हिन्दी"),
    ("id", "Bahasa Indonesia"), ("it", "Italiano"), ("ja", "日本語"), ("ko", "한국어"),
    ("pl", "Polski"), ("pt", "Português"), ("ro", "Română"), ("ru", "Русский"),
    ("es", "Español"), ("th", "ไทย"), ("tr", "Türkçe"), ("uk", "Українська"),
    ("vi", "Tiếng Việt"),
];

const TIMES: &[(&str, &str)] = &[
    ("", "Anytime"), ("d", "Past 24 hours"), ("w", "Past week"),
    ("m", "Past month"), ("y", "Past year"),
];

const SS: &[(&str, &str)] = &[
    ("0", "SafeSearch: Off"), ("1", "SafeSearch: Moderate"), ("2", "SafeSearch: Strict"),
];

const CATS: &[(&str, &str)] = &[
    ("all", "All"), ("images", "Images"), ("videos", "Videos"), ("news", "News"),
    ("maps", "Maps"), ("music", "Music"), ("it", "IT"), ("science", "Science"),
    ("files", "Files"), ("social", "Social Media"), ("onion", "Onion"),
];

fn label_of<'a>(options: &'a [(&'a str, &'a str)], value: &str) -> &'a str {
    options.iter().find(|(v, _)| *v == value).map(|(_, l)| *l).unwrap_or(options.first().map(|(_, l)| *l).unwrap_or(""))
}

/// Custom dropdown — never the browser default look.
/// `nav`: selecting an option reloads the search with that parameter.
fn dd(id: &str, key: &str, options: &[(&str, &str)], current: &str, nav: bool) -> String {
    let mut o = format!(
        r#"<div class="k-dd" id="{id}" data-k="{key}"{}>"#,
        if nav { r#" data-nav="1""# } else { "" });
    o.push_str(&format!(
        r#"<button type="button" class="k-dd-btn" aria-haspopup="listbox" aria-expanded="false"><span class="k-dd-cur">{}</span>{}</button>"#,
        esc(label_of(options, current)), chevron_svg()));
    o.push_str(r#"<div class="k-dd-menu" role="listbox" hidden>"#);
    for (v, l) in options {
        let on = if *v == current { " on" } else { "" };
        o.push_str(&format!(r#"<div class="k-dd-opt{on}" role="option" data-v="{v}">{}</div>"#, esc(l)));
    }
    o.push_str("</div></div>");
    o
}

/// query string builder that keeps the shared params
fn qs(base: &str, q: &str, cat: &str, lang: &str, time: &str, ss: u8, page: u32) -> String {
    let mut s = format!("q={}", urlencode(q));
    if cat != "all" { s.push_str(&format!("&c={cat}")); }
    if lang != "auto" && !lang.is_empty() { s.push_str(&format!("&lang={lang}")); }
    if !time.is_empty() { s.push_str(&format!("&time={time}")); }
    if ss > 0 { s.push_str(&format!("&ss={ss}")); }
    if page > 1 { s.push_str(&format!("&page={page}")); }
    format!("{base}?{s}")
}

fn cat_tabs(q: &str, current: &str, lang: &str, time: &str, ss: u8) -> String {
    let mut o = String::from(r#"<nav class="k-cats" aria-label="Categories">"#);
    for (id, label) in CATS {
        let on = if *id == current { " on" } else { "" };
        let href = qs("/search", q, id, lang, time, ss, 1);
        o.push_str(&format!(r#"<a class="k-cat{on}" href="{href}" data-c="{id}">{label}</a>"#));
    }
    o.push_str("</nav>");
    o
}

fn favicon_links() -> &'static str {
    r#"<link rel="icon" href="/static/brand/favicon.ico" sizes="any">
  <link rel="icon" href="/static/brand/favicon.svg" type="image/svg+xml">
  <link rel="icon" href="/static/brand/logomark-192.png" type="image/png" sizes="192x192">
  <link rel="apple-touch-icon" href="/static/brand/apple-touch-icon.png">
  <link rel="manifest" href="/static/manifest.webmanifest">"#
}

fn layout_open(title: &str, desc: &str, body_class: &str) -> String {
    format!(r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
  <title>{}</title>
  <meta name="description" content="{}">
  <meta name="theme-color" content="#000000">
  <meta property="og:title" content="{}">
  <meta property="og:description" content="{}">
  <meta property="og:image" content="/static/brand/og-image.png">
  <meta property="og:type" content="website">
  <meta name="referrer" content="no-referrer">
  {}
  <link rel="stylesheet" href="/static/css/kuroka.css?v={VERSION}">
  <script src="/static/js/accent.js?v={VERSION}"></script>
</head>
<body class="{}">
<header id="kZenProbe" aria-hidden="true" style="position:fixed;left:-9px;top:-9px;width:1px;height:1px;background-color:#010203;pointer-events:none"></header>
<script>(function(){{var p=document.getElementById('kZenProbe'),zen=false;try{{var c=getComputedStyle(p).backgroundColor;zen=(c==='rgba(0, 0, 0, 0)'||c==='transparent');}}catch(e){{}}document.documentElement.classList.add(zen?'k-zen':'k-amoled');p.parentNode.removeChild(p);}})();</script>
"##,
    esc(title), esc(desc), esc(title), esc(desc), favicon_links(), body_class)
}

fn layout_close() -> String {
    format!(r#"<button id="kTop" class="k-top" type="button" aria-label="Back to top">{}</button>
<div id="kToast" class="k-toast" role="status" hidden></div>
{}
<script src="/static/js/kuroka.js?v={VERSION}" defer></script>
</body>
</html>"#, chevron_up_svg(), settings_modal())
}

fn chevron_up_svg() -> &'static str {
    r#"<svg width="20" height="20" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M6 14.5 12 8.5l6 6" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>"#
}

// ---------------------------------------------------------------------------
// search bar — clean input, gear outside on the right, overlay suggestions
// ---------------------------------------------------------------------------

struct BarCtx<'a> {
    q: &'a str,
    cat: &'a str,
    home: bool,
}

fn search_bar(c: &BarCtx) -> String {
    let mut o = String::from(r#"<div class="k-sbar">"#);
    if c.home {
        o.push_str(r#"<form class="k-srow k-srow-home" action="/search" method="get" role="search">"#);
        o.push_str(r#"<input type="hidden" name="c" value="all">"#);
    } else {
        o.push_str(r#"<form class="k-srow" id="kForm" action="/search" method="get" role="search">"#);
        o.push_str(&format!(r#"<input type="hidden" name="c" value="{}">"#, esc(c.cat)));
    }
    o.push_str(&format!(
        r#"<input class="k-in" id="kQ" name="q" value="{}" autocomplete="off" spellcheck="false" placeholder="Search for..." aria-label="Search query">"#,
        esc(c.q)));
    o.push_str("</form>");
    // gear sits OUTSIDE the bar, right side, same line — opens the settings popup
    o.push_str(&format!(
        r#"<button type="button" class="k-gear" id="kGear" aria-label="Settings" title="Settings">{}</button>"#,
        gear_svg(22)));
    // suggestions overlay on top of whatever is below — nothing ever shifts
    o.push_str(r#"<div class="k-ac" id="kAc" hidden></div>"#);
    o.push_str("</div>");
    o
}

// ---------------------------------------------------------------------------
// home
// ---------------------------------------------------------------------------

pub fn home_page() -> String {
    let mut o = layout_open("Kuroka",
        "KurokaSearch — a privacy-first metasearch engine. No tracking, no logs, no ads.", "k-page-home");
    o.push_str(r#"<main class="k-home">
  <div class="k-home-center">
    <div class="k-mascot-wrap" aria-hidden="true">
      <div class="k-mascot-glow"></div>
      <div class="k-mascot-spark k-sp1"></div>
      <div class="k-mascot-spark k-sp2"></div>
      <div class="k-mascot-spark k-sp3"></div>
      <img class="k-mascot" src="/static/brand/mascot.png" alt="" width="340" height="340">
    </div>
    <div class="k-wordmark" aria-hidden="true">Kuroka</div>
"#);
    let bar = search_bar(&BarCtx { q: "", cat: "all", home: true });
    o.push_str(&bar);
    o.push_str("</div>\n</main>\n");
    o.push_str(&layout_close());
    o
}

// ---------------------------------------------------------------------------
// results page
// ---------------------------------------------------------------------------

pub fn search_page(out: &SearchOutcome, p: &SearchParams) -> String {
    let cat = p.category.as_str();
    let title = if p.query.is_empty() { "Kuroka".to_string() } else { format!("{} — Kuroka", p.query) };
    let mut o = layout_open(&title, &format!("Search results for {}", p.query), "k-page-results");

    // ---- sticky header: brand wordmark left (user-supplied art), core center ----
    o.push_str(&format!(r#"<header class="k-hdr" id="kHdr">
  <a class="k-brand" href="/" aria-label="Kuroka home"><img src="/static/brand/wordmark.webp" alt="Kuroka" width="1003" height="249"></a>
  <div class="k-hdr-core">
"#));

    let bar = search_bar(&BarCtx { q: &p.query, cat, home: false });
    o.push_str(&bar);

    // ---- cats + filters share one collapsing shell (scroll-away anim) ----
    o.push_str(r#"<div class="k-hdr-sub"><div class="k-hdr-sub-in">"#);
    o.push_str(&cat_tabs(&p.query, cat, &p.language, &p.time, p.safesearch));

    // filters row: custom dropdowns (never the browser default) + unified extras
    o.push_str(r#"<div class="k-filters">"#);
    o.push_str(&dd("ddLang", "lang", LANGS, &p.language, true));
    o.push_str(&dd("ddTime", "time", TIMES, &p.time, true));
    o.push_str(&dd("ddSs", "ss", SS, &p.safesearch.to_string(), true));
    o.push_str(&format!(
        r#"<button type="button" class="k-fbtn" id="kExtrasBtn" aria-expanded="false" aria-controls="kExtras"><span>Extras</span>{}</button>"#,
        chevron_svg()));
    o.push_str("</div>");

    o.push_str("</div></div></div></header>\n");

    // ---- body: results run the full width in ONE column; Extras sits BESIDE
    // them when open. The AI report is a full-width horizontal band in the
    // gap between the filter buttons and the first link (All tab only). ----
    // infinite scroll keeps going while ANY queried engine pages natively
    // (previously a single unpaged engine — youtube on videos — silently
    // disabled pagination for the whole tab)
    let more = {
        let engines = engines_for(p);
        (engines.iter().any(|e| e.paged) && out.results.len() >= 4 && p.page < 50) as u8
    };
    if p.category == Category::All {
        o.push_str(&format!(r#"<div class="k-ai-band">{}</div>"#, ai_card(&p.query, out)));
    }
    o.push_str(r#"<div class="k-body">"#);
    o.push_str(&format!(r#"<main class="k-main" id="kMain" data-cat="{cat}" data-q="{}" data-page="{}" data-more="{more}">"#, esc(&p.query), p.page));

    if out.results.is_empty() {
        o.push_str(&no_results(p));
    } else {
        match p.category {
            Category::Images => o.push_str(&image_grid(&out.results)),
            Category::Videos => o.push_str(&video_grid(&out.results)),
            Category::Music => o.push_str(&music_grid(&out.results)),
            Category::Maps => o.push_str(&map_section(&p.query, &out.results)),
            _ => o.push_str(&web_list(&out.results)),
        }
    }
    o.push_str("</main>\n");

    // ---- extras: ONE piece beside the results — image filters, engine
    // toggles, engine stats. Lives in the grid's side column, so it can
    // never cover results and never fall off the right edge. ----
    o.push_str(r#"<aside class="k-extras" id="kExtras" hidden>"#);
    if cat == "images" {
        o.push_str(r#"<div class="k-x-sec k-x-row">"#);
        o.push_str(&format!(r#"<label class="k-x-l">Size {}</label>"#,
            dd("ddXSize", "size", &[("any","Any"),("small","Small"),("medium","Medium"),("large","Large"),("wallpaper","Wallpaper")], &p.size, true)));
        o.push_str(&format!(r#"<label class="k-x-l">Layout {}</label>"#,
            dd("ddXLayout", "layout", &[("any","Any"),("wide","Wide"),("tall","Tall"),("square","Square")], &p.layout, true)));
        o.push_str("</div>");
    }
    o.push_str(r#"<div class="k-x-sec"><div class="k-x-h">Engines</div><div class="k-x-engines">"#);
    let active: Vec<&str> = if p.engines.is_empty() {
        REGISTRY.iter().filter(|e| e.category == cat && e.default_on).map(|e| e.id).collect()
    } else {
        p.engines.iter().map(|x| x.as_str()).collect()
    };
    for e in REGISTRY.iter().filter(|e| e.category == cat) {
        let on = if active.contains(&e.id) { " checked" } else { "" };
        o.push_str(&format!(
            r#"<label class="k-x-e"><input type="checkbox" name="k-engine" value="{}"{on}>{}</label>"#,
            e.id, e.name));
    }
    o.push_str(r#"</div><button type="button" class="k-fbtn k-x-apply" id="kExtrasApply">Apply</button></div>"#);
    // engine telemetry lives inside extras — everything extra opens from one place
    o.push_str(&format!(r#"<div class="k-x-sec k-x-stats"><div class="k-x-h">Engines on this search</div>{}</div>"#,
        info_table(out)));
    o.push_str("</aside></div>\n");

    o.push_str(&layout_close());
    o
}

fn info_table(out: &SearchOutcome) -> String {
    let total: usize = out.engines.iter().map(|e| e.count).sum();
    let ok = out.engines.iter().filter(|e| e.ok).count();
    let mut rows = String::new();
    for e in &out.engines {
        let status = if e.ok { "ok" } else { "err" };
        let time = if e.time_ms >= 1000 {
            format!("{:.1} s", e.time_ms as f64 / 1000.0)
        } else {
            format!("{} ms", e.time_ms)
        };
        let err = e.error.as_deref().map(|x| format!(r#"<span class="k-inf-err">{}</span>"#, esc(x))).unwrap_or_default();
        rows.push_str(&format!(
            r#"<tr><td>{}</td><td>{}</td><td>{}</td><td><span class="k-inf-dot k-inf-{}"></span>{}</td></tr>"#,
            esc(&e.name), e.count, time, status, err));
    }
    let total_str = if out.total_ms >= 1000 { format!("{:.1} s", out.total_ms as f64 / 1000.0) } else { format!("{} ms", out.total_ms) };
    format!(r#"<div class="k-inf-sum">{ok} of {} engines replied in {total_str} · {} results</div>
<table class="k-inf-tbl"><thead><tr><th>engine</th><th>results</th><th>time</th><th>status</th></tr></thead><tbody>{rows}</tbody></table>"#,
        out.engines.len(), total)
}

fn ai_card(q: &str, out: &SearchOutcome) -> String {
    // numbered context for the client-side provider call: top 10 results,
    // each tagged [n] (domain) title — desc. The same numbering ships as a
    // JSON source list so the browser can turn [n] citations into links.
    let mut ctx = String::new();
    let mut sources = String::new();
    for (i, r) in out.results.iter().take(10).enumerate() {
        ctx.push_str(&format!("[{}] ({}) {} — {}\n", i + 1, r.source, r.title, r.desc));
        sources.push_str(&format!(
            "{{\"n\":{},\"u\":{},\"d\":{}}},",
            i + 1,
            serde_json::to_string(&r.url).unwrap_or_default(),
            serde_json::to_string(&r.source).unwrap_or_default()));
    }
    let sources_json = if sources.is_empty() {
        "[]".to_string()
    } else {
        format!("[{}]", sources.trim_end_matches(','))
    };
    let ctx_json = serde_json::to_string(&clip(&ctx, 7000)).unwrap_or_default();
    format!(r#"<div class="k-ai" id="kAi">
  <img class="k-ai-ava" src="/static/brand/ai-avatar.png" alt="" width="44" height="44">
  <div class="k-ai-body">
    <div class="k-ai-head">AI Report <span class="k-ai-prov" id="kAiProv">Google Gemini</span><span class="k-ai-model" id="kAiModel"></span></div>
    <div class="k-ai-note">Summarizes these results with citations. Bring your own key in <button type="button" class="k-linklike" id="kAiOpenSet">Settings</button> — it never leaves this browser.</div>
    <div class="k-ai-out" id="kAiOut" hidden></div>
    <div class="k-ai-fups" id="kAiFups" hidden></div>
    <div class="k-ai-acts">
      <button class="k-ai-go" id="kAiGo" type="button" data-q="{}" data-ctx="{}" data-sources="{}">Generate</button>
      <button type="button" class="k-ai-ghost" id="kAiStop" hidden>Stop</button>
      <button type="button" class="k-ai-ghost" id="kAiCopy" hidden>Copy</button>
      <button type="button" class="k-ai-ghost" id="kAiRegen" hidden>Regenerate</button>
    </div>
  </div>
</div>
"#, esc(q), esc(&ctx_json), esc(&sources_json))
}

fn no_results(p: &SearchParams) -> String {
    let cat_label = p.category.label();
    format!(r#"<div class="k-nors">
  <img class="k-nors-img" src="/static/brand/mascot-sad.png" alt="" width="220" height="220">
  <h2>No results in {}</h2>
  <p>Kuroka could not find anything for <strong>{}</strong> here.</p>
  <ul class="k-nors-tips">
    <li>check the spelling, or try broader keywords</li>
    <li>switch to the <a href="{}">All</a> category</li>
    <li>turn off strict filters (time, SafeSearch) above</li>
  </ul>
</div>"#, esc(cat_label), esc(&p.query), qs("/search", &p.query, "all", &p.language, &p.time, p.safesearch, 1))
}

fn web_list(rs: &[SearchResult]) -> String {
    let mut o = String::from(r#"<div class="k-rs">"#);
    for r in rs {
        let av = avatar(&r.source);
        let magnet = if r.url.starts_with("magnet:") {
            r#"<span class="k-r-mag" title="magnet link">magnet</span>"#
        } else {
            ""
        };
        // engine chip floats to the right of the site line
        let mut eng_chip = String::new();
        let mut meta = String::new();
        if let Some(d) = &r.published {
            if !d.is_empty() {
                meta.push_str(&format!(r#"<span class="k-r-date" data-d="{}"></span>"#, esc(d)));
            }
        }
        for (k, v) in &r.meta {
            if k == "engine" {
                eng_chip = format!(r#"<span class="k-r-eng">{}</span>"#, esc(v));
            } else if k == "page" {
                meta.push_str(&format!(r#"<a class="k-r-kv k-r-kv-a" href="{}" rel="noopener noreferrer" target="_blank">details</a>"#, esc(v)));
            } else {
                meta.push_str(&format!(r#"<span class="k-r-kv"><b>{}</b> {}</span>"#, esc(k), esc(v)));
            }
        }
        // breadcrumb: the URL path under the host name
        let path = url_path(&r.url);
        let path_html = if path.is_empty() { String::new() } else {
            format!(r#"<span class="k-r-path">{}</span>"#, esc(&path))
        };
        let desc = if r.desc.is_empty() { String::new() } else {
            format!(r#"<p class="k-r-desc">{}</p>"#, esc(&r.desc))
        };
        o.push_str(&format!(r#"<article class="k-r">
  <div class="k-r-head">{av}<span class="k-r-site"><a class="k-r-url" href="{url}" rel="noopener noreferrer">{src}</a>{path_html}</span>{eng_chip}{magnet}</div>
  <a class="k-r-title" href="{url}" rel="noopener noreferrer">{title}</a>
  {desc}
  <div class="k-r-meta">{meta}</div>
</article>
"#,
        url = esc(&r.url), src = esc(&r.source), title = esc(&r.title)));
    }
    o.push_str("</div>");
    o
}

/// Human breadcrumb of a result URL: "/repo/name/…", empty for bare hosts
/// and magnets.
fn url_path(u: &str) -> String {
    if u.starts_with("magnet:") { return String::new(); }
    let rest = u.split_once("://").map(|(_, r)| r).unwrap_or(u);
    let Some((_, tail)) = rest.split_once('/') else { return String::new() };
    let tail = tail.split('?').next().unwrap_or(tail);
    let tail = tail.split('#').next().unwrap_or(tail);
    let tail = tail.trim_end_matches('/');
    if tail.is_empty() { return String::new(); }
    let mut out = format!("/{tail}");
    if out.chars().count() > 56 {
        out = format!("…{}", out.chars().skip(out.chars().count() - 55).collect::<String>());
    }
    out
}

fn image_grid(rs: &[SearchResult]) -> String {
    let mut o = String::from(r#"<div class="k-imgs">"#);
    for r in rs {
        let raw = r.thumb.clone().or_else(|| r.img.clone()).unwrap_or_default();
        let thumb = if raw.is_empty() { String::new() } else { format!("/img?u={}", urlencode(&raw)) };
        let res = match (r.width, r.height) {
            (Some(w), Some(h)) => format!("{w}×{h}"),
            _ => String::new(),
        };
        // tile height is reserved by the uniform CSS aspect-ratio on .k-img
        // (a collapsed grid would make the scroll sentinel fire early and
        // infinite scroll burn through all pages unseen)
        let mut lic = String::new();
        for (k, v) in &r.meta {
            if k == "license" { lic = v.clone(); }
        }
        let title_attr = esc(&r.title);
        o.push_str(&format!(r#"<a class="k-img" href="{}" rel="noopener noreferrer" target="_blank">
  <img src="{thumb}" loading="lazy" alt="{}" onerror="if(window.kurokaImgErr)kurokaImgErr(this);else this.closest('.k-img').classList.add('k-img-dead')">
  <span class="k-img-cap">{}</span>
  <span class="k-img-res">{res}</span>
  <span class="k-img-src">{}{lic_s}</span>
</a>
"#,
        esc(&r.url), esc(&r.title), title_attr,
        esc(&r.source),
        lic_s = if lic.is_empty() { String::new() } else { format!(" · {}", esc(&lic)) }));
    }
    o.push_str("</div>");
    o
}

fn video_grid(rs: &[SearchResult]) -> String {
    let mut o = String::from(r#"<div class="k-vids">"#);
    for r in rs {
        let thumb = r.thumb.clone().unwrap_or_default();
        let thumb = if thumb.is_empty() { String::new() } else { format!("/img?u={}", urlencode(&thumb)) };
        let dur = r.duration.clone().unwrap_or_default();
        let by = r.publisher.clone().filter(|p| !p.is_empty()).unwrap_or_else(|| r.source.clone());
        let thumb_html = if thumb.is_empty() { String::new() } else {
            format!(r#"<img src="{}" loading="lazy" alt="" onerror="this.style.display='none'">"#, thumb)
        };
        // byline: channel · views · date — everything on one quiet line
        let mut bybits: Vec<String> = vec![esc(&by)];
        for (k, v) in &r.meta {
            if k == "views" || k == "seeders" || k == "size" { bybits.push(esc(v)); }
        }
        if let Some(d) = &r.published {
            if !d.is_empty() {
                bybits.push(format!(r#"<span class="k-r-date" data-d="{}"></span>"#, esc(d)));
            }
        }
        let byline = bybits.join(" · ");
        o.push_str(&format!(
            r#"<a class="k-vid" href="{}" rel="noopener noreferrer" target="_blank">
  <span class="k-vid-thumb">{thumb_html}<span class="k-vid-dur">{}</span></span>
  <span class="k-vid-body">
    <span class="k-vid-title">{}</span>
    <span class="k-vid-by">{byline}</span>
  </span>
</a>
"#,
            esc(&r.url), esc(&dur), esc(&r.title)));
    }
    o.push_str("</div>");
    o
}

fn music_grid(rs: &[SearchResult]) -> String {
    let mut o = String::from(r#"<div class="k-mus">"#);
    for r in rs {
        let thumb = r.thumb.clone().unwrap_or_default();
        let dur = r.duration.clone().unwrap_or_default();
        let play = if let Some(pv) = &r.preview {
            format!(r#"<button class="k-mus-play" type="button" data-pv="{}" aria-label="Play preview"></button>"#, esc(pv))
        } else { String::new() };
        let mut meta = String::new();
        for (k, v) in &r.meta {
            if k != "engine" {
                meta.push_str(&format!(r#"<span class="k-r-kv"><b>{}</b> {}</span>"#, esc(k), esc(v)));
            }
        }
        o.push_str(&format!(r#"<article class="k-mus-card">
  <a class="k-mus-art" href="{}" rel="noopener noreferrer" target="_blank">{img}{play}</a>
  <a class="k-mus-title" href="{}" rel="noopener noreferrer" target="_blank">{}</a>
  <span class="k-mus-by">{}</span>
  <span class="k-mus-dur">{dur}</span>
  <div class="k-r-meta">{meta}</div>
</article>
"#,
        esc(&r.url), esc(&r.url), esc(&r.title), esc(&r.desc),
        img = if thumb.is_empty() { String::new() } else { format!(r#"<img src="/img?u={}" loading="lazy" alt="" onerror="this.style.display='none'">"#, urlencode(&thumb)) }));
    }
    o.push_str("</div>");
    o
}

fn map_section(query: &str, rs: &[SearchResult]) -> String {
    // live Google Maps embed for the query — the classic output=embed
    // endpoint needs no key and renders the interactive map client-side
    let gq = urlencode(query);
    let mut o = format!(r#"<section class="k-gmap-wrap">
  <iframe class="k-gmap" src="https://maps.google.com/maps?q={gq}&amp;output=embed" loading="lazy" referrerpolicy="no-referrer" title="Google Maps — {qt}"></iframe>
  <div class="k-gmap-bar">
    <span class="k-gmap-tag">Google Maps</span>
    <a class="k-gmap-open" href="https://www.google.com/maps/search/?api=1&amp;query={gq}" target="_blank" rel="noopener noreferrer">Open in Google Maps</a>
  </div>
</section>
"#, qt = esc(query));
    o.push_str(&map_grid(rs));
    o
}

fn map_grid(rs: &[SearchResult]) -> String {
    let mut o = String::from(r#"<div class="k-maps">"#);
    for r in rs {
        let mut meta = String::new();
        for (k, v) in &r.meta {
            if k == "engine" { continue; }
            meta.push_str(&format!(r#"<span class="k-r-kv"><b>{}</b> {}</span>"#, esc(k), esc(v)));
        }
        let (coords, gmap_q) = match (r.lat, r.lon) {
            (Some(la), Some(lo)) => (
                format!(r#"<span class="k-map-geo">{:.4}, {:.4}</span>"#, la, lo),
                format!("{:.6},{:.6}", la, lo),
            ),
            _ => (String::new(), format!("{} {}", r.title, r.desc)),
        };
        let gq = urlencode(&gmap_q);
        let dq = urlencode(&gmap_q);
        o.push_str(&format!(r#"<article class="k-map">
  <a class="k-map-title" href="{}" rel="noopener noreferrer" target="_blank">{}</a>
  <p class="k-map-addr">{}</p>
  <div class="k-r-meta">{meta}{coords}</div>
  <div class="k-map-act">
    <a href="https://www.google.com/maps/search/?api=1&amp;query={gq}" target="_blank" rel="noopener noreferrer">Maps</a>
    <a href="https://www.google.com/maps/dir/?api=1&amp;destination={dq}" target="_blank" rel="noopener noreferrer">Directions</a>
  </div>
</article>
"#, esc(&r.url), esc(&r.title), esc(&r.desc)));
    }
    o.push_str("</div>");
    o
}

// ---------------------------------------------------------------------------
// settings popup (modal, blurred backdrop, animated — rendered on every page)
// ---------------------------------------------------------------------------

fn settings_modal() -> String {
    // preset accent swatches (hex, name) — the first one is the default gold
    const ACCENT_SWATCHES: [(&str, &str); 9] = [
        ("#e6b445", "Gold — default"), ("#f08c3a", "Amber"), ("#e05d5d", "Crimson"),
        ("#d95db4", "Magenta"), ("#9a6cf0", "Violet"), ("#5d7de0", "Azure"),
        ("#3ec4d8", "Cyan"), ("#3ecf8e", "Emerald"), ("#b8c24a", "Lime"),
    ];
    let mut o = String::from(r#"<div class="k-set" id="kSet" hidden>
  <div class="k-set-bg" id="kSetBg"></div>
  <div class="k-set-win" role="dialog" aria-modal="true" aria-label="Settings">
    <div class="k-set-head">
      <span class="k-set-title">Settings</span>
      <button type="button" class="k-set-x" id="kSetX" aria-label="Close">"#);
    o.push_str(close_svg());
    o.push_str(r#"</button>
    </div>
    <div class="k-set-body">
      <nav class="k-set-nav" aria-label="Settings sections">
        <button type="button" class="on" data-t="search">Search</button>
        <button type="button" data-t="appearance">Appearance</button>
        <button type="button" data-t="ai">AI Report</button>
        <button type="button" data-t="engines">Engines</button>
        <button type="button" data-t="privacy">Privacy</button>
      </nav>
      <div class="k-set-panes">
        <section class="k-set-pane on" data-pane="search">
          <div class="k-p-row"><label for="s-lang">Language</label>"#);
    o.push_str(&dd("s-dd-lang", "lang", LANGS, "auto", false));
    o.push_str(r#"</div>
          <div class="k-p-row"><label for="s-cat">Default category</label>"#);
    o.push_str(&dd("s-dd-cat", "cat", CATS, "all", false));
    o.push_str(r#"</div>
          <div class="k-p-row"><label for="s-time">Time range</label>"#);
    o.push_str(&dd("s-dd-time", "time", TIMES, "", false));
    o.push_str(r#"</div>
          <div class="k-p-row"><label for="s-ss">SafeSearch</label>"#);
    o.push_str(&dd("s-dd-ss", "ss", &[("0","Off"),("1","Moderate"),("2","Strict")], "0", false));
    o.push_str(r#"</div>
          <div class="k-p-row"><label for="s-ac">Autocomplete</label><label class="k-switch"><input type="checkbox" id="s-ac"><span class="k-sl"></span></label></div>
          <div class="k-p-row"><label for="s-newtab">Open results in new tab</label><label class="k-switch"><input type="checkbox" id="s-newtab"><span class="k-sl"></span></label></div>
          <div class="k-p-row k-p-gh"><span class="k-p-gh-note">KurokaSearch is free software (AGPL)</span><a class="k-gh-link" href="https://github.com/Abdullayews/KurokaSearch" target="_blank" rel="noopener">GitHub</a></div>
        </section>
        <section class="k-set-pane" data-pane="appearance">
          <div class="k-p-row"><label for="s-theme">Theme</label>"#);
    o.push_str(&dd("s-dd-theme", "theme", &[("dark","Dark"),("light","Light")], "dark", false));
    o.push_str(r#"</div>
          <div class="k-p-row"><label for="s-glow">Backdrop glow</label><label class="k-switch"><input type="checkbox" id="s-glow" checked><span class="k-sl"></span></label></div>
          <div class="k-p-row k-p-accent"><label for="s-accent-hex">Accent color</label>
            <div class="k-accent-pick">
              <div class="k-swatches" role="radiogroup" aria-label="Accent color">"#);
    for (i, sw) in ACCENT_SWATCHES.iter().enumerate() {
        o.push_str(&format!(
            r#"<button type="button" class="k-sw{cls}" data-c="{hex}" style="--sw:{hex}" title="{name}" aria-label="{name}"></button>"#,
            cls = if i == 0 { " def on" } else { "" },
            hex = sw.0, name = sw.1));
    }
    o.push_str(r#"<label class="k-sw k-sw-custom" title="Custom color" aria-label="Custom color">
                  <input type="color" id="s-accent">
                </label>
              </div>
              <input type="text" id="s-accent-hex" class="k-p-in k-hex-in" maxlength="7" spellcheck="false" autocomplete="off" aria-label="Accent hex code">
            </div>
          </div>
          <div class="k-accent-bar" aria-hidden="true"></div>
          <p class="k-set-note">The accent recolors buttons, links, borders, highlights and particles across every tab. The mascot, the wordmark and the app icons keep their original artwork. Stored only in this browser; empty hex = default gold.</p>
          <p class="k-set-note">Background is transparent by default: with the Zen Internet extension your wallpaper shows through; without it Kuroka falls back to a true-black AMOLED theme.</p>
        </section>
        <section class="k-set-pane" data-pane="ai">
          <div class="k-p-row"><label for="s-ai-prov">Provider</label>"#);
    o.push_str(&dd("s-dd-prov", "aiProv", &[
        ("gemini","Google Gemini"),("openai","OpenAI"),("groq","Groq"),
        ("openrouter","OpenRouter"),("zai","Z.ai (GLM)"),("custom","Custom (OpenAI-compatible)")], "gemini", false));
    o.push_str(r#"</div>
          <div class="k-p-row"><label for="s-ai-model">Model <span class="k-p-note">(any id the provider accepts)</span></label><input type="text" id="s-ai-model" class="k-p-in" placeholder="gemini-flash-latest" autocomplete="off"></div>
          <div class="k-p-row"><label for="s-ai-key">API key</label><input type="password" id="s-ai-key" class="k-p-in" placeholder="paste key — stored locally" autocomplete="off"></div>
          <div class="k-p-row"><label for="s-ai-base">Base URL <span class="k-p-note">(custom providers)</span></label><input type="text" id="s-ai-base" class="k-p-in" placeholder="https://…/v1" autocomplete="off"></div>
          <div class="k-p-row"><label for="s-ai-auto">Auto-generate the report</label><label class="k-switch"><input type="checkbox" id="s-ai-auto" checked><span class="k-sl"></span></label></div>
          <p class="k-set-note">Bring your own key — requests stream straight from this browser to the provider. The report cites the results it used and ends with follow-up searches.</p>
        </section>
        <section class="k-set-pane" data-pane="engines">"#);
    // per-category engine toggles
    for (cid, clabel) in CATS {
        let engines: Vec<&EngineInfo> = REGISTRY.iter().filter(|e| e.category == *cid).collect();
        if engines.is_empty() { continue; }
        o.push_str(&format!(r#"<div class="k-eng-cat"><div class="k-x-h">{clabel}</div><div class="k-x-engines">"#));
        for e in engines {
            let on = if e.default_on { " checked" } else { "" };
            o.push_str(&format!(
                r#"<label class="k-x-e"><input type="checkbox" data-eng="{eid}" data-cat="{cid}"{on}>{}</label>"#,
                e.name, eid = e.id));
        }
        o.push_str("</div></div>");
    }
    o.push_str(r#"<p class="k-set-note">Chosen engines apply to every new search in that category.</p>
        </section>
        <section class="k-set-pane" data-pane="privacy">
          <p class="k-set-p">Queries are proxied through this server and never logged. No cookies, no fingerprinting, no ad networks.</p>
          <p class="k-set-p">Settings and AI keys live in this browser's localStorage only — clear them any time:</p>
          <button type="button" class="k-pg-btn" id="s-wipe">Clear local data</button>
        </section>
      </div>
    </div>
    <div class="k-set-foot">
      <button type="button" class="k-pg-btn" id="kSetReset">Reset</button>
      <button type="button" class="k-ai-go" id="kSetSave">Save</button>
    </div>
  </div>
</div>"#);
    o
}

// ---------------------------------------------------------------------------
// error page
// ---------------------------------------------------------------------------

pub fn error_page(code: u16, msg: &str) -> String {
    let mut o = layout_open(&format!("{code} — Kuroka"), "Something went wrong", "k-page-err");
    o.push_str(&format!(r#"<main class="k-err">
  <img class="k-err-img" src="/static/brand/mascot-error.png" alt="" width="240" height="240">
  <h1>{code}</h1>
  <p>{}</p>
  <a class="k-pg-btn" href="/">Back to Kuroka</a>
</main>
"#, esc(msg)));
    o.push_str(&layout_close());
    o
}
