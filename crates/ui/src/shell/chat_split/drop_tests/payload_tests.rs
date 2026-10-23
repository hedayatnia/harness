use super::*;

fn assert_current_intact(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) {
    shell.read_with(cx, |shell, cx| {
        assert_eq!(shell.chat_split.as_ref().unwrap().focus, 1);
        assert_eq!(
            shell.state.read(cx).selected_chat.as_deref(),
            Some("current")
        );
        let composer = shell.composer.read(cx);
        assert_eq!(composer.current_key, "current");
        assert_eq!(composer.staged().len(), 1);
        assert_eq!(composer.input.read(cx).text(), "Keep the current draft");
        assert!(composer.failure.is_none());
        assert!(!composer.attachments.contains_key("peer"));
    });
}

#[gpui::test]
fn batches_with_no_actual_paths_are_noops_in_both_panes(cx: &mut gpui::TestAppContext) {
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        with_panes(cx, axis, false, |shell, _, peer, current, cx| {
            for paths in [vec![], vec![PathBuf::new()], vec![PathBuf::new(); 3]] {
                for position in [peer, current] {
                    enter_many(cx, position, paths.clone());
                    file_event(cx, FileDropEvent::Submit { position });
                    assert_current_intact(&shell, cx);
                }
            }
        });
    }
}

#[gpui::test]
fn blank_entries_in_a_real_batch_are_ignored_without_false_errors(cx: &mut gpui::TestAppContext) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        false,
        |shell, image, peer, _, cx| {
            enter_many(cx, peer, vec![PathBuf::new(), image, PathBuf::new()]);
            file_event(cx, FileDropEvent::Submit { position: peer });
            shell.read_with(cx, |shell, cx| {
                let composer = shell.composer.read(cx);
                assert_eq!(composer.current_key, "peer");
                assert_eq!(composer.staged().len(), 1);
                assert!(
                    composer.failure.is_none(),
                    "blank entries must not report a failed file"
                );
                assert_eq!(composer.attachments["current"].len(), 1);
            });
        },
    );
}

#[gpui::test]
fn pending_and_submit_without_enter_do_not_become_pane_clicks(cx: &mut gpui::TestAppContext) {
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        with_panes(cx, axis, false, |shell, _, peer, _, cx| {
            file_event(cx, FileDropEvent::Pending { position: peer });
            file_event(cx, FileDropEvent::Submit { position: peer });
            assert_current_intact(&shell, cx);
        });
    }
}

#[gpui::test]
fn a_late_submit_after_exit_does_not_activate_the_cancelled_target(cx: &mut gpui::TestAppContext) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        false,
        |shell, image, peer, _, cx| {
            enter(cx, peer, image);
            file_event(cx, FileDropEvent::Exited);
            file_event(cx, FileDropEvent::Pending { position: peer });
            file_event(cx, FileDropEvent::Submit { position: peer });
            assert_current_intact(&shell, cx);
        },
    );
}

#[gpui::test]
fn a_duplicate_submit_cannot_activate_another_pane_or_duplicate_files(
    cx: &mut gpui::TestAppContext,
) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        false,
        |shell, image, peer, current, cx| {
            enter(cx, current, image);
            file_event(cx, FileDropEvent::Submit { position: current });
            file_event(cx, FileDropEvent::Submit { position: peer });
            shell.read_with(cx, |shell, cx| {
                assert_eq!(shell.chat_split.as_ref().unwrap().focus, 1);
                assert_eq!(
                    shell.state.read(cx).selected_chat.as_deref(),
                    Some("current")
                );
                let composer = shell.composer.read(cx);
                assert_eq!(composer.staged().len(), 2);
                assert!(!composer.attachments.contains_key("peer"));
            });
        },
    );
}

#[gpui::test]
fn a_new_enter_replaces_an_empty_payload_with_real_files(cx: &mut gpui::TestAppContext) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        false,
        |shell, image, peer, _, cx| {
            enter_many(cx, peer, vec![]);
            enter(cx, peer, image);
            file_event(cx, FileDropEvent::Submit { position: peer });
            shell.read_with(cx, |shell, cx| {
                let composer = shell.composer.read(cx);
                assert_eq!(composer.current_key, "peer");
                assert_eq!(composer.staged().len(), 1);
                assert_eq!(composer.staged()[0].name, "dropped.png");
                assert!(composer.failure.is_none());
            });
        },
    );
}

#[gpui::test]
fn a_new_empty_enter_does_not_drop_files_from_an_old_payload(cx: &mut gpui::TestAppContext) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        false,
        |shell, image, peer, _, cx| {
            enter(cx, peer, image);
            enter_many(cx, peer, vec![]);
            file_event(cx, FileDropEvent::Submit { position: peer });
            assert_current_intact(&shell, cx);
        },
    );
}

#[gpui::test]
fn the_latest_enter_owns_the_files_not_just_the_position(cx: &mut gpui::TestAppContext) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        false,
        |shell, image, peer, current, cx| {
            let replacement = image.parent().unwrap().join("replacement.png");
            std::fs::copy(&image, &replacement).unwrap();
            enter(cx, current, image);
            enter(cx, peer, replacement);
            file_event(cx, FileDropEvent::Submit { position: peer });
            shell.read_with(cx, |shell, cx| {
                let composer = shell.composer.read(cx);
                assert_eq!(composer.current_key, "peer");
                assert_eq!(composer.staged().len(), 1);
                assert_eq!(composer.staged()[0].name, "replacement.png");
                assert_eq!(composer.attachments["current"].len(), 1);
            });
        },
    );
}

