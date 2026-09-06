//! Everything fetched, and the fetching of it.
//!
//! One entity holds every answer GitHub has given this window, each as a
//! [`Fetch`] so a refresh keeps the old value on screen. Every call goes to
//! the background executor and comes back through `this.update`; nothing
//! here blocks the UI thread, and nothing but this file calls the trait.
//!
//! Two caches make it fast. The HTTP layer keeps answers with their
//! `ETag`s (`e1_github::HttpCache`), so a refresh GitHub answers with `304`
//! costs a round trip and no rate limit. And the store writes what it knows
//! to a [`Snapshot`] after every answer, and reads it back before the first
//! request on the next launch, so the window opens full and revalidates
//! rather than opening empty and waiting.

use e1_github::{
    FileContent, GitHub, Item, ListKind, Notification, PullFile, Repo, RepoId, Tree, Viewer,
};
use e1_ui::fetch::describe;
use e1_ui::snapshot::{self, ItemDetail, Snapshot};
use e1_ui::{Fetch, Focus, Section};
use gpui::{AppContext as _, Context, EventEmitter};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// The key of a detail: which item, in which repository.
pub type ItemKey = (RepoId, u64);

/// The key of a file: which path, in which repository.
pub type FileKey = (RepoId, String);

/// Everything the detail panel draws for one item.
pub type Detail = ItemDetail;

/// Emitted whenever an answer lands.
pub enum StoreEvent {
    /// Something changed; views re-read what they show.
    Changed,
}

/// The window's memory of GitHub.
pub struct Store {
    github: Arc<dyn GitHub>,
    viewer: Fetch<Viewer>,
    repos: Fetch<Vec<Repo>>,
    inbox: Fetch<Vec<Notification>>,
    lists: HashMap<Focus, Fetch<Vec<Item>>>,
    details: HashMap<ItemKey, Fetch<Detail>>,
    /// The order details were opened in, oldest first, for the snapshot.
    opened: Vec<ItemKey>,
    pull_files: HashMap<ItemKey, Fetch<Vec<PullFile>>>,
    trees: HashMap<RepoId, Fetch<Tree>>,
    contents: HashMap<FileKey, Fetch<FileContent>>,
    /// Where the snapshot is written, when it is.
    snapshot: Option<PathBuf>,
}

impl EventEmitter<StoreEvent> for Store {}

impl Store {
    /// A store over a source. Nothing is fetched until asked.
    pub fn new(github: Arc<dyn GitHub>) -> Self {
        Self {
            github,
            viewer: Fetch::Idle,
            repos: Fetch::Idle,
            inbox: Fetch::Idle,
            lists: HashMap::new(),
            details: HashMap::new(),
            opened: Vec::new(),
            pull_files: HashMap::new(),
            trees: HashMap::new(),
            contents: HashMap::new(),
            snapshot: None,
        }
    }

    /// Remember what lands at this path, and start from what is there.
    pub fn with_snapshot(mut self, path: PathBuf) -> Self {
        if let Some(snapshot) = snapshot::load(&path) {
            self.adopt(snapshot);
        }
        self.remembering(path)
    }

    /// Remember what lands at this path, without reading what is there:
    /// for a window that opens signed out, whose snapshot — if one survived
    /// — belongs to whoever was signed in before.
    pub fn remembering(mut self, path: PathBuf) -> Self {
        self.snapshot = Some(path);
        self
    }

    /// Take a snapshot's contents as what is on screen. They are `Ready`
    /// rather than stale so that the first refresh keeps them, the way a
    /// refresh keeps anything.
    fn adopt(&mut self, snapshot: Snapshot) {
        if let Some(viewer) = snapshot.viewer {
            self.viewer = Fetch::Ready(viewer);
        }
        if !snapshot.repos.is_empty() {
            self.repos = Fetch::Ready(snapshot.repos);
        }
        self.inbox = Fetch::Ready(snapshot.inbox);
        for (focus, items) in snapshot.lists {
            self.lists.insert(focus, Fetch::Ready(items));
        }
        for (key, detail) in snapshot.details {
            self.opened.push(key.clone());
            self.details.insert(key, Fetch::Ready(detail));
        }
    }

    /// What the next launch should open on.
    fn snapshot(&self) -> Snapshot {
        let mut snapshot = Snapshot::new();
        snapshot.viewer = self.viewer.value().cloned();
        snapshot.repos = self.repos.value().cloned().unwrap_or_default();
        snapshot.inbox = self.inbox.value().cloned().unwrap_or_default();
        snapshot.lists = self
            .lists
            .iter()
            .filter_map(|(focus, fetch)| fetch.value().map(|items| (focus.clone(), items.clone())))
            .collect();
        snapshot.details = self
            .opened
            .iter()
            .filter_map(|key| {
                self.details
                    .get(key)
                    .and_then(Fetch::value)
                    .map(|detail| (key.clone(), detail.clone()))
            })
            .collect();
        snapshot.trim();
        snapshot
    }

