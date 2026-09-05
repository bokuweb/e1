//! The standalone window: three resizable columns under three header strips,
//! and no title bar (`docs/ui.md` §3.1).
//!
//! This is the one view a host will not mount: it owns the window's controls,
//! the arrangement and its persistence, all of which are the host's when the
//! views are embedded. Everything under it — the sidebar, the list, the
//! detail — is mounted here exactly the way a host would mount it.

use crate::detail::Detail;
use crate::list::{ItemEvent, ItemList};
use crate::sidebar::{Sidebar, SidebarEvent};
use crate::store::{Store, StoreEvent};
use e1_github::{GitHub, ListKind, StatusFilter};
use e1_ui::settings::{self, AppSettings};
use e1_ui::{Focus, HEADER_HEIGHT, Layout, Panel, Paths, TRAFFIC_LIGHT_INSET, Tokens};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::resizable::{ResizableState, h_resizable, resizable_panel};
use gpui_component::tooltip::Tooltip;
use gpui_component::{Icon, IconName, InteractiveElementExt as _, StyledExt as _, h_flex, v_flex};
use std::sync::Arc;

actions!(e1, [ToggleSidebar, ToggleRightPanel, Refresh]);

/// The key context the shell's chords are bound in.
const CONTEXT: &str = "E1Shell";

/// Bind the panel toggles and refresh.
///
/// The chords follow VS Code, because that is the muscle memory everyone
/// using this app already has.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-b", ToggleSidebar, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-b", ToggleSidebar, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-alt-b", ToggleRightPanel, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-alt-b", ToggleRightPanel, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-r", Refresh, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-r", Refresh, Some(CONTEXT)),
    ]);
}

