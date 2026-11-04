// SPDX-License-Identifier: AGPL-3.0-or-later
//! Auto-proxy pool — keyless egress diversity for IP-walled engines.
//!
//! Research (2026-11, measured from a datacenter IP): of the long-lived,
//! self-updating public proxy sources, two stand out — both plain files on
//! GitHub's CDN, no signup, no API key, no rate limit:
//!   * monosans/proxy-list     — re-checked hourly since 2021 (MIT)
//!   * proxifly/free-proxy-list — refreshed every 5 minutes (MIT)
//! Measured against the walled engines:
//!   Deezer  ~20% of live exits pass  -> the pool genuinely unlocks it
//!   Google   0/100+ exits pass       -> JS-wall follows the exit IP; the
//!   Reddit   0 exits pass               pool still gets one honest attempt
//!                                        per wall-cache window (10 min)
//! Home users on residential IPs rarely need any of this — direct egress
//! works there; the wall is a datacenter-IP phenomenon.
//!
//! Privacy contract: the pool is used ONLY by engines whose direct egress
//! is IP-walled (`is_walled`). Everything travels over HTTPS, so a proxy
//! operator sees the target domain (SNI) but never the query or content.
//!
//! Precedence: `KUROKA_PROXY` (manual, routes everything) >
//! `KUROKA_AUTO_PROXY=0` (off) > auto pool for walled engines only.

use crate::engine::USER_AGENT;
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

/// engines allowed to fall back to the public pool: google, reddit, deezer
/// (wired at their call sites; extend there as walls move)

/// manual proxy wins over everything; pool stays off in that case
fn manual_proxy() -> Option<String> {
    match std::env::var("KUROKA_PROXY") {
        Ok(p) if !p.trim().is_empty() => Some(p.trim().to_string()),
        _ => None,
    }
}

pub fn enabled() -> bool {
    manual_proxy().is_none()
        && !matches!(std::env::var("KUROKA_AUTO_PROXY"), Ok(v) if v.trim() == "0")
}

// ---------------------------------------------------------------------------
// pool state
// ---------------------------------------------------------------------------

struct PoolInner {
    /// verified-alive proxy URLs, fastest first
    urls: Vec<String>,
    /// round-robin cursor — spreads load, not always the same exit
    next: usize,
    refreshed_at: Instant,
}

fn pool() -> &'static RwLock<Option<PoolInner>> {
    static P: OnceLock<RwLock<Option<PoolInner>>> = OnceLock::new();
    P.get_or_init(|| RwLock::new(None))
}

fn refresh_lock() -> &'static Mutex<()> {
    static M: OnceLock<Mutex<()>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(()))
}

/// next live candidate (round-robin), if the pool holds one
fn acquire() -> Option<String> {
    let guard = pool().read().unwrap_or_else(|e| e.into_inner());
    let inner = guard.as_ref()?;
    if inner.urls.is_empty() { return None; }
    let idx = inner.next % inner.urls.len();
    Some(inner.urls[idx].clone())
}

/// drop a candidate that failed at transport level; rotate to the next
fn mark_dead(url: &str) {
    let mut guard = pool().write().unwrap_or_else(|e| e.into_inner());
    if let Some(inner) = guard.as_mut() {
        let before = inner.urls.len();
        inner.urls.retain(|u| u != url);
        if inner.urls.len() != before {
            inner.next = inner.next.saturating_sub(1) % inner.urls.len().max(1);
        }
    }
}

fn pool_len() -> usize {
    let guard = pool().read().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().map(|i| i.urls.len()).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// list sources (fastest-first plain text, host:port per line)
// ---------------------------------------------------------------------------

const SOURCES: &[(&str, &str)] = &[
    // (url, scheme to prepend)
    ("https://raw.githubusercontent.com/monosans/proxy-list/main/proxies/socks5.txt", "socks5h"),
    ("https://raw.githubusercontent.com/monosans/proxy-list/main/proxies/http.txt", "http"),
    // second donor — used when the first answers empty (rare)
    ("https://raw.githubusercontent.com/proxifly/free-proxy-list/main/proxies/socks/data.txt", "socks5h"),
    ("https://raw.githubusercontent.com/proxifly/free-proxy-list/main/proxies/http/data.txt", "http"),
];

/// how many lines of each list enter verification (they are fastest-first)
const CANDIDATES_PER_SOURCE: usize = 28;
/// verified exits kept in the pool
const POOL_CAP: usize = 12;
/// full refresh cadence of the background thread
const REFRESH_EVERY: Duration = Duration::from_secs(900);

fn download_candidates() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (url, scheme) in SOURCES {
        if out.len() >= CANDIDATES_PER_SOURCE * 2 { break; }
        let body = match crate::engine::http_get(url, USER_AGENT, &[], Duration::from_secs(15)) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let got: usize = body.lines()
            .filter_map(|l| l.trim().split_whitespace().next())
            .filter(|l| l.contains(':') && !l.contains('/'))
            .take(CANDIDATES_PER_SOURCE)
            .map(|l| format!("{scheme}://{l}"))
            .inspect(|u| out.push(u.clone()))
            .count();
        let _ = got;
    }
    out
}

