//! The centre column as a repository's history: `docs/ui.md` §3.3.
//!
//! One row per commit, with the rail down the left that says which line of
//! development it sits on. The lanes are worked out in `e1_ui::graph`; what
//! is here is the drawing and the picking.

use crate::store::{Store, StoreEvent};
use e1_github::RepoId;
use e1_ui::{Tokens, graph};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{h_flex, v_flex};

/// How tall a commit row is. Two lines and their air, as the item rows.
const ROW_HEIGHT: Pixels = px(52.);

/// How far apart the rail's lanes sit.
const LANE_WIDTH: f32 = 14.;

/// How wide the rail is before the message starts, whatever the lanes do.
const RAIL_MIN: f32 = 26.;

/// Emitted when the reader picks a commit.
pub enum HistoryEvent {
    /// Read this commit.
    Open {
        /// Which repository.
        repo: RepoId,
        /// Which commit.
        sha: String,
    },
}

impl EventEmitter<HistoryEvent> for History {}

/// A repository's commits.
pub struct History {
    store: Entity<Store>,
    repo: Option<RepoId>,
    /// The rail, one row per commit, worked out when the history lands.
    rail: Vec<graph::Row>,
    /// How many lanes the rail uses.
    lanes: usize,
    selected: Option<String>,
    /// Open the newest commit as soon as there is one. What a launch
    /// argument asks for; a reader picks their own.
    open_newest: bool,
}

