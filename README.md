# KurokaSearch 🎃

**KurokaSearch** is a privacy-first, self-hostable **metasearch engine** written from
scratch in Rust. It fans a single query out to independent, public search APIs and
open endpoints, merges and de-duplicates the results, and renders them in a fast
black-and-gold interface.

- **No tracking** — no analytics, no fingerprinting, no session cookies
- **No logs** — queries are proxied through the server and never stored
- **No ads, no ranking games** — round-robin interleave keeps engine diversity
- **Bring your own AI** — optional AI summaries run in *your* browser with *your* API key
- **Single ~5 MB binary**, no runtime dependencies

## Categories & engines

| Category | Engines |
| --- | --- |
| All | Bing Web (RSS), Wikipedia |
| Images | Openverse, Wikimedia Commons |
| Videos | Dailymotion, Internet Archive |
| News | Google News RSS, Bing News RSS |
| Maps | OpenStreetMap Nominatim |
| Music | Apple Music (iTunes), MusicBrainz |
| IT | Stack Exchange, GitHub |
| Science | arXiv, Crossref |
| Files | PDF search |
| Social Media | site-scoped social search |

## Build & run

```bash
cargo build --release
KUROKA_ADDR=127.0.0.1:3000 ./target/release/kuroka-search
```

Open `http://127.0.0.1:3000/` — done. Static assets are served from `./static`
(override with `KUROKA_STATIC=/path/to/static`).

### Environment

| Variable | Default | Purpose |
| --- | --- | --- |
| `KUROKA_ADDR` | `127.0.0.1:3000` | Listen address |
| `KUROKA_STATIC` | `./static` | Static asset directory |

## How it works

```
browser ──▶ kuroka-search (tiny_http workers)
                ├── /search    ─▶ fan-out to N engine threads ─▶ merge + dedupe ─▶ HTML
                ├── /img       ─▶ image proxy (thumbnails never touch the client IP)
                ├── /ac        ─▶ autocomplete proxy
                └── /static    ─▶ css / js / brand assets
```

* Each engine runs in its own thread with its own timeout; slow or broken engines
  simply drop out of the page and are reported in the **Info** panel.
* Results are interleaved round-robin across engines so no single provider
  dominates the page, then de-duplicated by normalized URL.
* The AI Report is generated **client-side**: your provider key lives in
  `localStorage` and is sent only to the AI provider you choose.

## Project layout

```
src/main.rs        HTTP server: routing, static files, image & autocomplete proxies
src/engine.rs      models, engine registry, fan-out runner, HTTP/date/URL helpers
src/render.rs      server-side HTML rendering (no template engine, no frontend build)
src/engines/       one module per upstream engine
static/css         the Kuroka stylesheet
static/js          progressive-enhancement behaviors
static/brand       mascot, logomarks, favicons, PWA icons
```

## Privacy notes

* The server keeps **no request logs**; stderr only receives fatal errors.
* Outbound engine calls carry a generic browser User-Agent (or the project's
  contact UA for open APIs that ask for one) — never anything user-identifying.
* Thumbnail images are fetched by the server-side `/img` proxy so the client's
  IP address is never exposed to third-party CDNs.
* Preferences (language, SafeSearch, AI key, …) are stored **only** in the
  browser's `localStorage`.

## License

Copyright 2026 Abdullayews.
Released under the [GNU AGPL-3.0-or-later](LICENSE).
