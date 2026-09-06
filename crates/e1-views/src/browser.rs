//! The file finder: a repository's tree, searched by typing.
//!
//! One request fetches every path in the repository and the matching is
//! local, so the list answers between keystrokes. Rows are a
//! `uniform_list` because a repository can have twenty thousand paths and a
//! finder that built an element for each would not.

use crate::store::{Store, StoreEvent};
use e1_github::RepoId;
use e1_ui::{Tokens, finder};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Icon, IconName, h_flex, v_flex};

/// How tall a path row is.
const ROW_HEIGHT: Pixels = px(28.);

/// How many matches are listed. Past this the reader types another letter.
const MATCH_CAP: usize = 400;

/// Emitted when the reader picks a file.
pub enum BrowserEvent {
    /// Read this file.
    Open {
        /// Which repository.
        repo: RepoId,
        /// Which path.
        path: String,
    },
}

impl EventEmitter<BrowserEvent> for FileBrowser {}

/// The file finder.
pub struct FileBrowser {
    store: Entity<Store>,
    repo: Option<RepoId>,
    query: Entity<InputState>,
    /// Every file path in the tree, in GitHub's order.
    paths: Vec<String>,
    /// The paths that match the query, as indices into `paths`.
    matches: Vec<usize>,
    /// Whether GitHub cut the tree short.
    truncated: bool,
    selected: Option<String>,
}

impl FileBrowser {
    /// A finder over a store, showing nothing until told which repository.
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| {
            InputState::new(window, cx).placeholder(rust_i18n::t!("files.search").to_string())
        });
        cx.subscribe(&query, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.rematch(cx);
            }
        })
        .detach();
        cx.subscribe(&store, |this, _, _: &StoreEvent, cx| this.rebuild(cx))
            .detach();
        Self {
            store,
            repo: None,
            query,
            paths: Vec::new(),
            matches: Vec::new(),
            truncated: false,
            selected: None,
        }
    }

    /// Show a repository's files, fetching the tree if it never has been.
    pub fn set_repo(&mut self, repo: RepoId, cx: &mut Context<Self>) {
        if self.repo.as_ref() != Some(&repo) {
            self.selected = None;
        }
        self.repo = Some(repo.clone());
        self.store
            .update(cx, |store, cx| store.ensure_tree(repo, cx));
        self.rebuild(cx);
    }

    /// Which repository is on screen.
    pub fn repo(&self) -> Option<&RepoId> {
        self.repo.as_ref()
    }

    /// Fetch the tree again.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(repo) = self.repo.clone() {
            self.store.update(cx, |store, cx| store.load_tree(repo, cx));
        }
    }

    /// Whether the tree is being fetched.
    pub fn is_loading(&self, cx: &App) -> bool {
        self.repo
            .as_ref()
            .and_then(|repo| self.store.read(cx).tree(repo))
            .is_some_and(|tree| tree.is_loading())
    }

    /// Rebuild the path list from the store's tree.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let tree = self
            .repo
            .as_ref()
            .and_then(|repo| self.store.read(cx).tree(repo))
            .and_then(|fetch| fetch.value())
            .cloned();
        match tree {
            Some(tree) => {
                self.truncated = tree.truncated;
                self.paths = tree.files().map(|entry| entry.path.clone()).collect();
            }
            None => {
                self.truncated = false;
                self.paths.clear();
            }
        }
        self.rematch(cx);
    }

    /// Re-run the match after the query or the tree changed.
    fn rematch(&mut self, cx: &mut Context<Self>) {
        let query = self.query.read(cx).value().to_string();
        self.matches = finder::find(&self.paths, &query, MATCH_CAP);
        cx.notify();
    }

    fn open(&mut self, index: usize, cx: &mut Context<Self>) {
        let (Some(repo), Some(path)) = (
            self.repo.clone(),
            self.matches
                .get(index)
                .and_then(|i| self.paths.get(*i))
                .cloned(),
        ) else {
            return;
        };
        self.selected = Some(path.clone());
        cx.emit(BrowserEvent::Open { repo, path });
        cx.notify();
    }

    fn row(&self, index: usize, mono: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::global(cx);
        let Some(path) = self.matches.get(index).and_then(|i| self.paths.get(*i)) else {
            return div().h(ROW_HEIGHT).into_any_element();
        };
        let selected = self.selected.as_deref() == Some(path.as_str());
        let (dir, name) = match path.rsplit_once('/') {
            Some((dir, name)) => (Some(format!("{dir}/")), name.to_string()),
            None => (None, path.clone()),
        };
        div()
            .h(ROW_HEIGHT)
            .w_full()
            .px_2()
            .child(
                h_flex()
                    .id(("path", index))
                    .size_full()
                    .px_2p5()
                    .gap_2()
                    .items_center()
                    .rounded(px(tokens.radius.row))
                    .cursor_pointer()
                    .when(selected, |this| this.bg(tokens.colors().row_active()))
                    .hover(|this| this.bg(tokens.colors().row_hover()))
                    .on_click(cx.listener(move |this, _, _, cx| this.open(index, cx)))
                    .child(
                        Icon::new(IconName::FileText)
                            .size_3p5()
                            .text_color(tokens.colors().text_muted),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .overflow_hidden()
                            .font_family(mono)
                            .text_size(px(11.5))
                            .children(dir.map(|dir| {
                                div()
                                    .text_color(tokens.colors().text_muted)
                                    .truncate()
                                    .child(dir)
                            }))
                            .child(
                                div()
                                    .text_color(if selected {
                                        tokens.colors().text_primary
                                    } else {
                                        tokens.colors().text_secondary
                                    })
                                    .child(name),
                            ),
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

impl Render for FileBrowser {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::global(cx).clone();
        let mono = gpui_component::Theme::global(cx).mono_font_family.clone();
        let error = self
            .repo
            .as_ref()
            .and_then(|repo| self.store.read(cx).tree(repo))
            .and_then(|tree| tree.error())
            .map(str::to_string);
        let loading = self.is_loading(cx);

        let body: AnyElement = if self.paths.is_empty() {
            match error {
                Some(error) => self.notice(error, true, cx),
                None if loading => {
                    self.notice(rust_i18n::t!("files.loading").to_string(), false, cx)
                }
                None => self.notice(rust_i18n::t!("files.empty").to_string(), false, cx),
            }
        } else if self.matches.is_empty() {
            self.notice(rust_i18n::t!("files.empty").to_string(), false, cx)
        } else {
            let this = cx.entity();
            let mono = mono.clone();
            uniform_list("paths", self.matches.len(), move |range, _window, cx| {
                this.update(cx, |this, cx| {
                    range
                        .map(|index| this.row(index, mono.clone(), cx))
                        .collect()
                })
            })
            .flex_1()
            .size_full()
            .py_1()
            .into_any_element()
        };

        v_flex()
            .size_full()
            .child(
                div()
                    .w_full()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(tokens.colors().border_subtle)
                    .child(Input::new(&self.query).cleanable(true)),
            )
            .when(self.truncated, |this| {
                this.child(
                    div()
                        .px_4()
                        .py_1()
                        .text_size(px(11.5))
                        .text_color(tokens.colors().status_attention)
                        .child(rust_i18n::t!("files.truncated").to_string()),
                )
            })
            .child(body)
    }
}
