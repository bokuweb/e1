//! The right column: `docs/ui.md` §3.4.
//!
//! One thing, read in full: an item — its header, its labels, assignees
//! and projects, the checks and the merge, its description and comments,
//! or its files and their diffs — or a file out of the tree. The long parts
//! are virtualized: a pull's diffs are one `uniform_list` of rows across
//! every file, and a file's lines are another, so a thousand-line diff
//! costs what the screen shows (`AGENTS.md` rule 7).
//!
//! The editable parts borrow GitHub's own shapes, because a reader who
//! knows those is not asked to learn ours: a facet is a heading with a
//! gear, and the gear opens a filterable list where a click adds or
//! removes; the merge is a card that says what the checks came to and
//! whether the branch conflicts, then a green button with the method on
//! it and the other methods behind a chevron.

use crate::avatar::avatar;
use crate::store::{FileKey, ItemKey, Store, StoreEvent};
use chrono::Utc;
use e1_github::{CheckState, Comment, FileStatus, MergeMethod, ReviewEvent};
use e1_ui::Tokens;
use e1_ui::diff;
use e1_ui::rows::{Glyph, LabelChip};
use e1_ui::time::age;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::text::TextView;
use gpui_component::{Icon, IconName, StyledExt as _, h_flex, v_flex};
use std::collections::HashSet;

/// The reading measure, in pixels. Long-form text stays readable because the
/// column stops growing, not because the window does.
const MEASURE: f32 = 720.;

/// How tall a diff row is: a file's header and a line of it are the same
/// height, which is what lets every file's diff be one virtualized list.
const DIFF_ROW: Pixels = px(22.);

/// How tall a line of a file is.
const CODE_ROW: Pixels = px(20.);

/// Which half of a pull is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    /// The description and the comments.
    Conversation,
    /// The files and their diffs.
    Files,
}

/// What the column is reading.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Showing {
    /// A pull or an issue.
    Item(ItemKey),
    /// A file out of a repository's tree.
    File(FileKey),
}

/// One row of the diff list.
enum DiffRow {
    /// A file's header: status, path, counts. Picking it folds the diff.
    File {
        index: usize,
        name: SharedString,
        status: FileStatus,
        additions: u64,
        deletions: u64,
        collapsed: bool,
    },
    /// A line of a diff.
    Line(diff::Line),
    /// A file with nothing to show under it.
    Note(SharedString),
}

/// One of the three lists a reader can edit from the head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Picker {
    /// The repository's labels.
    Labels,
    /// The repository's assignable people.
    Assignees,
    /// The owner's projects.
    Projects,
}

/// The right column.
pub struct Detail {
    store: Entity<Store>,
    showing: Option<Showing>,
    tab: Tab,
    /// The files whose diffs are folded. Everything starts open: the list is
    /// virtualized, so a hundred files cost nothing until they are scrolled
    /// to, and a review reads top to bottom.
    collapsed: HashSet<String>,
    diff_rows: Vec<DiffRow>,
    /// A file's lines, split once when it lands.
    lines: Vec<SharedString>,
    /// The comment being written.
    composer: Entity<TextareaState>,
    /// Empty the composer at the next frame: clearing needs the window,
    /// which the answer that asks for it does not have.
    clear_composer: bool,
    /// The picker that is open under its facet, if one is.
    picker: Option<Picker>,
    /// What is typed into the open picker.
    filter: Entity<InputState>,
    /// Empty the filter at the next frame, when a picker opens.
    clear_filter: bool,
    /// The list of merge methods is open under the merge button.
    merge_menu: bool,
    /// How the next merge is done.
    merge_method: MergeMethod,
    /// The merge button was pressed once; the next press merges.
    confirm_merge: bool,
    /// The checks card is unfolded to its runs.
    checks_open: bool,
}

