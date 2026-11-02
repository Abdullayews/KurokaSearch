// SPDX-License-Identifier: AGPL-3.0-or-later
//! Engine implementations. Each module exposes `search(&SearchParams) -> Result<Vec<SearchResult>, String>`.

pub mod bing_web;
pub mod bing_news;
pub mod gnews;
pub mod wikipedia;
pub mod openverse;
pub mod commons;
pub mod dailymotion;
pub mod archiveorg;
pub mod nominatim;
pub mod itunes;
pub mod musicbrainz;
pub mod stackexchange;
pub mod github;
pub mod arxiv;
pub mod crossref;
pub mod bing_site;

use crate::engine::{SearchParams, SearchResult};

pub fn run(id: &str, p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    match id {
        "bing" => bing_web::search(p),
        "bing_news" => bing_news::search(p),
        "gnews" => gnews::search(p),
        "wikipedia" => wikipedia::search(p),
        "openverse" => openverse::search(p),
        "commons" => commons::search(p),
        "dailymotion" => dailymotion::search(p),
        "archiveorg" => archiveorg::search(p),
        "nominatim" => nominatim::search(p),
        "itunes" => itunes::search(p),
        "musicbrainz" => musicbrainz::search(p),
        "stackexchange" => stackexchange::search(p),
        "github" => github::search(p),
        "arxiv" => arxiv::search(p),
        "crossref" => crossref::search(p),
        "bing_files" => bing_site::search_files(p),
        "bing_social" => bing_site::search_social(p),
        other => Err(format!("unknown engine: {other}")),
    }
}
