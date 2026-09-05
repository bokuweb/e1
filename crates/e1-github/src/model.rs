//! What a view draws.
//!
//! One type per thing on screen, shaped for drawing rather than for the wire:
//! the wire's shapes stop in `wire.rs`, and a view that read them directly
//! would be re-deciding what to show every time GitHub grew a field.

use chrono::{DateTime, Utc};
use std::fmt;

/// `owner/name`: the key every repository-scoped thing hangs off.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepoId {
    /// The user or organisation.
    pub owner: String,
    /// The repository.
    pub name: String,
}

impl RepoId {
    /// Build one from its two halves.
    pub fn new(owner: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            name: name.into(),
        }
    }

    /// Parse `owner/name`. `None` when there is not exactly one slash with
    /// something either side of it.
    pub fn parse(full_name: &str) -> Option<Self> {
        let (owner, name) = full_name.split_once('/')?;
        if owner.is_empty() || name.is_empty() || name.contains('/') {
            return None;
        }
        Some(Self::new(owner, name))
    }

    /// Parse the repository out of an API URL such as
    /// `https://api.github.com/repos/owner/name/pulls/12`.
    pub fn from_api_url(url: &str) -> Option<Self> {
        let rest = url.split("/repos/").nth(1)?;
        let mut parts = rest.split('/');
        let owner = parts.next()?;
        let name = parts.next()?;
        if owner.is_empty() || name.is_empty() {
            return None;
        }
        Some(Self::new(owner, name))
    }

    /// Where the repository lives on the web.
    pub fn html_url(&self) -> String {
        format!("https://github.com/{self}")
    }
}

impl fmt::Display for RepoId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// A repository the viewer can reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// Its key.
    pub id: RepoId,
    /// The one-line description, when there is one.
    pub description: Option<String>,
    /// Private repositories are marked in the sidebar.
    pub private: bool,
    /// What a pull request targets unless told otherwise.
    pub default_branch: String,
    /// Stargazers.
    pub stars: u64,
    /// Open issues and pulls together, as GitHub counts them.
    pub open_issues: u64,
    /// The last push, which is what the sidebar sorts by.
    pub pushed_at: Option<DateTime<Utc>>,
    /// Where it lives on the web.
    pub html_url: String,
}

/// A GitHub account, as it appears on an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    /// The login, which is what is shown; GitHub does not send display names
    /// on items.
    pub login: String,
    /// The avatar, for when avatars are drawn (M5).
    pub avatar_url: String,
}

/// Who the token is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewer {
    /// The login, used in the sidebar and in search queries (`author:login`).
    pub login: String,
    /// The display name, when set.
    pub name: Option<String>,
    /// The avatar.
    pub avatar_url: String,
}

/// A label on an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// The text.
    pub name: String,
    /// Six hex digits without a `#`, as GitHub sends it.
    pub color: String,
}

/// What kind of thing a notification is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubjectKind {
    /// A pull request.
    PullRequest,
    /// An issue.
    Issue,
    /// A release.
    Release,
    /// A discussion.
    Discussion,
    /// A commit.
    Commit,
    /// A check suite.
    CheckSuite,
    /// Something this app does not draw specially; the wire's own name.
    Other(String),
}

impl SubjectKind {
    /// Map GitHub's `subject.type`.
    pub fn parse(kind: &str) -> Self {
        match kind {
            "PullRequest" => Self::PullRequest,
            "Issue" => Self::Issue,
            "Release" => Self::Release,
            "Discussion" => Self::Discussion,
            "Commit" => Self::Commit,
            "CheckSuite" => Self::CheckSuite,
            other => Self::Other(other.to_string()),
        }
    }
}

/// One inbox row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// GitHub's thread id, which is what marking it read will take.
    pub id: String,
    /// Whether it is unread.
    pub unread: bool,
    /// Why it is in the inbox: `review_requested`, `mention`, `subscribed`…
    pub reason: String,
    /// When the thread last moved.
    pub updated_at: DateTime<Utc>,
    /// The repository it belongs to.
    pub repo: RepoId,
    /// The subject's title.
    pub title: String,
    /// What the subject is.
    pub kind: SubjectKind,
    /// The pull or issue number, when the subject has one.
    pub number: Option<u64>,
}

