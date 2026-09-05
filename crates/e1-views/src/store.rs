//! Everything fetched, and the fetching of it.
//!
//! One entity holds every answer GitHub has given this window, each as a
//! [`Fetch`] so a refresh keeps the old value on screen. Every call goes to
//! the background executor and comes back through `this.update`; nothing
//! here blocks the UI thread, and nothing but this file calls the trait.

use e1_github::{
    Comment, GitHub, Item, ListKind, Notification, Pull, PullFile, Repo, RepoId, Viewer,
};
use e1_ui::fetch::describe;
use e1_ui::{Fetch, Focus, Section};
use gpui::{AppContext as _, Context, EventEmitter};
use std::collections::HashMap;
use std::sync::Arc;

/// The key of a detail: which item, in which repository.
pub type ItemKey = (RepoId, u64);

/// Everything the detail panel draws for one item, fetched together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
    /// The item.
    pub item: Item,
    /// What only a pull has, when it is one.
    pub pull: Option<Pull>,
    /// Its comments, oldest first.
    pub comments: Vec<Comment>,
}

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
    files: HashMap<ItemKey, Fetch<Vec<PullFile>>>,
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
            files: HashMap::new(),
        }
    }

    /// Swap the source and forget everything the old one said.
    ///
    /// Signing in and out: a token change is a different GitHub, and a list
    /// fetched as one person must not be shown to the next. Everything is
    /// fetched again from the new source.
    pub fn set_source(&mut self, github: Arc<dyn GitHub>, cx: &mut Context<Self>) {
        self.github = github;
        self.viewer = Fetch::Idle;
        self.repos = Fetch::Idle;
        self.inbox = Fetch::Idle;
        self.lists.clear();
        self.details.clear();
        self.files.clear();
        self.refresh_all(cx);
        cx.emit(StoreEvent::Changed);
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

    /// Fetch a list, whether or not it has been fetched before.
    pub fn load_list(&mut self, focus: Focus, cx: &mut Context<Self>) {
        if focus == Focus::Section(Section::Inbox) {
            return self.load_inbox(cx);
        }
        self.lists.entry(focus.clone()).or_default().begin();
        let key = focus.clone();
        self.fetch(
            cx,
            move |github| match &focus {
                Focus::Section(section) => {
                    // Every section but the inbox is a search, and the inbox
                    // was handled above.
                    github.search(section.query().unwrap_or_default())
                }
                Focus::Repo { repo, kind, status } => github.items(repo, *kind, *status),
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

    /// A pull's files, if they have ever been asked for.
    pub fn files(&self, key: &ItemKey) -> Option<&Fetch<Vec<PullFile>>> {
        self.files.get(key)
    }

    /// Fetch a pull's files.
    pub fn load_files(&mut self, key: ItemKey, cx: &mut Context<Self>) {
        self.files.entry(key.clone()).or_default().begin();
        let (repo, number) = key.clone();
        self.fetch(
            cx,
            move |github| github.pull_files(&repo, number),
            move |this, result| {
                this.files.entry(key).or_default().finish(result);
            },
        );
    }

    /// Fetch a pull's files only if they never have been.
    pub fn ensure_files(&mut self, key: ItemKey, cx: &mut Context<Self>) {
        if self.files.get(&key).is_none_or(Fetch::is_idle) {
            self.load_files(key, cx);
        }
    }

    /// The kind of list a focus is, for the row that opens an item out of it.
    pub fn kind_of(focus: &Focus) -> Option<ListKind> {
        match focus {
            Focus::Repo { kind, .. } => Some(*kind),
            Focus::Section(_) => None,
        }
    }

    /// Run `work` against the source on a background thread, then `apply`
    /// its answer on the UI thread and tell every view.
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
            })
            .ok();
        })
        .detach();
    }
}
