//! The navigation column: `docs/ui.md` §3.2.
//!
//! Four fixed sections, then the repositories. It reads the store and emits
//! what the reader picked; the shell decides what the centre column does
//! with that, so the sidebar does not know what a list is and the shell does
//! not know how a row is drawn.

use crate::store::{Store, StoreEvent};
use e1_github::Repo;
use e1_ui::assets::icon;
use e1_ui::{Focus, Section, Tokens};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::tooltip::Tooltip;
use gpui_component::{Icon, IconName, StyledExt as _, h_flex, v_flex};

/// Emitted when the reader picks a row.
pub enum SidebarEvent {
    /// Show this in the centre column.
    Focus(Focus),
    /// Forget the token and go back to the sign-in screen.
    SignOut,
}

impl EventEmitter<SidebarEvent> for Sidebar {}

/// The navigation column.
pub struct Sidebar {
    store: Entity<Store>,
    selected: Option<Focus>,
}

impl Sidebar {
    /// A sidebar over a store. It redraws whenever the store changes, because
    /// the inbox count and the repository list are the store's.
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.subscribe(&store, |_, _, _: &StoreEvent, cx| cx.notify())
            .detach();
        Self {
            store,
            selected: None,
        }
    }

    /// Pick something and say so.
    pub fn select(&mut self, focus: Focus, cx: &mut Context<Self>) {
        self.selected = Some(focus.clone());
        cx.emit(SidebarEvent::Focus(focus));
        cx.notify();
    }

    /// Move the highlight without announcing it, for when the shell changed
    /// the list itself (a kind toggle) and announcing it back would have the
    /// shell answer its own message.
    pub fn adopt(&mut self, focus: Focus, cx: &mut Context<Self>) {
        self.selected = Some(focus);
        cx.notify();
    }

    /// Highlight nothing: a search belongs to no row.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        cx.notify();
    }

    /// What is highlighted.
    pub fn selected(&self) -> Option<&Focus> {
        self.selected.as_ref()
    }

    /// The app's name, and whose GitHub this is.
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let tokens = Tokens::global(cx);
        let login: SharedString = self
            .store
            .read(cx)
            .viewer()
            .value()
            .map(|viewer| viewer.login.clone().into())
            .unwrap_or_else(|| rust_i18n::t!("app.signed_out").to_string().into());
        h_flex()
            .w_full()
            .px_3()
            .py_2p5()
            .gap_2()
            .items_center()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(tokens.colors().text_primary)
                    .child(rust_i18n::t!("app.name").to_string()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(tokens.colors().text_muted)
                    .truncate()
                    .child(login),
            )
    }

    /// A small muted label over a run of rows.
    fn section_label(&self, label: String, cx: &App) -> impl IntoElement + use<> {
        let tokens = Tokens::global(cx);
        div()
            .w_full()
            .px_3()
            .pt_3()
            .pb_1()
            .text_xs()
            .text_color(tokens.colors().text_muted)
            .child(label)
    }

    /// One of the four fixed rows.
    fn section_row(&self, section: Section, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let tokens = Tokens::global(cx);
        let focus = Focus::Section(section);
        let selected = self.selected.as_ref() == Some(&focus);
        let count: Option<usize> = match section {
            Section::Inbox => self
                .store
                .read(cx)
                .inbox()
                .value()
                .map(|inbox| inbox.iter().filter(|n| n.unread).count())
                .filter(|count| *count > 0),
            other => self
                .store
                .read(cx)
                .list(&Focus::Section(other))
                .and_then(|list| list.value())
                .map(|items| items.len())
                .filter(|count| *count > 0),
        };
        h_flex()
            .id(SharedString::from(format!("section:{section:?}")))
            .w_full()
            .px_2p5()
            .py_1p5()
            .gap_2()
            .items_center()
            .rounded(px(tokens.radius.row))
            .cursor_pointer()
            .when(selected, |this| this.bg(tokens.colors().row_active()))
            .hover(|this| this.bg(tokens.colors().row_hover()))
            .on_click(cx.listener(move |this, _, _, cx| this.select(focus.clone(), cx)))
            .child(
                Icon::empty()
                    .path(section.icon())
                    .size_4()
                    .text_color(if selected {
                        tokens.colors().text_primary
                    } else {
                        tokens.colors().text_secondary
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .text_sm()
                    .when(selected, |this| this.font_medium())
                    .text_color(if selected {
                        tokens.colors().text_primary
                    } else {
                        tokens.colors().text_secondary
                    })
                    .child(rust_i18n::t!(section.label_key()).to_string()),
            )
            .children(count.map(|count| {
                div()
                    .text_xs()
                    .text_color(if section == Section::Inbox {
                        tokens.colors().accent
                    } else {
                        tokens.colors().text_muted
                    })
                    .child(count.to_string())
            }))
    }

    /// A repository.
    fn repo_row(
        &self,
        index: usize,
        repo: &Repo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let tokens = Tokens::global(cx);
        let selected = self
            .selected
            .as_ref()
            .and_then(Focus::repo_id)
            .is_some_and(|id| id == &repo.id);
        let id = repo.id.clone();
        h_flex()
            .id(("repo", index))
            .w_full()
            .px_2p5()
            .py_1p5()
            .gap_2()
            .items_center()
            .rounded(px(tokens.radius.row))
            .cursor_pointer()
            .when(selected, |this| this.bg(tokens.colors().row_active()))
            .hover(|this| this.bg(tokens.colors().row_hover()))
            .on_click(cx.listener(move |this, _, _, cx| this.select(Focus::repo(id.clone()), cx)))
            .child(
                h_flex()
                    .flex_1()
                    .overflow_hidden()
                    .text_sm()
                    .when(selected, |this| this.font_medium())
                    .text_color(if selected {
                        tokens.colors().text_primary
                    } else {
                        tokens.colors().text_secondary
                    })
                    .child(
                        div()
                            .text_color(tokens.colors().text_muted)
                            .child(format!("{}/", repo.id.owner)),
                    )
                    .child(div().truncate().child(repo.id.name.clone())),
            )
            .when(repo.private, |this| {
                this.child(
                    Icon::empty()
                        .path(icon::LOCK)
                        .size_3()
                        .text_color(tokens.colors().text_muted),
                )
            })
    }

    /// Whose window this is.
    fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let tokens = Tokens::global(cx);
        let viewer = self.store.read(cx).viewer().value().cloned();
        let signed_in = viewer.is_some();
        let (initial, login): (SharedString, SharedString) = match viewer {
            Some(viewer) => (
                viewer
                    .login
                    .chars()
                    .next()
                    .map(|c| c.to_ascii_uppercase().to_string())
                    .unwrap_or_default()
                    .into(),
                viewer.name.clone().unwrap_or(viewer.login).into(),
            ),
            None => (
                "?".into(),
                rust_i18n::t!("app.signed_out").to_string().into(),
            ),
        };
        h_flex()
            .w_full()
            .px_3()
            .py_2p5()
            .gap_2()
            .items_center()
            .border_t_1()
            .border_color(tokens.colors().border_subtle)
            .child(
                div()
                    .size_6()
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
                    .flex_1()
                    .text_sm()
                    .text_color(tokens.colors().text_secondary)
                    .truncate()
                    .child(login),
            )
            .when(signed_in, |this| {
                this.child(
                    div()
                        .id("sign-out")
                        .p_1()
                        .rounded(px(tokens.radius.row))
                        .cursor_pointer()
                        .hover(|this| this.bg(tokens.colors().row_hover()))
                        .tooltip(|window, cx| {
                            Tooltip::new(rust_i18n::t!("sidebar.sign_out").to_string())
                                .build(window, cx)
                        })
                        .child(
                            Icon::new(IconName::CircleX)
                                .size_3p5()
                                .text_color(tokens.colors().text_muted),
                        )
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(SidebarEvent::SignOut))),
                )
            })
    }
}

