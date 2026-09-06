//! The standalone window: three resizable columns under three header strips,
//! and no title bar (`docs/ui.md` §3.1).
//!
//! This is the one view a host will not mount: it owns the window's controls,
//! the arrangement and its persistence, all of which are the host's when the
//! views are embedded. Everything under it — the sidebar, the list, the
//! detail — is mounted here exactly the way a host would mount it.

use crate::browser::{BrowserEvent, FileBrowser};
use crate::detail::Detail;
use crate::list::{ItemEvent, ItemList};
use crate::sidebar::{Sidebar, SidebarEvent};
use crate::signin::{SignIn, SignInEvent};
use crate::store::{ItemKey, Store, StoreEvent};
use e1_github::auth::{Keychain, Source};
use e1_github::{GitHub, HttpCache, Rest, Scripted, StatusFilter};
use e1_ui::settings::{self, AppSettings};
use e1_ui::{Focus, HEADER_HEIGHT, Layout, Panel, Paths, RepoTab, TRAFFIC_LIGHT_INSET, Tokens};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
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
    browser: Entity<FileBrowser>,
    sign_in: Entity<SignIn>,
    /// The search box in the centre strip.
    search: Entity<InputState>,
    /// What the centre column was last pointed at.
    current: Option<Focus>,
    /// Whether there is a GitHub to draw. Without one the centre column is
    /// the sign-in screen.
    signed_in: bool,
    /// Where the token came from, which is what signing out has to undo.
    token_source: Option<Source>,
    focus_handle: FocusHandle,
    /// With no title bar the strips are what the window is dragged by, and a
    /// drag is a press that then moved. Set on the press, cleared on the
    /// release, acted on by the first move in between.
    dragging: bool,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    /// Open over a source, with the arrangement the settings remember.
    ///
    /// `github` is `None` when no token was found: the window opens on the
    /// sign-in screen and everything else waits. `token_source` says where
    /// a token came from, so signing out knows whether it can delete it.
    pub fn new(
        github: Option<Arc<dyn GitHub>>,
        token_source: Option<Source>,
        paths: Paths,
        settings: AppSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let signed_in = github.is_some();
        let source: Arc<dyn GitHub> = github.unwrap_or_else(|| Arc::new(Scripted::empty()));
        let snapshot = paths.snapshot();
        let store = cx.new(|_| {
            let store = Store::new(source);
            // A window that opens signed out must not read a snapshot that
            // belongs to whoever was signed in before.
            if signed_in {
                store.with_snapshot(snapshot)
            } else {
                store.remembering(snapshot)
            }
        });
        let sign_in = cx.new(|_| SignIn::new());
        let browser = cx.new(|cx| FileBrowser::new(store.clone(), window, cx));
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(rust_i18n::t!("search.placeholder").to_string())
        });
        let sidebar = cx.new(|cx| Sidebar::new(store.clone(), cx));
        let list = cx.new(|cx| ItemList::new(store.clone(), cx));
        let detail = cx.new(|cx| Detail::new(store.clone(), cx));

        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe(&browser, |this, _, event, cx| match event {
            BrowserEvent::Open { repo, path } => {
                let key = (repo.clone(), path.clone());
                this.detail
                    .update(cx, |detail, cx| detail.show_file(key, cx));
                if !this.layout.is_open(Panel::RightPanel) {
                    this.toggle(Panel::RightPanel, cx);
                }
            }
        }));
        subscriptions.push(
            cx.subscribe(&search, |this, search, event: &InputEvent, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let query = search.read(cx).value().trim().to_string();
                    if !query.is_empty() {
                        this.search_for(query, cx);
                    }
                }
            }),
        );
        subscriptions.push(cx.subscribe(&sidebar, |this, _, event, cx| match event {
            SidebarEvent::Focus(focus) => this.focus_on(focus.clone(), cx),
            SidebarEvent::SignOut => this.sign_out(cx),
        }));
        subscriptions.push(cx.subscribe(&sign_in, |this, _, event, cx| match event {
            SignInEvent::SignedIn(token) => {
                let cache = HttpCache::new(this.paths.http_cache());
                let github: Arc<dyn GitHub> = Arc::new(Rest::new(token.clone()).with_cache(cache));
                this.signed_in = true;
                this.token_source = Some(Source::Keychain);
                this.store
                    .update(cx, |store, cx| store.set_source(github, cx));
                this.open_inbox(cx);
                cx.notify();
            }
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
        let mut this = Self {
            paths,
            settings,
            layout,
            store,
            sidebar,
            list,
            detail,
            browser,
            sign_in,
            search,
            current: None,
            signed_in,
            token_source,
            focus_handle,
            dragging: false,
            _subscriptions: subscriptions,
        };
        if signed_in {
            this.store.update(cx, |store, cx| store.refresh_all(cx));
            // The window opens on the inbox, which is the question a person
            // opens GitHub to answer. Pointed directly rather than through
            // the sidebar's event, which would land after whatever the
            // caller does next and undo it.
            this.open_inbox(cx);
        }
        this
    }

    /// Open an item on its files as soon as the window is up.
    ///
    /// For demos and screenshots (`E1_DEMO_OPEN=owner/name#12:src/main.rs`,
    /// the path optional): a native window cannot be driven from a script
    /// the way a page can, and a screenshot of the diff view is worth an
    /// environment variable.
    pub fn open_at_launch(&mut self, key: ItemKey, file: Option<String>, cx: &mut Context<Self>) {
        self.detail.update(cx, |detail, cx| {
            detail.show(key, None, cx);
            if file.is_some() {
                detail.show_files(file, cx);
            }
        });
        if !self.layout.is_open(Panel::RightPanel) {
            self.toggle(Panel::RightPanel, cx);
        }
    }

    /// Open a repository's finder with a file read, as soon as the window is
    /// up. For screenshots (`E1_DEMO_FILES=owner/name:src/main.rs`).
    pub fn browse_at_launch(
        &mut self,
        repo: e1_github::RepoId,
        path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.refocus(Focus::files(repo.clone()), cx);
        if let Some(path) = path {
            self.detail
                .update(cx, |detail, cx| detail.show_file((repo, path), cx));
        }
    }

    /// Forget the token and go back to the sign-in screen.
    ///
    /// Only a token this app stored is deleted. One from the environment or
    /// from `gh` is dropped for this session and found again next launch,
    /// because deleting it would be reaching into someone else's setup.
    fn sign_out(&mut self, cx: &mut Context<Self>) {
        if self.token_source == Some(Source::Keychain) {
            cx.background_spawn(async {
                if let Err(error) = Keychain::forget() {
                    tracing::warn!(%error, "could not delete the keychain entry");
                }
            })
            .detach();
        }
        self.signed_in = false;
        self.token_source = None;
        self.store.update(cx, |store, cx| {
            store.set_source(Arc::new(Scripted::empty()), cx)
        });
        cx.notify();
    }

    /// Point the centre column at something.
    fn focus_on(&mut self, focus: Focus, cx: &mut Context<Self>) {
        if let Some(repo) = focus.repo_id() {
            self.settings.last_repo = Some(repo.to_string());
            self.persist();
        }
        self.current = Some(focus.clone());
        match &focus {
            Focus::Files { repo } => {
                let repo = repo.clone();
                self.browser
                    .update(cx, |browser, cx| browser.set_repo(repo, cx));
            }
            _ => self.list.update(cx, |list, cx| list.set_focus(focus, cx)),
        }
        cx.notify();
    }

    /// Point the window at the inbox, highlight and all.
    fn open_inbox(&mut self, cx: &mut Context<Self>) {
        let inbox = Focus::Section(e1_ui::Section::Inbox);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.adopt(inbox.clone(), cx));
        self.focus_on(inbox, cx);
    }

    /// Search GitHub for what was typed.
    fn search_for(&mut self, query: String, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| sidebar.clear(cx));
        self.focus_on(Focus::search(query), cx);
    }

    /// What the centre column is showing, whichever view is showing it.
    fn focus(&self, cx: &App) -> Option<Focus> {
        let list = self.list.read(cx).focus().cloned();
        // The finder's repository is the focus while the finder is what is
        // on screen, which is when the list's focus is older than it.
        match self.centre_is_browser(cx) {
            true => self.browser_focus(cx),
            false => list,
        }
    }

    fn browser_focus(&self, cx: &App) -> Option<Focus> {
        self.browser.read(cx).repo().cloned().map(Focus::files)
    }

    /// Whether the finder is the centre column right now.
    fn centre_is_browser(&self, cx: &App) -> bool {
        matches!(self.current.as_ref(), Some(Focus::Files { .. }))
            && self.browser.read(cx).repo().is_some()
    }

    /// Change the list without going through the sidebar: the kind and
    /// status toggles in the centre strip.
    fn refocus(&mut self, focus: Focus, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.adopt(focus.clone(), cx));
        self.focus_on(focus, cx);
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
        self.browser.update(cx, |browser, cx| browser.refresh(cx));
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
        let focus = self.focus(cx);
        let loading = self.list.read(cx).is_loading(cx) || self.browser.read(cx).is_loading(cx);
        let title: Option<SharedString> = if self.signed_in {
            focus.as_ref().map(|focus| focus.title().into())
        } else {
            Some(rust_i18n::t!("signin.title").to_string().into())
        };
        let subtitle: Option<SharedString> = focus
            .as_ref()
            .filter(|_| self.signed_in)
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
        let focus = focus.filter(|_| self.signed_in);
        let tab_chips = focus
            .as_ref()
            .and_then(|focus| focus.repo_tab())
            .map(|tab| {
                self.chips(
                    "tab",
                    RepoTab::ALL
                        .iter()
                        .map(|tab| (*tab, rust_i18n::t!(tab.label_key()).to_string()))
                        .collect(),
                    tab,
                    cx,
                    |this, tab, cx| {
                        if let Some(focus) = this.focus(cx).and_then(|focus| focus.with_tab(tab)) {
                            this.refocus(focus, cx);
                        }
                    },
                )
            });
        let status_chips = focus
            .as_ref()
            .and_then(|focus| match focus {
                Focus::Repo { status, .. } => Some(*status),
                _ => None,
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
                        if let Some(focus) =
                            this.focus(cx).and_then(|focus| focus.with_status(status))
                        {
                            this.refocus(focus, cx);
                        }
                    },
                )
            });
        let search = self.signed_in.then(|| {
            div()
                .w(px(240.))
                .flex_shrink_0()
                .child(Input::new(&self.search).cleanable(true))
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
                    .children(tab_chips)
                    .children(status_chips)
                    .children(search)
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
        let centre: AnyElement = if !self.signed_in {
            self.sign_in.clone().into_any_element()
        } else if self.centre_is_browser(cx) {
            self.browser.clone().into_any_element()
        } else {
            self.list.clone().into_any_element()
        };

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
                                    .child(centre)
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
