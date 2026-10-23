use super::*;

mod lifecycle_tests;
mod sequence_tests;

/// Real native file-drop translation and hit testing, without an engine or
/// transcript. Both windows belong to the same App, as in production.
#[derive(Default)]
struct FileWindow {
    drops: Vec<Vec<PathBuf>>,
    moves: usize,
    releases: usize,
    feedback: std::rc::Rc<std::cell::Cell<usize>>,
}

impl Render for FileWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("file-window")
            .size_full()
            .drag_over::<ExternalPaths>({
                let feedback = self.feedback.clone();
                move |style, _, _, _| {
                    feedback.set(feedback.get() + 1);
                    style
                }
            })
            .on_drag_move::<ExternalPaths>(cx.listener(|this, _, _, _| this.moves += 1))
            .capture_any_mouse_up(cx.listener(|this, _, _, _| this.releases += 1))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, _| {
                this.drops.push(paths.paths().to_vec())
            }))
    }
}

fn with_windows(
    cx: &mut gpui::TestAppContext,
    run: impl FnOnce(
        Entity<FileWindow>,
        &mut gpui::VisualTestContext,
        Entity<FileWindow>,
        &mut gpui::VisualTestContext,
    ),
) {
    let (first, first_cx) = cx.add_window_view(|_, _| FileWindow::default());
    let mut first_cx = first_cx.clone();
    let (second, second_cx) = cx.add_window_view(|_, _| FileWindow::default());
    let mut second_cx = second_cx.clone();
    draw(&mut first_cx);
    draw(&mut second_cx);
    run(first, &mut first_cx, second, &mut second_cx);
}

fn position() -> Point<Pixels> {
    gpui::point(px(100.0), px(100.0))
}

#[gpui::test]
fn only_the_owning_window_paints_external_drop_feedback(cx: &mut gpui::TestAppContext) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        let before = first.read_with(first_cx, |first, _| first.feedback.get());
        assert!(before > 0, "the current owner must show its feedback");
        enter(second_cx, position(), "second.png".into());
        draw(first_cx);
        first.read_with(first_cx, |first, _| {
            assert_eq!(
                first.feedback.get(),
                before,
                "the old window must not paint an affordance for someone elses drag"
            )
        });
        second.read_with(second_cx, |second, _| assert!(second.feedback.get() > 0));
        file_event(second_cx, FileDropEvent::Exited);
        let second_before = second.read_with(second_cx, |second, _| second.feedback.get());
        draw(second_cx);
        second.read_with(second_cx, |second, _| {
            assert_eq!(second.feedback.get(), second_before)
        });
    });
}

fn assert_no_drop(view: &Entity<FileWindow>, cx: &gpui::VisualTestContext) {
    view.read_with(cx, |view, _| {
        assert!(view.drops.is_empty());
        assert_eq!(
            view.releases, 0,
            "unmatched file events must not synthesize mouse-up"
        );
    });
}

fn assert_drop(view: &Entity<FileWindow>, cx: &gpui::VisualTestContext, name: &str) {
    view.read_with(cx, |view, _| {
        assert_eq!(view.drops, vec![vec![PathBuf::from(name)]]);
        assert_eq!(view.releases, 1);
    });
}

#[gpui::test]
fn a_window_without_enter_cannot_submit_another_windows_payload(cx: &mut gpui::TestAppContext) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        file_event(
            second_cx,
            FileDropEvent::Pending {
                position: position(),
            },
        );
        file_event(
            second_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_no_drop(&second, second_cx);
        file_event(
            first_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_drop(&first, first_cx, "first.png");
    });
}

#[gpui::test]
fn a_late_exit_from_the_old_window_cannot_cancel_the_new_windows_drag(
    cx: &mut gpui::TestAppContext,
) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        enter(second_cx, position(), "second.png".into());
        file_event(first_cx, FileDropEvent::Exited);
        second_cx.update(|_, cx| assert!(cx.has_active_drag()));
        file_event(
            second_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_no_drop(&first, first_cx);
        assert_drop(&second, second_cx, "second.png");
    });
}

