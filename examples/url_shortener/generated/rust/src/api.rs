// intentcode: ../../intent/api.icode
// spec-hash: bae9c16f62ea7fda  (sha256 prefix of: api.icode, repository.icode, utils.icode, types.icode)
// generated: claude-sonnet-5-5, by hand-driven compile, not reproducible
use std::collections::{HashMap, HashSet};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::repository::{Link, Repository, RepositoryError};
use crate::utils::{Clock, exponential, retry};

// intent: Api::LinkResponse
// JSON Schema in the spec: slug, target (uri), created (date-time), expires (date-time | null),
// all four required.
#[derive(Serialize)]
pub struct LinkResponse {
    slug: String,
    target: String,
    created: DateTime<Utc>,
    expires: Option<DateTime<Utc>>,
}

impl From<&Link> for LinkResponse {
    fn from(l: &Link) -> Self {
        LinkResponse {
            slug: l.slug.clone(),
            target: l.target.clone(),
            created: l.created,
            expires: l.expires,
        }
    }
}

// intent: Api
#[derive(Clone)]
pub struct AppState {
    pub repo: Repository,
    pub clock: Clock,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        // intent: Api::GET /url?slug, Api::POST /url
        .route("/url", get(get_info).post(post_url))
        // intent: Api::GET /{slug}
        .route("/{slug}", get(redirect))
        .with_state(state)
}

/// `Link.slug`: `4 <= len <= 12`.
fn valid_slug(slug: &str) -> bool {
    (4..=12).contains(&slug.chars().count())
}

/// The default expiry: `expires or 30d default`.
fn default_expiry(now: DateTime<Utc>) -> DateTime<Utc> {
    now + Duration::days(30)
}

fn expired_or_deleted(link: &Link, now: DateTime<Utc>) -> bool {
    link.deleted.is_some() || link.expires.is_some_and(|e| e <= now)
}

/// Status for a repository failure: `NotFound` is 404, `Unavailable` is 503 on every endpoint.
fn repo_status(e: RepositoryError) -> StatusCode {
    match e {
        RepositoryError::NotFound => StatusCode::NOT_FOUND,
        RepositoryError::Unavailable | RepositoryError::Duplicate => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// The shared part of both GETs: `@Repository.get(slug)` and the status table.
async fn lookup(state: &AppState, slug: &str) -> Result<Link, StatusCode> {
    if !valid_slug(slug) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let link = state.repo.get(slug).await.map_err(repo_status)?;
    if expired_or_deleted(&link, state.clock.now()) {
        return Err(StatusCode::GONE);
    }
    Ok(link)
}

// intent: Api::GET /url?slug
async fn get_info(State(state): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(slug) = q.get("slug") else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match lookup(&state, slug).await {
        Ok(link) => Json(LinkResponse::from(&link)).into_response(),
        Err(status) => status.into_response(),
    }
}

// intent: Api::GET /{slug}
async fn redirect(State(state): State<AppState>, Path(slug): Path<String>) -> Response {
    match lookup(&state, &slug).await {
        Ok(link) => (StatusCode::FOUND, [(header::LOCATION, link.target)]).into_response(),
        Err(status) => status.into_response(),
    }
}

/// `hash = [Hash the target using <hashing algorithm>]`
/// Filled hole: SHA-256, encoded as URL-safe base64 without padding (A-Z a-z 0-9 - _).
pub fn hash_of(target: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(target.as_bytes()).as_slice())
}

/// `hash[0:4]`, a half-open slice: the first 4 characters, the shortest valid slug.
const MIN_SLUG: usize = 4;
const MAX_SLUG: usize = 12;

/// Why the block of POST /url did not produce a link.
#[derive(Debug)]
enum ShortenError {
    Repo(RepositoryError),
    /// every substring of hash is the slug of a link with a different target
    NoFreeSlug,
}

impl From<RepositoryError> for ShortenError {
    fn from(e: RepositoryError) -> Self {
        ShortenError::Repo(e)
    }
}

/// The block of POST /url, run once:
/// query, then the existing link is the answer, otherwise insert the smallest free slug.
async fn shorten_once(
    state: &AppState,
    target: &str,
    expires: Option<DateTime<Utc>>,
) -> Result<(StatusCode, Link), ShortenError> {
    let now = state.clock.now();
    let hash = hash_of(target);
    let links = state.repo.with_prefix(&hash[..MIN_SLUG]).await?;

    if let Some(existing) = links.iter().find(|l| l.target == target) {
        // the existing link is the answer; if it has expired or is soft deleted, renew it first
        let mut link = existing.clone();
        if expired_or_deleted(&link, now) {
            let new_expiry = expires.unwrap_or_else(|| default_expiry(now));
            state.repo.renew(&link.slug, new_expiry).await?;
            link.expires = Some(new_expiry);
            link.deleted = None;
        }
        return Ok((StatusCode::OK, link));
    }

    // slug = [smallest prefix of hash that is not used by another link]
    // smallest prefix of hash, the same family `with_prefix` was queried for.
    let taken: HashSet<&str> = links.iter().map(|l| l.slug.as_str()).collect();
    let slug = (MIN_SLUG..=MAX_SLUG)
        .map(|n| &hash[..n])
        .find(|candidate| !taken.contains(candidate))
        .ok_or(ShortenError::NoFreeSlug)?;

    let link = Link {
        slug: slug.to_string(),
        target: target.to_string(),
        created: now,
        expires: Some(expires.unwrap_or_else(|| default_expiry(now))),
        deleted: None,
    };
    state.repo.insert(link.clone()).await?;
    Ok((StatusCode::CREATED, link))
}

/// A valid `target`, see `data url` in types.icode: an http(s) url with a host.
fn valid_target(target: &str) -> bool {
    url::Url::parse(target).is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some())
}

/// Parses `{ "target": url, "expires"?: time }` (`data time`: an RFC 3339 string in JSON).
/// `expires` must be in the future. `None` means 400.
fn parse_body(body: &[u8], now: DateTime<Utc>) -> Option<(String, Option<DateTime<Utc>>)> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let object = value.as_object()?;
    let target = object.get("target")?.as_str()?;
    if !valid_target(target) {
        return None;
    }
    let expires = match object.get("expires") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => {
            let parsed = DateTime::parse_from_rfc3339(v.as_str()?).ok()?.with_timezone(&Utc);
            if parsed <= now {
                return None;
            }
            Some(parsed)
        }
    };
    Some((target.to_string(), expires))
}

// intent: Api::POST /url
async fn post_url(State(state): State<AppState>, body: Bytes) -> Response {
    let Some((target, expires)) = parse_body(&body, state.clock.now()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    // [ ... ] @retry(2^n*0.05s, 3 tries) on Duplicate
    // On Duplicate another request took the slug first; running the block again sees its link.
    let result = retry(
        exponential(0.05),
        3,
        |e: &ShortenError| matches!(e, ShortenError::Repo(RepositoryError::Duplicate)),
        || shorten_once(&state, &target, expires),
    )
    .await;

    match result {
        Ok((status, link)) => (status, Json(LinkResponse::from(&link))).into_response(),
        Err(ShortenError::NoFreeSlug) => StatusCode::CONFLICT.into_response(),
        // Unavailable, or still Duplicate after the retries
        Err(ShortenError::Repo(_)) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
