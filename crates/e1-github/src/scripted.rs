//! A [`GitHub`] that answers from memory.
//!
//! What tests are written against, and what `E1_DEMO=1` runs the window over,
//! so that neither needs a token or a network. It answers the same questions
//! the REST client does, including a small evaluator for the search queries
//! this app generates, so a section that lists "pulls waiting for my review"
//! can be tested end to end.

use crate::model::*;
use crate::{Error, GitHub, ListKind, Result, StatusFilter};
use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::sync::Mutex;

/// What only a pull carries, keyed by item.
#[derive(Debug, Clone)]
struct PullExtra {
    head: String,
    base: String,
    additions: u64,
    deletions: u64,
    changed_files: u64,
    mergeable: Option<bool>,
}

#[derive(Default)]
struct Data {
    viewer: Option<Viewer>,
    repos: Vec<Repo>,
    notifications: Vec<Notification>,
    items: Vec<Item>,
    pulls: HashMap<(RepoId, u64), PullExtra>,
    comments: HashMap<(RepoId, u64), Vec<Comment>>,
    files: HashMap<(RepoId, u64), Vec<PullFile>>,
    /// When set, every call fails with this. For testing the error states.
    failing: Option<String>,
}

/// GitHub from memory.
#[derive(Default)]
pub struct Scripted {
    data: Mutex<Data>,
}

/// Build a user from a login.
pub fn user(login: &str) -> User {
    User {
        login: login.to_string(),
        avatar_url: format!("https://avatars.githubusercontent.com/{login}"),
    }
}

impl Scripted {
    /// Nothing in it. Every listing is empty and there is no viewer.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Who the token is.
    pub fn with_viewer(self, login: &str, name: Option<&str>) -> Self {
        self.data.lock().unwrap().viewer = Some(Viewer {
            login: login.to_string(),
            name: name.map(str::to_string),
            avatar_url: format!("https://avatars.githubusercontent.com/{login}"),
        });
        self
    }

    /// A repository the viewer can reach.
    pub fn with_repo(self, repo: Repo) -> Self {
        self.data.lock().unwrap().repos.push(repo);
        self
    }

    /// An issue or a pull. A pull gets default branch and count details;
    /// use [`Scripted::with_pull`] to say what they are.
    pub fn with_item(self, item: Item) -> Self {
        self.data.lock().unwrap().items.push(item);
        self
    }

    /// A pull with its own details.
    pub fn with_pull(self, pull: Pull) -> Self {
        {
            let mut data = self.data.lock().unwrap();
            data.pulls.insert(
                (pull.item.repo.clone(), pull.item.number),
                PullExtra {
                    head: pull.head,
                    base: pull.base,
                    additions: pull.additions,
                    deletions: pull.deletions,
                    changed_files: pull.changed_files,
                    mergeable: pull.mergeable,
                },
            );
            data.items.push(pull.item);
        }
        self
    }

    /// The comments on an item.
    pub fn with_comments(self, repo: &RepoId, number: u64, comments: Vec<Comment>) -> Self {
        self.data
            .lock()
            .unwrap()
            .comments
            .insert((repo.clone(), number), comments);
        self
    }

    /// The files a pull changes.
    pub fn with_files(self, repo: &RepoId, number: u64, files: Vec<PullFile>) -> Self {
        self.data
            .lock()
            .unwrap()
            .files
            .insert((repo.clone(), number), files);
        self
    }

    /// An inbox row.
    pub fn with_notification(self, notification: Notification) -> Self {
        self.data.lock().unwrap().notifications.push(notification);
        self
    }

    /// Make every call fail, for testing what a view does with that.
    pub fn failing(self, message: &str) -> Self {
        self.data.lock().unwrap().failing = Some(message.to_string());
        self
    }

