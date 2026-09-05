//! GitHub, as this app sees it.
//!
//! The data model, the [`GitHub`] trait every view reaches the network
//! through, its REST implementation, the token discovery that feeds it, and a
//! scripted fake for tests and for running without a network. Nothing here
//! knows about GPUI: this crate is what a host that owns its own state would
//! implement the trait against (`docs/roadmap.md` §4.3, E2).

pub mod auth;
pub mod model;
pub mod rest;
pub mod scripted;
mod wire;

pub use auth::{Source, Token};
pub use model::*;
pub use rest::Rest;
pub use scripted::Scripted;

use chrono::{DateTime, Utc};

/// Why a request did not produce an answer.
///
/// Typed rather than stringly so a view can tell "sign in" from "try again
/// later" from "this is a bug": the first two are states it draws, the third
/// is a log line.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Nothing supplied a token; see [`auth::discover`] for where one is
    /// looked for.
    #[error("no GitHub token: set E1_GITHUB_TOKEN or GITHUB_TOKEN, or run `gh auth login`")]
    NoToken,
    /// GitHub answered with a status this app does not treat as success.
    #[error("GitHub answered {status} for {path}: {message}")]
    Status {
        /// The HTTP status.
        status: u16,
        /// The request path, without the host.
        path: String,
        /// GitHub's own `message`, when the body carried one.
        message: String,
    },
    /// The token's rate limit is exhausted until `reset`.
    #[error("rate limited by GitHub")]
    RateLimited {
        /// When the window opens again, when GitHub said.
        reset: Option<DateTime<Utc>>,
    },
    /// The request never got an answer: DNS, TLS, a timeout.
    #[error("could not reach GitHub: {0}")]
    Transport(String),
    /// GitHub answered, and the answer was not the shape this app expects.
    #[error("could not read GitHub's answer: {0}")]
    Decode(String),
    /// The implementation in use cannot do this. A host that proxies through
    /// its own daemon may lag behind the trait, and a view must be able to
    /// draw that rather than panic.
    #[error("this GitHub source cannot {0}")]
    Unsupported(&'static str),
}

/// The crate's result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Which of a repository's lists is wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListKind {
    /// Pull requests.
    Pulls,
    /// Issues, with pull requests filtered out: GitHub's issues endpoint
    /// returns both, and a list that said "issues" and showed pulls would
    /// be lying.
    Issues,
}

/// Which items of a list are wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StatusFilter {
    /// Open items only, which is what a list shows first.
    #[default]
    Open,
    /// Closed items only, merged pulls included.
    Closed,
    /// Everything.
    All,
}

impl StatusFilter {
    /// The value GitHub's `state` query parameter takes.
    pub fn as_query(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::All => "all",
        }
    }
}

/// Everything a view may ask of GitHub.
///
/// Blocking, and `Send + Sync`: views call it on the background executor and
/// never on the UI thread. Blocking rather than async is deliberate (roadmap
/// E2): a host that answers over its own socket can implement a blocking call
/// with `block_on`, whereas an async trait would commit both apps to one
/// executor. Each method is one screen's question, and each returns the model
/// type that screen draws.
pub trait GitHub: Send + Sync {
    /// Who the token belongs to.
    fn viewer(&self) -> Result<Viewer>;
    /// The unread inbox, newest first.
    fn notifications(&self) -> Result<Vec<Notification>>;
    /// The repositories the viewer can reach, most recently pushed first.
    fn repositories(&self) -> Result<Vec<Repo>>;
    /// A repository's pulls or issues, filtered by status, newest first.
    fn items(&self, repo: &RepoId, kind: ListKind, status: StatusFilter) -> Result<Vec<Item>>;
    /// Items matching a GitHub search query (`is:pr review-requested:@me`).
    fn search(&self, query: &str) -> Result<Vec<Item>>;
    /// One item by number, whether it is a pull or an issue.
    fn item(&self, repo: &RepoId, number: u64) -> Result<Item>;
    /// A pull with what only a pull has. `Error::Status(404)` for an issue.
    fn pull(&self, repo: &RepoId, number: u64) -> Result<Pull>;
    /// An item's comments, oldest first.
    fn comments(&self, repo: &RepoId, number: u64) -> Result<Vec<Comment>>;
    /// The files a pull changes, with their diffs.
    ///
    /// Defaulted to [`Error::Unsupported`] so a host implementation that
    /// lags behind the trait still compiles; the view draws that.
    fn pull_files(&self, repo: &RepoId, number: u64) -> Result<Vec<PullFile>> {
        let _ = (repo, number);
        Err(Error::Unsupported("list a pull's files"))
    }
}
