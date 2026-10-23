use super::*;

#[gpui::test]
fn project_removal_invalidates_a_drop_even_when_the_fallback_host_is_identical(
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
                    } else {
                        cx.update(|_, cx| {
                            shell.update(cx, |shell, cx| {
                                shell.state.update(cx, |state, _| {
                                    state.selected_device = Some("project-host".into());
                                });
                            });
                        });
                        draw(cx);
                    }
                    enter(cx, peer, image);
                    submit_before_repaint(&shell, cx, peer, |shell, _, cx| {
                        shell.state.update(cx, |state, _| {
                            state.spaces_synced = true;
                            state.spaces.clear();
                        });
                    });
                    assert_no_new_attachment(&shell, cx);
                },
            );
        }
    }
}

#[gpui::test]
fn an_active_canvas_project_host_change_invalidates_its_old_drop(cx: &mut gpui::TestAppContext) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        true,
        |shell, image, peer, _, cx| {
            park_project(&shell, cx, false);
            focus_canvas(&shell, cx);
            enter(cx, peer, image);
            submit_before_repaint(&shell, cx, peer, |shell, _, cx| {
                shell
                    .state
                    .update(cx, |state, _| state.spaces[0].device_id = "new-host".into());
            });
            assert_no_new_attachment(&shell, cx);
        },
    );
}

#[gpui::test]
fn canvas_drop_destination_matches_real_focus_restoration_matrix(cx: &mut gpui::TestAppContext) {
    // Cross both layout axes, live/no-project/default-device fallbacks, valid
    // and missing filters, and every parked-target restoration branch. The
    // oracle is the production focus path, not another resolver in the test.
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        for live in 0..3 {
            for filter in 0..3 {
                for parked in 0..6 {
                    with_panes(cx, axis, true, |shell, image, peer, _, cx| {
                        cx.update(|_, cx| {
                            shell.update(cx, |shell, cx| {
                                shell.state.update(cx, |state, _| {
                                    let mut live_project = project();
                                    live_project.id = "live-project".into();
                                    live_project.device_id = "live-host".into();
                                    state.spaces = vec![project(), live_project];
                                    state.local_device_id = Some("local-fallback".into());
                                    state.selected_space =
                                        (live == 1).then(|| "live-project".into());
                                    state.no_project = live == 0;
                                    state.selected_device =
                                        (live != 2).then(|| "live-device".into());
                                });
                                let split = shell.chat_split.as_mut().unwrap();
                                split.projects[0] = match filter {
                                    0 => None,
                                    1 => Some("canvas-project".into()),
                                    _ => Some("missing-project".into()),
                                };
                                let target = match parked {
                                    0 => None,
                                    1 | 2 => Some(CanvasTarget {
                                        space: None,
                                        no_project: true,
                                        device: (parked == 2).then(|| "pinned-device".into()),
                                    }),
                                    3 | 4 => Some(CanvasTarget {
                                        space: Some(
                                            if parked == 3 {
                                                "canvas-project"
                                            } else {
                                                "missing-project"
                                            }
                                            .into(),
                                        ),
                                        no_project: false,
                                        device: Some("ignored-device".into()),
                                    }),
                                    _ => Some(CanvasTarget {
                                        space: None,
                                        no_project: false,
                                        device: Some("ignored-device".into()),
                                    }),
                                };
                                split.drafts[0] = target.map(|target| CanvasDraft {
                                    target: Some(target),
                                    ..Default::default()
                                });
                            });
                        });
                        draw(cx);
                        let expected = shell.read_with(cx, |shell, cx| {
                            shell
                                .attachment_drop_target(Some(0), cx)
                                .canvas
                                .unwrap()
                                .resolved
                        });
                        enter(cx, peer, image);
                        file_event(cx, FileDropEvent::Submit { position: peer });
                        assert_canvas_attachment(&shell, cx);
                        shell.read_with(cx, |shell, cx| {
                            let state = shell.state.read(cx);
                            let actual = CanvasTarget {
                                space: state.selected_space.clone(),
                                no_project: state.no_project,
                                device: state.effective_device_id(),
                            };
                            assert_eq!(
                                actual, expected,
                                "axis={axis:?} live={live} filter={filter} parked={parked}"
                            );
                        });
                    });
                }
            }
        }
    }
}