    /// A small, plausible account: two repositories, a handful of pulls and
    /// issues in each, an inbox, and comments. What `E1_DEMO=1` shows.
    pub fn sample() -> Self {
        let now = Utc::now();
        let e1 = RepoId::new("bokuweb", "e1");
        let ginka = RepoId::new("bokuweb", "ginka");
        let item = |repo: &RepoId,
                    number: u64,
                    title: &str,
                    kind: Kind,
                    status: Status,
                    author: &str,
                    hours: i64,
                    comments: u64,
                    labels: &[(&str, &str)]| Item {
            repo: repo.clone(),
            number,
            title: title.to_string(),
            kind,
            status,
            author: user(author),
            created_at: now - Duration::hours(hours + 24),
            updated_at: now - Duration::hours(hours),
            comments: Some(comments),
            labels: labels
                .iter()
                .map(|(name, color)| Label {
                    name: name.to_string(),
                    color: color.to_string(),
                })
                .collect(),
            assignees: Vec::new(),
            requested_reviewers: Vec::new(),
            html_url: format!(
                "https://github.com/{repo}/{}/{number}",
                if matches!(kind, Kind::Pull { .. }) {
                    "pull"
                } else {
                    "issues"
                }
            ),
            body: format!(
                "## Summary\n\nThis is *scripted* data for **{title}**.\n\n- one thing\n- another thing\n\n```rust\nfn main() {{ println!(\"hello\"); }}\n```"
            ),
        };
        let open = Kind::Pull {
            draft: false,
            merged: false,
        };
        let draft = Kind::Pull {
            draft: true,
            merged: false,
        };
        let merged = Kind::Pull {
            draft: false,
            merged: true,
        };
        let pull = |item: Item, head: &str, adds: u64, dels: u64, files: u64| Pull {
            item,
            head: head.to_string(),
            base: "main".to_string(),
            additions: adds,
            deletions: dels,
            changed_files: files,
            mergeable: Some(true),
        };
        let comment = |id: u64, author: &str, hours: i64, body: &str| Comment {
            id,
            author: user(author),
            created_at: now - Duration::hours(hours),
            body: body.to_string(),
            html_url: String::new(),
        };
        let file =
            |name: &str, status: FileStatus, adds: u64, dels: u64, patch: Option<&str>| PullFile {
                filename: name.to_string(),
                previous_filename: None,
                status,
                additions: adds,
                deletions: dels,
                patch: patch.map(str::to_string),
            };
        let patch = "@@ -1,6 +1,9 @@\n use gpui::*;\n \n-fn open(cx: &mut App) {\n-    let bounds = Bounds::centered(None, size(px(1440.), px(920.)), cx);\n+/// Open the window where the reader left it, or centred the first time.\n+fn open(cx: &mut App, remembered: Option<Bounds<Pixels>>) {\n+    let bounds = remembered\n+        .unwrap_or_else(|| Bounds::centered(None, size(px(1440.), px(920.)), cx));\n     cx.open_window(bounds, |window, cx| shell(window, cx))\n }\n@@ -20,3 +23,4 @@ impl Shell {\n     fn persist(&mut self) {\n         self.layout.write_into(&mut self.settings);\n+        self.settings.bounds = Some(self.bounds);\n     }";
        let repo = |id: &RepoId, description: &str, private: bool, hours: i64| Repo {
            id: id.clone(),
            description: Some(description.to_string()),
            private,
            default_branch: "main".to_string(),
            stars: 12,
            open_issues: 4,
            pushed_at: Some(now - Duration::hours(hours)),
            html_url: id.html_url(),
        };

        let mut reviewer_item = item(
            &ginka,
            12,
            "Start a chat before it has a workspace",
            open,
            Status::Open,
            "alice",
            2,
            3,
            &[("enhancement", "a2eeef")],
        );
        reviewer_item.requested_reviewers.push(user("bokuweb"));
        let mut assigned = item(
            &e1,
            3,
            "Window opens behind the terminal when launched from a shell",
            Kind::Issue,
            Status::Open,
            "carol",
            5,
            1,
            &[("bug", "d73a4a"), ("macos", "0e8a16")],
        );
        assigned.assignees.push(user("bokuweb"));

        Self::empty()
            .with_viewer("bokuweb", Some("bokuweb"))
            .with_repo(repo(&e1, "A native GitHub client on GPUI", false, 1))
            .with_repo(repo(&ginka, "An IDE-agnostic coding-agent orchestrator", true, 3))
            .with_pull(pull(item(&e1, 7, "Draw the inbox with the reason where the author would be", open, Status::Open, "bokuweb", 1, 0, &[]), "inbox-rows", 212, 18, 6))
            .with_pull(pull(item(&e1, 6, "Persist the column widths across a restart", draft, Status::Open, "bokuweb", 9, 2, &[("wip", "fbca04")]), "persist-layout", 88, 12, 3))
            .with_pull(pull(item(&e1, 5, "Seed the lock file from Ginka's", merged, Status::Closed, "bokuweb", 30, 1, &[]), "seed-lock", 3, 1, 1))
            .with_pull(pull(item(&e1, 4, "Try an async client", Kind::Pull { draft: false, merged: false }, Status::Closed, "dave", 50, 4, &[]), "async", 400, 12, 9))
            .with_item(assigned)
            .with_item(item(&e1, 2, "Labels should be readable in the light theme", Kind::Issue, Status::Open, "bokuweb", 20, 0, &[("design", "c5def5")]))
            .with_item(item(&e1, 1, "Decide the licence", Kind::Issue, Status::Closed, "bokuweb", 70, 6, &[]))
            .with_pull(pull(reviewer_item, "start-a-chat-before-it-has-a-workspace", 412, 38, 9))
            .with_pull(pull(item(&ginka, 11, "Add a project, and start a chat, from the window", merged, Status::Closed, "bokuweb", 26, 2, &[]), "add-project", 300, 40, 12))
            .with_item(item(&ginka, 9, "Terminal dock forgets its height", Kind::Issue, Status::Open, "erin", 4, 2, &[("bug", "d73a4a")]))
            .with_comments(&ginka, 12, vec![
                comment(1, "bokuweb", 3, "Looks right to me. One question: does the scratch worktree get cleaned up when the chat is archived?"),
                comment(2, "alice", 2, "Not yet — I'd rather land this and do the cleanup in the archive change, since that is where the rule lives.\n\n```rust\nfn archive(&mut self) { /* … */ }\n```"),
            ])
            .with_comments(&e1, 3, vec![comment(3, "bokuweb", 4, "Reproduced. `cx.activate(true)` after the window opens fixes it.")])
            .with_files(&e1, 6, vec![
                file("src/main.rs", FileStatus::Modified, 6, 2, Some(patch)),
                file("crates/e1-ui/src/settings.rs", FileStatus::Modified, 1, 0, Some("@@ -18,4 +18,5 @@ pub struct AppSettings {\n     pub right_panel_width: f32,\n+    pub bounds: Option<WindowBounds>,\n     pub locale: Option<String>,\n     pub last_repo: Option<String>,\n }")),
                file("assets/icons/window.svg", FileStatus::Added, 0, 0, None),
            ])
            .with_files(&ginka, 12, vec![
                file("src/shell.rs", FileStatus::Modified, 300, 30, Some(patch)),
                file("crates/ginka-core/src/project.rs", FileStatus::Modified, 80, 8, Some("@@ -1,3 +1,4 @@\n+//! Projects, and the scratch one a chat starts in.\n use std::path::PathBuf;\n \n pub struct Project {")),
                file("docs/ui.md", FileStatus::Modified, 32, 0, Some("@@ -40,2 +40,3 @@\n ## 3. Regions\n+A chat can start before it has a workspace.\n ")),
            ])
            .with_notification(Notification {
                id: "1".into(),
                unread: true,
                reason: "review_requested".into(),
                updated_at: now - Duration::hours(2),
                repo: ginka.clone(),
                title: "Start a chat before it has a workspace".into(),
                kind: SubjectKind::PullRequest,
                number: Some(12),
            })
            .with_notification(Notification {
                id: "2".into(),
                unread: true,
                reason: "assign".into(),
                updated_at: now - Duration::hours(5),
                repo: e1.clone(),
                title: "Window opens behind the terminal when launched from a shell".into(),
                kind: SubjectKind::Issue,
                number: Some(3),
            })
            .with_notification(Notification {
                id: "3".into(),
                unread: false,
                reason: "subscribed".into(),
                updated_at: now - Duration::hours(30),
                repo: e1,
                title: "v0.1.0".into(),
                kind: SubjectKind::Release,
                number: None,
            })
    }