/// one candidate = one throwaway client; verified against Google's 204
/// endpoint so only CONNECT/socks-capable, actually-alive exits survive
fn verify(proxy_url: &str) -> Option<Duration> {
    let p = reqwest::Proxy::all(proxy_url).ok()?;
    let c = reqwest::blocking::Client::builder()
        .proxy(p)
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(4))
        .gzip(true)
        .build()
        .ok()?;
    let t0 = Instant::now();
    let resp = c.get("https://www.gstatic.com/generate_204")
        .header("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(5))
        .send()
        .ok()?;
    if !resp.status().is_success() { return None; }
    Some(t0.elapsed())
}

/// download + verify + store. One caller at a time; a concurrent second
/// caller simply returns while the first works.
pub fn refresh_once() {
    if !enabled() { return; }
    let Ok(_guard) = refresh_lock().try_lock() else { return };
    let candidates = download_candidates();
    if candidates.is_empty() { return; }

    // verification fan-out — every candidate gets its own short-lived thread
    let (tx, rx) = std::sync::mpsc::channel::<(String, Duration)>();
    for c in candidates {
        let tx = tx.clone();
        std::thread::spawn(move || {
            if let Some(lat) = verify(&c) {
                let _ = tx.send((c, lat));
            }
        });
    }
    drop(tx);
    let mut alive: Vec<(String, Duration)> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(7);
    while alive.len() < POOL_CAP {
        let Some(left) = deadline.checked_duration_since(Instant::now()) else { break };
        match rx.recv_timeout(left) {
            Ok((u, lat)) => alive.push((u, lat)),
            Err(_) => break,
        }
    }
    alive.sort_by_key(|(_, lat)| *lat);
    let urls: Vec<String> = alive.into_iter().map(|(u, _)| u).collect();

    let mut guard = pool().write().unwrap_or_else(|e| e.into_inner());
    // never shrink a warm pool to zero on a bad round — keep the old one
    if !urls.is_empty() {
        *guard = Some(PoolInner { urls, next: 0, refreshed_at: Instant::now() });
    }
}

/// background refresher — call once from main(); sleeps first so the web
/// server binds before any list traffic starts
pub fn spawn_refresher() {
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_secs(25));
            refresh_once();
            std::thread::sleep(REFRESH_EVERY);
        }
    });
}

/// lazy warm-up for the very first search (arrives before the 25 s mark)
pub fn ensure_warm() {
    if !enabled() { return; }
    let warm = {
        let guard = pool().read().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().map(|i| i.refreshed_at.elapsed() < REFRESH_EVERY).unwrap_or(false)
    };
    if !warm {
        std::thread::spawn(refresh_once);
    }
}

// ---------------------------------------------------------------------------
// pool fetch — used only by walled engines after their direct attempt fails
// ---------------------------------------------------------------------------

fn client_for(proxy_url: &str) -> Option<reqwest::blocking::Client> {
    let p = reqwest::Proxy::all(proxy_url).ok()?;
    reqwest::blocking::Client::builder()
        .proxy(p)
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .gzip(true)
        .build()
        .ok()
}

/// GET through up to two live pool exits; transport failures rotate the
/// pool. Returns status + body like `http_get_any_status`.
pub fn fetch_via(url: &str, ua: &str, extra: &[(&str, &str)], timeout: Duration)
    -> Result<(u16, String), String>
{
    for _ in 0..2 {
        let Some(px) = acquire() else { return Err("pool empty".into()) };
        let Some(c) = client_for(&px) else { mark_dead(&px); continue };
        let mut rb = c.get(url).timeout(timeout).header("User-Agent", ua);
        for (k, v) in extra { rb = rb.header(*k, *v); }
        match rb.send() {
            Ok(resp) => {
                let status = resp.status().as_u16();
                match resp.text() {
                    Ok(t) => return Ok((status, t)),
                    Err(e) => return Err(format!("read: {e}")),
                }
            }
            Err(_) => { mark_dead(&px); continue; }
        }
    }
    Err("pool exhausted".into())
}

/// JSON-flavoured helper for walled API engines: direct first, pool second.
/// A non-2xx direct answer (Reddit/Deezer answer 403 to datacenter IPs)
/// falls through to the pool instead of failing outright.
pub fn get_json_walled(url: &str, ua: &str, extra: &[(&str, &str)], timeout: Duration)
    -> Result<String, String>
{
    ensure_warm();
    match crate::engine::http_get_any_status(url, ua, extra, timeout) {
        Ok((status, body)) if (200..300).contains(&status) => return Ok(body),
        direct => {
            if let Ok((status, body)) = fetch_via(url, ua, extra, timeout) {
                if (200..300).contains(&status) && !body.is_empty() {
                    return Ok(body);
                }
            }
            // direct error is the more honest message when the pool also failed
            match direct {
                Ok((status, _)) => Err(format!("http {status}")),
                Err(e) => Err(e),
            }
        }
    }
}

/// diagnostics for /healthz
pub fn status_line() -> String {
    if !enabled() {
        return match manual_proxy() {
            Some(_) => "manual".into(),
            None => "off".into(),
        };
    }
    format!("auto:{} exits", pool_len())
}
