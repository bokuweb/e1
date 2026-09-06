//! The right column: `docs/ui.md` §3.4.
//!
//! One thing, read in full: an item — its header, labels, description and
//! comments, or its files and their diffs — or a file out of the tree. The
//! long parts are virtualized: a pull's diffs are one `uniform_list` of
//! rows across every file, and a file's lines are another, so a
//! thousand-line diff costs what the screen shows (`AGENTS.md` rule 7).

use crate::avatar::avatar;
use crate::store::{FileKey, ItemKey, Store, StoreEvent};
use chrono::Utc;
use e1_github::{Comment, FileStatus};
use e1_ui::Tokens;
use e1_ui::diff;
use e1_ui::rows::{Glyph, LabelChip};
use e1_ui::time::age;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::input::{InputEvent, Textarea, TextareaState};
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
    /// The merge button was pressed once; the next press merges.
    confirm_merge: bool,
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
        Self {
            store,
            showing: None,
            tab: Tab::Conversation,
            collapsed: HashSet::new(),
            diff_rows: Vec::new(),
            lines: Vec::new(),
            composer,
            clear_composer: false,
            confirm_merge: false,
        }
    }

    /// Post what is in the composer.
    fn send_comment(&mut self, cx: &mut Context<Self>) {
        let Some(Showing::Item(key)) = self.showing.clone() else {
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

    fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if let Some(Showing::Item(key)) = self.showing.clone() {
            self.store
                .update(cx, |store, cx| store.set_open(key, open, cx));
            cx.notify();
        }
    }

    /// Merge, on the second press. The first only arms the button: a merge
    /// is the one thing here git cannot take back.
    fn merge(&mut self, cx: &mut Context<Self>) {
        if !self.confirm_merge {
            self.confirm_merge = true;
            cx.notify();
            return;
        }
        self.confirm_merge = false;
        if let Some(Showing::Item(key)) = self.showing.clone() {
            self.store.update(cx, |store, cx| store.merge(key, cx));
            cx.notify();
        }
    }

    /// Show an item, fetching it if it never has been.
    pub fn show(&mut self, key: ItemKey, is_pull: Option<bool>, cx: &mut Context<Self>) {
        let showing = Showing::Item(key.clone());
        if self.showing.as_ref() != Some(&showing) {
            self.tab = Tab::Conversation;
            self.collapsed.clear();
            self.confirm_merge = false;
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
            && let Some(Showing::Item(key)) = &self.showing
        {
            let files = self
                .store
                .read(cx)
                .pull_files(key)
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
                self.store.update(cx, |store, cx| {
                    store.load_detail(key.clone(), None, cx);
                    if files {
                        store.load_pull_files(key, cx);
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

    fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        if tab == Tab::Files
            && let Some(Showing::Item(key)) = self.showing.clone()
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

    /// Recompute the virtualized rows from what the store has.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.diff_rows.clear();
        self.lines.clear();
        if let Some(Showing::Item(key)) = &self.showing {
            let urls: Vec<String> = self
                .store
                .read(cx)
                .detail(key)
                .and_then(|fetch| fetch.value())
                .map(|detail| {
                    std::iter::once(detail.item.author.avatar_url.clone())
                        .chain(detail.comments.iter().map(|c| c.author.avatar_url.clone()))
                        .collect()
                })
                .unwrap_or_default();
            self.store.update(cx, |store, cx| {
                for url in urls {
                    store.ensure_avatar(&url, cx);
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
                    .text_sm()
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
                            .text_sm()
                            .font_medium()
                            .text_color(tokens.colors().text_primary)
                            .child(comment.author.login.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(tokens.colors().text_muted)
                            .child(age(Utc::now(), comment.created_at)),
                    ),
            )
            .child(
                div().pl_7().child(
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
                .text_xs()
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
                            .text_xs()
                            .font_family(mono.clone())
                            .text_color(mark_color)
                            .child(status.letter()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_xs()
                            .font_family(mono)
                            .text_color(tokens.colors().text_primary)
                            .truncate()
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(tokens.colors().status_done)
                            .child(format!("+{additions}")),
                    )
                    .child(
                        div()
                            .text_xs()
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
                    .text_xs()
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
                .text_xs()
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
            .text_xs()
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
                None if loading => {
                    self.notice(rust_i18n::t!("detail.files.loading").to_string(), false, cx)
                }
                None => self.notice(rust_i18n::t!("detail.files.empty").to_string(), false, cx),
            },
        }
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
                None if loading => {
                    self.notice(rust_i18n::t!("detail.loading").to_string(), false, cx)
                }
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

        let head =
            v_flex()
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
                                .text_sm()
                                .text_color(muted)
                                .child(format!("#{}", item.number)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(15.))
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
                                        .text_xs()
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
                                        .text_xs()
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
                                .text_xs()
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
                .when(!labels.is_empty(), |this| {
                    this.child(h_flex().gap_1p5().flex_wrap().children(labels.iter().map(
                        |label| {
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded(px(tokens.radius.row))
                                .bg(label.fill())
                                .text_xs()
                                .text_color(label.color)
                                .child(label.name.clone())
                        },
                    )))
                })
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
            v_flex()
                .id("detail-scroll")
                .flex_1()
                .overflow_y_scroll()
                .px_5()
                .py_4()
                .child(
                    v_flex()
                        .w_full()
                        .max_w(px(MEASURE))
                        .gap_4()
                        .child(if item.body.trim().is_empty() {
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child(rust_i18n::t!("detail.no_body").to_string())
                                .into_any_element()
                        } else {
                            TextView::markdown(
                                SharedString::from(format!("body:{}/{}", item.repo, item.number)),
                                item.body.clone(),
                            )
                            .selectable(true)
                            .into_any_element()
                        })
                        .when(!comments.is_empty(), |this| {
                            this.child(div().h_px().w_full().bg(tokens.colors().border_subtle))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(rust_i18n::t!("detail.comments").to_string()),
                                )
                                .children(comments)
                        })
                        .child(self.composer(cx)),
                )
                .into_any_element()
        };

        v_flex()
            .size_full()
            .child(head)
            .child(body)
            .into_any_element()
    }

    /// A small button in the head.
    fn action_button(
        &self,
        id: &'static str,
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
            .rounded(px(tokens.radius.row))
            .cursor_pointer()
            .text_xs()
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

    /// Close, reopen, merge — and what the last one of those said.
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
        let draft = matches!(item.kind, e1_github::Kind::Pull { draft: true, .. });
        let mergeable = open
            && !draft
            && detail
                .pull
                .as_ref()
                .is_some_and(|pull| pull.mergeable != Some(false));

        let mut row = h_flex().gap_2().items_center().flex_wrap();
        if busy {
            row = row.child(
                div()
                    .text_xs()
                    .text_color(tokens.colors().text_muted)
                    .child(rust_i18n::t!("detail.working").to_string()),
            );
        } else if !merged {
            if mergeable {
                let label = if self.confirm_merge {
                    rust_i18n::t!("detail.merge.confirm").to_string()
                } else {
                    rust_i18n::t!("detail.merge").to_string()
                };
                row = row
                    .child(self.action_button("merge", label, true, cx, |this, cx| this.merge(cx)));
            }
            let (label, to_open) = if open {
                (rust_i18n::t!("detail.close").to_string(), false)
            } else {
                (rust_i18n::t!("detail.reopen").to_string(), true)
            };
            row =
                row.child(
                    self.action_button("toggle-open", label, false, cx, move |this, cx| {
                        this.set_open(to_open, cx)
                    }),
                );
        }
        if let Some(complaint) = complaint {
            row = row.child(
                div()
                    .text_xs()
                    .text_color(tokens.colors().status_error)
                    .child(complaint),
            );
        }
        row.into_any_element()
    }

    /// The comment box under the conversation.
    fn composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::global(cx).clone();
        v_flex()
            .w_full()
            .gap_2()
            .pt_2()
            .child(
                div()
                    .w_full()
                    .rounded(px(tokens.radius.card))
                    .bg(tokens.colors().bg_surface)
                    .border_1()
                    .border_color(tokens.colors().border_subtle)
                    .p_1()
                    .child(Textarea::new(&self.composer)),
            )
            .child(h_flex().w_full().justify_end().child(self.action_button(
                "send-comment",
                rust_i18n::t!("detail.comment.send").to_string(),
                true,
                cx,
                |this, cx| this.send_comment(cx),
            )))
            .into_any_element()
    }

    /// A file out of the tree.
    fn file(&self, key: FileKey, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::global(cx).clone();
        let mono = gpui_component::Theme::global(cx).mono_font_family.clone();
        let fetch = self.store.read(cx).content(&key).cloned();
        let (content, error, loading) = match &fetch {
            Some(fetch) => (
                fetch.value().cloned(),
                fetch.error().map(str::to_string),
                fetch.is_loading(),
            ),
            None => (None, None, false),
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
                    .text_sm()
                    .font_family(mono.clone())
                    .text_color(tokens.colors().text_primary)
                    .child(key.1.clone()),
            )
            .child(
                h_flex()
                    .gap_3()
                    .text_xs()
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
                None if loading => {
                    self.notice(rust_i18n::t!("file.loading").to_string(), false, cx)
                }
                None => self.notice(rust_i18n::t!("file.loading").to_string(), false, cx),
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
        let body = match self.showing.clone() {
            None => self.notice(rust_i18n::t!("detail.empty").to_string(), false, cx),
            Some(Showing::Item(key)) => self.item(key, cx),
            Some(Showing::File(key)) => self.file(key, cx),
        };
        v_flex().size_full().child(body)
    }
}
