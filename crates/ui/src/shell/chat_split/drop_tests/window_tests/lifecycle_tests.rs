use super::*;

mod callback_tests;

struct DragPreview;

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[derive(Default)]
struct DragWindow {
    file_payload: bool,
    files: Vec<Vec<PathBuf>>,
    internal_drops: usize,
}

impl Render for DragWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("drag-window")
            .size_full()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, _| {
                this.files.push(paths.paths().to_vec())
            }))
            .on_drop(cx.listener(|this, _: &(), _, _| this.internal_drops += 1))
            .child(
                div()
                    .id("drag-source")
                    .w(px(60.0))
                    .h(px(60.0))
                    .when(self.file_payload, |el| {
                        el.on_drag(
                            ExternalPaths(vec![PathBuf::from("in-app.png")].into()),
                            |_, _, _, cx| cx.new(|_| DragPreview),
                        )
                    })
                    .when(!self.file_payload, |el| {
                        el.on_drag((), |_, _, _, cx| cx.new(|_| DragPreview))
                    }),
            )
    }
}

fn drag_windows(
    cx: &mut gpui::TestAppContext,
    file_payload: bool,
    run: impl FnOnce(
        Entity<DragWindow>,
        &mut gpui::VisualTestContext,
        Entity<DragWindow>,
        &mut gpui::VisualTestContext,
    ),
) {
    let (first, first_cx) = cx.add_window_view(|_, _| DragWindow {
        file_payload,
        ..Default::default()
    });
    let mut first_cx = first_cx.clone();
    let (second, second_cx) = cx.add_window_view(|_, _| DragWindow::default());
    let mut second_cx = second_cx.clone();
    draw(&mut first_cx);
    draw(&mut second_cx);
    run(first, &mut first_cx, second, &mut second_cx);
}

