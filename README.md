# KurokaSearch 🎃

**KurokaSearch** is a privacy-first, self-hostable **metasearch engine** written from
scratch in Rust. It fans a single query out to independent, public search APIs and
open endpoints, merges and de-duplicates the results, and renders them in a fast
black-and-gold interface.

- **No tracking** — no analytics, no fingerprinting, no session cookies
- **No logs** — queries are proxied through the server and never stored
- **No ads, no ranking games** — round-robin interleave keeps engine diversity
- **Bring your own AI** — optional AI summaries run in *your* browser with *your* API key
- **Transparent by default** — true-black AMOLED theme, and wallpaper-transparent
  under the Zen Internet extension
- **Single ~7 MB binary**, no runtime dependencies

## Categories & engines — 51 sources, no keys, no cookies

| Category | Engines |
| --- | --- |
| All | Google*, Bing Web, DuckDuckGo, Wikipedia, Wikidata |
| Images | Bing Images, Openverse, Wikimedia Commons, DuckDuckGo Images (off) |
| Videos | YouTube, DuckDuckGo, Dailymotion, Odysee, Internet Archive |
| News | Google News, Bing News, DuckDuckGo News, Yahoo News |
| Maps | OpenStreetMap (Nominatim), Photon, live Google Maps embed |
| Music | Apple Music (iTunes), MusicBrainz, Mixcloud, Archive.org Audio, Deezer (off) |
| IT | Stack Overflow, GitHub, GitLab, Hacker News, npm, crates.io, MDN, Docker Hub, Hugging Face |
| Science | arXiv, Crossref, PubMed, Europe PMC, Zenodo, DOAJ, ERIC |
| Files | Anna's Archive, LibGen, Nyaa, SolidTorrents, The Pirate Bay, Project Gutenberg, PDF search (off) |
| Social Media | YouTube, Hacker News, Lemmy, Reddit (off), site-scoped social sweep (x.com, instagram, tiktok, facebook, reddit threads via Bing's index) |
| Onion | Ahmia (Tor hidden services) |

\* Google Web: Google serves datacenter IPs a "JavaScript required" shell.
Behind `KUROKA_PROXY` (any residential exit) the same engine parses real
results; without a proxy it detects the wall once, remembers it for ten
minutes and fails in 0 ms — never slowing the page down. The built-in
**auto-proxy pool** (below) gives every walled engine one extra attempt
through a live public exit per wall window. On home/residential connections
Google answers directly and no proxy is involved at all. Google results are
meanwhile fully live on News (RSS), Videos (YouTube) and Maps (embed).
Reddit and Deezer are IP-walled in the same way — Deezer is measurably
unlocked by the auto-proxy pool (~20% of public exits pass), while Reddit
responds to the pool plus one toggle in Settings → Engines.

## Build & run

```bash
cargo build --release
KUROKA_ADDR=127.0.0.1:3000 ./target/release/kuroka-search
```

Open `http://127.0.0.1:3000/` — done. Static assets are served from `./static`
(override with `KUROKA_STATIC=/path/to/static`). A `PORT` environment
variable (set by Render, Railway, Fly…) is honored automatically: the server
then binds `0.0.0.0:$PORT`.

## Deploy on Render (free tier)

1. Push this repo to your GitHub, then in the [Render dashboard](https://dashboard.render.com):
   **New → Web Service → connect the repo**.
2. Language: **Docker** — the `Dockerfile` at the repo root builds itself
   (the release build is capped to a single cargo job so it fits the free
   tier's 512 MB). Leave the start command empty; the image listens on
   `0.0.0.0:$PORT` (Render default 10000).
3. Health check path: `/healthz`.
4. Optional env vars (dashboard → Environment): `KUROKA_PROXY` with a
   residential proxy URL to unlock the IP-walled engines (below).

What to expect from a Render deployment: 48 of 51 engines work out of the
box — cloud IPs are datacenter IPs, and Google Web, Reddit and Deezer wall
those. The built-in auto-proxy pool recovers Deezer (~20% of public exits
pass it) and gives Google/Reddit one attempt per wall window; everything
else (Bing, DuckDuckGo, YouTube, Wikipedia, OpenStreetMap, …) is unaffected.
A `KUROKA_PROXY` residential exit restores the full 51. Self-hosters on a
home connection get all engines without any configuration — the wall is a
datacenter-IP phenomenon.

Free-tier notes: the service sleeps after 15 minutes without traffic and
wakes in about a minute on the next request; a free pinger such as
cron-job.org hitting `/healthz` every 10 minutes keeps it warm. The image
cache directory is ephemeral — it refills after each deploy.

### Environment

| Variable | Default | Purpose |
| --- | --- | --- |
| `KUROKA_ADDR` | `127.0.0.1:3000` | Listen address |
| `KUROKA_STATIC` | `./static` | Static asset directory |
| `KUROKA_CACHE` | `./cache/img` | Image/favicon proxy cache directory |
| `KUROKA_PROXY` | — | Route ALL engine traffic through an http/https/socks5 proxy (a residential exit unblocks Google, Reddit, Deezer). Wins over the auto pool. |
| `KUROKA_AUTO_PROXY` | `1` | Keyless auto-proxy pool for IP-walled engines (Google Web, Reddit, Deezer): fetches the hourly-rechecked [monosans/proxy-list](https://github.com/monosans/proxy-list) and the 5-minute [proxifly/free-proxy-list](https://github.com/proxifly/free-proxy-list) from GitHub, keeps up to 12 verified live exits, rotates on failure. `0` disables. Only walled engines ever touch it; all traffic is HTTPS, so an exit sees the domain but never the query. |

## How it works

```
browser ──▶ kuroka-search (tiny_http workers)
                ├── /search    ─▶ fan-out to N engine threads ─▶ merge + dedupe ─▶ HTML
                ├── /img       ─▶ image proxy (thumbnails never touch the client IP)
                ├── /ac        ─▶ autocomplete proxy
                └── /static    ─▶ css / js / brand assets
```

* Each engine runs in its own thread with its own timeout; slow or broken engines
  simply drop out of the page and are reported in the **Extras → Engines on this search** panel.
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
