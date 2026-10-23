use super::*;

#[gpui::test]
fn deleted_parked_selection_is_not_resurrected_by_switching(cx: &mut gpui::TestAppContext) {
    for axis in [None, Some(SplitAxis::Horizontal), Some(SplitAxis::Vertical)] {
        let (_dir, window) = parked_window(cx, axis);
        delete_chats(window, &["current"], cx);
        window
            .update(cx, |shell, window, cx| shell.switch_chat_tab(0, window, cx))
            .unwrap();
        cx.run_until_parked();
        window
            .update(cx, |shell, _, cx| {
                assert!(
                    shell.state.read(cx).selected_chat.is_none(),
                    "reactivation must not resurrect a deleted parked selection"
                );
                assert!(shell.settings.last_chat_id.is_none());
                if axis.is_some() {
                    assert_eq!(
                        shell.chat_split.as_ref().unwrap().panes[1].as_deref(),
                        Some("peer")
                    );
                    assert!(shell.peer_chat_views.contains_key("peer"));
                }
                assert_eq!(shell.state.read(cx).chats.len(), 3);
            })
            .unwrap();
    }
}

#[gpui::test]
fn closing_another_tab_cannot_restore_a_deleted_parked_selection(cx: &mut gpui::TestAppContext) {
    for axis in [None, Some(SplitAxis::Horizontal), Some(SplitAxis::Vertical)] {
        let (_dir, window) = parked_window(cx, axis);
        delete_chats(window, &["current"], cx);
        window
            .update(cx, |shell, window, cx| {
                assert!(shell.close_chat_tab(window, cx));
                assert!(shell.state.read(cx).selected_chat.is_none());
                assert!(shell.chat_tabs.is_empty());
                if axis.is_some() {
                    assert_eq!(
                        shell.chat_split.as_ref().unwrap().panes[1].as_deref(),
                        Some("peer")
                    );
                }
            })
            .unwrap();
    }
}

#[gpui::test]
fn deleting_every_session_in_a_parked_split_restores_blank_panes(cx: &mut gpui::TestAppContext) {
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        let (_dir, window) = parked_window(cx, Some(axis));
        delete_chats(window, &["current", "peer"], cx);
        window
            .update(cx, |shell, window, cx| shell.switch_chat_tab(0, window, cx))
            .unwrap();
        cx.run_until_parked();
        window
            .update(cx, |shell, window, cx| {
                assert!(shell.state.read(cx).selected_chat.is_none());
                assert!(
                    shell
                        .chat_split
                        .as_ref()
                        .unwrap()
                        .panes
                        .iter()
                        .all(Option::is_none)
                );
                assert!(shell.peer_chat_views.is_empty());
                shell.focus_chat_pane(1, window, cx);
                assert!(shell.state.read(cx).selected_chat.is_none());
                assert_eq!(shell.state.read(cx).chats.len(), 2);
            })
            .unwrap();
    }
}

#[gpui::test]
fn parked_selection_waits_for_authoritative_metadata_before_being_cleared(
    cx: &mut gpui::TestAppContext,
) {
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        for returns in [false, true] {
            let (_dir, window) = parked_window(cx, Some(axis));
            let complete = window
                .update(cx, |shell, _, cx| {
                    shell.state.update(cx, |state, cx| {
                        let complete = state.chats.clone();
                        state.chats.retain(|chat| chat.id != "current");
                        state.chats_synced = false;
                        cx.notify();
                        complete
                    })
                })
                .unwrap();
            window
                .update(cx, |shell, window, cx| shell.switch_chat_tab(0, window, cx))
                .unwrap();
            cx.run_until_parked();
            window
                .update(cx, |shell, _, cx| {
                    assert_eq!(
                        shell.state.read(cx).selected_chat.as_deref(),
                        Some("current"),
                        "incomplete metadata is not proof of deletion"
                    );
                    shell.state.update(cx, |state, cx| {
                        state.apply_chats(if returns {
                            complete
                        } else {
                            state.chats.clone()
                        });
                        cx.notify();
                    });
                })
                .unwrap();
            cx.run_until_parked();
            window
                .update(cx, |shell, _, cx| {
                    assert_eq!(
                        shell.state.read(cx).selected_chat.as_deref(),
                        returns.then_some("current")
                    );
                    assert_eq!(
                        shell.settings.last_chat_id.as_deref(),
                        returns.then_some("current")
                    );
                    assert_eq!(
                        shell.chat_split.as_ref().unwrap().panes[1].as_deref(),
                        Some("peer")
                    );
                    assert!(shell.state.read(cx).chats.iter().all(|chat| chat.archived));
                })
                .unwrap();
        }
    }
}

