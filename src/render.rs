// SPDX-License-Identifier: AGPL-3.0-or-later
//! Server-side HTML rendering: home, results, preferences, error pages.
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
    format!(r#"<span class="k-ava" style="--h:{hue}" aria-hidden="true">{}</span>"#, esc(&letter.to_string()))
}

fn gear_svg(size: u32) -> String {
    format!(r#"<svg width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M12 15.5A3.5 3.5 0 1 1 15.5 12 3.5 3.5 0 0 1 12 15.5Zm7.4-2.6a7.6 7.6 0 0 0 0-1.8l2-1.6-2-3.4-2.4 1a7.4 7.4 0 0 0-1.6-.9L15 3.5h-4l-.4 2.6a7.4 7.4 0 0 0-1.6 1l-2.4-1-2 3.4 2 1.6a7.6 7.6 0 0 0 0 1.8l-2 1.6 2 3.4 2.4-1a7.4 7.4 0 0 0 1.6.9l.4 2.7h4l.4-2.7a7.4 7.4 0 0 0 1.6-.9l2.4 1 2-3.4Z" fill="currentColor" fill-rule="evenodd" clip-rule="evenodd"/></svg>"#)
}

fn chevron_svg() -> &'static str {
    r#"<svg width="22" height="22" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M6 14.5 12 8.5l6 6" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>"#
}

const LANGS: &[(&str, &str)] = &[
    ("auto", "Auto-detect"), ("en", "English"), ("az", "Azərbaycanca"), ("tr", "Türkçe"),
    ("ru", "Русский"), ("de", "Deutsch"), ("fr", "Français"), ("es", "Español"),
    ("ar", "العربية"), ("ja", "日本語"), ("zh", "中文"),
];

const TIMES: &[(&str, &str)] = &[
    ("", "Anytime"), ("d", "Past 24 hours"), ("w", "Past week"),
    ("m", "Past month"), ("y", "Past year"),
];

const SS: &[(&str, &str)] = &[("0", "SafeSearch: Off"), ("1", "SafeSearch: Moderate"), ("2", "SafeSearch: Strict")];

const CATS: &[(&str, &str)] = &[
    ("all", "All"), ("images", "Images"), ("videos", "Videos"), ("news", "News"),
    ("maps", "Maps"), ("music", "Music"), ("it", "IT"), ("science", "Science"),
    ("files", "Files"), ("social", "Social Media"),
];

fn sel(name: &str, options: &[(&str, &str)], current: &str, cls: &str) -> String {
    let mut o = format!(r#"<select name="{name}" class="{cls}" aria-label="{name}">"#);
    for (v, l) in options {
        let sel = if *v == current { " selected" } else { "" };
        o.push_str(&format!(r#"<option value="{v}"{sel}>{}</option>"#, esc(l)));
    }
    o.push_str("</select>");
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
  <meta name="theme-color" content="#0a0a0c">
  <meta property="og:title" content="{}">
  <meta property="og:description" content="{}">
  <meta property="og:image" content="/static/brand/og-image.png">
  <meta property="og:type" content="website">
  <meta name="referrer" content="no-referrer">
  {}
  <link rel="stylesheet" href="/static/css/kuroka.css">
</head>
<body class="{}">
"##,
    esc(title), esc(desc), esc(title), esc(desc), favicon_links(), body_class)
}

fn layout_close() -> &'static str {
    r#"<button id="kTop" class="k-top" type="button" aria-label="Back to top">CHEV</button>
<div id="kToast" class="k-toast" role="status" hidden></div>
<script src="/static/js/kuroka.js" defer></script>
</body>
</html>"#
}

// ---------------------------------------------------------------------------
// search bar (shared by home and results header)
// ---------------------------------------------------------------------------

struct BarCtx<'a> {
    q: &'a str,
    cat: &'a str,
    home: bool,
}