impl Notification {
    /// Where the subject lives on the web, when this app can say.
    ///
    /// GitHub only sends the API URL of the subject; the web URL is derived,
    /// and only for the kinds whose web path is known.
    pub fn html_url(&self) -> Option<String> {
        let number = self.number?;
        let segment = match self.kind {
            SubjectKind::PullRequest => "pull",
            SubjectKind::Issue => "issues",
            SubjectKind::Discussion => "discussions",
            _ => return None,
        };
        Some(format!("{}/{segment}/{number}", self.repo.html_url()))
    }
}

/// Whether an item is a pull or an issue, and what only a pull can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An issue.
    Issue,
    /// A pull request.
    Pull {
        /// Marked as a draft.
        draft: bool,
        /// Merged. The wire never says "merged": a merged pull is `closed`
        /// with `merged_at` set, and this is where that is decided.
        merged: bool,
    },
}

/// Open or closed, as the wire says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Open.
    Open,
    /// Closed, merged pulls included.
    Closed,
}

/// The state a row draws, which is the wire's status refined by the kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// An open issue or a ready pull.
    Open,
    /// A draft pull.
    Draft,
    /// A merged pull.
    Merged,
    /// A closed issue or an unmerged closed pull.
    Closed,
}

/// A pull request or an issue, as a list row and a detail header draw it.
///
/// One type for both because every list and every header draws them the same
/// way; the state glyph is the one difference, and [`Kind`] carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The repository.
    pub repo: RepoId,
    /// The number, unique within the repository across pulls and issues.
    pub number: u64,
    /// The title.
    pub title: String,
    /// Pull or issue.
    pub kind: Kind,
    /// Open or closed.
    pub status: Status,
    /// Who opened it.
    pub author: User,
    /// When it was opened.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
    /// How many comments it has, when the endpoint said (pull lists do not).
    pub comments: Option<u64>,
    /// Its labels.
    pub labels: Vec<Label>,
    /// Who it is assigned to.
    pub assignees: Vec<User>,
    /// Whose review is requested. Empty for an issue.
    pub requested_reviewers: Vec<User>,
    /// Where it lives on the web.
    pub html_url: String,
    /// The description, as markdown. Empty when none was written.
    pub body: String,
}

impl Item {
    /// The state a view draws.
    pub fn state(&self) -> State {
        match (self.kind, self.status) {
            (Kind::Pull { merged: true, .. }, _) => State::Merged,
            (Kind::Pull { draft: true, .. }, Status::Open) => State::Draft,
            (_, Status::Open) => State::Open,
            (_, Status::Closed) => State::Closed,
        }
    }

    /// Whether this is a pull request.
    pub fn is_pull(&self) -> bool {
        matches!(self.kind, Kind::Pull { .. })
    }
}

/// A pull request with what only a pull has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pull {
    /// Everything it shares with an issue.
    pub item: Item,
    /// The branch being merged.
    pub head: String,
    /// The branch it merges into.
    pub base: String,
    /// Lines added.
    pub additions: u64,
    /// Lines removed.
    pub deletions: u64,
    /// Files touched.
    pub changed_files: u64,
    /// Whether GitHub thinks it can be merged. `None` while GitHub is still
    /// computing it, which is the usual answer right after a push.
    pub mergeable: Option<bool>,
}

