//! The right column: `docs/ui.md` §3.4.
//!
//! One item, read in full: the header, the labels, the description and the
//! comments. It draws whatever the store has for the item it was told to
//! show, so opening the same item twice costs nothing and a refresh keeps
//! the old text on screen until the new one lands.

use crate::store::{ItemKey, Store, StoreEvent};
use chrono::Utc;
use e1_github::{Comment, FileStatus, PullFile};
use e1_ui::Tokens;
use e1_ui::diff;
use e1_ui::rows::{Glyph, LabelChip};
use e1_ui::time::age;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::text::TextView;
use gpui_component::{Icon, StyledExt as _, h_flex, v_flex};

/// The reading measure, in pixels. Long-form text stays readable because the
/// column stops growing, not because the window does.
const MEASURE: f32 = 720.;

/// Which half of a pull is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    /// The description and the comments.
    Conversation,
    /// The files and their diffs.
    Files,
}

/// The right column.
pub struct Detail {
    store: Entity<Store>,
    showing: Option<ItemKey>,
    tab: Tab,
    /// The file whose diff is open. One at a time: a review is read file by
    /// file, and nine diffs at once is a wall.
    expanded: Option<String>,
}

impl Detail {
    /// A detail over a store, showing nothing until told what to.
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.subscribe(&store, |_, _, _: &StoreEvent, cx| cx.notify())
            .detach();
        Self {
            store,
            showing: None,
            tab: Tab::Conversation,
            expanded: None,
        }
    }

    /// Show an item, fetching it if it never has been.
    pub fn show(&mut self, key: ItemKey, is_pull: Option<bool>, cx: &mut Context<Self>) {
        if self.showing.as_ref() != Some(&key) {
            self.tab = Tab::Conversation;
            self.expanded = None;
        }
        self.showing = Some(key.clone());
        self.store
            .update(cx, |store, cx| store.ensure_detail(key, is_pull, cx));
        cx.notify();
    }

    /// Fetch the item again, and its files if they are on screen.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self.showing.clone() {
            let files = self.tab == Tab::Files;
            self.store.update(cx, |store, cx| {
                store.load_detail(key.clone(), None, cx);
                if files {
                    store.load_files(key, cx);
                }
            });
        }
    }

    /// Switch to the files of the pull on screen, with one of them open.
    pub fn show_files(&mut self, expanded: Option<String>, cx: &mut Context<Self>) {
        self.expanded = expanded;
        self.set_tab(Tab::Files, cx);
    }

    fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        if tab == Tab::Files
            && let Some(key) = self.showing.clone()
        {
            self.store
                .update(cx, |store, cx| store.ensure_files(key, cx));
        }
        cx.notify();
    }

    fn toggle_file(&mut self, name: String, cx: &mut Context<Self>) {
        self.expanded = if self.expanded.as_deref() == Some(&name) {
            None
        } else {
            Some(name)
        };
        cx.notify();
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

    /// One file: its row, and its diff when it is the open one.
    fn file(&self, index: usize, file: &PullFile, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::global(cx).clone();
        let mono = gpui_component::Theme::global(cx).mono_font_family.clone();
        let expanded = self.expanded.as_deref() == Some(file.filename.as_str());
        let name = file.filename.clone();
        let mark_color = match file.status {
            FileStatus::Added => tokens.colors().status_done,
            FileStatus::Removed => tokens.colors().status_error,
            FileStatus::Renamed => tokens.colors().accent,
            FileStatus::Modified | FileStatus::Other => tokens.colors().status_attention,
        };
        let row = h_flex()
            .id(("file", index))
            .w_full()
            .px_2()
            .py_1p5()
            .gap_2()
            .items_center()
            .rounded(px(tokens.radius.row))
            .cursor_pointer()
            .when(expanded, |this| this.bg(tokens.colors().row_active()))
            .hover(|this| this.bg(tokens.colors().row_hover()))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_file(name.clone(), cx)))
            .child(
                div()
                    .w_4()
                    .text_xs()
                    .font_family(mono.clone())
                    .text_color(mark_color)
                    .child(file.status.letter()),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_xs()
                    .font_family(mono.clone())
                    .text_color(tokens.colors().text_primary)
                    .truncate()
                    .child(match &file.previous_filename {
                        Some(previous) => format!("{previous} → {}", file.filename),
                        None => file.filename.clone(),
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(tokens.colors().status_done)
                    .child(format!("+{}", file.additions)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(tokens.colors().status_error)
                    .child(format!("−{}", file.deletions)),
            );

        let mut column = v_flex().w_full().child(row);
        if expanded {
            column = column.child(match &file.patch {
                Some(patch) => self.patch(index, patch, mono, cx),
                None => div()
                    .px_2()
                    .py_2()
                    .text_xs()
                    .text_color(tokens.colors().text_muted)
                    .child(rust_i18n::t!("detail.file.no_diff").to_string())
                    .into_any_element(),
            });
        }
        column.into_any_element()
    }

    /// A diff, line by line, in a box that scrolls sideways so a long line
    /// does not wrap the whole column.
    fn patch(&self, index: usize, patch: &str, mono: SharedString, cx: &App) -> AnyElement {
        let tokens = Tokens::global(cx);
        let lines = diff::parse(patch);
        let added = tokens.colors().status_done.opacity(0.12);
        let removed = tokens.colors().status_error.opacity(0.12);
        let hunk = tokens.colors().code_bg;
        v_flex()
            .id(("patch", index))
            .w_full()
            .my_1()
            .rounded(px(tokens.radius.panel))
            .bg(tokens.colors().bg_surface)
            .border_1()
            .border_color(tokens.colors().border_subtle)
            .overflow_x_scroll()
            .py_1()
            .children(lines.into_iter().enumerate().map(|(n, line)| {
                let (fill, color, marker) = match line.kind {
                    diff::Kind::Added => (Some(added), tokens.colors().text_primary, "+"),
                    diff::Kind::Removed => (Some(removed), tokens.colors().text_secondary, "-"),
                    diff::Kind::Hunk => (Some(hunk), tokens.colors().text_muted, ""),
                    diff::Kind::Context => (None, tokens.colors().text_secondary, " "),
                };
                let number = |value: Option<u32>| {
                    div()
                        .w(px(36.))
                        .flex_shrink_0()
                        .text_right()
                        .pr_1()
                        .text_color(tokens.colors().text_muted)
                        .child(value.map(|v| v.to_string()).unwrap_or_default())
                };
                h_flex()
                    .id(("line", n))
                    .min_w_full()
                    .px_1()
                    .font_family(mono.clone())
                    .text_xs()
                    .whitespace_nowrap()
                    .when_some(fill, |this, fill| this.bg(fill))
                    .child(number(line.old))
                    .child(number(line.new))
                    .child(div().w_3().flex_shrink_0().text_color(color).child(marker))
                    .child(div().text_color(color).child(line.text))
            }))
            .into_any_element()
    }

    /// The files tab's body.
    fn files(&self, key: &ItemKey, cx: &mut Context<Self>) -> AnyElement {
        let fetch = self.store.read(cx).files(key).cloned();
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
            Some(files) => v_flex()
                .w_full()
                .gap_0p5()
                .children(
                    files
                        .iter()
                        .enumerate()
                        .map(|(index, file)| self.file(index, file, cx)),
                )
                .into_any_element(),
            None => match error {
                Some(error) => self.notice(error, true, cx),
                None if loading => {
                    self.notice(rust_i18n::t!("detail.files.loading").to_string(), false, cx)
                }
                None => self.notice(rust_i18n::t!("detail.files.empty").to_string(), false, cx),
            },
        }
    }

    /// What is showing.
    pub fn showing(&self) -> Option<&ItemKey> {
        self.showing.as_ref()
    }

    /// Where the item on screen lives on the web, when it is known.
    pub fn html_url(&self, cx: &App) -> Option<String> {
        let key = self.showing.as_ref()?;
        self.store
            .read(cx)
            .detail(key)
            .and_then(|detail| detail.value())
            .map(|detail| detail.item.html_url.clone())
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
        let initial: SharedString = comment
            .author
            .login
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_default()
            .into();
        v_flex()
            .w_full()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .size_5()
                            .rounded_full()
                            .bg(tokens.colors().row_active())
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .text_color(tokens.colors().text_primary)
                            .child(initial),
                    )
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
}

