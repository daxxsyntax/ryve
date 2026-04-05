// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Unsplash API client for background image search and download.

use std::path::{Path, PathBuf};

use serde::Deserialize;

// ── Types ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Photo {
    pub id: String,
    /// Small thumbnail URL (~200px, for search grid).
    pub thumb_url: String,
    /// Regular-size URL (~1080px, for download).
    pub regular_url: String,
    /// Photographer display name.
    pub photographer: String,
    /// Photographer profile URL on Unsplash.
    pub photographer_url: String,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub photos: Vec<Photo>,
    pub total_pages: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ── API response deserialization ─────────────────────

#[derive(Deserialize)]
struct ApiSearchResponse {
    results: Vec<ApiPhoto>,
    total_pages: u32,
}

#[derive(Deserialize)]
struct ApiPhoto {
    id: String,
    urls: ApiUrls,
    user: ApiUser,
}

#[derive(Deserialize)]
struct ApiUrls {
    thumb: String,
    regular: String,
}

#[derive(Deserialize)]
struct ApiUser {
    name: String,
    links: ApiUserLinks,
}

#[derive(Deserialize)]
struct ApiUserLinks {
    html: String,
}

// ── Public API ───────────────────────────────────────

const BASE_URL: &str = "https://api.unsplash.com";

/// Search Unsplash photos by query.
pub async fn search(api_key: &str, query: &str, page: u32) -> Result<SearchResult, Error> {
    let client = reqwest::Client::new();
    let resp: ApiSearchResponse = client
        .get(format!("{BASE_URL}/search/photos"))
        .header("Authorization", format!("Client-ID {api_key}"))
        .query(&[
            ("query", query),
            ("page", &page.to_string()),
            ("per_page", "12"),
            ("orientation", "landscape"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let photos = resp
        .results
        .into_iter()
        .map(|p| Photo {
            id: p.id,
            thumb_url: p.urls.thumb,
            regular_url: p.urls.regular,
            photographer: p.user.name,
            photographer_url: p.user.links.html,
        })
        .collect();

    Ok(SearchResult {
        photos,
        total_pages: resp.total_pages,
    })
}

/// Download a photo's regular-size image to the backgrounds directory.
/// Returns the filename (e.g. `"abc123.jpg"`).
///
/// Also triggers the Unsplash download endpoint per API guidelines.
pub async fn download(api_key: &str, photo: &Photo, dest_dir: &Path) -> Result<String, Error> {
    let client = reqwest::Client::new();

    // Trigger download tracking (Unsplash API requirement)
    let _ = client
        .get(format!("{BASE_URL}/photos/{}/download", photo.id))
        .header("Authorization", format!("Client-ID {api_key}"))
        .send()
        .await;

    // Download the actual image bytes
    let bytes = client
        .get(&photo.regular_url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    let filename = format!("{}.jpg", photo.id);
    let dest_path = dest_dir.join(&filename);
    tokio::fs::write(&dest_path, &bytes).await?;

    Ok(filename)
}

/// Download a thumbnail image and return its bytes (for preview in the picker).
pub async fn fetch_thumbnail_bytes(url: &str) -> Result<Vec<u8>, Error> {
    let bytes = reqwest::get(url).await?.error_for_status()?.bytes().await?;
    Ok(bytes.to_vec())
}

/// Resolve the full path for a background image filename.
pub fn background_path(backgrounds_dir: &Path, filename: &str) -> PathBuf {
    backgrounds_dir.join(filename)
}