fn search_bar(c: &BarCtx) -> String {
    let mut o = String::new();
    if c.home {
        o.push_str(r#"<form class="k-srow k-srow-home" action="/search" method="get" role="search">"#);
        o.push_str(r#"<input type="hidden" name="c" value="all">"#);
    } else {
        o.push_str(r#"<form class="k-srow" id="kForm" action="/search" method="get" role="search">"#);
        o.push_str(&format!(r#"<input type="hidden" name="c" value="{}">"#, esc(c.cat)));
    }
    o.push_str(r#"<span class="k-chip"><img src="/static/brand/logomark-48.png" alt="" width="22" height="22"></span>"#);
    o.push_str(&format!(
        r#"<input class="k-in" id="kQ" name="q" value="{}" autocomplete="off" spellcheck="false" placeholder="Search for..." aria-label="Search query">"#,
        esc(c.q)));
    o.push_str(r#"<button type="button" class="k-clear" id="kClear" hidden aria-label="Clear query">&times;</button>"#);
    o.push_str(r#"<button class="k-go" type="submit">Search</button>"#);
    if c.home {
        // settings button sits right of the search bar — gear rotation on hover
        o.push_str(r#"<a class="k-gear" href="/preferences" aria-label="Preferences" title="Preferences">GEAR</a>"#);
    }
    o.push_str("</form>");
    o.push_str(r#"<div class="k-ac" id="kAc" hidden></div>"#);
    o
}

// ---------------------------------------------------------------------------
// home
// ---------------------------------------------------------------------------

pub fn home_page() -> String {
    let mut o = layout_open("Kuroka — search privately",
        "KurokaSearch — a privacy-first metasearch engine. No tracking, no logs, no ads.", "k-page-home");
    o.push_str(r#"<main class="k-home">
  <div class="k-home-center">
    <img class="k-mascot" src="/static/brand/mascot.png" alt="Kuroka, the cat-eared mascot of KurokaSearch" width="340" height="340">
    <div class="k-wordmark" aria-hidden="true">Kuroka</div>
"#);
    let bar = search_bar(&BarCtx { q: "", cat: "all", home: true });
    o.push_str(&bar);
    o.push_str(&cat_tabs("", "all", "auto", "", 0));
    o.push_str(r#"</div>
  <footer class="k-foot">
    <span class="k-foot-brand">KurokaSearch</span>
    <span class="k-foot-note">no tracking · no logs · no ads — queries are proxied, never stored</span>
    <span class="k-foot-links"><a href="/preferences">Preferences</a><a href="https://github.com/Abdullayews/KurokaSearch" rel="noopener">GitHub</a></span>
  </footer>
</main>
"#);
    o.push_str(&layout_close().replace("CHEV", chevron_svg()).replace("GEAR", &gear_svg(22)));
    o
}

// ---------------------------------------------------------------------------
// results page
// ---------------------------------------------------------------------------

pub fn search_page(out: &SearchOutcome, p: &SearchParams) -> String {
    let cat = p.category.as_str();
    let title = if p.query.is_empty() { "Kuroka".to_string() } else { format!("{} — Kuroka", p.query) };
    let mut o = layout_open(&title, &format!("Search results for {}", p.query), "k-page-results");

    // ---- sticky header ----
    o.push_str(r#"<header class="k-hdr" id="kHdr">
  <a class="k-brand" href="/" aria-label="Kuroka home"><img src="/static/brand/logomark-48.png" alt="" width="28" height="28"><span>Kuroka</span></a>
  <a class="k-gearc" href="/preferences" aria-label="Preferences" title="Preferences">GEARC</a>
  <div class="k-hdr-core">
"#);
    let bar = search_bar(&BarCtx { q: &p.query, cat, home: false });
    o.push_str(&bar);
    o.push_str(&cat_tabs(&p.query, cat, &p.language, &p.time, p.safesearch));

    // filters row (language / time / safesearch / extras / info)
    o.push_str(r#"<div class="k-filters">"#);
    o.push_str(&sel("lang", LANGS, &p.language, "k-sel k-sel-auto"));
    o.push_str(&sel("time", TIMES, &p.time, "k-sel"));
    o.push_str(&sel("ss", SS, &p.safesearch.to_string(), "k-sel"));
    o.push_str(r#"<button type="button" class="k-fbtn" id="kExtrasBtn" aria-expanded="false">Extras<em>CHEVSM</em></button>"#);
    o.push_str(r#"<button type="button" class="k-fbtn k-info-btn" id="kInfoBtn" aria-expanded="false">Info</button>"#);
    o.push_str("</div>");

    // extras panel — engine toggles (+ image filters)
    o.push_str(r#"<div class="k-extras" id="kExtras" hidden>"#);
    if cat == "images" {
        o.push_str(&format!(r#"<label class="k-x-l">Size {}</label>"#,
            sel("x-size", &[("any","Any"),("small","Small"),("medium","Medium"),("large","Large"),("wallpaper","Wallpaper")], &p.size, "k-sel k-sel-x")));
        o.push_str(&format!(r#"<label class="k-x-l">Layout {}</label>"#,
            sel("x-layout", &[("any","Any"),("wide","Wide"),("tall","Tall"),("square","Square")], &p.layout, "k-sel k-sel-x")));
    }
    o.push_str(r#"<div class="k-x-engines">"#);
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

    // info panel — response times & engine telemetry (server-rendered)
    o.push_str(&info_panel(out));

    o.push_str("</div></header>\n");

    // ---- main results ----
    o.push_str(&format!(r#"<main class="k-main" id="kMain" data-cat="{cat}" data-q="{}">"#, esc(&p.query)));

    if p.category == Category::All {
        o.push_str(&ai_card(&p.query, out));
    }

    if out.results.is_empty() {
        o.push_str(&no_results(p));
    } else {
        match p.category {
            Category::Images => o.push_str(&image_grid(&out.results)),
            Category::Videos => o.push_str(&video_grid(&out.results)),
            Category::Music => o.push_str(&music_grid(&out.results)),
            Category::Maps => o.push_str(&map_grid(&out.results)),
            _ => o.push_str(&web_list(&out.results)),
        }
        o.push_str(&pagination(out, p));
    }
    o.push_str("</main>\n");
    o.push_str(&layout_close().replace("CHEV", chevron_svg()).replace("GEARC", &gear_svg(22)));
    o
}

fn info_panel(out: &SearchOutcome) -> String {
    let total: usize = out.engines.iter().map(|e| e.count).sum();
    let ok = out.engines.iter().filter(|e| e.ok).count();
    let mut rows = String::new();
    for e in &out.engines {
        let status = if e.ok { "ok" } else { "err" };
        let err = e.error.as_deref().map(|x| format!(r#"<span class="k-inf-err">{}</span>"#, esc(x))).unwrap_or_default();
        rows.push_str(&format!(
            r#"<tr><td>{}</td><td>{}</td><td>{} ms</td><td><span class="k-inf-dot k-inf-{}"></span>{}</td></tr>"#,
            esc(&e.name), e.count, e.time_ms, status, err));
    }
    format!(r#"<div class="k-info-panel" id="kInfoPanel" hidden>
  <div class="k-inf-sum">{ok} of {} engines replied in {} ms · {} results</div>
  <table class="k-inf-tbl"><thead><tr><th>engine</th><th>results</th><th>time</th><th>status</th></tr></thead><tbody>{rows}</tbody></table>
</div>"#, out.engines.len(), out.total_ms, total)
}

fn ai_card(q: &str, out: &SearchOutcome) -> String {
    // context for the client-side provider call: top results, compact
    let mut ctx = String::new();
    for r in out.results.iter().take(8) {
        ctx.push_str(&format!("- {} — {}\n", r.title, r.desc));
    }
    let ctx_json = serde_json::to_string(&clip(&ctx, 6000)).unwrap_or_default();
    format!(r#"<section class="k-ai" id="kAi">
  <img class="k-ai-ava" src="/static/brand/ai-avatar.png" alt="" width="44" height="44">
  <div class="k-ai-body">
    <div class="k-ai-head">AI Report <span class="k-ai-prov" id="kAiProv">Google Gemini</span></div>
    <div class="k-ai-note">Summarizes the results below. Add your own API key in <a href="/preferences">Preferences</a> — keys stay in your browser.</div>
    <div class="k-ai-out" id="kAiOut" hidden></div>
    <button class="k-ai-go" id="kAiGo" type="button" data-q="{}" data-ctx="{}">Generate</button>
  </div>
</section>
"#, esc(q), esc(&ctx_json))
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
        let mut meta = String::new();
        if let Some(d) = &r.published {
            if !d.is_empty() {
                meta.push_str(&format!(r#"<span class="k-r-date" data-d="{}"></span>"#, esc(d)));
            }
        }
        for (k, v) in &r.meta {
            if k == "engine" {
                meta.push_str(&format!(r#"<span class="k-r-eng">{}</span>"#, esc(v)));
            } else {
                meta.push_str(&format!(r#"<span class="k-r-kv"><b>{}</b> {}</span>"#, esc(k), esc(v)));
            }
        }
        o.push_str(&format!(r#"<article class="k-r">
  <div class="k-r-head">{av}<a class="k-r-url" href="{url}" rel="noopener noreferrer">{src}</a></div>
  <a class="k-r-title" href="{url}" rel="noopener noreferrer">{title}</a>
  <p class="k-r-desc">{desc}</p>
  <div class="k-r-meta">{meta}</div>
</article>
"#,
        url = esc(&r.url), src = esc(&r.source), title = esc(&r.title), desc = esc(&r.desc)));
    }
    o.push_str("</div>");
    o
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
        let mut lic = String::new();
        for (k, v) in &r.meta {
            if k == "license" { lic = v.clone(); }
        }
        o.push_str(&format!(r#"<a class="k-img" href="{}" rel="noopener noreferrer" target="_blank">
  <img src="{thumb}" loading="lazy" alt="{}" onerror="this.closest('.k-img').classList.add('k-img-dead')">
  <span class="k-img-res">{res}</span>
  <span class="k-img-src">{}{lic_s}</span>
</a>
"#,
        esc(&r.url), esc(&r.title), esc(&r.source),
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
            format!(r#"<img src="{}" loading="lazy" alt="">"#, thumb)
        };
        o.push_str(&format!(
            r#"<a class="k-vid" href="{}" rel="noopener noreferrer" target="_blank">
  <span class="k-vid-thumb">{thumb_html}<span class="k-vid-dur">{}</span></span>
  <span class="k-vid-title">{}</span>
  <span class="k-vid-by">{}</span>
</a>
"#,
            esc(&r.url), esc(&dur), esc(&r.title), esc(&by)));
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
        img = if thumb.is_empty() { String::new() } else { format!(r#"<img src="/img?u={}" loading="lazy" alt="">"#, urlencode(&thumb)) }));
    }
    o.push_str("</div>");
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
        let coords = match (r.lat, r.lon) {
            (Some(la), Some(lo)) => format!(r#"<span class="k-map-geo">{:.4}, {:.4}</span>"#, la, lo),
            _ => String::new(),
        };
        o.push_str(&format!(r#"<article class="k-map">
  <a class="k-map-title" href="{}" rel="noopener noreferrer" target="_blank">{}</a>
  <p class="k-map-addr">{}</p>
  <div class="k-r-meta">{meta}{coords}</div>
</article>
"#, esc(&r.url), esc(&r.title), esc(&r.desc)));
    }
    o.push_str("</div>");
    o
}

fn pagination(out: &SearchOutcome, p: &SearchParams) -> String {
    let all_paged = {
        let engines = engines_for(p);
        !engines.is_empty() && engines.iter().all(|e| e.paged)
    };
    let next_disabled = !all_paged || out.results.len() < 4;
    let prev = if p.page > 1 {
        format!(r#"<a class="k-pg-btn" href="{}">Previous</a>"#,
            qs("/search", &p.query, p.category.as_str(), &p.language, &p.time, p.safesearch, p.page - 1))
    } else {
        r#"<span class="k-pg-btn k-pg-off">Previous</span>"#.to_string()
    };
    let next = if next_disabled {
        r#"<span class="k-pg-btn k-pg-off">Next</span>"#.to_string()
    } else {
        format!(r#"<a class="k-pg-btn" href="{}">Next</a>"#,
            qs("/search", &p.query, p.category.as_str(), &p.language, &p.time, p.safesearch, p.page + 1))
    };
    format!(r#"<nav class="k-pg" aria-label="Pagination">{prev}<span class="k-pg-n">Page {page}</span>{next}</nav>"#,
        page = p.page)
}

// ---------------------------------------------------------------------------
// preferences
// ---------------------------------------------------------------------------

pub fn prefs_page() -> String {
    let mut o = layout_open("Preferences — Kuroka", "KurokaSearch preferences", "k-page-prefs");
    o.push_str(r#"<header class="k-hdr k-hdr-prefs">
  <a class="k-brand" href="/" aria-label="Kuroka home"><img src="/static/brand/logomark-48.png" alt="" width="28" height="28"><span>Kuroka</span></a>
</header>
<main class="k-prefs">
  <h1>Preferences</h1>
  <p class="k-prefs-sub">Settings live in <strong>your browser</strong> (localStorage). Nothing is sent to or stored on the server.</p>

  <section class="k-p-sec">
    <h2>Search</h2>
    <div class="k-p-row"><label for="p-lang">Language</label><select id="p-lang" class="k-sel"></select></div>
    <div class="k-p-row"><label for="p-time">Time range</label><select id="p-time" class="k-sel"></select></div>
    <div class="k-p-row"><label for="p-ss">SafeSearch</label><select id="p-ss" class="k-sel"></select></div>
    <div class="k-p-row"><label for="p-cat">Default category</label><select id="p-cat" class="k-sel"></select></div>
    <div class="k-p-row"><label for="p-newtab">Open results in new tab</label><input type="checkbox" id="p-newtab" class="k-p-check"></div>
    <div class="k-p-row"><label for="p-ac">Autocomplete</label><input type="checkbox" id="p-ac" class="p-check k-p-check" checked></div>
  </section>

  <section class="k-p-sec" id="ai">
    <h2>AI Report <span class="k-p-note">bring your own key — it never leaves your browser</span></h2>
    <div class="k-p-row"><label for="p-ai-prov">Provider</label><select id="p-ai-prov" class="k-sel"></select></div>
    <div class="k-p-row"><label for="p-ai-model">Model</label><input type="text" id="p-ai-model" class="k-p-in" placeholder="gemini-flash-lite-latest" autocomplete="off"></div>
    <div class="k-p-row"><label for="p-ai-key">API key</label><input type="password" id="p-ai-key" class="k-p-in" placeholder="paste key — stored locally" autocomplete="off"></div>
    <div class="k-p-row"><label for="p-ai-base">Base URL <span class="k-p-note">(custom providers)</span></label><input type="text" id="p-ai-base" class="k-p-in" placeholder="https://…/v1" autocomplete="off"></div>
  </section>

  <div class="k-p-actions">
    <button class="k-ai-go" id="p-save" type="button">Save preferences</button>
    <a class="k-pg-btn" href="/">Back to search</a>
  </div>
</main>
"#);
    o.push_str(&layout_close().replace("CHEV", chevron_svg()));
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
    o.push_str(&layout_close().replace("CHEV", chevron_svg()));
    o
}
