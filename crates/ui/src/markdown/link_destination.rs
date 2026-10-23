//! Destination disclosure is local text only. Never fetch link previews.
use crate::theme::Theme;
use gpui::{
    Anchor, AnyElement, App, Bounds, Context, Pixels, Point, Render, SharedString, Window, div,
    prelude::*, px,
};

pub struct Destination(
    pub String,
    pub std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
);
impl Render for Destination {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        destination_card(&self.0, self.1.clone(), window, cx)
    }
}
fn viewport_limits(width: f32, height: f32) -> (f32, f32) {
    (
        (width - 24.).clamp(1., 360.),
        (height - 80.).clamp(1., 160.),
    )
}
/// Invisible break opportunities let even a single long path segment wrap.
/// This presentation string never replaces the original link destination.
fn breakable_url(url: &str) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    url.graphemes(true).collect::<Vec<_>>().join("\u{200b}")
}
pub fn destination_card(
    url: &str,
    bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    window: &Window,
    cx: &App,
) -> AnyElement {
    card(url, bounds, None, window, cx)
}
// Keep a keyboard disclosure entirely above or below its source, even before
// the next repaint switches back to pointer input. Padding and borders add 10px.
fn focused_placement(top: f32, bottom: f32, height: f32) -> Option<(bool, f32)> {
    let above = (top - 12.).max(0.);
    let below = (height - bottom - 12.).max(0.);
    let available = above.max(below) - 10.;
    (available >= 1.).then_some((above > below, available))
}
pub fn focused_destination_card(
    url: &str,
    bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    source: Bounds<Pixels>,
    window: &Window,
    cx: &App,
) -> Option<(AnyElement, Point<Pixels>, Anchor)> {
    let (above, height) = focused_placement(
        source.top().into(),
        source.bottom().into(),
        window.viewport_size().height.into(),
    )?;
    let (position, anchor) = if above {
        (
            source.origin - gpui::point(px(0.), px(4.)),
            Anchor::BottomLeft,
        )
    } else {
        (
            source.bottom_left() + gpui::point(px(0.), px(4.)),
            Anchor::TopLeft,
        )
    };
    Some((
        card(url, bounds, Some(height), window, cx),
        position,
        anchor,
    ))
}
fn card(
    url: &str,
    bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    height_limit: Option<f32>,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let theme = Theme::of(cx);
    let (max_width, height) = viewport_limits(
        window.viewport_size().width.into(),
        window.viewport_size().height.into(),
    );
    let height = height_limit.map_or(height, |limit| height.min(limit));
    let text: SharedString = breakable_url(url).into();
    // GPUI measures hover tooltips with min-content constraints. Use the
    // shaped text width so short URLs stay on one line despite wrap points.
    let natural_width = window
        .text_system()
        .shape_line(
            text.clone(),
            px(11.),
            &[window.text_style().to_run(text.len())],
            None,
        )
        .width();
    let width = (f32::from(natural_width).ceil() + 14.).min(max_width);
    div()
        .id("web-destination-card")
        .relative()
        .child(
            gpui::canvas(|_, _, _| (), move |rect, _, _, _| bounds.set(Some(rect)))
                .absolute()
                .size_full(),
        )
        .w(px(width))
        .max_w(px(width))
        .px(px(6.))
        .py(px(4.))
        .rounded(px(4.))
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.surface_raised)
        .shadow_sm()
        .text_size(px(11.))
        .line_height(px(14.))
        .text_color(theme.text)
        .flex()
        .flex_col()
        .child(
            div()
                .id("web-destination-scroll")
                .max_h(px(height))
                .overflow_y_scroll()
                .child(text),
        )
        .into_any_element()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyboard_disclosure_stays_outside_the_entire_link() {
        for (top, bottom, height) in [(216., 240., 240.), (0., 24., 240.), (40., 96., 120.)] {
            let (above, limit) = focused_placement(top, bottom, height).unwrap();
            let card_height = viewport_limits(400., height).1.min(limit) + 10.;
            if above {
                assert!(top - 4. - card_height >= 8.);
            } else {
                assert!(bottom + 4. + card_height <= height - 8.);
            }
        }
        assert!(focused_placement(0., 24., 24.).is_none());
    }
    #[test]
    fn disclosure_fits_small_viewports_and_keeps_the_complete_destination() {
        for (w, h) in [(320., 240.), (1000., 800.), (80., 80.)] {
            let (width, height) = viewport_limits(w, h);
            assert!(width < w && height < h);
        }
        let url = format!("https://example.com/{}?q=🙂", "á界".repeat(5000));
        assert_eq!(breakable_url(&url).replace('\u{200b}', ""), url);
    }
}
