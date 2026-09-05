//! The e1 desktop app.
//!
//! Deliberately thin: it finds where settings live, decides the language,
//! finds a token, opens a frameless window and mounts [`e1_views::Shell`].
//! Everything that draws is in `e1-views`, so a host can mount the same
//! views without this file (`AGENTS.md` rule 1).

rust_i18n::i18n!("locales", fallback = "en");

use anyhow::Result;
use e1_github::auth::Source;
use e1_github::{GitHub, RepoId, Rest, Scripted};
use e1_ui::settings::{self, AppSettings};
use e1_ui::{Mode, Paths};
use gpui::{
    App, AppContext as _, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowOptions, point, px, size,
};
use gpui_component::Root;
use std::sync::Arc;

/// Where the window's data comes from, and where its token came from.
///
/// `E1_DEMO=1` runs over scripted data with no network at all. Otherwise the
/// token is discovered; without one the window still opens, on the sign-in
/// screen — a window that refuses to open cannot tell the reader what to do
/// about it.
fn source() -> (Option<Arc<dyn GitHub>>, Option<Source>) {
    if std::env::var_os("E1_DEMO").is_some_and(|value| value == "1") {
        tracing::info!("running over scripted data");
        return (Some(Arc::new(Scripted::sample())), None);
    }
    match e1_github::auth::discover() {
        Some((token, source)) => {
            tracing::info!(?source, "token found");
            (Some(Arc::new(Rest::new(token))), Some(source))
        }
        None => {
            tracing::warn!("no GitHub token was found; opening on the sign-in screen");
            (None, None)
        }
    }
}

/// `E1_DEMO_OPEN=owner/name#12:src/main.rs`: an item to open, on its files,
/// with one of them expanded, as soon as the window is up. The path is
/// optional. For screenshots; see `Shell::open_at_launch`.
fn open_at_launch() -> Option<((RepoId, u64), Option<String>)> {
    let value = std::env::var("E1_DEMO_OPEN").ok()?;
    let (item, file) = match value.split_once(':') {
        Some((item, file)) => (item, Some(file.to_string())),
        None => (value.as_str(), None),
    };
    let (repo, number) = item.split_once('#')?;
    Some(((RepoId::parse(repo)?, number.parse().ok()?), file))
}

fn main() -> Result<()> {
    let paths = Paths::from_env()?;
    paths.ensure()?;
    let _log_guard = e1_ui::logging::init(&paths)?;
    let app_settings: AppSettings = settings::load(&paths.app_settings());
    let locale = e1_ui::i18n::init(app_settings.locale.as_deref());
    tracing::info!(%locale, "language");
    let (github, token_source) = source();
    let shell_paths = paths.clone();

    let application = gpui_platform::application().with_assets(e1_ui::Assets);

    application.run(move |cx: &mut App| {
        gpui_component::init(cx);
        e1_views::init(cx);
        let mode = Mode::resolve(app_settings.appearance, cx.window_appearance());
        e1_ui::theme::apply(mode, cx);

        cx.spawn(async move |cx| {
            // No title bar of any kind: the columns run to the top of the
            // window and carry their own controls (`docs/ui.md` §3.1). What
            // is left for the platform is the traffic lights, positioned to
            // sit on the same line as those controls.
            let mut options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(13.), px(15.))),
                }),
                // Our own header strips move the window, so AppKit must not
                // also treat them as a system drag region.
                app_owns_titlebar_drag: true,
                ..Default::default()
            };
            options.window_min_size = Some(size(px(880.), px(560.)));
            // The glass surface of docs/ui.md §1: the window is translucent
            // and the desktop behind it is blurred.
            options.window_background = WindowBackgroundAppearance::Blurred;
            options.window_bounds = Some(cx.update(|cx| {
                WindowBounds::Windowed(Bounds::centered(None, size(px(1440.), px(920.)), cx))
            }));

            let open_at_launch = open_at_launch();
            cx.open_window(options, |window, cx| {
                let shell = cx.new(|cx| {
                    e1_views::Shell::new(
                        github,
                        token_source,
                        shell_paths,
                        app_settings,
                        window,
                        cx,
                    )
                });
                if let Some((key, file)) = open_at_launch {
                    shell.update(cx, |shell, cx| shell.open_at_launch(key, file, cx));
                }
                cx.new(|cx| Root::new(shell, window, cx))
            })
            .expect("failed to open the main window");

            // Launched from a terminal rather than an app bundle, the window
            // opens behind whatever was frontmost unless we ask for focus.
            cx.update(|cx| cx.activate(true));
        })
        .detach();
    });

    Ok(())
}