#[gpui::test]
fn a_late_submit_from_the_old_window_cannot_steal_new_files(cx: &mut gpui::TestAppContext) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        enter(second_cx, position(), "second.png".into());
        file_event(
            first_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_no_drop(&first, first_cx);
        file_event(
            second_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_drop(&second, second_cx, "second.png");
    });
}

#[gpui::test]
fn a_late_pending_event_cannot_move_the_drag_in_the_old_window(cx: &mut gpui::TestAppContext) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        let moves = first.read_with(first_cx, |first, _| first.moves);
        enter(second_cx, position(), "second.png".into());
        file_event(
            first_cx,
            FileDropEvent::Pending {
                position: gpui::point(px(150.0), px(150.0)),
            },
        );
        first.read_with(first_cx, |first, _| assert_eq!(first.moves, moves));
        file_event(
            second_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_drop(&second, second_cx, "second.png");
    });
}

#[gpui::test]
fn cancelling_the_new_window_does_not_revive_the_old_windows_payload(
    cx: &mut gpui::TestAppContext,
) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        enter(second_cx, position(), "second.png".into());
        file_event(second_cx, FileDropEvent::Exited);
        file_event(
            first_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        file_event(
            second_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_no_drop(&first, first_cx);
        assert_no_drop(&second, second_cx);
    });
}

#[gpui::test]
fn alternating_window_handoffs_accept_only_the_latest_payload(cx: &mut gpui::TestAppContext) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        for ix in 0..3 {
            enter(first_cx, position(), format!("first-{ix}.png").into());
            enter(second_cx, position(), format!("second-{ix}.png").into());
            file_event(first_cx, FileDropEvent::Exited);
        }
        file_event(
            second_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_no_drop(&first, first_cx);
        assert_drop(&second, second_cx, "second-2.png");
    });
}

#[gpui::test]
fn stopping_a_drag_explicitly_invalidates_late_submit_in_both_windows(
    cx: &mut gpui::TestAppContext,
) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        first_cx.update(|window, cx| assert!(cx.stop_active_drag(window)));
        file_event(
            first_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        file_event(
            second_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_no_drop(&first, first_cx);
        assert_no_drop(&second, second_cx);
        enter(second_cx, position(), "second.png".into());
        file_event(
            second_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_drop(&second, second_cx, "second.png");
    });
}

#[gpui::test]
fn an_empty_handoff_cannot_submit_the_previous_windows_real_files(cx: &mut gpui::TestAppContext) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        enter_many(second_cx, position(), vec![]);
        file_event(
            first_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_no_drop(&first, first_cx);
        file_event(second_cx, FileDropEvent::Exited);
        enter(second_cx, position(), "second.png".into());
        file_event(
            second_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_drop(&second, second_cx, "second.png");
    });
}

#[gpui::test]
fn closing_the_drag_owner_clears_the_app_wide_payload(cx: &mut gpui::TestAppContext) {
    with_windows(cx, |_, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        first_cx.update(|window, _| window.remove_window());
        second_cx.update(|_, cx| {
            assert!(
                !cx.has_active_drag(),
                "closing the owner must cancel its external drag"
            )
        });
        file_event(
            second_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_no_drop(&second, second_cx);
        enter(second_cx, position(), "second.png".into());
        file_event(
            second_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_drop(&second, second_cx, "second.png");
    });
}

#[gpui::test]
fn closing_the_old_window_preserves_a_drag_handed_to_the_new_window(cx: &mut gpui::TestAppContext) {
    with_windows(cx, |_, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        enter(second_cx, position(), "second.png".into());
        first_cx.update(|window, _| window.remove_window());
        second_cx.update(|_, cx| assert!(cx.has_active_drag()));
        file_event(
            second_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_drop(&second, second_cx, "second.png");
    });
}

#[gpui::test]
fn an_ordinary_mouse_release_in_another_window_cannot_drop_external_files(
    cx: &mut gpui::TestAppContext,
) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        second_cx.simulate_mouse_up(position(), MouseButton::Left, Default::default());
        draw(second_cx);
        assert_no_drop(&second, second_cx);
        first_cx.update(|_, cx| assert!(cx.has_active_drag()));
        file_event(
            first_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_drop(&first, first_cx, "first.png");
    });
}