/// The window.
pub struct Shell {
    paths: Paths,
    settings: AppSettings,
    layout: Layout,
    store: Entity<Store>,
    sidebar: Entity<Sidebar>,
    list: Entity<ItemList>,
    detail: Entity<Detail>,
    focus_handle: FocusHandle,
    /// With no title bar the strips are what the window is dragged by, and a
    /// drag is a press that then moved. Set on the press, cleared on the
    /// release, acted on by the first move in between.
    dragging: bool,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    /// Open over a source, with the arrangement the settings remember.
    pub fn new(
        github: Arc<dyn GitHub>,
        paths: Paths,
        settings: AppSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let store = cx.new(|_| Store::new(github));
        let sidebar = cx.new(|cx| Sidebar::new(store.clone(), cx));
        let list = cx.new(|cx| ItemList::new(store.clone(), cx));
        let detail = cx.new(|cx| Detail::new(store.clone(), cx));

        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe(&sidebar, |this, _, event, cx| match event {
            SidebarEvent::Focus(focus) => this.focus_on(focus.clone(), cx),
        }));
        subscriptions.push(cx.subscribe(&list, |this, _, event, cx| match event {
            ItemEvent::Open { key, is_pull } => {
                let (key, is_pull) = (key.clone(), *is_pull);
                this.detail
                    .update(cx, |detail, cx| detail.show(key, Some(is_pull), cx));
                // A row that was opened wants to be read; a closed reading
                // pane would make the click do nothing visible.
                if !this.layout.is_open(Panel::RightPanel) {
                    this.toggle(Panel::RightPanel, cx);
                }
            }
            ItemEvent::OpenUrl(url) => cx.open_url(url),
        }));
        // The headers show what the store knows (a title, a spinner), so a
        // change there is a redraw here.
        subscriptions.push(cx.subscribe(&store, |_, _, _: &StoreEvent, cx| cx.notify()));

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let layout = Layout::from_settings(&settings);
        let this = Self {
            paths,
            settings,
            layout,
            store,
            sidebar,
            list,
            detail,
            focus_handle,
            dragging: false,
            _subscriptions: subscriptions,
        };
        this.store.update(cx, |store, cx| store.refresh_all(cx));
        // The window opens on the inbox, which is the question a person
        // opens GitHub to answer.
        this.sidebar.update(cx, |sidebar, cx| {
            sidebar.select(Focus::Section(e1_ui::Section::Inbox), cx)
        });
        this
    }

    /// Point the centre column at something.
    fn focus_on(&mut self, focus: Focus, cx: &mut Context<Self>) {
        if let Some(repo) = focus.repo_id() {
            self.settings.last_repo = Some(repo.to_string());
            self.persist();
        }
        self.list.update(cx, |list, cx| list.set_focus(focus, cx));
        cx.notify();
    }

    /// Change the list without going through the sidebar: the kind and
    /// status toggles in the centre strip.
    fn refocus(&mut self, focus: Focus, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.adopt(focus.clone(), cx));
        self.list.update(cx, |list, cx| list.set_focus(focus, cx));
        cx.notify();
    }

    fn toggle(&mut self, panel: Panel, cx: &mut Context<Self>) {
        self.layout.toggle(panel);
        self.persist();
        cx.notify();
    }

    fn persist(&mut self) {
        self.layout.write_into(&mut self.settings);
        if let Err(error) = settings::save(&self.paths.app_settings(), &self.settings) {
            tracing::warn!(%error, "could not persist the window's settings");
        }
    }

    /// Store the sizes a divider drag produced.
    fn record_resize(
        &mut self,
        slots: Vec<Option<Panel>>,
        state: &Entity<ResizableState>,
        cx: &mut Context<Self>,
    ) {
        let sizes = state.read(cx).sizes().clone();
        self.layout.record_sizes(&slots, &sizes);
        self.persist();
    }

    fn on_toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle(Panel::Sidebar, cx);
    }

    fn on_toggle_right_panel(
        &mut self,
        _: &ToggleRightPanel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle(Panel::RightPanel, cx);
    }

    fn on_refresh(&mut self, _: &Refresh, _: &mut Window, cx: &mut Context<Self>) {
        self.refresh(cx);
    }

    /// Fetch again what is on screen: the sidebar's lists, the centre list,
    /// and the item being read.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| store.refresh_all(cx));
        self.list.update(cx, |list, cx| list.refresh(cx));
        self.detail.update(cx, |detail, cx| detail.refresh(cx));
    }

    /// A small square control in a header strip.
    fn icon_button(
        &self,
        id: &'static str,
        icon: Icon,
        tip: String,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let tokens = Tokens::global(cx);
        let hover = tokens.colors().bg_raised;
        let radius = px(tokens.radius.row);
        div()
            .id(id)
            .p_1()
            .rounded(radius)
            .cursor_pointer()
            .hover(move |this| this.bg(hover))
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            .child(icon)
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
    }

    /// A control that opens or closes a panel. The icon reports the *state*,
    /// so an open panel shows the "close" variant and reads without hovering.
    fn panel_toggle(
        &self,
        panel: Panel,
        open_icon: IconName,
        closed_icon: IconName,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let open = self.layout.is_open(panel);
        let color = Tokens::global(cx).colors().text_secondary;
        let id = match panel {
            Panel::Sidebar => "toggle-sidebar",
            Panel::RightPanel => "toggle-right",
        };
        self.icon_button(
            id,
            Icon::new(if open { open_icon } else { closed_icon })
                .size_4()
                .text_color(color),
            rust_i18n::t!(panel.label_key()).to_string(),
            cx,
            move |this, _, cx| this.toggle(panel, cx),
        )
    }

    /// Make a strip the window can be dragged by.
    ///
    /// A drag is a press that then moved: acting on the press alone would
    /// carry the window off whenever a control on the strip was clicked.
    fn draggable(&self, strip: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        strip
            .on_double_click(|_, window, _| window.titlebar_double_click())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = false),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, _| this.dragging = false))
            .on_mouse_move(cx.listener(|this, _, window, _| {
                if this.dragging {
                    this.dragging = false;
                    window.start_window_move();
                }
            }))
    }

    /// The window's own controls, across the top of the leading column: room
    /// for the traffic lights, then the sidebar toggle.
    fn window_controls(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let strip = h_flex()
            .id("window-controls")
            .flex_shrink_0()
            .h(HEADER_HEIGHT)
            .pl(TRAFFIC_LIGHT_INSET)
            .gap_3()
            .items_center()
            .child(self.panel_toggle(
                Panel::Sidebar,
                IconName::PanelLeftClose,
                IconName::PanelLeftOpen,
                cx,
            ));
        self.draggable(strip, cx)
    }

    /// A two-way toggle in the centre strip, drawn as a row of chips.
    fn chips<T: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        options: Vec<(T, String)>,
        current: T,
        cx: &mut Context<Self>,
        pick: impl Fn(&mut Self, T, &mut Context<Self>) + Clone + 'static,
    ) -> AnyElement {
        let tokens = Tokens::global(cx).clone();
        h_flex()
            .id(id)
            .gap_0p5()
            .p_0p5()
            .rounded(px(tokens.radius.row))
            .bg(tokens.colors().bg_surface)
            .children(
                options
                    .into_iter()
                    .enumerate()
                    .map(|(index, (value, label))| {
                        let selected = value == current;
                        let pick = pick.clone();
                        div()
                            .id((id, index))
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
                            .on_click(cx.listener(move |this, _, _, cx| pick(this, value, cx)))
                    }),
            )
            .into_any_element()
    }

    /// The strip across the top of the centre column: what the list is, and
    /// the controls for it and for the panels either side.
    fn column_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let tokens = Tokens::global(cx).clone();
        let muted = tokens.colors().text_muted;
        let secondary = tokens.colors().text_secondary;
        let primary = tokens.colors().text_primary;
        let leading = !self.layout.is_open(Panel::Sidebar);
        let focus = self.list.read(cx).focus().cloned();
        let loading = self.list.read(cx).is_loading(cx);
        let title: Option<SharedString> = focus.as_ref().map(|focus| focus.title().into());
        let subtitle: Option<SharedString> = focus
            .as_ref()
            .and_then(|focus| focus.subtitle())
            .map(Into::into);

        let sidebar_toggle = leading.then(|| {
            self.panel_toggle(
                Panel::Sidebar,
                IconName::PanelLeftClose,
                IconName::PanelLeftOpen,
                cx,
            )
        });
        let kind_chips = focus
            .as_ref()
            .and_then(|focus| match focus {
                Focus::Repo { kind, .. } => Some(*kind),
                Focus::Section(_) => None,
            })
            .map(|kind| {
                self.chips(
                    "kind",
                    vec![
                        (ListKind::Pulls, rust_i18n::t!("list.pulls").to_string()),
                        (ListKind::Issues, rust_i18n::t!("list.issues").to_string()),
                    ],
                    kind,
                    cx,
                    |this, kind, cx| {
                        if let Some(focus) = this
                            .list
                            .read(cx)
                            .focus()
                            .and_then(|focus| focus.with_kind(kind))
                        {
                            this.refocus(focus, cx);
                        }
                    },
                )
            });
        let status_chips = focus
            .as_ref()
            .and_then(|focus| match focus {
                Focus::Repo { status, .. } => Some(*status),
                Focus::Section(_) => None,
            })
            .map(|status| {
                self.chips(
                    "status",
                    vec![
                        (StatusFilter::Open, rust_i18n::t!("list.open").to_string()),
                        (
                            StatusFilter::Closed,
                            rust_i18n::t!("list.closed").to_string(),
                        ),
                    ],
                    status,
                    cx,
                    |this, status, cx| {
                        if let Some(focus) = this
                            .list
                            .read(cx)
                            .focus()
                            .and_then(|focus| focus.with_status(status))
                        {
                            this.refocus(focus, cx);
                        }
                    },
                )
            });
        let refresh = self.icon_button(
            "refresh",
            Icon::new(IconName::RotateCw)
                .size_4()
                .text_color(if loading {
                    tokens.colors().accent
                } else {
                    secondary
                }),
            rust_i18n::t!("list.refresh").to_string(),
            cx,
            |this, _, cx| this.refresh(cx),
        );
        let right_toggle = self.panel_toggle(
            Panel::RightPanel,
            IconName::PanelRightClose,
            IconName::PanelRightOpen,
            cx,
        );

        let strip = h_flex()
            .id("column-header")
            .flex_shrink_0()
            .w_full()
            .h(HEADER_HEIGHT)
            .pr_3()
            .gap_2()
            .items_center()
            .when(leading, |this| this.pl(TRAFFIC_LIGHT_INSET))
            .when(!leading, |this| this.pl_4())
            .children(sidebar_toggle)
            .child(
                h_flex()
                    .flex_1()
                    .gap_2()
                    .items_center()
                    .overflow_hidden()
                    .children(title.map(|title| {
                        div()
                            .text_sm()
                            .font_medium()
                            .text_color(primary)
                            .truncate()
                            .child(title)
                    }))
                    .children(subtitle.map(|subtitle| {
                        div().text_xs().text_color(muted).truncate().child(subtitle)
                    })),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .children(kind_chips)
                    .children(status_chips)
                    .child(refresh)
                    .child(right_toggle),
            );
        self.draggable(strip, cx)
    }

    /// The strip across the top of the right column: the way to the item on
    /// the web.
    fn right_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let tokens = Tokens::global(cx).clone();
        let url = self.detail.read(cx).html_url(cx);
        let open = url.map(|url| {
            self.icon_button(
                "open-on-github",
                Icon::new(IconName::ExternalLink)
                    .size_4()
                    .text_color(tokens.colors().text_secondary),
                rust_i18n::t!("detail.open_on_github").to_string(),
                cx,
                move |_, _, cx| cx.open_url(&url),
            )
        });
        let strip = h_flex()
            .id("right-header")
            .flex_shrink_0()
            .w_full()
            .h(HEADER_HEIGHT)
            .px_3()
            .gap_2()
            .items_center()
            .justify_end()
            .border_b_1()
            .border_color(tokens.colors().border_subtle)
            .children(open);
        self.draggable(strip, cx)
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::global(cx).clone();
        let sidebar_open = self.layout.is_open(Panel::Sidebar);
        let right_open = self.layout.is_open(Panel::RightPanel);
        let sidebar_width = self.layout.size(Panel::Sidebar);
        let right_width = self.layout.size(Panel::RightPanel);

        // Built before the column chain: the headers bind listeners, and the
        // chain's own closures hold `self` while they run.
        let window_controls = sidebar_open.then(|| {
            div()
                .w_full()
                .bg(tokens.colors().bg_sidebar)
                .border_r_1()
                .border_color(tokens.colors().border_subtle)
                .child(self.window_controls(cx))
                .into_any_element()
        });
        let column_header = self.column_header(cx).into_any_element();
        let right_header = right_open.then(|| self.right_header(cx).into_any_element());

        v_flex()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_toggle_sidebar))
            .on_action(cx.listener(Self::on_toggle_right_panel))
            .on_action(cx.listener(Self::on_refresh))
            .size_full()
            // No background here: `Root` already paints the translucent
            // window and painting it again composites the alpha away.
            .text_color(tokens.colors().text_primary)
            .child(
                div().flex_1().w_full().overflow_hidden().child(
                    h_resizable("shell-columns")
                        .on_resize({
                            let this = cx.entity();
                            let slots = self.layout.columns();
                            move |state, _, cx| {
                                let slots = slots.clone();
                                let state = state.clone();
                                this.update(cx, |this, cx| this.record_resize(slots, &state, cx));
                            }
                        })
                        .when(sidebar_open, |this| {
                            this.child(
                                resizable_panel()
                                    .size(sidebar_width)
                                    .size_range(px(200.)..px(400.))
                                    .child(
                                        v_flex()
                                            .size_full()
                                            .children(window_controls)
                                            .child(self.sidebar.clone())
                                            .into_any_element(),
                                    ),
                            )
                        })
                        .child(
                            resizable_panel().child(
                                v_flex()
                                    .size_full()
                                    .child(column_header)
                                    .child(self.list.clone())
                                    .into_any_element(),
                            ),
                        )
                        .when(right_open, |this| {
                            this.child(
                                resizable_panel()
                                    .size(right_width)
                                    .size_range(px(280.)..px(720.))
                                    .child(
                                        v_flex()
                                            .size_full()
                                            .border_l_1()
                                            .border_color(tokens.colors().border_subtle)
                                            .children(right_header)
                                            .child(self.detail.clone())
                                            .into_any_element(),
                                    ),
                            )
                        }),
                ),
            )
    }
}
