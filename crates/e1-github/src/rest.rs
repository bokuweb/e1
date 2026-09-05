//! The [`GitHub`] trait over GitHub's REST API.
//!
//! Blocking `ureq` over rustls, run by the caller on a background thread: no
//! async runtime, which is what lets this crate be linked into a host that has
//! its own (`docs/roadmap.md` E3). Listings follow the `Link: rel="next"`
//! header up to a page cap, because an unbounded walk of a large
//! repository's closed issues is a hang, not a feature.

use crate::auth::Token;
use crate::model::*;
use crate::wire::*;
use crate::{Error, GitHub, ListKind, Result, StatusFilter};
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use std::time::Duration;

/// Where GitHub's API is.
pub const API: &str = "https://api.github.com";

/// How many pages a listing walks before stopping. At a hundred per page this
/// is enough for any list a person scrolls, and bounded for the ones nobody
/// does.
const PAGE_CAP: usize = 3;

/// How long one request may take.
const TIMEOUT: Duration = Duration::from_secs(30);

/// GitHub over HTTPS.
pub struct Rest {
    agent: ureq::Agent,
    token: Token,
    base: String,
}

impl Rest {
    /// A client for `api.github.com` with this token.
    pub fn new(token: Token) -> Self {
        Self::with_base(token, API)
    }

    /// A client for another host, which is how a test or an enterprise
    /// instance would point it elsewhere.
    pub fn with_base(token: Token, base: impl Into<String>) -> Self {
        let config = ureq::Agent::config_builder()
            // Non-2xx answers are read for their `message` rather than thrown
            // away as an error with no body.
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .build();
        Self {
            agent: config.into(),
            token,
            base: base.into().trim_end_matches('/').to_string(),
        }
    }

    /// One GET, decoded. `path` is absolute (`/user`) or, for a `Link`
    /// continuation, a full URL.
    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<(T, Option<String>)> {
        let url = if path.starts_with("http") {
            path.to_string()
        } else {
            format!("{}{path}", self.base)
        };
        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.token.secret()))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "e1")
            .call()
            .map_err(|error| Error::Transport(error.to_string()))?;

        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        };
        let next = header("link").and_then(|link| next_link(&link));
        let remaining = header("x-ratelimit-remaining");
        let reset = header("x-ratelimit-reset");

        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|error| Error::Transport(error.to_string()))?;

        if (200..300).contains(&status) {
            let value =
                serde_json::from_str(&body).map_err(|error| Error::Decode(error.to_string()))?;
            return Ok((value, next));
        }
        if matches!(status, 403 | 429) && remaining.as_deref() == Some("0") {
            return Err(Error::RateLimited {
                reset: reset
                    .and_then(|epoch| epoch.parse::<i64>().ok())
                    .and_then(reset_at),
            });
        }
        let message = serde_json::from_str::<WireMessage>(&body)
            .map(|wire| wire.message)
            .unwrap_or_default();
        Err(Error::Status {
            status,
            path: path.to_string(),
            message,
        })
    }

    /// Every page of a listing, up to [`PAGE_CAP`].
    fn get_pages<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>> {
        let mut collected = Vec::new();
        let mut next = Some(path.to_string());
        let mut pages = 0;
        while let Some(url) = next.take() {
            if pages == PAGE_CAP {
                break;
            }
            pages += 1;
            let (page, following): (Vec<T>, _) = self.get(&url)?;
            collected.extend(page);
            next = following;
        }
        Ok(collected)
    }
}

/// The `next` URL out of a `Link` header, if it has one.
///
/// The header is a comma-separated list of `<url>; rel="name"` entries. Only
/// `next` is read: `last` would say how far the walk could go, and the cap
/// decides that instead.
pub fn next_link(header: &str) -> Option<String> {
    header.split(',').find_map(|entry| {
        let (url, params) = entry.split_once(';')?;
        if !params.contains("rel=\"next\"") {
            return None;
        }
        let url = url.trim().strip_prefix('<')?.strip_suffix('>')?;
        Some(url.to_string())
    })
}

/// A Unix timestamp as GitHub's `x-ratelimit-reset` sends it.
fn reset_at(epoch: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(epoch, 0)
}

/// Percent-encode a search query for the URL.
///
/// GitHub's search syntax is spaces and colons; only the characters that
/// would end or split the query need escaping, so a hand-rolled encoder is
/// smaller than a dependency and no less correct for this input.
fn encode_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for byte in query.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