impl Detail {
    /// A detail over a store, showing nothing until told what to.
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.subscribe(&store, |this, _, _: &StoreEvent, cx| this.rebuild(cx))
            .detach();
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(rust_i18n::t!("detail.comment.placeholder").to_string())
                .auto_grow(2, 8)
        });
        cx.subscribe(&composer, |this, _, event: &InputEvent, cx| {
            // ⌘⏎ sends, the way it does on GitHub; a plain ⏎ is a newline.
            if let InputEvent::PressEnter {
                secondary: true, ..
            } = event
            {
                this.send_comment(cx);
            }
        })
        .detach();
        let filter = cx.new(|cx| InputState::new(window, cx));
        cx.subscribe(&filter, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        Self {
            store,
            showing: None,
            tab: Tab::Conversation,
            collapsed: HashSet::new(),
            diff_rows: Vec::new(),
            lines: Vec::new(),
            composer,
            clear_composer: false,
            picker: None,
            filter,
            clear_filter: false,
            merge_menu: false,
            merge_method: MergeMethod::default(),
            confirm_merge: false,
            checks_open: false,
        }
    }

    fn item_key(&self) -> Option<ItemKey> {
        match &self.showing {
            Some(Showing::Item(key)) => Some(key.clone()),
            _ => None,
        }
    }

    /// Post what is in the composer.
    fn send_comment(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.item_key() else {
            return;
        };
        let body = self.composer.read(cx).value().trim().to_string();
        if body.is_empty() {
            return;
        }
        self.clear_composer = true;
        self.store
            .update(cx, |store, cx| store.comment_on(key, body, cx));
        cx.notify();
    }

    /// Submit a review with what is in the composer as its body. An
    /// approval needs no words; a request for changes reads better with
    /// some, but GitHub accepts either.
    fn send_review(&mut self, event: ReviewEvent, cx: &mut Context<Self>) {
        let Some(key) = self.item_key() else {
            return;
        };
        let body = self.composer.read(cx).value().trim().to_string();
        self.clear_composer = true;
        self.store
            .update(cx, |store, cx| store.review(key, event, body, cx));
        cx.notify();
    }

    fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if let Some(key) = self.item_key() {
            self.store
                .update(cx, |store, cx| store.set_open(key, open, cx));
            cx.notify();
        }
    }

    /// The merge button: the first press arms it, the second merges. A
    /// merge is the one thing here git cannot take back.
    fn press_merge(&mut self, cx: &mut Context<Self>) {
        self.merge_menu = false;
        if !self.confirm_merge {
            self.confirm_merge = true;
            cx.notify();
            return;
        }
        self.confirm_merge = false;
        let method = self.merge_method;
        if let Some(key) = self.item_key() {
            self.store
                .update(cx, |store, cx| store.merge(key, method, cx));
            cx.notify();
        }
    }

    /// Open one of the pickers, fetching what it lists, or close it.
    fn toggle_picker(&mut self, picker: Picker, cx: &mut Context<Self>) {
        if self.picker == Some(picker) {
            self.picker = None;
            cx.notify();
            return;
        }
        self.picker = Some(picker);
        self.clear_filter = true;
        if let Some(key) = self.item_key() {
            let repo = key.0.clone();
            self.store.update(cx, |store, cx| match picker {
                Picker::Labels => store.ensure_repo_labels(repo, cx),
                Picker::Assignees => store.ensure_candidates(repo, cx),
                Picker::Projects => {
                    store.ensure_projects(repo.owner.clone(), cx);
                    store.ensure_memberships(key, cx);
                }
            });
        }
        cx.notify();
    }

    fn toggle_label(&mut self, name: String, has: bool, cx: &mut Context<Self>) {
        if let Some(key) = self.item_key() {
            self.store.update(cx, |store, cx| {
                if has {
                    store.remove_label(key, name, cx)
                } else {
                    store.add_label(key, name, cx)
                }
            });
        }
    }

    fn toggle_assignee(&mut self, login: String, has: bool, cx: &mut Context<Self>) {
        if let Some(key) = self.item_key() {
            self.store.update(cx, |store, cx| {
                if has {
                    store.remove_assignee(key, login, cx)
                } else {
                    store.add_assignee(key, login, cx)
                }
            });
        }
    }

    fn toggle_project(
        &mut self,
        project_id: String,
        item_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(key) = self.item_key() {
            self.store.update(cx, |store, cx| match item_id {
                Some(item_id) => store.remove_from_project(key, project_id, item_id, cx),
                None => store.add_to_project(key, project_id, cx),
            });
        }
    }

    /// Show an item, fetching it if it never has been.
    pub fn show(&mut self, key: ItemKey, is_pull: Option<bool>, cx: &mut Context<Self>) {
        let showing = Showing::Item(key.clone());
        if self.showing.as_ref() != Some(&showing) {
            self.tab = Tab::Conversation;
            self.collapsed.clear();
            self.confirm_merge = false;
            self.merge_menu = false;
            self.picker = None;
            self.checks_open = false;
        }
        self.showing = Some(showing);
        self.store
            .update(cx, |store, cx| store.ensure_detail(key, is_pull, cx));
        self.rebuild(cx);
    }

    /// Show a file, fetching it if it never has been.
    pub fn show_file(&mut self, key: FileKey, cx: &mut Context<Self>) {
        self.showing = Some(Showing::File(key.clone()));
        self.store
            .update(cx, |store, cx| store.ensure_content(key, cx));
        self.rebuild(cx);
    }

    /// Switch to the files of the pull on screen, with one of them singled
    /// out by folding the rest.
    pub fn show_files(&mut self, only: Option<String>, cx: &mut Context<Self>) {
        if let Some(only) = only
            && let Some(key) = self.item_key()
        {
            let files = self
                .store
                .read(cx)
                .pull_files(&key)
                .and_then(|fetch| fetch.value())
                .cloned()
                .unwrap_or_default();
            self.collapsed = files
                .iter()
                .map(|file| file.filename.clone())
                .filter(|name| name != &only)
                .collect();
        }
        self.set_tab(Tab::Files, cx);
    }

    /// Fetch again whatever is on screen.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        match self.showing.clone() {
            Some(Showing::Item(key)) => {
                let files = self.tab == Tab::Files;
                let sha = self.head_sha(cx);
                self.store.update(cx, |store, cx| {
                    store.load_detail(key.clone(), None, cx);
                    if files {
                        store.load_pull_files(key.clone(), cx);
                    }
                    if let Some(sha) = sha {
                        store.load_checks(key.0.clone(), sha, cx);
                    }
                });
            }
            Some(Showing::File(key)) => {
                self.store
                    .update(cx, |store, cx| store.load_content(key, cx));
            }
            None => {}
        }
    }

    /// Where what is on screen lives on the web, when it is known.
    pub fn html_url(&self, cx: &App) -> Option<String> {
        let store = self.store.read(cx);
        match self.showing.as_ref()? {
            Showing::Item(key) => store
                .detail(key)
                .and_then(|detail| detail.value())
                .map(|detail| detail.item.html_url.clone()),
            Showing::File(key) => store
                .content(key)
                .and_then(|content| content.value())
                .map(|content| content.html_url.clone()),
        }
    }

    /// The head commit of the pull on screen, once the detail has landed.
    fn head_sha(&self, cx: &App) -> Option<String> {
        let key = self.item_key()?;
        self.store
            .read(cx)
            .detail(&key)
            .and_then(|fetch| fetch.value())
            .and_then(|detail| detail.pull.as_ref())
            .map(|pull| pull.head_sha.clone())
            .filter(|sha| !sha.is_empty())
    }

    fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        if tab == Tab::Files
            && let Some(key) = self.item_key()
        {
            self.store
                .update(cx, |store, cx| store.ensure_pull_files(key, cx));
        }
        self.rebuild(cx);
    }

    fn toggle_file(&mut self, name: String, cx: &mut Context<Self>) {
        if !self.collapsed.remove(&name) {
            self.collapsed.insert(name);
        }
        self.rebuild(cx);
    }

    /// Recompute the virtualized rows from what the store has, and ask for
    /// the pictures and the checks the conversation will draw.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.diff_rows.clear();
        self.lines.clear();
        if let Some(key) = self.item_key() {
            let (urls, sha) = {
                let store = self.store.read(cx);
                let detail = store.detail(&key).and_then(|fetch| fetch.value());
                let urls: Vec<String> = detail
                    .map(|detail| {
                        std::iter::once(detail.item.author.avatar_url.clone())
                            .chain(detail.item.assignees.iter().map(|u| u.avatar_url.clone()))
                            .chain(detail.comments.iter().map(|c| c.author.avatar_url.clone()))
                            .collect()
                    })
                    .unwrap_or_default();
                let sha = detail
                    .and_then(|detail| detail.pull.as_ref())
                    .map(|pull| pull.head_sha.clone())
                    .filter(|sha| !sha.is_empty());
                (urls, sha)
            };
            let repo = key.0.clone();
            self.store.update(cx, |store, cx| {
                for url in urls {
                    store.ensure_avatar(&url, cx);
                }
                if let Some(sha) = sha {
                    store.ensure_checks(repo, sha, cx);
                }
            });
        }
        match &self.showing {
            Some(Showing::Item(key)) if self.tab == Tab::Files => {
                let files = self
                    .store
                    .read(cx)
                    .pull_files(key)
                    .and_then(|fetch| fetch.value())
                    .cloned()
                    .unwrap_or_default();
                for (index, file) in files.iter().enumerate() {
                    let collapsed = self.collapsed.contains(&file.filename);
                    self.diff_rows.push(DiffRow::File {
                        index,
                        name: match &file.previous_filename {
                            Some(previous) => format!("{previous} → {}", file.filename).into(),
                            None => file.filename.clone().into(),
                        },
                        status: file.status,
                        additions: file.additions,
                        deletions: file.deletions,
                        collapsed,
                    });
                    if collapsed {
                        continue;
                    }
                    match &file.patch {
                        Some(patch) => self
                            .diff_rows
                            .extend(diff::parse(patch).into_iter().map(DiffRow::Line)),
                        None => self.diff_rows.push(DiffRow::Note(
                            rust_i18n::t!("detail.file.no_diff").to_string().into(),
                        )),
                    }
                }
            }
            Some(Showing::File(key)) => {
                if let Some(text) = self
                    .store
                    .read(cx)
                    .content(key)
                    .and_then(|fetch| fetch.value())
                    .and_then(|content| content.text.as_deref())
                {
                    self.lines = text
                        .lines()
                        .map(|line| SharedString::from(line.to_string()))
                        .collect();
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// A single muted line in the middle of the column.
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

    /// One comment: who, when, and what.
    fn comment(&self, comment: &Comment, cx: &App) -> AnyElement {
        let tokens = Tokens::global(cx);
        let picture = avatar(
            self.store.read(cx).avatar(&comment.author.avatar_url),
            &comment.author.login,
            px(20.),
            cx,
        );
        v_flex()
            .w_full()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(picture)
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_medium()
                            .text_color(tokens.colors().text_primary)
                            .child(comment.author.login.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(tokens.colors().text_muted)
                            .child(age(Utc::now(), comment.created_at)),
                    ),
            )
            .child(
                div()
                    .pl_7()
                    .text_size(px(14.))
                    .line_height(relative(1.6))
                    .child(
                        TextView::markdown(("comment", comment.id as usize), comment.body.clone())
                            .selectable(true),
                    ),
            )
            .into_any_element()
    }

    /// The two chips that switch a pull between its halves.
    fn tabs(&self, files: u64, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::global(cx).clone();
        let chip = |this: &Self, tab: Tab, label: String, cx: &mut Context<Self>| {
            let selected = this.tab == tab;
            div()
                .id(match tab {
                    Tab::Conversation => "tab-conversation",
                    Tab::Files => "tab-files",
                })
                .px_2()
                .py_0p5()
                .rounded(px(tokens.radius.row - 2.))
                .cursor_pointer()
                .text_size(px(11.5))
                .when(selected, |this| {
                    this.bg(tokens.colors().row_active())
                        .text_color(tokens.colors().text_primary)
                })
                .when(!selected, |this| {
                    this.text_color(tokens.colors().text_muted)
                })
                .hover(|this| this.bg(tokens.colors().row_hover()))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.set_tab(tab, cx)))
        };
        h_flex()
            .gap_0p5()
            .p_0p5()
            .rounded(px(tokens.radius.row))
            .bg(tokens.colors().bg_surface)
            .child(chip(
                self,
                Tab::Conversation,
                rust_i18n::t!("detail.tab.conversation").to_string(),
                cx,
            ))
            .child(chip(
                self,
                Tab::Files,
                format!("{} {files}", rust_i18n::t!("detail.tab.files")),
                cx,
            ))
            .into_any_element()
    }

    /// One row of the diff list.
    fn diff_row(&self, index: usize, mono: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::global(cx);
        let Some(row) = self.diff_rows.get(index) else {
            return div().h(DIFF_ROW).into_any_element();
        };
        match row {
            DiffRow::File {
                index: file_index,
                name,
                status,
                additions,
                deletions,
                collapsed,
            } => {
                let mark_color = match status {
                    FileStatus::Added => tokens.colors().status_done,
                    FileStatus::Removed => tokens.colors().status_error,
                    FileStatus::Renamed => tokens.colors().accent,
                    FileStatus::Modified | FileStatus::Other => tokens.colors().status_attention,
                };
                let toggle = name.to_string();
                let file_index = *file_index;
                h_flex()
                    .id(("file", file_index))
                    .h(DIFF_ROW)
                    .w_full()
                    .px_2()
                    .gap_2()
                    .items_center()
                    .bg(tokens.colors().bg_raised)
                    .cursor_pointer()
                    .hover(|this| this.bg(tokens.colors().surface_hover()))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.toggle_file(toggle.clone(), cx)),
                    )
                    .child(
                        Icon::new(if *collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .size_3()
                        .text_color(tokens.colors().text_muted),
                    )
                    .child(
                        div()
                            .w_3()
                            .text_size(px(11.5))
                            .font_family(mono.clone())
                            .text_color(mark_color)
                            .child(status.letter()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_size(px(11.5))
                            .font_family(mono)
                            .text_color(tokens.colors().text_primary)
                            .truncate()
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(tokens.colors().status_done)
                            .child(format!("+{additions}")),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(tokens.colors().status_error)
                            .child(format!("−{deletions}")),
                    )
                    .into_any_element()
            }
            DiffRow::Line(line) => {
                let (fill, color, marker) = match line.kind {
                    diff::Kind::Added => (
                        Some(tokens.colors().status_done.opacity(0.12)),
                        tokens.colors().text_primary,
                        "+",
                    ),
                    diff::Kind::Removed => (
                        Some(tokens.colors().status_error.opacity(0.12)),
                        tokens.colors().text_secondary,
                        "-",
                    ),
                    diff::Kind::Hunk => (
                        Some(tokens.colors().code_bg),
                        tokens.colors().text_muted,
                        "",
                    ),
                    diff::Kind::Context => (None, tokens.colors().text_secondary, " "),
                };
                let number = |value: Option<u32>| {
                    div()
                        .w(px(40.))
                        .flex_shrink_0()
                        .text_right()
                        .pr_1()
                        .text_color(tokens.colors().text_muted)
                        .child(value.map(|v| v.to_string()).unwrap_or_default())
                };
                h_flex()
                    .h(DIFF_ROW)
                    .w_full()
                    .px_1()
                    .items_center()
                    .font_family(mono)
                    .text_size(px(11.5))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .when_some(fill, |this, fill| this.bg(fill))
                    .child(number(line.old))
                    .child(number(line.new))
                    .child(div().w_3().flex_shrink_0().text_color(color).child(marker))
                    .child(div().text_color(color).child(line.text.clone()))
                    .into_any_element()
            }
            DiffRow::Note(text) => div()
                .h(DIFF_ROW)
                .px_3()
                .text_size(px(11.5))
                .text_color(tokens.colors().text_muted)
                .child(text.clone())
                .into_any_element(),
        }
    }

    /// One line of a file.
    fn code_row(&self, index: usize, mono: SharedString, cx: &App) -> AnyElement {
        let tokens = Tokens::global(cx);
        let Some(line) = self.lines.get(index) else {
            return div().h(CODE_ROW).into_any_element();
        };
        h_flex()
            .h(CODE_ROW)
            .w_full()
            .px_1()
            .items_center()
            .font_family(mono)
            .text_size(px(11.5))
            .whitespace_nowrap()
            .overflow_hidden()
            .child(
                div()
                    .w(px(48.))
                    .flex_shrink_0()
                    .text_right()
                    .pr_3()
                    .text_color(tokens.colors().text_muted)
                    .child((index + 1).to_string()),
            )
            .child(
                div()
                    .text_color(tokens.colors().text_primary)
                    .child(line.clone()),
            )
            .into_any_element()
    }

    /// The files tab's body: the virtualized diff, or why there is none.
    fn diff_list(&self, key: &ItemKey, mono: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let fetch = self.store.read(cx).pull_files(key).cloned();
        let (files, error, loading) = match &fetch {
            Some(fetch) => (
                fetch.value().cloned(),
                fetch.error().map(str::to_string),
                fetch.is_loading(),
            ),
            None => (None, None, false),
        };
        match files {
            Some(files) if files.is_empty() => {
                self.notice(rust_i18n::t!("detail.files.empty").to_string(), false, cx)
            }
            Some(_) => {
                let this = cx.entity();
                uniform_list("diff", self.diff_rows.len(), move |range, _window, cx| {
                    this.update(cx, |this, cx| {
                        range
                            .map(|index| this.diff_row(index, mono.clone(), cx))
                            .collect()
                    })
                })
                .flex_1()
                .size_full()
                .into_any_element()
            }
            None => match error {
                Some(error) => self.notice(error, true, cx),
                None if loading => crate::skeleton::diff(cx),
                None => self.notice(rust_i18n::t!("detail.files.empty").to_string(), false, cx),
            },
        }
    }

    /// A small button: filled for the one thing the row is for, quiet
    /// otherwise, and `danger` when it is the destructive one.
    fn button(
        &self,
        id: impl Into<ElementId>,
        label: String,
        loud: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let tokens = Tokens::global(cx);
        div()
            .id(id)
            .px_2p5()
            .py_1()
            .rounded(px(tokens.radius.control()))
            .cursor_pointer()
            .text_size(px(11.5))
            .when(loud, |this| {
                this.bg(tokens.colors().accent)
                    .text_color(tokens.colors().bg_window)
                    .hover(|this| this.opacity(0.85))
            })
            .when(!loud, |this| {
                this.bg(tokens.colors().bg_surface)
                    .text_color(tokens.colors().text_primary)
                    .hover(|this| this.bg(tokens.colors().row_hover()))
            })
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)))
            .into_any_element()
    }

    /// Close or reopen, open on the web, and what the last write said.
    fn actions(
        &self,
        key: &ItemKey,
        detail: &crate::store::Detail,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = Tokens::global(cx).clone();
        let action = self.store.read(cx).action(key).cloned();
        let busy = action.as_ref().is_some_and(|action| action.is_loading());
        let complaint = action
            .as_ref()
            .and_then(|action| action.error().map(str::to_string));
        let item = &detail.item;
        let open = item.status == e1_github::Status::Open;
        let merged = matches!(item.kind, e1_github::Kind::Pull { merged: true, .. });
        let url = item.html_url.clone();

        let mut row = h_flex().gap_2().items_center().flex_wrap();
        if busy {
            row = row.child(
                div()
                    .text_size(px(11.5))
                    .text_color(tokens.colors().text_muted)
                    .child(rust_i18n::t!("detail.working").to_string()),
            );
        } else if !merged {
            let (label, to_open) = if open {
                (rust_i18n::t!("detail.close").to_string(), false)
            } else {
                (rust_i18n::t!("detail.reopen").to_string(), true)
            };
            row = row.child(
                self.button("toggle-open", label, false, cx, move |this, cx| {
                    this.set_open(to_open, cx)
                }),
            );
        }
        row = row.child(
            h_flex()
                .id("open-web")
                .gap_1()
                .items_center()
                .px_2p5()
                .py_1()
                .rounded(px(tokens.radius.control()))
                .cursor_pointer()
                .text_size(px(11.5))
                .text_color(tokens.colors().text_secondary)
                .bg(tokens.colors().bg_surface)
                .hover(|this| this.bg(tokens.colors().row_hover()))
                .child(
                    Icon::new(IconName::ExternalLink)
                        .size_3()
                        .text_color(tokens.colors().text_secondary),
                )
                .child(rust_i18n::t!("detail.open_on_github").to_string())
                .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url))),
        );
        if let Some(complaint) = complaint {
            row = row.child(
                div()
                    .text_size(px(11.5))
                    .text_color(tokens.colors().status_error)
                    .child(complaint),
            );
        }
        row.into_any_element()
    }

    /// A facet's heading: its name and the gear that opens its picker.
    fn facet_heading(&self, picker: Picker, name: String, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::global(cx);
        let open = self.picker == Some(picker);
        let id = match picker {
            Picker::Labels => "facet-labels",
            Picker::Assignees => "facet-assignees",
            Picker::Projects => "facet-projects",
        };
        h_flex()
            .w_full()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .text_size(px(11.5))
                    .font_medium()
                    .text_color(if open {
                        tokens.colors().accent
                    } else {
                        tokens.colors().text_secondary
                    })
                    .child(name),
            )
            .child(
                div()
                    .id(id)
                    .p_1()
                    .rounded(px(tokens.radius.control()))
                    .cursor_pointer()
                    .hover(|this| this.bg(tokens.colors().row_hover()))
                    .child(
                        Icon::new(IconName::Settings)
                            .size_3p5()
                            .text_color(if open {
                                tokens.colors().accent
                            } else {
                                tokens.colors().text_muted
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_picker(picker, cx))),
            )
            .into_any_element()
    }

    /// One row of a picker: something to add or remove, with a check when
    /// the item has it. Eight arguments because a row is eight facts; a
    /// struct for them would be the same eight facts with a name.
    #[allow(clippy::too_many_arguments)]
    fn picker_row(
        &self,
        id: impl Into<ElementId>,
        leading: AnyElement,
        title: String,
        subtitle: Option<String>,
        has: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let tokens = Tokens::global(cx);
        h_flex()
            .id(id)
            .w_full()
            .px_2p5()
            .py_1p5()
            .gap_2()
            .items_center()
            .cursor_pointer()
            .border_t_1()
            .border_color(tokens.colors().border_subtle)
            .hover(|this| this.bg(tokens.colors().row_hover()))
            .child(div().w_4().flex_shrink_0().child(if has {
                Icon::new(IconName::Check)
                    .size_3p5()
                    .text_color(tokens.colors().text_primary)
                    .into_any_element()
            } else {
                div().into_any_element()
            }))
            .child(leading)
            .child(
                v_flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(tokens.colors().text_primary)
                            .truncate()
                            .child(title),
                    )
                    .children(subtitle.map(|subtitle| {
                        div()
                            .text_size(px(11.5))
                            .text_color(tokens.colors().text_muted)
                            .truncate()
                            .child(subtitle)
                    })),
            )
            .on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)))
            .into_any_element()
    }

    /// The picker under a facet: a filter, then the rows that match it.
    fn picker(
        &self,
        placeholder: String,
        rows: Vec<AnyElement>,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = Tokens::global(cx);
        let _ = placeholder;
        v_flex()
            .w_full()
            .mt_1()
            .rounded(px(tokens.radius.panel))
            .bg(tokens.colors().bg_raised)
            .border_1()
            .border_color(tokens.colors().border_strong)
            .overflow_hidden()
            .child(div().p_2().child(Input::new(&self.filter).cleanable(true)))
            .child(match error {
                Some(error) => div()
                    .px_3()
                    .py_2()
                    .text_size(px(11.5))
                    .text_color(tokens.colors().status_error)
                    .child(error)
                    .into_any_element(),
                None if rows.is_empty() => div()
                    .px_3()
                    .py_2()
                    .text_size(px(11.5))
                    .text_color(tokens.colors().text_muted)
                    .child(rust_i18n::t!("facet.no_match").to_string())
                    .into_any_element(),
                None => v_flex()
                    .id("picker-rows")
                    .w_full()
                    .max_h(px(280.))
                    .overflow_y_scroll()
                    .children(rows)
                    .into_any_element(),
            })
            .into_any_element()
    }

    /// The labels, the assignees and the projects, each a heading, its
    /// values, and — while its gear is on — its picker.
    fn facets(
        &self,
        key: &ItemKey,
        detail: &crate::store::Detail,
        labels: &[LabelChip],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = Tokens::global(cx).clone();
        let muted = tokens.colors().text_muted;
        let item = detail.item.clone();
        let query = self.filter.read(cx).value().trim().to_lowercase();
        let matches = |text: &str, more: Option<&str>| {
            query.is_empty()
                || text.to_lowercase().contains(&query)
                || more.is_some_and(|more| more.to_lowercase().contains(&query))
        };
        // Copied out of the store, so the rows below can bind listeners
        // through `cx` without a read of the store held across them.
        let (
            viewer,
            avatars,
            memberships,
            offered_labels,
            offered_people,
            offered_projects,
            membership_error,
        ) = {
            let store = self.store.read(cx);
            let avatars: Vec<Option<std::path::PathBuf>> = item
                .assignees
                .iter()
                .map(|user| store.avatar(&user.avatar_url))
                .collect();
            (
                store.viewer().value().map(|viewer| viewer.login.clone()),
                avatars,
                store
                    .memberships(key)
                    .and_then(|fetch| fetch.value())
                    .cloned()
                    .unwrap_or_default(),
                store.repo_labels(&key.0).cloned(),
                store.candidates(&key.0).cloned(),
                store.projects(&key.0.owner).cloned(),
                store
                    .memberships(key)
                    .and_then(|fetch| fetch.error().map(str::to_string)),
            )
        };
        let none = || {
            div()
                .text_size(px(11.5))
                .text_color(muted)
                .child(rust_i18n::t!("detail.none").to_string())
                .into_any_element()
        };

        // Labels.
        let label_values: AnyElement = if labels.is_empty() {
            none()
        } else {
            h_flex()
                .gap_1p5()
                .flex_wrap()
                .children(labels.iter().map(|label| {
                    div()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .bg(label.fill())
                        .text_size(px(11.5))
                        .text_color(label.color)
                        .child(label.name.clone())
                }))
                .into_any_element()
        };
        let label_picker = (self.picker == Some(Picker::Labels)).then(|| {
            let rows: Vec<AnyElement> = offered_labels
                .as_ref()
                .and_then(|fetch| fetch.value())
                .map(|offered| {
                    offered
                        .iter()
                        .filter(|label| matches(&label.name, label.description.as_deref()))
                        .enumerate()
                        .map(|(index, label)| {
                            let has = item.labels.iter().any(|mine| mine.name == label.name);
                            let name = label.name.clone();
                            let color = e1_ui::theme::parse_hex(&label.color).unwrap_or(muted);
                            self.picker_row(
                                ("pick-label", index),
                                div()
                                    .size_3()
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .bg(color)
                                    .into_any_element(),
                                label.name.clone(),
                                label.description.clone(),
                                has,
                                cx,
                                move |this, cx| this.toggle_label(name.clone(), has, cx),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.picker(
                rust_i18n::t!("facet.filter.labels").to_string(),
                rows,
                offered_labels
                    .as_ref()
                    .and_then(|f| f.error().map(str::to_string)),
                cx,
            )
        });

        // Assignees.
        let mut assignee_values = h_flex().gap_2().flex_wrap().items_center();
        if item.assignees.is_empty() {
            assignee_values = assignee_values.child(none());
            if let Some(me) = viewer.clone() {
                let already = item.assignees.iter().any(|user| user.login == me);
                if !already {
                    let login = me.clone();
                    assignee_values = assignee_values.child(
                        div()
                            .id("assign-self")
                            .text_size(px(11.5))
                            .text_color(tokens.colors().accent)
                            .cursor_pointer()
                            .child(rust_i18n::t!("facet.assign_self").to_string())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_assignee(login.clone(), false, cx)
                            })),
                    );
                }
            }
        }
        for (user, picture) in item.assignees.iter().zip(avatars) {
            assignee_values = assignee_values.child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(avatar(picture, &user.login, px(16.), cx))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(tokens.colors().text_secondary)
                            .child(user.login.clone()),
                    ),
            );
        }
        let assignee_picker = (self.picker == Some(Picker::Assignees)).then(|| {
            let rows: Vec<AnyElement> = offered_people
                .as_ref()
                .and_then(|fetch| fetch.value())
                .map(|people| {
                    people
                        .iter()
                        .filter(|user| matches(&user.login, None))
                        .enumerate()
                        .map(|(index, user)| {
                            let has = item.assignees.iter().any(|mine| mine.login == user.login);
                            let login = user.login.clone();
                            let picture = self.store.read(cx).avatar(&user.avatar_url);
                            self.picker_row(
                                ("pick-assignee", index),
                                avatar(picture, &user.login, px(18.), cx),
                                user.login.clone(),
                                None,
                                has,
                                cx,
                                move |this, cx| this.toggle_assignee(login.clone(), has, cx),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.picker(
                rust_i18n::t!("facet.filter.assignees").to_string(),
                rows,
                offered_people
                    .as_ref()
                    .and_then(|f| f.error().map(str::to_string)),
                cx,
            )
        });

        // Projects.
        let project_values: AnyElement = if memberships.is_empty() {
            none()
        } else {
            h_flex()
                .gap_1p5()
                .flex_wrap()
                .children(memberships.iter().map(|membership| {
                    div()
                        .px_2()
                        .py_0p5()
                        .rounded(px(tokens.radius.control()))
                        .bg(tokens.colors().code_bg)
                        .text_size(px(11.5))
                        .text_color(tokens.colors().text_secondary)
                        .child(membership.title.clone())
                }))
                .into_any_element()
        };
        let project_picker = (self.picker == Some(Picker::Projects)).then(|| {
            let rows: Vec<AnyElement> = offered_projects
                .as_ref()
                .and_then(|fetch| fetch.value())
                .map(|projects| {
                    projects
                        .iter()
                        .filter(|project| !project.closed && matches(&project.title, None))
                        .enumerate()
                        .map(|(index, project)| {
                            let item_id = memberships
                                .iter()
                                .find(|m| m.project_id == project.id)
                                .map(|m| m.item_id.clone());
                            let has = item_id.is_some();
                            let project_id = project.id.clone();
                            self.picker_row(
                                ("pick-project", index),
                                Icon::new(IconName::LayoutDashboard)
                                    .size_3p5()
                                    .text_color(muted)
                                    .into_any_element(),
                                project.title.clone(),
                                Some(format!("#{}", project.number)),
                                has,
                                cx,
                                move |this, cx| {
                                    this.toggle_project(project_id.clone(), item_id.clone(), cx)
                                },
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let error = offered_projects
                .as_ref()
                .and_then(|f| f.error().map(str::to_string))
                .or(membership_error);
            self.picker(
                rust_i18n::t!("facet.filter.projects").to_string(),
                rows,
                error,
                cx,
            )
        });

        let rule = || div().h_px().w_full().bg(tokens.colors().border_subtle);
        v_flex()
            .w_full()
            .gap_2()
            .child(self.facet_heading(
                Picker::Labels,
                rust_i18n::t!("detail.labels").to_string(),
                cx,
            ))
            .child(label_values)
            .children(label_picker)
            .child(rule())
            .child(self.facet_heading(
                Picker::Assignees,
                rust_i18n::t!("detail.assignees").to_string(),
                cx,
            ))
            .child(assignee_values)
            .children(assignee_picker)
            .child(rule())
            .child(self.facet_heading(
                Picker::Projects,
                rust_i18n::t!("detail.projects").to_string(),
                cx,
            ))
            .child(project_values)
            .children(project_picker)
            .into_any_element()
    }

    /// What the checks came to, whether the branch conflicts, and the
    /// merge — GitHub's own card, rebuilt.
    fn merge_card(
        &self,
        key: &ItemKey,
        detail: &crate::store::Detail,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tokens = Tokens::global(cx).clone();
        let pull = detail.pull.as_ref()?;
        let item = &detail.item;
        let open = item.status == e1_github::Status::Open;
        let draft = matches!(item.kind, e1_github::Kind::Pull { draft: true, .. });
        if !open {
            return None;
        }
        let checks = self
            .store
            .read(cx)
            .checks(&key.0, &pull.head_sha)
            .and_then(|fetch| fetch.value())
            .cloned();
        let busy = self
            .store
            .read(cx)
            .action(key)
            .is_some_and(|action| action.is_loading());
        let green = tokens.colors().status_done;
        let red = tokens.colors().status_error;
        let amber = tokens.colors().status_attention;
        let muted = tokens.colors().text_muted;

        let status_row =
            |icon: AnyElement, title: String, subtitle: String, trailing: Option<AnyElement>| {
                h_flex()
                    .w_full()
                    .px_4()
                    .py_3()
                    .gap_3()
                    .items_center()
                    .child(icon)
                    .child(
                        v_flex()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_medium()
                                    .text_color(tokens.colors().text_primary)
                                    .child(title),
                            )
                            .child(div().text_size(px(11.5)).text_color(muted).child(subtitle)),
                    )
                    .children(trailing)
            };
        let badge = |color: Hsla, icon: IconName| {
            div()
                .size_6()
                .flex_shrink_0()
                .rounded_full()
                .bg(color)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(icon)
                        .size_3p5()
                        .text_color(tokens.colors().bg_window),
                )
                .into_any_element()
        };

        // Checks.
        let (passed, failed, pending) = checks.as_ref().map(|c| c.tally()).unwrap_or_default();
        let overall = checks.as_ref().map(|c| c.overall());
        let (check_icon, check_title, check_sub) = match overall {
            None => (
                badge(muted, IconName::LoaderCircle),
                rust_i18n::t!("checks.unknown").to_string(),
                String::new(),
            ),
            Some(CheckState::Neutral) => (
                badge(muted, IconName::Minus),
                rust_i18n::t!("checks.none").to_string(),
                String::new(),
            ),
            Some(CheckState::Success) => (
                badge(green, IconName::Check),
                rust_i18n::t!("checks.passed").to_string(),
                rust_i18n::t!("checks.passed_count", count = passed).to_string(),
            ),
            Some(CheckState::Failure) => (
                badge(red, IconName::Close),
                rust_i18n::t!("checks.failed").to_string(),
                rust_i18n::t!("checks.failed_count", failed = failed, passed = passed).to_string(),
            ),
            Some(CheckState::Pending) => (
                badge(amber, IconName::LoaderCircle),
                rust_i18n::t!("checks.pending").to_string(),
                rust_i18n::t!("checks.pending_count", count = pending).to_string(),
            ),
        };
        let has_runs = checks.as_ref().is_some_and(|c| !c.runs.is_empty());
        let chevron = has_runs.then(|| {
            Icon::new(if self.checks_open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .size_3p5()
            .text_color(muted)
            .into_any_element()
        });
        let checks_row = div()
            .id("checks-row")
            .w_full()
            .when(has_runs, |this| this.cursor_pointer())
            .child(status_row(check_icon, check_title, check_sub, chevron))
            .on_click(cx.listener(|this, _, _, cx| {
                this.checks_open = !this.checks_open;
                cx.notify();
            }));
        let runs: Vec<AnyElement> = if self.checks_open {
            checks
                .as_ref()
                .map(|checks| {
                    checks
                        .runs
                        .iter()
                        .enumerate()
                        .map(|(index, run)| {
                            let (color, icon) = match run.state {
                                CheckState::Success => (green, IconName::Check),
                                CheckState::Failure => (red, IconName::Close),
                                CheckState::Pending => (amber, IconName::LoaderCircle),
                                CheckState::Neutral => (muted, IconName::Minus),
                            };
                            let url = run.html_url.clone();
                            h_flex()
                                .w_full()
                                .pl(px(52.))
                                .pr_4()
                                .py_1p5()
                                .gap_2()
                                .items_center()
                                .border_t_1()
                                .border_color(tokens.colors().border_subtle)
                                .child(Icon::new(icon).size_3p5().text_color(color))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(11.5))
                                        .text_color(tokens.colors().text_secondary)
                                        .truncate()
                                        .child(run.name.clone()),
                                )
                                .children(url.map(|url| {
                                    div()
                                        .id(("check-details", index))
                                        .text_size(px(11.5))
                                        .text_color(tokens.colors().accent)
                                        .cursor_pointer()
                                        .child(rust_i18n::t!("checks.details").to_string())
                                        .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url)))
                                }))
                                .into_any_element()
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        // Conflicts.
        let conflicts_row = match pull.mergeable {
            Some(true) => status_row(
                badge(green, IconName::Check),
                rust_i18n::t!("conflicts.none").to_string(),
                rust_i18n::t!("conflicts.auto").to_string(),
                None,
            ),
            Some(false) => status_row(
                badge(red, IconName::Close),
                rust_i18n::t!("conflicts.some").to_string(),
                String::new(),
                None,
            ),
            None => status_row(
                badge(muted, IconName::LoaderCircle),
                rust_i18n::t!("conflicts.unknown").to_string(),
                String::new(),
                None,
            ),
        };

        // The merge.
        let can_merge = !draft && pull.mergeable != Some(false) && !busy;
        let method = self.merge_method;
        let button_label = rust_i18n::t!(match (self.confirm_merge, method) {
            (false, MergeMethod::Merge) => "merge.button.merge",
            (false, MergeMethod::Squash) => "merge.button.squash",
            (false, MergeMethod::Rebase) => "merge.button.rebase",
            (true, MergeMethod::Merge) => "merge.confirm.merge",
            (true, MergeMethod::Squash) => "merge.confirm.squash",
            (true, MergeMethod::Rebase) => "merge.confirm.rebase",
        })
        .to_string();
        let button_color = if can_merge { green } else { muted };
        let merge_button = h_flex()
            .rounded(px(tokens.radius.control()))
            .overflow_hidden()
            .child(
                div()
                    .id("merge")
                    .px_3()
                    .py_1p5()
                    .bg(button_color)
                    .text_size(px(12.))
                    .font_medium()
                    .text_color(gpui::white())
                    .when(can_merge, |this| {
                        this.cursor_pointer()
                            .hover(|this| this.opacity(0.9))
                            .on_click(cx.listener(|this, _, _, cx| this.press_merge(cx)))
                    })
                    .child(button_label),
            )
            .child(
                div()
                    .id("merge-menu")
                    .px_2()
                    .py_1p5()
                    .bg(button_color)
                    .border_l_1()
                    .border_color(gpui::white().opacity(0.25))
                    .when(can_merge, |this| {
                        this.cursor_pointer()
                            .hover(|this| this.opacity(0.9))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.merge_menu = !this.merge_menu;
                                cx.notify();
                            }))
                    })
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .size_3p5()
                            .text_color(gpui::white()),
                    ),
            );
        let cancel = self.confirm_merge.then(|| {
            self.button(
                "merge-cancel",
                rust_i18n::t!("detail.merge.cancel").to_string(),
                false,
                cx,
                |this, cx| {
                    this.confirm_merge = false;
                    cx.notify();
                },
            )
        });
        let menu = self.merge_menu.then(|| {
            v_flex()
                .w_full()
                .max_w(px(420.))
                .mt_1()
                .rounded(px(tokens.radius.panel))
                .bg(tokens.colors().bg_raised)
                .border_1()
                .border_color(tokens.colors().border_strong)
                .overflow_hidden()
                .children(
                    MergeMethod::ALL
                        .iter()
                        .enumerate()
                        .map(|(index, candidate)| {
                            let candidate = *candidate;
                            let (title, desc) = match candidate {
                                MergeMethod::Merge => {
                                    ("merge.method.merge.title", "merge.method.merge.desc")
                                }
                                MergeMethod::Squash => {
                                    ("merge.method.squash.title", "merge.method.squash.desc")
                                }
                                MergeMethod::Rebase => {
                                    ("merge.method.rebase.title", "merge.method.rebase.desc")
                                }
                            };
                            self.picker_row(
                                ("merge-method", index),
                                div().into_any_element(),
                                rust_i18n::t!(title).to_string(),
                                Some(rust_i18n::t!(desc).to_string()),
                                candidate == method,
                                cx,
                                move |this, cx| {
                                    this.merge_method = candidate;
                                    this.merge_menu = false;
                                    this.confirm_merge = false;
                                    cx.notify();
                                },
                            )
                        }),
                )
        });

        Some(
            v_flex()
                .w_full()
                .rounded(px(tokens.radius.panel))
                .bg(tokens.colors().bg_surface)
                .border_1()
                .border_color(match overall {
                    Some(CheckState::Failure) => red.opacity(0.5),
                    Some(CheckState::Success) if pull.mergeable == Some(true) => green.opacity(0.5),
                    _ => tokens.colors().border_subtle,
                })
                .overflow_hidden()
                .child(checks_row)
                .children(runs)
                .child(div().h_px().w_full().bg(tokens.colors().border_subtle))
                .child(conflicts_row)
                .child(div().h_px().w_full().bg(tokens.colors().border_subtle))
                .child(
                    v_flex()
                        .w_full()
                        .px_4()
                        .py_3()
                        .gap_2()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(merge_button)
                                .children(cancel)
                                .when(draft, |this| {
                                    this.child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(muted)
                                            .child(rust_i18n::t!("state.draft").to_string()),
                                    )
                                }),
                        )
                        .children(menu),
                )
                .into_any_element(),
        )
    }

    /// An item, in either of its tabs.
    fn item(&self, key: ItemKey, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::global(cx).clone();
        let mono = gpui_component::Theme::global(cx).mono_font_family.clone();
        let fetch = self.store.read(cx).detail(&key).cloned();
        let (detail, error, loading) = match &fetch {
            Some(fetch) => (
                fetch.value().cloned(),
                fetch.error().map(str::to_string),
                fetch.is_loading(),
            ),
            None => (None, None, false),
        };
        let Some(detail) = detail else {
            return match error {
                Some(error) => self.notice(error, true, cx),
                None if loading => crate::skeleton::detail(cx),
                None => self.notice(rust_i18n::t!("detail.empty").to_string(), false, cx),
            };
        };

        let item = &detail.item;
        let glyph = Glyph::for_item(item);
        let glyph_color = glyph.role().color(tokens.colors());
        let muted = tokens.colors().text_muted;
        let labels: Vec<LabelChip> = item
            .labels
            .iter()
            .map(|label| LabelChip::new(&label.name, &label.color, muted))
            .collect();
        let tabs = detail
            .pull
            .as_ref()
            .map(|pull| self.tabs(pull.changed_files, cx));
        let showing_files = self.tab == Tab::Files && detail.pull.is_some();

        let head = v_flex()
            .w_full()
            .px_5()
            .pt_4()
            .pb_3()
            .gap_2()
            .border_b_1()
            .border_color(tokens.colors().border_subtle)
            .child(
                h_flex()
                    .gap_2()
                    .items_baseline()
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(muted)
                            .child(format!("#{}", item.number)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(14.))
                            .font_medium()
                            .text_color(tokens.colors().text_primary)
                            .child(item.title.clone()),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .flex_wrap()
                    .child(
                        h_flex()
                            .gap_1p5()
                            .items_center()
                            .px_2()
                            .py_0p5()
                            .rounded(px(tokens.radius.row))
                            .bg(glyph_color.opacity(0.18))
                            .child(
                                Icon::empty()
                                    .path(glyph.icon())
                                    .size_3p5()
                                    .text_color(glyph_color),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_medium()
                                    .text_color(glyph_color)
                                    .child(rust_i18n::t!(glyph.label_key()).to_string()),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1p5()
                            .items_center()
                            .child(avatar(
                                self.store.read(cx).avatar(&item.author.avatar_url),
                                &item.author.login,
                                px(18.),
                                cx,
                            ))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(tokens.colors().text_secondary)
                                    .child(format!(
                                        "{} · {}",
                                        item.author.login,
                                        age(Utc::now(), item.created_at)
                                    )),
                            ),
                    )
                    .children(detail.pull.as_ref().map(|pull| {
                        h_flex()
                            .gap_3()
                            .items_center()
                            .text_size(px(11.5))
                            .child(
                                div()
                                    .px_1p5()
                                    .rounded(px(tokens.radius.row))
                                    .bg(tokens.colors().code_bg)
                                    .text_color(tokens.colors().text_secondary)
                                    .child(
                                        rust_i18n::t!(
                                            "detail.wants_to_merge",
                                            head = pull.head,
                                            base = pull.base
                                        )
                                        .to_string(),
                                    ),
                            )
                            .child(
                                div()
                                    .text_color(tokens.colors().status_done)
                                    .child(format!("+{}", pull.additions)),
                            )
                            .child(
                                div()
                                    .text_color(tokens.colors().status_error)
                                    .child(format!("−{}", pull.deletions)),
                            )
                            .child(
                                div().text_color(muted).child(
                                    rust_i18n::t!("detail.files", count = pull.changed_files)
                                        .to_string(),
                                ),
                            )
                    })),
            )
            .child(self.actions(&key, &detail, cx))
            .children(tabs);

        let body: AnyElement = if showing_files {
            self.diff_list(&key, mono, cx)
        } else {
            let comments: Vec<AnyElement> = detail
                .comments
                .iter()
                .map(|comment| self.comment(comment, cx))
                .collect();
            let merge_card = self.merge_card(&key, &detail, cx);
            let facets = self.facets(&key, &detail, &labels, cx);
            v_flex()
                .id("detail-scroll")
                .flex_1()
                // Without a floor of zero the scroll takes its content's
                // height and pushes the composer under the window.
                .min_h_0()
                .overflow_y_scroll()
                .px_5()
                .py_4()
                .child(
                    v_flex()
                        .w_full()
                        .max_w(px(MEASURE))
                        .gap_4()
                        .child(facets)
                        .child(div().h_px().w_full().bg(tokens.colors().border_subtle))
                        .children(merge_card)
                        .child(if item.body.trim().is_empty() {
                            div()
                                .text_size(px(13.))
                                .text_color(muted)
                                .child(rust_i18n::t!("detail.no_body").to_string())
                                .into_any_element()
                        } else {
                            div()
                                .text_size(px(14.))
                                .line_height(relative(1.6))
                                .child(
                                    TextView::markdown(
                                        SharedString::from(format!(
                                            "body:{}/{}",
                                            item.repo, item.number
                                        )),
                                        item.body.clone(),
                                    )
                                    .selectable(true),
                                )
                                .into_any_element()
                        })
                        .when(!comments.is_empty(), |this| {
                            this.child(div().h_px().w_full().bg(tokens.colors().border_subtle))
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(muted)
                                        .child(rust_i18n::t!("detail.comments").to_string()),
                                )
                                .children(comments)
                        }),
                )
                .into_any_element()
        };
        let composer = (!showing_files).then(|| self.composer(item.is_pull(), cx));

        v_flex()
            .size_full()
            .child(head)
            .child(body)
            .children(composer)
            .into_any_element()
    }

    /// The comment box at the foot of the conversation, always in view:
    /// a box that scrolled away with the thread had its button below the
    /// fold more often than not. For a pull the same words can be a
    /// review — approving, or asking for changes — so those are here too.
    fn composer(&self, is_pull: bool, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::global(cx).clone();
        let mut buttons = h_flex().w_full().justify_end().gap_1p5().items_center();
        if is_pull {
            buttons = buttons
                .child(self.button(
                    "request-changes",
                    rust_i18n::t!("detail.review.request_changes").to_string(),
                    false,
                    cx,
                    |this, cx| this.send_review(ReviewEvent::RequestChanges, cx),
                ))
                .child(self.button(
                    "approve",
                    rust_i18n::t!("detail.review.approve").to_string(),
                    false,
                    cx,
                    |this, cx| this.send_review(ReviewEvent::Approve, cx),
                ));
        }
        buttons = buttons.child(self.button(
            "send-comment",
            rust_i18n::t!("detail.comment.send").to_string(),
            true,
            cx,
            |this, cx| this.send_comment(cx),
        ));
        div()
            .w_full()
            .flex_shrink_0()
            .px_5()
            .pb_4()
            .pt_2()
            .border_t_1()
            .border_color(tokens.colors().border_subtle)
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(MEASURE))
                    .rounded(px(tokens.radius.control() + 2.))
                    .bg(tokens.colors().bg_surface)
                    .border_1()
                    .border_color(tokens.colors().border_subtle)
                    .p_1()
                    .gap_1()
                    .child(Textarea::new(&self.composer))
                    .child(buttons.pr_1().pb_0p5()),
            )
            .into_any_element()
    }

    /// A file out of the tree.
    fn file(&self, key: FileKey, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::global(cx).clone();
        let mono = gpui_component::Theme::global(cx).mono_font_family.clone();
        let fetch = self.store.read(cx).content(&key).cloned();
        let (content, error) = match &fetch {
            Some(fetch) => (fetch.value().cloned(), fetch.error().map(str::to_string)),
            None => (None, None),
        };
        let head = v_flex()
            .w_full()
            .px_5()
            .pt_4()
            .pb_3()
            .gap_1()
            .border_b_1()
            .border_color(tokens.colors().border_subtle)
            .child(
                div()
                    .text_size(px(13.))
                    .font_family(mono.clone())
                    .text_color(tokens.colors().text_primary)
                    .child(key.1.clone()),
            )
            .child(
                h_flex()
                    .gap_3()
                    .text_size(px(11.5))
                    .text_color(tokens.colors().text_muted)
                    .child(key.0.to_string())
                    .children(content.as_ref().map(|content| {
                        rust_i18n::t!("file.bytes", count = content.size).to_string()
                    }))
                    .when(!self.lines.is_empty(), |this| {
                        this.child(
                            rust_i18n::t!("file.lines", count = self.lines.len()).to_string(),
                        )
                    }),
            );

        let body: AnyElement = match content {
            Some(content) if content.text.is_some() => {
                let this = cx.entity();
                uniform_list("code", self.lines.len(), move |range, _window, cx| {
                    this.update(cx, |this, cx| {
                        range
                            .map(|index| this.code_row(index, mono.clone(), cx))
                            .collect()
                    })
                })
                .flex_1()
                .size_full()
                .py_1()
                .into_any_element()
            }
            Some(_) => self.notice(rust_i18n::t!("file.too_large").to_string(), false, cx),
            None => match error {
                Some(error) => self.notice(error, true, cx),
                None => crate::skeleton::diff(cx),
            },
        };
        v_flex()
            .size_full()
            .child(head)
            .child(body)
            .into_any_element()
    }
}

impl Render for Detail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.clear_composer {
            self.clear_composer = false;
            self.composer
                .update(cx, |composer, cx| composer.set_value("", window, cx));
        }
        if self.clear_filter {
            self.clear_filter = false;
            self.filter
                .update(cx, |filter, cx| filter.set_value("", window, cx));
        }
        let body = match self.showing.clone() {
            None => self.notice(rust_i18n::t!("detail.empty").to_string(), false, cx),
            Some(Showing::Item(key)) => self.item(key, cx),
            Some(Showing::File(key)) => self.file(key, cx),
        };
        v_flex().size_full().child(body)
    }
}
