use super::*;

#[gpui::test]
fn a_project_canvas_ignores_an_explicit_device_pick_that_cannot_change_its_host(
    cx: &mut gpui::TestAppContext,
) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        true,
        |shell, image, peer, _, cx| {
            park_project(&shell, cx, false);
            focus_canvas(&shell, cx);
            enter(cx, peer, image);
            submit_before_repaint(&shell, cx, peer, |shell, _, cx| {
                shell.state.update(cx, |state, _| {
                    state.selected_device = Some("unrelated-device".into())
                });
            });
            assert_canvas_attachment(&shell, cx);
            shell.read_with(cx, |shell, cx| {
                assert_eq!(
                    shell.state.read(cx).effective_device_id().as_deref(),
                    Some("project-host")
                );
            });
        },
    );
}

#[gpui::test]
fn a_projectless_canvas_ignores_unrelated_project_deletion_and_sync_state(
    cx: &mut gpui::TestAppContext,
) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        true,
        |shell, image, peer, _, cx| {
            park_project(&shell, cx, false);
            focus_canvas(&shell, cx);
            cx.update(|_, cx| {
                shell.update(cx, |shell, cx| {
                    shell
                        .state
                        .update(cx, |state, cx| state.select_space(None, cx));
                });
            });
            draw(cx);
            enter(cx, peer, image);
            submit_before_repaint(&shell, cx, peer, |shell, _, cx| {
                shell.state.update(cx, |state, _| {
                    state.spaces.clear();
                    state.spaces_synced = true;
                });
            });
            assert_canvas_attachment(&shell, cx);
            shell.read_with(cx, |shell, cx| assert!(shell.state.read(cx).no_project));
        },
    );
}

#[gpui::test]
fn refreshing_the_same_project_row_keeps_active_and_inactive_canvas_drops_valid(
    cx: &mut gpui::TestAppContext,
) {
    for active in [true, false] {
        for draft in [true, false] {
            with_panes(
                cx,
                SplitAxis::Horizontal,
                true,
                |shell, image, peer, _, cx| {
                    park_project(&shell, cx, draft);
                    if active {
                        focus_canvas(&shell, cx);
                    }
                    enter(cx, peer, image);
                    submit_before_repaint(&shell, cx, peer, |shell, _, cx| {
                        shell.state.update(cx, |state, _| {
                            state.spaces.clear();
                            let mut refreshed = project();
                            refreshed.name = Some("Refreshed name".into());
                            refreshed.git_detected = true;
                            state.spaces.push(refreshed);
                            state.spaces_synced = true;
                        });
                    });
                    assert_canvas_attachment(&shell, cx);
                },
            );
        }
    }
}

#[gpui::test]
fn inherited_local_and_project_host_fallbacks_cannot_change_before_drop(
    cx: &mut gpui::TestAppContext,
) {
    for project_fallback in [true, false] {
        with_panes(
            cx,
            SplitAxis::Horizontal,
            true,
            |shell, image, peer, _, cx| {
                cx.update(|_, cx| {
                    shell.update(cx, |shell, cx| {
                        shell.state.update(cx, |state, _| {
                            state.selected_device = None;
                            state.local_device_id = Some("old-local".into());
                            if project_fallback {
                                state.spaces.push(project());
                                state.selected_space = Some("canvas-project".into());
                                state.no_project = false;
                            }
                        });
                        if project_fallback {
                            // A parked no-project target without a device inherits
                            // the current project's host at focus restoration time.
                            shell.chat_split.as_mut().unwrap().drafts[0] = Some(CanvasDraft {
                                target: Some(CanvasTarget {
                                    no_project: true,
                                    ..Default::default()
                                }),
                                ..Default::default()
                            });
                        }
                    });
                });
                draw(cx);
                enter(cx, peer, image);
                submit_before_repaint(&shell, cx, peer, |shell, _, cx| {
                    shell.state.update(cx, |state, _| {
                        if project_fallback {
                            state.spaces[0].device_id = "changed-host".into();
                        } else {
                            state.local_device_id = Some("changed-local".into());
                        }
                    });
                });
                assert_no_new_attachment(&shell, cx);
            },
        );
    }
}