    /// Write the snapshot, off the UI thread.
    fn persist(&self, cx: &mut Context<Self>) {
        let Some(path) = self.snapshot.clone() else {
            return;
        };
        let snapshot = self.snapshot();
        cx.background_spawn(async move {
            if let Err(error) = snapshot::save(&path, &snapshot) {
                tracing::debug!(%error, "could not write the snapshot");
            }
        })
        .detach();
    }

    /// Who the token is.
    pub fn viewer(&self) -> &Fetch<Viewer> {
        &self.viewer
    }

    /// The repositories.
    pub fn repos(&self) -> &Fetch<Vec<Repo>> {
        &self.repos
    }

    /// The inbox.
    pub fn inbox(&self) -> &Fetch<Vec<Notification>> {
        &self.inbox
    }

    /// A list, if it has ever been asked for.
    pub fn list(&self, focus: &Focus) -> Option<&Fetch<Vec<Item>>> {
        self.lists.get(focus)
    }

    /// A detail, if it has ever been asked for.
    pub fn detail(&self, key: &ItemKey) -> Option<&Fetch<Detail>> {
        self.details.get(key)
    }

    /// A pull's files, if they have ever been asked for.
    pub fn pull_files(&self, key: &ItemKey) -> Option<&Fetch<Vec<PullFile>>> {
        self.pull_files.get(key)
    }

    /// A repository's tree, if it has ever been asked for.
    pub fn tree(&self, repo: &RepoId) -> Option<&Fetch<Tree>> {
        self.trees.get(repo)
    }

    /// A file, if it has ever been asked for.
    pub fn content(&self, key: &FileKey) -> Option<&Fetch<FileContent>> {
        self.contents.get(key)
    }

    /// Swap the source and forget everything the old one said.
    ///
    /// Signing in and out: a token change is a different GitHub, and a list
    /// fetched as one person must not be shown to the next. Everything is
    /// fetched again from the new source, and the snapshot goes too.
    pub fn set_source(&mut self, github: Arc<dyn GitHub>, cx: &mut Context<Self>) {
        self.github = github;
        self.viewer = Fetch::Idle;
        self.repos = Fetch::Idle;
        self.inbox = Fetch::Idle;
        self.lists.clear();
        self.details.clear();
        self.opened.clear();
        self.pull_files.clear();
        self.trees.clear();
        self.contents.clear();
        if let Some(path) = self.snapshot.clone() {
            cx.background_spawn(async move {
                if let Err(error) = snapshot::forget(&path) {
                    tracing::warn!(%error, "could not delete the snapshot");
                }
            })
            .detach();
        }
        self.refresh_all(cx);
        cx.emit(StoreEvent::Changed);
    }

    /// Fetch what the sidebar needs: the viewer, the repositories and the
    /// inbox count.
    pub fn refresh_all(&mut self, cx: &mut Context<Self>) {
        self.load_viewer(cx);
        self.load_repos(cx);
        self.load_inbox(cx);
    }

    /// Fetch the viewer.
    pub fn load_viewer(&mut self, cx: &mut Context<Self>) {
        self.viewer.begin();
        self.fetch(
            cx,
            |github| github.viewer(),
            |this, result| this.viewer.finish(result),
        );
    }

    /// Fetch the repositories.
    pub fn load_repos(&mut self, cx: &mut Context<Self>) {
        self.repos.begin();
        self.fetch(
            cx,
            |github| github.repositories(),
            |this, result| this.repos.finish(result),
        );
    }

    /// Fetch the inbox.
    pub fn load_inbox(&mut self, cx: &mut Context<Self>) {
        self.inbox.begin();
        self.fetch(
            cx,
            |github| github.notifications(),
            |this, result| this.inbox.finish(result),
        );
    }

    /// Fetch a list, whether or not it has been fetched before. The file
    /// finder is not a list; see [`Store::load_tree`].
    pub fn load_list(&mut self, focus: Focus, cx: &mut Context<Self>) {
        if focus == Focus::Section(Section::Inbox) {
            return self.load_inbox(cx);
        }
        if !focus.is_list() {
            return;
        }
        self.lists.entry(focus.clone()).or_default().begin();
        let key = focus.clone();
        self.fetch(
            cx,
            move |github| match &focus {
                Focus::Section(section) => github.search(section.query().unwrap_or_default()),
                Focus::Search { query } => github.search(query),
                Focus::Repo { repo, kind, status } => github.items(repo, *kind, *status),
                Focus::Files { .. } => Ok(Vec::new()),
            },
            move |this, result| {
                this.lists.entry(key).or_default().finish(result);
            },
        );
    }

    /// Fetch a list only if it never has been. What a view calls when it
    /// starts showing one: a list already on screen is not re-fetched by
    /// looking at it again.
    pub fn ensure_list(&mut self, focus: Focus, cx: &mut Context<Self>) {
        let idle = match &focus {
            Focus::Section(Section::Inbox) => self.inbox.is_idle(),
            other => self.lists.get(other).is_none_or(Fetch::is_idle),
        };
        if idle {
            self.load_list(focus, cx);
        }
    }