#[gpui::test]
fn empty_drag_cancel_reenter_cycles_do_not_poison_a_later_valid_drop(
    cx: &mut gpui::TestAppContext,
) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        false,
        |shell, image, peer, _, cx| {
            for _ in 0..3 {
                enter_many(cx, peer, vec![]);
                file_event(cx, FileDropEvent::Exited);
                enter(cx, peer, image.clone());
                file_event(cx, FileDropEvent::Exited);
                assert_current_intact(&shell, cx);
            }
            enter(cx, peer, image);
            file_event(cx, FileDropEvent::Submit { position: peer });
            shell.read_with(cx, |shell, cx| {
                let composer = shell.composer.read(cx);
                assert_eq!(composer.current_key, "peer");
                assert_eq!(composer.staged().len(), 1);
                assert!(composer.failure.is_none());
            });
        },
    );
}

#[gpui::test]
fn dropping_outside_the_panes_consumes_the_payload_without_attaching(
    cx: &mut gpui::TestAppContext,
) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        false,
        |shell, image, peer, _, cx| {
            enter(cx, peer, image.clone());
            file_event(
                cx,
                FileDropEvent::Submit {
                    position: gpui::point(px(-20.0), px(-20.0)),
                },
            );
            assert_current_intact(&shell, cx);
            enter(cx, peer, image);
            file_event(cx, FileDropEvent::Submit { position: peer });
            shell.read_with(cx, |shell, cx| {
                assert_eq!(shell.composer.read(cx).staged().len(), 1);
                assert_eq!(shell.composer.read(cx).current_key, "peer");
            });
        },
    );
}

#[gpui::test]
fn direct_staging_also_ignores_empty_paths_without_native_event_filtering(
    cx: &mut gpui::TestAppContext,
) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        false,
        |shell, image, _, _, cx| {
            cx.update(|_, cx| {
                shell.update(cx, |shell, cx| {
                    shell.composer.update(cx, |composer, cx| {
                        composer.add_paths(vec![], cx);
                        composer.add_paths(vec![PathBuf::new(); 2], cx);
                        assert_eq!(composer.staged().len(), 1);
                        assert!(composer.failure.is_none());
                        composer.add_paths(vec![PathBuf::new(), image, PathBuf::new()], cx);
                        assert_eq!(composer.staged().len(), 2);
                        assert!(composer.failure.is_none());
                        assert_eq!(composer.input.read(cx).text(), "Keep the current draft");
                    });
                })
            });
        },
    );
}

struct InternalDrag {
    moves: usize,
    drops: usize,
}

struct InternalPreview;

impl Render for InternalPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Render for InternalDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("internal-drop-target")
            .size_full()
            .on_drag_move::<()>(cx.listener(|this, _, _, _| this.moves += 1))
            .on_drop(cx.listener(|this, _: &(), _, _| this.drops += 1))
            .child(
                div()
                    .id("internal-drag-source")
                    .w(px(60.0))
                    .h(px(60.0))
                    .on_drag((), |_, _, _, cx| cx.new(|_| InternalPreview)),
            )
    }
}

#[gpui::test]
fn stray_os_file_events_cannot_submit_move_or_cancel_an_internal_drag(
    cx: &mut gpui::TestAppContext,
) {
    let (host, cx) = cx.add_window_view(|_, _| InternalDrag { moves: 0, drops: 0 });
    draw(cx);
    let modifiers = gpui::Modifiers::default();
    cx.simulate_mouse_down(
        gpui::point(px(10.0), px(10.0)),
        MouseButton::Left,
        modifiers,
    );
    cx.simulate_mouse_move(
        gpui::point(px(45.0), px(45.0)),
        Some(MouseButton::Left),
        modifiers,
    );
    draw(cx);
    cx.update(|_, cx| assert!(cx.has_active_drag()));
    let before = host.read_with(cx, |host, _| host.moves);
    let position = gpui::point(px(120.0), px(120.0));
    for event in [
        FileDropEvent::Entered {
            position,
            paths: ExternalPaths::default(),
        },
        FileDropEvent::Pending { position },
        FileDropEvent::Submit { position },
        FileDropEvent::Exited,
    ] {
        file_event(cx, event);
        cx.update(|_, cx| {
            assert!(
                cx.has_active_drag(),
                "OS file events must leave an in-app drag alone"
            )
        });
        host.read_with(cx, |host, _| {
            assert_eq!(host.moves, before);
            assert_eq!(host.drops, 0);
        });
    }
    cx.simulate_mouse_move(position, Some(MouseButton::Left), modifiers);
    cx.simulate_mouse_up(position, MouseButton::Left, modifiers);
    draw(cx);
    host.read_with(cx, |host, _| assert_eq!(host.drops, 1));
    cx.update(|_, cx| assert!(!cx.has_active_drag()));
}