impl Render for Sidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::global(cx).clone();
        let repos: Vec<Repo> = self
            .store
            .read(cx)
            .repos()
            .value()
            .cloned()
            .unwrap_or_default();
        let repos_error = self.store.read(cx).repos().error().map(str::to_string);

        let sections: Vec<AnyElement> = Section::ALL
            .iter()
            .map(|section| self.section_row(*section, cx).into_any_element())
            .collect();
        let repo_rows: Vec<AnyElement> = repos
            .iter()
            .enumerate()
            .map(|(index, repo)| self.repo_row(index, repo, cx).into_any_element())
            .collect();

        v_flex()
            .size_full()
            .bg(tokens.colors().bg_sidebar)
            .border_r_1()
            .border_color(tokens.colors().border_subtle)
            .child(self.header(cx))
            .child(
                v_flex()
                    .id("sidebar-scroll")
                    .flex_1()
                    .px_2()
                    .overflow_y_scroll()
                    .children(sections)
                    .child(
                        self.section_label(rust_i18n::t!("sidebar.repositories").to_string(), cx),
                    )
                    .children(repo_rows)
                    .when(repos.is_empty(), |this| {
                        this.child(
                            div()
                                .px_3()
                                .py_1()
                                .text_xs()
                                .text_color(match &repos_error {
                                    Some(_) => tokens.colors().status_error,
                                    None => tokens.colors().text_muted,
                                })
                                .child(repos_error.unwrap_or_else(|| {
                                    rust_i18n::t!("sidebar.repositories.empty").to_string()
                                })),
                        )
                    }),
            )
            .child(self.footer(cx))
    }
}