fn start_internal(cx: &mut gpui::VisualTestContext) {
    cx.simulate_mouse_down(
        gpui::point(px(10.0), px(10.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        gpui::point(px(45.0), px(45.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    draw(cx);
    cx.update(|_, cx| assert!(cx.has_active_drag()));
}

fn finish_internal(cx: &mut gpui::VisualTestContext) {
    cx.simulate_mouse_move(position(), Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(position(), MouseButton::Left, Default::default());
    draw(cx);
}

#[gpui::test]
fn auxiliary_releases_do_not_commit_or_cancel_native_file_drags(cx: &mut gpui::TestAppContext) {
    with_windows(cx, |first, first_cx, _, _| {
        enter(first_cx, position(), "first.png".into());
        for button in [MouseButton::Right, MouseButton::Middle] {
            first_cx.simulate_mouse_up(position(), button, Default::default());
            first_cx.update(|_, cx| {
                assert!(cx.has_active_drag(), "only the primary release ends a drag")
            });
            first.read_with(first_cx, |first, _| assert!(first.drops.is_empty()));
        }
        file_event(
            first_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        first.read_with(first_cx, |first, _| {
            assert_eq!(first.drops, vec![vec![PathBuf::from("first.png")]])
        });
    });
}

#[gpui::test]
fn auxiliary_releases_do_not_commit_or_cancel_internal_primary_drags(
    cx: &mut gpui::TestAppContext,
) {
    drag_windows(cx, false, |first, first_cx, second, second_cx| {
        start_internal(first_cx);
        for button in [MouseButton::Right, MouseButton::Middle] {
            first_cx.simulate_mouse_up(position(), button, Default::default());
            first_cx.update(|_, cx| assert!(cx.has_active_drag()));
            first.read_with(first_cx, |first, _| assert_eq!(first.internal_drops, 0));
        }
        finish_internal(second_cx);
        second.read_with(second_cx, |second, _| assert_eq!(second.internal_drops, 1));
    });
}

#[gpui::test]
fn external_enter_cannot_replace_an_in_app_drag_with_the_same_payload_type(
    cx: &mut gpui::TestAppContext,
) {
    drag_windows(cx, true, |_, first_cx, second, second_cx| {
        start_internal(first_cx);
        enter(first_cx, position(), "os.png".into());
        finish_internal(second_cx);
        second.read_with(second_cx, |second, _| {
            assert_eq!(second.files, vec![vec![PathBuf::from("in-app.png")]])
        });
    });
}

#[gpui::test]
fn external_exit_cannot_cancel_an_in_app_drag_with_the_same_payload_type(
    cx: &mut gpui::TestAppContext,
) {
    drag_windows(cx, true, |_, first_cx, second, second_cx| {
        start_internal(first_cx);
        file_event(first_cx, FileDropEvent::Exited);
        first_cx.update(|_, cx| assert!(cx.has_active_drag()));
        finish_internal(second_cx);
        second.read_with(second_cx, |second, _| {
            assert_eq!(second.files, vec![vec![PathBuf::from("in-app.png")]])
        });
    });
}

#[gpui::test]
fn internal_cross_window_drops_work_after_external_completion_or_cancellation(
    cx: &mut gpui::TestAppContext,
) {
    for how in ["submit", "exit", "stop"] {
        drag_windows(cx, false, |_, first_cx, second, second_cx| {
            enter(first_cx, position(), "os.png".into());
            match how {
                "submit" => file_event(
                    first_cx,
                    FileDropEvent::Submit {
                        position: position(),
                    },
                ),
                "exit" => file_event(first_cx, FileDropEvent::Exited),
                _ => first_cx.update(|window, cx| {
                    assert!(cx.stop_active_drag(window));
                }),
            }
            start_internal(first_cx);
            finish_internal(second_cx);
            second.read_with(second_cx, |second, _| {
                assert_eq!(second.internal_drops, 1);
                assert!(second.files.is_empty());
            });
        });
    }
}

#[gpui::test]
fn closing_a_former_external_owner_does_not_cancel_an_internal_drag(cx: &mut gpui::TestAppContext) {
    drag_windows(cx, false, |_, first_cx, second, second_cx| {
        enter(first_cx, position(), "os.png".into());
        file_event(first_cx, FileDropEvent::Exited);
        start_internal(first_cx);
        first_cx.update(|window, _| window.remove_window());
        second_cx.update(|_, cx| assert!(cx.has_active_drag()));
        finish_internal(second_cx);
        second.read_with(second_cx, |second, _| assert_eq!(second.internal_drops, 1));
    });
}

#[gpui::test]
fn stopping_from_a_foreign_window_invalidates_ownership_in_both_windows(
    cx: &mut gpui::TestAppContext,
) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        second_cx.update(|window, cx| assert!(cx.stop_active_drag(window)));
        for cx in [&mut *first_cx, &mut *second_cx] {
            file_event(
                cx,
                FileDropEvent::Submit {
                    position: position(),
                },
            );
        }
        assert_no_drop(&first, first_cx);
        assert_no_drop(&second, second_cx);
        first_cx.update(|_, cx| assert!(!cx.has_active_drag()));
        enter(first_cx, position(), "replacement.png".into());
        file_event(
            first_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_drop(&first, first_cx, "replacement.png");
    });
}

#[gpui::test]
fn repeated_stop_is_a_noop_and_does_not_poison_the_next_drag(cx: &mut gpui::TestAppContext) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        enter(first_cx, position(), "first.png".into());
        first_cx.update(|window, cx| assert!(cx.stop_active_drag(window)));
        second_cx.update(|window, cx| assert!(!cx.stop_active_drag(window)));
        file_event(
            first_cx,
            FileDropEvent::Submit {
                position: position(),
            },
        );
        assert_no_drop(&first, first_cx);
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

#[derive(Default)]
struct DropZones {
    parent_accepts: bool,
    child: bool,
    child_accepts: bool,
    duplicate: bool,
    parent_drops: usize,
    child_drops: usize,
    duplicate_drops: usize,
}

impl Render for DropZones {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let parent_accepts = self.parent_accepts;
        let child_accepts = self.child_accepts;
        div()
            .id("outer-zone")
            .size_full()
            .can_drop(move |_, _, _| parent_accepts)
            .on_drop(cx.listener(|this, _: &ExternalPaths, _, _| this.parent_drops += 1))
            .when(self.duplicate && !self.child, |el| {
                el.on_drop(cx.listener(|this, _: &ExternalPaths, _, _| this.duplicate_drops += 1))
            })
            .when(self.child, |el| {
                el.child(
                    div()
                        .id("inner-zone")
                        .size_full()
                        .can_drop(move |_, _, _| child_accepts)
                        .on_drop(cx.listener(|this, _: &ExternalPaths, _, _| this.child_drops += 1))
                        .when(self.duplicate, |el| {
                            el.on_drop(cx.listener(|this, _: &ExternalPaths, _, _| {
                                this.duplicate_drops += 1
                            }))
                        }),
                )
            })
    }
}

fn drop_into_zones(
    cx: &mut gpui::TestAppContext,
    zones: DropZones,
    check: impl FnOnce(&DropZones),
) {
    let (view, cx) = cx.add_window_view(|_, _| zones);
    draw(cx);
    enter(cx, position(), "file.png".into());
    file_event(
        cx,
        FileDropEvent::Submit {
            position: position(),
        },
    );
    view.read_with(cx, |view, _| check(view));
    cx.update(|_, cx| assert!(!cx.has_active_drag()));
}

#[gpui::test]
fn a_rejecting_child_does_not_swallow_its_parents_drop(cx: &mut gpui::TestAppContext) {
    drop_into_zones(
        cx,
        DropZones {
            parent_accepts: true,
            child: true,
            ..Default::default()
        },
        |zones| {
            assert_eq!(zones.parent_drops, 1);
            assert_eq!(zones.child_drops, 0);
        },
    );
}

#[gpui::test]
fn duplicate_matching_listeners_do_not_panic_or_deliver_the_payload_twice(
    cx: &mut gpui::TestAppContext,
) {
    drop_into_zones(
        cx,
        DropZones {
            parent_accepts: true,
            duplicate: true,
            ..Default::default()
        },
        |zones| {
            assert_eq!(zones.parent_drops, 1);
            assert_eq!(zones.duplicate_drops, 0);
        },
    );
}

#[gpui::test]
fn duplicate_rejecting_child_listeners_still_allow_parent_fallback(cx: &mut gpui::TestAppContext) {
    drop_into_zones(
        cx,
        DropZones {
            parent_accepts: true,
            child: true,
            duplicate: true,
            ..Default::default()
        },
        |zones| {
            assert_eq!(zones.parent_drops, 1);
            assert_eq!(zones.child_drops, 0);
            assert_eq!(zones.duplicate_drops, 0);
        },
    );
}

#[gpui::test]
fn rejection_everywhere_cleans_up_and_the_next_drop_can_succeed(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| DropZones {
        child: true,
        ..Default::default()
    });
    draw(cx);
    enter(cx, position(), "rejected.png".into());
    file_event(
        cx,
        FileDropEvent::Submit {
            position: position(),
        },
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.parent_drops + view.child_drops, 0)
    });
    cx.update(|_, cx| assert!(!cx.has_active_drag()));
    view.update(cx, |view, cx| {
        view.parent_accepts = true;
        cx.notify();
    });
    draw(cx);
    enter(cx, position(), "accepted.png".into());
    file_event(
        cx,
        FileDropEvent::Submit {
            position: position(),
        },
    );
    view.read_with(cx, |view, _| assert_eq!(view.parent_drops, 1));
}

#[gpui::test]
fn an_accepting_child_delivers_once_and_stops_parent_fallback(cx: &mut gpui::TestAppContext) {
    drop_into_zones(
        cx,
        DropZones {
            parent_accepts: true,
            child: true,
            child_accepts: true,
            ..Default::default()
        },
        |zones| {
            assert_eq!(zones.child_drops, 1);
            assert_eq!(zones.parent_drops, 0);
        },
    );
}
