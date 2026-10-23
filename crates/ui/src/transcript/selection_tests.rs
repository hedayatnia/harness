use super::*;

#[gpui::test]
fn whitespace_selection_anchors_at_line_boundaries_in_its_pane(cx: &mut gpui::TestAppContext) {
    let _selection = crate::markdown::selection::test_state_lock();
    struct Split(Entity<Transcript>, Entity<Transcript>);
    impl Render for Split {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_row()
                .child(div().w(px(450.0)).h_full().child(self.0.clone()))
                .child(div().w(px(450.0)).h_full().child(self.1.clone()))
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let (left_state, right_state) = cx.update(|cx| {
        gpui_base::init(cx);
        cx.set_global(Theme::dark());
        crate::settings::init(crate::settings::UiSettings::default(), dir.path(), cx);
        (cx.new(|_| AppState::new()), cx.new(|_| AppState::new()))
    });
    let (split, cx) = cx.add_window_view(|_, cx| {
        Split(
            cx.new(|cx| Transcript::new(left_state.clone(), cx)),
            cx.new(|cx| Transcript::new(right_state.clone(), cx)),
        )
    });
    cx.simulate_resize(gpui::size(px(900.0), px(800.0)));
    let (left, right) = split.read_with(cx, |s, _| (s.0.clone(), s.1.clone()));
    for (state, chat) in [(&left_state, "left-chat"), (&right_state, "right-chat")] {
        state.update(cx, |state, _| {
            state.selected_chat = Some(chat.into());
            state.transcript_replayed = true;
            state.transcript = vec![SessionMessageEntry {
                id: format!("{chat}-a"),
                role: MessageRole::Assistant,
                parts: vec![MessagePart::Text {
                    id: "t".into(),
                    text: "hello.\n\nnext sentence".into(),
                }],
                created_at: 0,
                device_id: "dev".into(),
                status: Some(MessageStatus::Complete),
                continuation_of: None,
                duration_ms: None,
            }];
            state.transcript_revision += 1;
        });
    }
    for _ in 0..3 {
        left.update(cx, |this, cx| this.sync(cx));
        right.update(cx, |this, cx| this.sync(cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
    }

    // Both panes have identical text and vertical geometry: ownership, not
    // registration order or proximity to the other pane, must pick the anchor.
    for pane_left in [0.0, 450.0] {
        let entries = render::selection_test_entries();
        let (key, _, bounds) = entries
            .iter()
            .find(|(_, text, bounds)| text == "hello." && bounds.left() > px(pane_left))
            .unwrap()
            .clone();
        let (_, layout, _) = render::selection_test_snapshot(&key);
        let (_, _, next) = entries
            .iter()
            .find(|(_, text, bounds)| text == "next sentence" && bounds.left() > px(pane_left))
            .unwrap()
            .clone();
        let y = bounds.top() + px(8.0);
        let start_left = gpui::point(px(pane_left + 1.0), y);
        let start_right = gpui::point(px(pane_left + 449.0), y);
        assert!(start_left.x < bounds.left());
        assert!(start_right.x > bounds.right());
        let partial = gpui::point(layout.position_for_index(3).unwrap().x + px(0.1), y);
        let next_end = gpui::point(start_right.x, next.top() + px(8.0));
        for (start, end, expected) in [
            (start_left, partial, Some("hel")),
            (
                start_right,
                start_right + gpui::point(px(-1.0), px(0.0)),
                None,
            ),
            (
                start_right - gpui::point(px(1.0), px(0.0)),
                start_right,
                None,
            ),
            (start_right, next_end, Some("next sentence")),
            (start_right, partial, Some("lo.")),
        ] {
            cx.simulate_event(gpui::MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                click_count: 1,
                ..Default::default()
            });
            assert!(crate::markdown::selection::is_dragging());
            cx.simulate_event(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            });
            assert_eq!(
                crate::markdown::selection::selected_text().as_deref(),
                expected,
                "pane {pane_left}, drag {start:?} -> {end:?}"
            );
            cx.simulate_event(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            });
            assert_eq!(
                crate::markdown::selection::selected_text().as_deref(),
                expected
            );
            crate::markdown::selection::clear_if_owner(&key);
        }
        // Blank space below the answer is not part of a text line.
        let below = gpui::point(start_left.x, next.bottom() + px(100.0));
        cx.simulate_event(gpui::MouseDownEvent {
            button: MouseButton::Left,
            position: below,
            click_count: 1,
            ..Default::default()
        });
        assert!(!crate::markdown::selection::is_dragging());
        cx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: below,
            ..Default::default()
        });
    }
}