impl GitHub for Rest {
    fn viewer(&self) -> Result<Viewer> {
        let (user, _): (WireUser, _) = self.get("/user")?;
        Ok(user.into())
    }

    fn notifications(&self) -> Result<Vec<Notification>> {
        let pages: Vec<WireNotification> = self.get_pages("/notifications?per_page=50")?;
        Ok(pages
            .into_iter()
            .filter_map(WireNotification::into_notification)
            .collect())
    }

    fn repositories(&self) -> Result<Vec<Repo>> {
        let pages: Vec<WireRepo> = self.get_pages(
            "/user/repos?sort=pushed&per_page=100&affiliation=owner,collaborator,organization_member",
        )?;
        Ok(pages.into_iter().filter_map(WireRepo::into_repo).collect())
    }

    fn items(&self, repo: &RepoId, kind: ListKind, status: StatusFilter) -> Result<Vec<Item>> {
        let state = status.as_query();
        match kind {
            ListKind::Pulls => {
                let path = format!(
                    "/repos/{repo}/pulls?state={state}&per_page=50&sort=updated&direction=desc"
                );
                let pages: Vec<WirePull> = self.get_pages(&path)?;
                Ok(pages.into_iter().map(|pull| pull.into_item(repo)).collect())
            }
            ListKind::Issues => {
                let path = format!(
                    "/repos/{repo}/issues?state={state}&per_page=50&sort=updated&direction=desc"
                );
                let pages: Vec<WireIssue> = self.get_pages(&path)?;
                Ok(pages
                    .into_iter()
                    .filter(|issue| !issue.is_pull())
                    .filter_map(|issue| issue.into_item(Some(repo)))
                    .collect())
            }
        }
    }

    fn search(&self, query: &str) -> Result<Vec<Item>> {
        let path = format!(
            "/search/issues?q={}&per_page=50&sort=updated&order=desc",
            encode_query(query)
        );
        let (search, _): (WireSearch, _) = self.get(&path)?;
        Ok(search
            .items
            .into_iter()
            .filter_map(|issue| issue.into_item(None))
            .collect())
    }

    fn item(&self, repo: &RepoId, number: u64) -> Result<Item> {
        let (issue, _): (WireIssue, _) = self.get(&format!("/repos/{repo}/issues/{number}"))?;
        if issue.is_pull() {
            // The issue view of a pull does not say whether it is a draft or
            // what it merges into; the pull endpoint does.
            return self.pull(repo, number).map(|pull| pull.item);
        }
        issue
            .into_item(Some(repo))
            .ok_or_else(|| Error::Decode("item without a repository".into()))
    }

    fn pull(&self, repo: &RepoId, number: u64) -> Result<Pull> {
        let (pull, _): (WirePull, _) = self.get(&format!("/repos/{repo}/pulls/{number}"))?;
        Ok(pull.into_pull(repo))
    }

    fn comments(&self, repo: &RepoId, number: u64) -> Result<Vec<Comment>> {
        let path = format!("/repos/{repo}/issues/{number}/comments?per_page=100");
        let pages: Vec<WireComment> = self.get_pages(&path)?;
        Ok(pages.into_iter().map(Into::into).collect())
    }

    fn pull_files(&self, repo: &RepoId, number: u64) -> Result<Vec<PullFile>> {
        let path = format!("/repos/{repo}/pulls/{number}/files?per_page=100");
        let pages: Vec<WirePullFile> = self.get_pages(&path)?;
        Ok(pages.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_next_link_is_read_out_of_the_header() {
        let header = r#"<https://api.github.com/user/repos?page=2>; rel="next", <https://api.github.com/user/repos?page=5>; rel="last""#;
        assert_eq!(
            next_link(header).as_deref(),
            Some("https://api.github.com/user/repos?page=2")
        );
    }

    #[test]
    fn a_last_page_has_no_next() {
        let header = r#"<https://api.github.com/user/repos?page=1>; rel="prev", <https://api.github.com/user/repos?page=1>; rel="first""#;
        assert_eq!(next_link(header), None);
        assert_eq!(next_link(""), None);
    }

    #[test]
    fn a_search_query_survives_the_url() {
        assert_eq!(
            encode_query("is:pr review-requested:@me state:open"),
            "is:pr+review-requested:%40me+state:open"
        );
    }

    #[test]
    fn the_reset_header_is_a_unix_timestamp() {
        assert_eq!(
            reset_at(1_788_566_400).map(|at| at.to_rfc3339()),
            Some("2026-09-05T00:00:00+00:00".to_string())
        );
    }
}
