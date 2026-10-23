use super::*;

mod completion_sweep;
mod matrix_tests;

fn project() -> harness_proto::Space {
    harness_proto::Space {
        id: "canvas-project".into(),
        device_id: "project-host".into(),
        path: "/canvas-project".into(),
        name: Some("Canvas project".into()),
        git_detected: false,
        git_checked_at: None,
        checkout_id: None,
        created_at: Utc::now(),
    }
}

fn park_project(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext, draft: bool) {
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell
                .state
                .update(cx, |state, _| state.spaces.push(project()));
            let split = shell.chat_split.as_mut().unwrap();
            if draft {
                split.drafts[0] = Some(CanvasDraft {
                    target: Some(CanvasTarget {
                        space: Some("canvas-project".into()),
                        no_project: false,
                        device: Some("project-host".into()),
                    }),
                    ..Default::default()
                });
            } else {
                split.projects[0] = Some("canvas-project".into());
            }
        });
    });
    draw(cx);
}

fn assert_canvas_attachment(shell: &Entity<Shell>, cx: &gpui::VisualTestContext) {
    shell.read_with(cx, |shell, cx| {
        assert!(shell.state.read(cx).selected_chat.is_none());
        assert_eq!(shell.chat_split.as_ref().unwrap().focus, 0);
        assert_eq!(shell.composer.read(cx).staged().len(), 1);
        assert_eq!(shell.composer.read(cx).attachments["current"].len(), 1);
    });
}

#[gpui::test]
fn an_unparked_canvas_rejects_changes_to_its_inherited_destination(cx: &mut gpui::TestAppContext) {
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        for field in ["space", "no_project", "device"] {
            with_panes(cx, axis, true, |shell, image, peer, _, cx| {
                enter(cx, peer, image);
                submit_before_repaint(&shell, cx, peer, |shell, _, cx| {
                    shell.state.update(cx, |state, _| match field {
                        "space" => state.selected_space = Some("new-project".into()),
                        "no_project" => state.no_project = !state.no_project,
                        "device" => state.selected_device = Some("new-device".into()),
                        _ => unreachable!(),
                    });
                });
                assert_no_new_attachment(&shell, cx);
                shell.read_with(cx, |shell, _| {
                    assert_eq!(shell.chat_split.as_ref().unwrap().focus, 1)
                });
            });
        }
    }
}

#[gpui::test]
fn a_parked_canvas_target_is_independent_of_the_active_panes_device(cx: &mut gpui::TestAppContext) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        true,
        |shell, image, peer, _, cx| {
            cx.update(|_, cx| {
                shell.update(cx, |shell, _| {
                    shell.chat_split.as_mut().unwrap().drafts[0] = Some(CanvasDraft {
                        target: Some(CanvasTarget {
                            space: None,
                            no_project: true,
                            device: Some("pinned-device".into()),
                        }),
                        ..Default::default()
                    });
                });
            });
            draw(cx);
            enter(cx, peer, image);
            submit_before_repaint(&shell, cx, peer, |shell, _, cx| {
                shell.state.update(cx, |state, _| {
                    state.selected_device = Some("new-device".into())
                });
            });
            assert_canvas_attachment(&shell, cx);
            shell.read_with(cx, |shell, cx| {
                assert_eq!(
                    shell.state.read(cx).selected_device.as_deref(),
                    Some("pinned-device")
                );
            });
        },
    );
}

#[gpui::test]
fn a_parked_project_drop_rejects_a_changed_host_before_repaint(cx: &mut gpui::TestAppContext) {
    for draft in [true, false] {
        with_panes(
            cx,
            SplitAxis::Horizontal,
            true,
            |shell, image, peer, _, cx| {
                park_project(&shell, cx, draft);
                enter(cx, peer, image);
                submit_before_repaint(&shell, cx, peer, |shell, _, cx| {
                    shell.state.update(cx, |state, _| {
                        state.spaces[0].device_id = "new-project-host".into()
                    });
                });
                assert_no_new_attachment(&shell, cx);
            },
        );
    }
}

#[gpui::test]
fn a_deleted_parked_project_cannot_silently_drop_into_the_fallback_device(
    cx: &mut gpui::TestAppContext,
) {
    for draft in [true, false] {
        with_panes(
            cx,
            SplitAxis::Horizontal,
            true,
            |shell, image, peer, _, cx| {
                park_project(&shell, cx, draft);
                enter(cx, peer, image);
                submit_before_repaint(&shell, cx, peer, |shell, _, cx| {
                    shell.state.update(cx, |state, _| state.spaces.clear());
                });
                assert_no_new_attachment(&shell, cx);
            },
        );
    }
}

#[gpui::test]
fn a_pinned_project_does_not_inherit_unrelated_active_device_changes(
    cx: &mut gpui::TestAppContext,
) {
    for draft in [true, false] {
        with_panes(
            cx,
            SplitAxis::Horizontal,
            true,
            |shell, image, peer, _, cx| {
                park_project(&shell, cx, draft);
                enter(cx, peer, image);
                submit_before_repaint(&shell, cx, peer, |shell, _, cx| {
                    shell.state.update(cx, |state, _| {
                        state.selected_device = Some("new-device".into())
                    });
                });
                assert_canvas_attachment(&shell, cx);
                shell.read_with(cx, |shell, cx| {
                    let state = shell.state.read(cx);
                    assert_eq!(state.selected_space.as_deref(), Some("canvas-project"));
                    assert_eq!(state.selected_device.as_deref(), Some("project-host"));
                });
            },
        );
    }
}

#[gpui::test]
fn project_metadata_and_model_changes_do_not_invalidate_a_canvas_drop(
    cx: &mut gpui::TestAppContext,
) {
    for draft in [true, false] {
        with_panes(
            cx,
            SplitAxis::Horizontal,
            true,
            |shell, image, peer, _, cx| {
                park_project(&shell, cx, draft);
                enter(cx, peer, image);
                submit_before_repaint(&shell, cx, peer, |shell, _, cx| {
                    shell.state.update(cx, |state, _| {
                        state.spaces[0].name = Some("Renamed project".into())
                    });
                    shell.chat_split.as_mut().unwrap().drafts[0]
                        .get_or_insert_with(Default::default)
                        .model = Some("new-model".into());
                });
                assert_canvas_attachment(&shell, cx);
            },
        );
    }
}

#[gpui::test]
fn repainting_a_changed_inherited_target_allows_the_next_drop(cx: &mut gpui::TestAppContext) {
    with_panes(
        cx,
        SplitAxis::Horizontal,
        true,
        |shell, image, peer, _, cx| {
            enter(cx, peer, image);
            cx.update(|_, cx| {
                shell.update(cx, |shell, cx| {
                    shell.state.update(cx, |state, _| {
                        state.selected_device = Some("new-device".into())
                    });
                });
            });
            draw(cx);
            file_event(cx, FileDropEvent::Submit { position: peer });
            assert_canvas_attachment(&shell, cx);
            shell.read_with(cx, |shell, cx| {
                assert_eq!(
                    shell.state.read(cx).selected_device.as_deref(),
                    Some("new-device")
                );
            });
        },
    );
}