impl Render for Detail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::global(cx).clone();
        let Some(key) = self.showing.clone() else {
            return v_flex().size_full().child(self.notice(
                rust_i18n::t!("detail.empty").to_string(),
                false,
                cx,
            ));
        };
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
            let notice = match error {
                Some(error) => self.notice(error, true, cx),
                None if loading => {
                    self.notice(rust_i18n::t!("detail.loading").to_string(), false, cx)
                }
                None => self.notice(rust_i18n::t!("detail.empty").to_string(), false, cx),
            };
            return v_flex().size_full().child(notice);
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
        let comments: Vec<AnyElement> = detail
            .comments
            .iter()
            .map(|comment| self.comment(comment, cx))
            .collect();
        let tabs = detail
            .pull
            .as_ref()
            .map(|pull| self.tabs(pull.changed_files, cx));
        let showing_files = self.tab == Tab::Files && detail.pull.is_some();
        let files = showing_files.then(|| self.files(&key, cx));

        v_flex().size_full().child(
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
                        // The head: number, title, then the state and the facts.
                        .child(
                            v_flex()
                                .gap_2()
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
                                                        .child(
                                                            rust_i18n::t!(glyph.label_key())
                                                                .to_string(),
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(tokens.colors().text_secondary)
                                                .child(format!(
                                                    "{} · {}",
                                                    item.author.login,
                                                    age(Utc::now(), item.created_at)
                                                )),
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
                                                        rust_i18n::t!(
                                                            "detail.files",
                                                            count = pull.changed_files
                                                        )
                                                        .to_string(),
                                                    ),
                                                )
                                        })),
                                )
                                .when(!labels.is_empty(), |this| {
                                    this.child(h_flex().gap_1p5().flex_wrap().children(
                                        labels.iter().map(|label| {
                                            div()
                                                .px_2()
                                                .py_0p5()
                                                .rounded(px(tokens.radius.row))
                                                .bg(label.fill())
                                                .text_xs()
                                                .text_color(label.color)
                                                .child(label.name.clone())
                                        }),
                                    ))
                                }),
                        )
                        .children(tabs)
                        .child(div().h_px().w_full().bg(tokens.colors().border_subtle))
                        .children(files)
                        // The description.
                        .when(!showing_files, |this| {
                            this.child(if item.body.trim().is_empty() {
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(rust_i18n::t!("detail.no_body").to_string())
                                    .into_any_element()
                            } else {
                                TextView::markdown(
                                    SharedString::from(format!(
                                        "body:{}/{}",
                                        item.repo, item.number
                                    )),
                                    item.body.clone(),
                                )
                                .selectable(true)
                                .into_any_element()
                            })
                        })
                        // The timeline.
                        .when(!showing_files && !comments.is_empty(), |this| {
                            this.child(div().h_px().w_full().bg(tokens.colors().border_subtle))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(rust_i18n::t!("detail.comments").to_string()),
                                )
                                .children(comments)
                        }),
                ),
        )
    }
}
