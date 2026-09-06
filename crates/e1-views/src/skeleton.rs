//! What a column shows while its first answer is on the way.
//!
//! Bars in the shape of what is coming, pulsing, rather than a word: a
//! reader who sees the shape of a list knows a list is coming and where
//! to look for it, and *Loading…* tells them only that they are waiting.
//! These stand in only for a first load — a refresh keeps the old value on
//! screen (`e1_ui::Fetch`) and needs nothing here.

use e1_ui::Tokens;
use gpui::*;
use gpui_component::skeleton::Skeleton;
use gpui_component::{h_flex, v_flex};

/// A bar `width` wide and `height` tall, rounded like a row.
fn bar(width: Pixels, height: Pixels, cx: &App) -> Skeleton {
    let tokens = Tokens::global(cx);
    Skeleton::new()
        .w(width)
        .h(height)
        .rounded(px(tokens.radius.control()))
}

/// The shape of `count` list rows: a glyph, a title, a line of meta.
pub fn list_rows(count: usize, cx: &App) -> AnyElement {
    v_flex()
        .w_full()
        .py_1()
        .children((0..count).map(|index| {
            // Titles vary; the bars do too, or the list reads as a grid.
            let title = px([0.72, 0.55, 0.64, 0.48, 0.68, 0.58][index % 6] * 520.);
            h_flex()
                .w_full()
                .h(px(56.))
                .px_4()
                .gap_2p5()
                .items_center()
                .child(Skeleton::new().size_4().rounded_full().flex_shrink_0())
                .child(
                    v_flex()
                        .flex_1()
                        .gap_2()
                        .child(bar(title, px(12.), cx))
                        .child(bar(px(160.), px(9.), cx).secondary()),
                )
        }))
        .into_any_element()
}

/// The shape of `count` file paths.
pub fn path_rows(count: usize, cx: &App) -> AnyElement {
    v_flex()
        .w_full()
        .py_1()
        .children((0..count).map(|index| {
            let width = px([0.5, 0.36, 0.62, 0.44, 0.58][index % 5] * 480.);
            h_flex()
                .w_full()
                .h(px(28.))
                .px_4()
                .gap_2()
                .items_center()
                .child(Skeleton::new().size_3p5().rounded(px(3.)).flex_shrink_0())
                .child(bar(width, px(10.), cx))
        }))
        .into_any_element()
}

/// The shape of an item being read: a title, a line of facts, paragraphs.
pub fn detail(cx: &App) -> AnyElement {
    v_flex()
        .w_full()
        .px_5()
        .py_4()
        .gap_3()
        .child(bar(px(360.), px(14.), cx))
        .child(
            h_flex()
                .gap_2()
                .child(bar(px(64.), px(18.), cx))
                .child(bar(px(120.), px(10.), cx).secondary()),
        )
        .child(div().h_3())
        .children(
            [0.95, 0.88, 0.6, 0.0, 0.9, 0.7]
                .into_iter()
                .map(|fraction| {
                    if fraction == 0.0 {
                        div().h_2().into_any_element()
                    } else {
                        Skeleton::new()
                            .w(relative(fraction))
                            .h(px(11.))
                            .rounded(px(3.))
                            .secondary()
                            .into_any_element()
                    }
                }),
        )
        .into_any_element()
}

/// The shape of a diff: file headers and lines.
pub fn diff(cx: &App) -> AnyElement {
    v_flex()
        .w_full()
        .py_1()
        .children((0..2).flat_map(|_| {
            std::iter::once(
                h_flex()
                    .w_full()
                    .h(px(22.))
                    .px_3()
                    .child(bar(px(220.), px(10.), cx))
                    .into_any_element(),
            )
            .chain((0..6).map(|line| {
                let width = px([0.5, 0.7, 0.3, 0.6, 0.45, 0.65][line] * 360.);
                h_flex()
                    .w_full()
                    .h(px(22.))
                    .pl(px(96.))
                    .items_center()
                    .child(bar(width, px(9.), cx).secondary())
                    .into_any_element()
            }))
        }))
        .into_any_element()
}