    /// Fetch an item, what only a pull has, and its comments, together.
    ///
    /// `is_pull` is a hint from the row that opened it: knowing saves the
    /// round trip that would otherwise find out. `None` asks.
    pub fn load_detail(&mut self, key: ItemKey, is_pull: Option<bool>, cx: &mut Context<Self>) {
        self.details.entry(key.clone()).or_default().begin();
        self.opened.retain(|opened| opened != &key);
        self.opened.push(key.clone());
        let (repo, number) = key.clone();
        self.fetch(
            cx,
            move |github| {
                let (item, pull) = match is_pull {
                    Some(true) => {
                        let pull = github.pull(&repo, number)?;
                        (pull.item.clone(), Some(pull))
                    }
                    Some(false) => (github.item(&repo, number)?, None),
                    None => {
                        let item = github.item(&repo, number)?;
                        let pull = if item.is_pull() {
                            Some(github.pull(&repo, number)?)
                        } else {
                            None
                        };
                        (item, pull)
                    }
                };
                let comments = github.comments(&repo, number)?;
                Ok(Detail {
                    item,
                    pull,
                    comments,
                })
            },
            move |this, result| {
                this.details.entry(key).or_default().finish(result);
            },
        );
    }

    /// Fetch a detail only if it never has been.
    pub fn ensure_detail(&mut self, key: ItemKey, is_pull: Option<bool>, cx: &mut Context<Self>) {
        if self.details.get(&key).is_none_or(Fetch::is_idle) {
            self.load_detail(key, is_pull, cx);
        }
    }

    /// Fetch a pull's files.
    pub fn load_pull_files(&mut self, key: ItemKey, cx: &mut Context<Self>) {
        self.pull_files.entry(key.clone()).or_default().begin();
        let (repo, number) = key.clone();
        self.fetch(
            cx,
            move |github| github.pull_files(&repo, number),
            move |this, result| {
                this.pull_files.entry(key).or_default().finish(result);
            },
        );
    }

    /// Fetch a pull's files only if they never have been.
    pub fn ensure_pull_files(&mut self, key: ItemKey, cx: &mut Context<Self>) {
        if self.pull_files.get(&key).is_none_or(Fetch::is_idle) {
            self.load_pull_files(key, cx);
        }
    }

    /// Fetch a repository's tree.
    pub fn load_tree(&mut self, repo: RepoId, cx: &mut Context<Self>) {
        self.trees.entry(repo.clone()).or_default().begin();
        let key = repo.clone();
        self.fetch(
            cx,
            move |github| github.tree(&repo),
            move |this, result| {
                this.trees.entry(key).or_default().finish(result);
            },
        );
    }

    /// Fetch a repository's tree only if it never has been.
    pub fn ensure_tree(&mut self, repo: RepoId, cx: &mut Context<Self>) {
        if self.trees.get(&repo).is_none_or(Fetch::is_idle) {
            self.load_tree(repo, cx);
        }
    }

    /// Fetch a file.
    pub fn load_content(&mut self, key: FileKey, cx: &mut Context<Self>) {
        self.contents.entry(key.clone()).or_default().begin();
        let (repo, path) = key.clone();
        self.fetch(
            cx,
            move |github| github.file(&repo, &path),
            move |this, result| {
                this.contents.entry(key).or_default().finish(result);
            },
        );
    }

    /// Fetch a file only if it never has been.
    pub fn ensure_content(&mut self, key: FileKey, cx: &mut Context<Self>) {
        if self.contents.get(&key).is_none_or(Fetch::is_idle) {
            self.load_content(key, cx);
        }
    }

    /// The kind of list a focus is, for the row that opens an item out of it.
    pub fn kind_of(focus: &Focus) -> Option<ListKind> {
        match focus {
            Focus::Repo { kind, .. } => Some(*kind),
            _ => None,
        }
    }

    /// Run `work` against the source on a background thread, then `apply`
    /// its answer on the UI thread, tell every view, and write the snapshot.
    ///
    /// The one place a trait call happens. The answer's error is turned into
    /// the reader's sentence here rather than in a view, because a view that
    /// formats errors is a view that has to know the error type.
    fn fetch<T, W, A>(&self, cx: &mut Context<Self>, work: W, apply: A)
    where
        T: Send + 'static,
        W: FnOnce(&dyn GitHub) -> e1_github::Result<T> + Send + 'static,
        A: FnOnce(&mut Self, Result<T, String>) + 'static,
    {
        cx.notify();
        let github = self.github.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    work(github.as_ref()).map_err(|error| {
                        tracing::warn!(%error, "GitHub request failed");
                        describe(&error)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                apply(this, result);
                cx.emit(StoreEvent::Changed);
                cx.notify();
                this.persist(cx);
            })
            .ok();
        })
        .detach();
    }
}
