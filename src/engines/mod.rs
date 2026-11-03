// SPDX-License-Identifier: AGPL-3.0-or-later
//! Engine implementations. Each module exposes `search(&SearchParams) -> Result<Vec<SearchResult>, String>`.

pub mod bing_web;
pub mod google;
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
pub mod solid;
pub mod nyaa;
pub mod libgen;
pub mod annas;
pub mod ahmia;
pub mod web_extra;
pub mod media_extra;
pub mod news_extra;
pub mod it_extra;
pub mod science_extra;
pub mod misc_extra;
pub mod ddg_media;
pub mod reddit;
pub mod deezer;
pub mod files_extra;

use crate::engine::{SearchParams, SearchResult};

pub fn run(id: &str, p: &SearchParams) -> Result<Vec<SearchResult>, String> {
    match id {
        "google" => google::search(p),
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
        "solid" => solid::search(p),
        "nyaa" => nyaa::search(p),
        "libgen" => libgen::search(p),
        "annas" => annas::search(p),
        "ahmia" => ahmia::search(p),
        // --- extras ---
        "ddg" => web_extra::ddg_search(p),
        "ddg_news" => web_extra::ddg_news(p),
        "wikidata" => web_extra::wikidata_search(p),
        "bing_images" => media_extra::bing_images(p),
        "youtube" => media_extra::youtube_search(p),
        "yahoo_news" => news_extra::yahoo_news(p),
        "hn" => it_extra::hn_search(p),
        "npm" => it_extra::npm_search(p),
        "crates" => it_extra::crates_search(p),
        "pubmed" => science_extra::pubmed_search(p),
        "europepmc" => science_extra::europepmc_search(p),
        "zenodo" => science_extra::zenodo_search(p),
        "doaj" => science_extra::doaj_search(p),
        "eric" => science_extra::eric_search(p),
        "apibay" => misc_extra::apibay_search(p),
        "mixcloud" => misc_extra::mixcloud_search(p),
        "lemmy" => misc_extra::lemmy_search(p),
        "photon" => misc_extra::photon_search(p),
        // --- task 21 additions ---
        "ddg_images" => ddg_media::ddg_images(p),
        "ddg_videos" => ddg_media::ddg_videos(p),
        "reddit" => reddit::search(p),
        "deezer" => deezer::search(p),
        // --- task 22 additions ---
        "mdn" => it_extra::mdn_search(p),
        "docker" => it_extra::docker_search(p),
        "huggingface" => it_extra::hf_search(p),
        "gitlab" => it_extra::gitlab_search(p),
        "odysee" => media_extra::odysee_search(p),
        "archive_audio" => archiveorg::search_audio(p),
        "gutenberg" => files_extra::gutenberg_search(p),
        other => Err(format!("unknown engine: {other}")),
    }
}