#[gpui::test]
fn archived_parked_selection_survives_renames_and_complete_list_refreshes(
    cx: &mut gpui::TestAppContext,
) {
    for axis in [None, Some(SplitAxis::Horizontal), Some(SplitAxis::Vertical)] {
        let (_dir, window) = parked_window(cx, axis);
        window
            .update(cx, |shell, _, cx| {
                shell.state.update(cx, |state, cx| {
                    let mut chats = state.chats.clone();
                    let current = chats.iter_mut().find(|chat| chat.id == "current").unwrap();
                    current.title = Some("Renamed archived reference".into());
                    current.archived = true;
                    chats.reverse();
                    state.apply_chats(chats);
                    cx.notify();
                });
            })
            .unwrap();
        cx.run_until_parked();
        for _ in 0..3 {
            window
                .update(cx, |shell, window, cx| {
                    shell.switch_chat_tab(0, window, cx);
                    assert_eq!(
                        shell.state.read(cx).selected_chat.as_deref(),
                        Some("current")
                    );
                    let chat = shell.state.read(cx).selected_chat_row().unwrap();
                    assert_eq!(chat.title.as_deref(), Some("Renamed archived reference"));
                    assert!(chat.archived);
                    shell.switch_chat_tab(1, window, cx);
                })
                .unwrap();
        }
    }
}

#[gpui::test]
fn focusing_a_deleted_peer_before_observers_run_cannot_select_it(cx: &mut gpui::TestAppContext) {
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        let (_dir, window) = parked_window(cx, Some(axis));
        window
            .update(cx, |shell, window, cx| {
                shell.switch_chat_tab(0, window, cx);
                shell.focus_chat_pane(1, window, cx);
                shell.state.update(cx, |state, cx| {
                    let mut chats = state.chats.clone();
                    chats.retain(|chat| chat.id != "current");
                    state.apply_chats(chats);
                    cx.notify();
                });
                // The focus event may arrive before the queued Shell observer
                // has removed the deleted peer from the visible split.
                shell.focus_chat_pane(0, window, cx);
                assert!(shell.state.read(cx).selected_chat.is_none());
                assert_eq!(
                    shell.chat_split.as_ref().unwrap().panes[1].as_deref(),
                    Some("peer")
                );
                assert!(!shell.peer_chat_views.contains_key("current"));
            })
            .unwrap();
    }
}

#[gpui::test]
fn closing_onto_a_deleted_neighbor_before_observers_run_leaves_a_canvas(
    cx: &mut gpui::TestAppContext,
) {
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        let (_dir, window) = parked_window(cx, Some(axis));
        window
            .update(cx, |shell, window, cx| {
                shell.switch_chat_tab(0, window, cx);
                shell.focus_chat_pane(1, window, cx);
                shell.state.update(cx, |state, cx| {
                    let mut chats = state.chats.clone();
                    chats.retain(|chat| chat.id != "current");
                    state.apply_chats(chats);
                    cx.notify();
                });
                assert!(shell.close_focused_chat_pane(window, cx));
                assert!(shell.chat_split.is_none());
                assert!(shell.state.read(cx).selected_chat.is_none());
                assert!(shell.peer_chat_views.is_empty());
            })
            .unwrap();
    }
}

#[gpui::test]
fn a_new_chat_may_be_selected_before_its_list_publication(cx: &mut gpui::TestAppContext) {
    let (_dir, window) = fixture(cx, false, None);
    window
        .update(cx, |shell, _, cx| {
            shell.boot_restored = true;
            shell.state.update(cx, |state, cx| {
                state.chats_synced = true;
                // Creation selects its returned ID before the next list frame.
                // Existing-view validation must not reject that separate path.
                state.select_chat(Some("newborn".into()), cx);
            });
            assert_eq!(
                shell.state.read(cx).selected_chat.as_deref(),
                Some("newborn")
            );
            shell.state.update(cx, |state, cx| {
                let mut chats = state.chats.clone();
                let mut newborn = chats[0].clone();
                newborn.id = "newborn".into();
                newborn.archived = false;
                chats.push(newborn);
                state.apply_chats(chats);
                cx.notify();
            });
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |shell, _, cx| {
            assert_eq!(
                shell.state.read(cx).selected_chat.as_deref(),
                Some("newborn")
            );
            assert!(!shell.state.read(cx).selected_chat_row().unwrap().archived);
        })
        .unwrap();
}

fn parked_window(
    cx: &mut gpui::TestAppContext,
    axis: Option<SplitAxis>,
) -> (tempfile::TempDir, gpui::WindowHandle<Shell>) {
    let (dir, window) = fixture(cx, false, None);
    window
        .update(cx, |shell, window, cx| {
            shell.boot_restored = true;
            shell.state.update(cx, |state, _| {
                state.chats_synced = true;
                for chat in &mut state.chats {
                    chat.archived = true;
                }
            });
            if let Some(axis) = axis {
                shell.split_chat_opening(axis, Some("peer".into()), window, cx);
                shell.focus_chat_pane(0, window, cx);
            }
            shell.new_chat_tab(Some("target".into()), window, cx);
        })
        .unwrap();
    (dir, window)
}

fn delete_chats(window: gpui::WindowHandle<Shell>, ids: &[&str], cx: &mut gpui::TestAppContext) {
    window
        .update(cx, |shell, _, cx| {
            shell.state.update(cx, |state, cx| {
                let mut chats = state.chats.clone();
                chats.retain(|chat| !ids.contains(&chat.id.as_str()));
                state.apply_chats(chats);
                cx.notify();
            });
        })
        .unwrap();
    cx.run_until_parked();
}