/// One entry of an item's timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// GitHub's id.
    pub id: u64,
    /// Who wrote it.
    pub author: User,
    /// When.
    pub created_at: DateTime<Utc>,
    /// The markdown.
    pub body: String,
    /// Where it lives on the web.
    pub html_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: Kind, status: Status) -> Item {
        Item {
            repo: RepoId::new("o", "r"),
            number: 1,
            title: String::new(),
            kind,
            status,
            author: User {
                login: "a".into(),
                avatar_url: String::new(),
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
            comments: None,
            labels: Vec::new(),
            assignees: Vec::new(),
            requested_reviewers: Vec::new(),
            html_url: String::new(),
            body: String::new(),
        }
    }

    #[test]
    fn a_merged_pull_is_merged_even_though_the_wire_says_closed() {
        let pull = item(
            Kind::Pull {
                draft: false,
                merged: true,
            },
            Status::Closed,
        );
        assert_eq!(pull.state(), State::Merged);
    }

    #[test]
    fn a_closed_pull_that_was_not_merged_is_closed() {
        let pull = item(
            Kind::Pull {
                draft: false,
                merged: false,
            },
            Status::Closed,
        );
        assert_eq!(pull.state(), State::Closed);
    }

    #[test]
    fn a_draft_is_only_a_draft_while_it_is_open() {
        let open = item(
            Kind::Pull {
                draft: true,
                merged: false,
            },
            Status::Open,
        );
        assert_eq!(open.state(), State::Draft);
        let closed = item(
            Kind::Pull {
                draft: true,
                merged: false,
            },
            Status::Closed,
        );
        assert_eq!(closed.state(), State::Closed);
    }

    #[test]
    fn an_issue_is_open_or_closed_and_nothing_else() {
        assert_eq!(item(Kind::Issue, Status::Open).state(), State::Open);
        assert_eq!(item(Kind::Issue, Status::Closed).state(), State::Closed);
    }

    #[test]
    fn repo_ids_parse_from_the_shapes_github_sends() {
        assert_eq!(
            RepoId::parse("bokuweb/e1"),
            Some(RepoId::new("bokuweb", "e1"))
        );
        assert_eq!(RepoId::parse("bokuweb"), None);
        assert_eq!(RepoId::parse("a/b/c"), None);
        assert_eq!(RepoId::parse("/b"), None);
        assert_eq!(
            RepoId::from_api_url("https://api.github.com/repos/bokuweb/e1/pulls/12"),
            Some(RepoId::new("bokuweb", "e1"))
        );
        assert_eq!(
            RepoId::from_api_url("https://api.github.com/repos/bokuweb/e1"),
            Some(RepoId::new("bokuweb", "e1"))
        );
        assert_eq!(RepoId::from_api_url("https://api.github.com/user"), None);
    }

    #[test]
    fn a_notification_knows_where_its_subject_lives_only_for_kinds_with_a_web_path() {
        let mut notification = Notification {
            id: "1".into(),
            unread: true,
            reason: "mention".into(),
            updated_at: Utc::now(),
            repo: RepoId::new("bokuweb", "e1"),
            title: "t".into(),
            kind: SubjectKind::PullRequest,
            number: Some(7),
        };
        assert_eq!(
            notification.html_url().as_deref(),
            Some("https://github.com/bokuweb/e1/pull/7")
        );
        notification.kind = SubjectKind::Issue;
        assert_eq!(
            notification.html_url().as_deref(),
            Some("https://github.com/bokuweb/e1/issues/7")
        );
        notification.kind = SubjectKind::Commit;
        assert_eq!(notification.html_url(), None);
    }
}

/// What happened to a file in a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// New in this pull.
    Added,
    /// Deleted by this pull.
    Removed,
    /// Changed in place.
    Modified,
    /// Moved, possibly changed too.
    Renamed,
    /// Anything else GitHub reports (`copied`, `changed`, `unchanged`).
    Other,
}

impl FileStatus {
    /// Map GitHub's `status`.
    pub fn parse(status: &str) -> Self {
        match status {
            "added" => Self::Added,
            "removed" => Self::Removed,
            "modified" => Self::Modified,
            "renamed" => Self::Renamed,
            _ => Self::Other,
        }
    }

    /// The one-letter mark a file list leads with.
    pub fn letter(self) -> &'static str {
        match self {
            Self::Added => "A",
            Self::Removed => "D",
            Self::Modified => "M",
            Self::Renamed => "R",
            Self::Other => "·",
        }
    }
}

/// One file of a pull request's diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullFile {
    /// The path after the pull.
    pub filename: String,
    /// The path before, when it moved.
    pub previous_filename: Option<String>,
    /// What happened to it.
    pub status: FileStatus,
    /// Lines added.
    pub additions: u64,
    /// Lines removed.
    pub deletions: u64,
    /// The unified diff, without the `---`/`+++` header. `None` for a
    /// binary file or one too large for GitHub to send.
    pub patch: Option<String>,
}