impl History {
    /// A history over a store, showing nothing until told which repository.
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.subscribe(&store, |this, _, _: &StoreEvent, cx| this.rebuild(cx))
            .detach();
        Self {
            store,
            repo: None,
            rail: Vec::new(),
            lanes: 1,
            selected: None,
            open_newest: false,
        }
    }

    /// Show a repository's history, fetching it if it never has been.
    pub fn set_repo(&mut self, repo: RepoId, cx: &mut Context<Self>) {
        if self.repo.as_ref() != Some(&repo) {
            self.selected = None;
        }
        self.repo = Some(repo.clone());
        self.store
            .update(cx, |store, cx| store.ensure_commits(repo, cx));
        self.rebuild(cx);
    }

    /// Read the newest commit as soon as the history lands.
    pub fn open_newest(&mut self, cx: &mut Context<Self>) {
        self.open_newest = true;
        self.rebuild(cx);
    }

    /// Which repository is on screen.
    pub fn repo(&self) -> Option<&RepoId> {
        self.repo.as_ref()
    }

    /// Fetch the history again.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(repo) = self.repo.clone() {
            self.store
                .update(cx, |store, cx| store.load_commits(repo, cx));
        }
    }

    /// Whether the history is being fetched.
    pub fn is_loading(&self, cx: &App) -> bool {
        self.repo
            .as_ref()
            .and_then(|repo| self.store.read(cx).commits(repo))
            .is_some_and(|fetch| fetch.is_loading())
    }

    /// Lay the rail out again from the store's history.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let commits = self
            .repo
            .as_ref()
            .and_then(|repo| self.store.read(cx).commits(repo))
            .and_then(|fetch| fetch.value());
        self.rail = match commits {
            Some(commits) => graph::lay_out(
                commits
                    .iter()
                    .map(|commit| (commit.sha.as_str(), commit.parents.as_slice())),
            ),
            None => Vec::new(),
        };
        self.lanes = graph::width(&self.rail).min(graph::MAX_LANES);
        let newest = self
            .open_newest
            .then(|| {
                self.repo
                    .as_ref()
                    .and_then(|repo| self.store.read(cx).commits(repo))
                    .and_then(|fetch| fetch.value())
                    .and_then(|commits| commits.first())
                    .map(|commit| commit.sha.clone())
            })
            .flatten();
        if let Some(sha) = newest {
            self.open_newest = false;
            self.open(sha, cx);
        }
        cx.notify();
    }

    fn open(&mut self, sha: String, cx: &mut Context<Self>) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        self.selected = Some(sha.clone());
        cx.emit(HistoryEvent::Open { repo, sha });
        cx.notify();
    }

    /// The rail beside one row: a line for every lane running through it,
    /// and the dot in this commit's own lane.
    fn rail(&self, index: usize, cx: &App) -> AnyElement {
        let tokens = Tokens::global(cx);
        let Some(row) = self.rail.get(index) else {
            return div().w(px(RAIL_MIN)).into_any_element();
        };
        let width = (self.lanes as f32 * LANE_WIDTH).max(RAIL_MIN);
        let at = |lane: usize| px(8. + lane as f32 * LANE_WIDTH);
        let line = tokens.colors().border_strong;
        let mut rail = div().relative().w(px(width)).h_full().flex_shrink_0();
        for lane in row
            .through
            .iter()
            .copied()
            .filter(|lane| *lane < self.lanes)
        {
            rail = rail.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(at(lane))
                    .w_px()
                    .bg(line),
            );
        }
        if row.lane < self.lanes {
            // The dot sits on the line, filled for a plain commit and
            // ringed for a merge, which is what says two lines met here.
            rail = rail.child(
                div()
                    .absolute()
                    .top(ROW_HEIGHT / 2. - px(4.))
                    .left(at(row.lane) - px(3.5))
                    .size(px(8.))
                    .rounded_full()
                    .when(row.merge, |this| {
                        this.border_2().border_color(tokens.colors().accent)
                    })
                    .bg(if row.merge {
                        tokens.colors().bg_window
                    } else {
                        tokens.colors().accent
                    }),
            );
        }
        rail.into_any_element()
    }

    fn row(&self, index: usize, mono: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::global(cx).clone();
        let Some(commit) = self
            .repo
            .as_ref()
            .and_then(|repo| self.store.read(cx).commits(repo))
            .and_then(|fetch| fetch.value())
            .and_then(|commits| commits.get(index))
        else {
            return div().h(ROW_HEIGHT).into_any_element();
        };
        let selected = self.selected.as_deref() == Some(commit.sha.as_str());
        let subject = commit.subject().to_string();
        let short = commit.short().to_string();
        let author = commit.author_name.clone();
        let age = e1_ui::time::age(chrono::Utc::now(), commit.authored_at);
        let sha = commit.sha.clone();
        let rail = self.rail(index, cx);
        h_flex()
            .id(("commit", index))
            .h(ROW_HEIGHT)
            .w_full()
            .px_2()
            .items_center()
            .cursor_pointer()
            .when(selected, |this| this.bg(tokens.colors().row_active()))
            .hover(|this| this.bg(tokens.colors().row_hover()))
            .on_click(cx.listener(move |this, _, _, cx| this.open(sha.clone(), cx)))
            .child(rail)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .w_full()
                            .text_size(px(13.))
                            .text_color(tokens.colors().text_primary)
                            .truncate()
                            .child(subject),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .text_size(px(11.5))
                            .text_color(tokens.colors().text_muted)
                            .child(div().truncate().child(author))
                            .child(div().child(age))
                            .child(div().font_family(mono).child(short)),
                    ),
            )
            .into_any_element()
    }

    fn notice(&self, text: String, error: bool, cx: &App) -> AnyElement {
        let tokens = Tokens::global(cx);
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .px_8()
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(if error {
                        tokens.colors().status_error
                    } else {
                        tokens.colors().text_muted
                    })
                    .text_center()
                    .child(text),
            )
            .into_any_element()
    }
}

impl Render for History {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = gpui_component::Theme::global(cx).mono_font_family.clone();
        let fetch = self
            .repo
            .as_ref()
            .and_then(|repo| self.store.read(cx).commits(repo));
        let error = fetch.and_then(|fetch| fetch.error()).map(str::to_string);
        let count = fetch
            .and_then(|fetch| fetch.value())
            .map(|commits| commits.len())
            .unwrap_or_default();
        let loading = self.is_loading(cx);

        if count == 0 {
            return v_flex()
                .size_full()
                .child(match error {
                    Some(error) => self.notice(error, true, cx),
                    None if loading => crate::skeleton::path_rows(10, cx),
                    None => self.notice(rust_i18n::t!("history.empty").to_string(), false, cx),
                })
                .into_any_element();
        }
        let this = cx.entity();
        v_flex()
            .size_full()
            .child(
                uniform_list("commits", count, move |range, _window, cx| {
                    this.update(cx, |this, cx| {
                        range
                            .map(|index| this.row(index, mono.clone(), cx))
                            .collect()
                    })
                })
                .flex_1()
                .size_full()
                .py_1(),
            )
            .into_any_element()
    }
}