    fn guard(&self) -> Result<std::sync::MutexGuard<'_, Data>> {
        let data = self.data.lock().unwrap();
        match &data.failing {
            Some(message) => Err(Error::Transport(message.clone())),
            None => Ok(data),
        }
    }
}

/// One clause of a search query this app generates.
#[derive(Debug, PartialEq, Eq)]
enum Clause<'a> {
    IsPull,
    IsIssue,
    IsOpen,
    IsClosed,
    Author(&'a str),
    Assignee(&'a str),
    ReviewRequested(&'a str),
    /// A clause this evaluator does not know: it matches nothing, so a test
    /// that reaches for a new one fails visibly.
    Unknown,
}

fn parse_query(query: &str) -> Vec<Clause<'_>> {
    query
        .split_whitespace()
        .map(|word| match word.split_once(':') {
            Some(("is", "pr")) => Clause::IsPull,
            Some(("is", "issue")) => Clause::IsIssue,
            Some(("is", "open")) | Some(("state", "open")) => Clause::IsOpen,
            Some(("is", "closed")) | Some(("state", "closed")) => Clause::IsClosed,
            Some(("author", who)) => Clause::Author(who),
            Some(("assignee", who)) => Clause::Assignee(who),
            Some(("review-requested", who)) => Clause::ReviewRequested(who),
            _ => Clause::Unknown,
        })
        .collect()
}

fn matches(item: &Item, clause: &Clause<'_>, viewer: Option<&str>) -> bool {
    let me = |who: &str| -> Option<String> {
        if who == "@me" {
            viewer.map(str::to_string)
        } else {
            Some(who.to_string())
        }
    };
    match clause {
        Clause::IsPull => item.is_pull(),
        Clause::IsIssue => !item.is_pull(),
        Clause::IsOpen => item.status == Status::Open,
        Clause::IsClosed => item.status == Status::Closed,
        Clause::Author(who) => me(who).as_deref() == Some(item.author.login.as_str()),
        Clause::Assignee(who) => me(who)
            .map(|who| item.assignees.iter().any(|user| user.login == who))
            .unwrap_or(false),
        Clause::ReviewRequested(who) => me(who)
            .map(|who| {
                item.requested_reviewers
                    .iter()
                    .any(|user| user.login == who)
            })
            .unwrap_or(false),
        Clause::Unknown => false,
    }
}

impl GitHub for Scripted {
    fn viewer(&self) -> Result<Viewer> {
        self.guard()?.viewer.clone().ok_or(Error::NoToken)
    }

    fn notifications(&self) -> Result<Vec<Notification>> {
        let mut rows = self.guard()?.notifications.clone();
        rows.sort_by_key(|item| std::cmp::Reverse(item.updated_at));
        Ok(rows)
    }

    fn repositories(&self) -> Result<Vec<Repo>> {
        let mut repos = self.guard()?.repos.clone();
        repos.sort_by_key(|repo| std::cmp::Reverse(repo.pushed_at));
        Ok(repos)
    }

    fn items(&self, repo: &RepoId, kind: ListKind, status: StatusFilter) -> Result<Vec<Item>> {
        let data = self.guard()?;
        let mut rows: Vec<Item> = data
            .items
            .iter()
            .filter(|item| &item.repo == repo)
            .filter(|item| match kind {
                ListKind::Pulls => item.is_pull(),
                ListKind::Issues => !item.is_pull(),
            })
            .filter(|item| match status {
                StatusFilter::Open => item.status == Status::Open,
                StatusFilter::Closed => item.status == Status::Closed,
                StatusFilter::All => true,
            })
            .cloned()
            .collect();
        rows.sort_by_key(|item| std::cmp::Reverse(item.updated_at));
        Ok(rows)
    }

    fn search(&self, query: &str) -> Result<Vec<Item>> {
        let data = self.guard()?;
        let viewer = data.viewer.as_ref().map(|viewer| viewer.login.as_str());
        let clauses = parse_query(query);
        let mut rows: Vec<Item> = data
            .items
            .iter()
            .filter(|item| clauses.iter().all(|clause| matches(item, clause, viewer)))
            .cloned()
            .collect();
        rows.sort_by_key(|item| std::cmp::Reverse(item.updated_at));
        Ok(rows)
    }

    fn item(&self, repo: &RepoId, number: u64) -> Result<Item> {
        self.guard()?
            .items
            .iter()
            .find(|item| &item.repo == repo && item.number == number)
            .cloned()
            .ok_or_else(|| Error::Status {
                status: 404,
                path: format!("/repos/{repo}/issues/{number}"),
                message: "Not Found".into(),
            })
    }

    fn pull(&self, repo: &RepoId, number: u64) -> Result<Pull> {
        let data = self.guard()?;
        let item = data
            .items
            .iter()
            .find(|item| &item.repo == repo && item.number == number && item.is_pull())
            .cloned();
        let extra = data.pulls.get(&(repo.clone(), number)).cloned();
        match (item, extra) {
            (Some(item), Some(extra)) => Ok(Pull {
                item,
                head: extra.head,
                base: extra.base,
                additions: extra.additions,
                deletions: extra.deletions,
                changed_files: extra.changed_files,
                mergeable: extra.mergeable,
            }),
            (Some(item), None) => Ok(Pull {
                item,
                head: "feature".into(),
                base: "main".into(),
                additions: 0,
                deletions: 0,
                changed_files: 0,
                mergeable: None,
            }),
            _ => Err(Error::Status {
                status: 404,
                path: format!("/repos/{repo}/pulls/{number}"),
                message: "Not Found".into(),
            }),
        }
    }

    fn comments(&self, repo: &RepoId, number: u64) -> Result<Vec<Comment>> {
        Ok(self
            .guard()?
            .comments
            .get(&(repo.clone(), number))
            .cloned()
            .unwrap_or_default())
    }

    fn pull_files(&self, repo: &RepoId, number: u64) -> Result<Vec<PullFile>> {
        Ok(self
            .guard()?
            .files
            .get(&(repo.clone(), number))
            .cloned()
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_answers_every_question_the_window_asks() {
        let github = Scripted::sample();
        let viewer = github.viewer().unwrap();
        assert_eq!(viewer.login, "bokuweb");
        assert_eq!(github.repositories().unwrap().len(), 2);
        assert!(github.notifications().unwrap().iter().any(|n| n.unread));

        let e1 = RepoId::new("bokuweb", "e1");
        let open_pulls = github
            .items(&e1, ListKind::Pulls, StatusFilter::Open)
            .unwrap();
        assert!(
            open_pulls
                .iter()
                .all(|item| item.is_pull() && item.status == Status::Open)
        );
        let closed = github
            .items(&e1, ListKind::Pulls, StatusFilter::Closed)
            .unwrap();
        assert!(closed.iter().any(|item| item.state() == State::Merged));
        let issues = github
            .items(&e1, ListKind::Issues, StatusFilter::All)
            .unwrap();
        assert!(issues.iter().all(|item| !item.is_pull()));

        let pull = github.pull(&RepoId::new("bokuweb", "ginka"), 12).unwrap();
        assert_eq!(pull.changed_files, 9);
        assert_eq!(github.comments(&pull.item.repo, 12).unwrap().len(), 2);
        let files = github.pull_files(&pull.item.repo, 12).unwrap();
        assert_eq!(files.len(), 3);
        assert!(files[0].patch.is_some());
    }

    #[test]
    fn the_search_evaluator_understands_the_queries_this_app_generates() {
        let github = Scripted::sample();
        let mine = github.search("is:pr author:@me is:open").unwrap();
        assert!(!mine.is_empty());
        assert!(
            mine.iter()
                .all(|item| item.author.login == "bokuweb" && item.is_pull())
        );

        let reviews = github.search("is:pr review-requested:@me is:open").unwrap();
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].number, 12);

        let assigned = github.search("is:issue assignee:@me is:open").unwrap();
        assert_eq!(assigned.len(), 1);
        assert_eq!(assigned[0].number, 3);
    }

    #[test]
    fn an_unknown_clause_matches_nothing_rather_than_everything() {
        let github = Scripted::sample();
        assert!(github.search("is:pr label:bug").unwrap().is_empty());
    }

    #[test]
    fn a_failing_source_fails_every_call() {
        let github = Scripted::sample().failing("offline");
        assert!(matches!(github.viewer(), Err(Error::Transport(_))));
        assert!(github.repositories().is_err());
    }

    #[test]
    fn an_empty_source_has_no_viewer() {
        assert!(matches!(Scripted::empty().viewer(), Err(Error::NoToken)));
    }
}
