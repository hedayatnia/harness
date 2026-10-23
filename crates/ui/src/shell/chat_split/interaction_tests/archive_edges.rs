use super::*;

fn archive_both(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) {
    shell.update(cx, |shell, cx| {
        shell.boot_restored = true;
        // Seed already-archived references, rather than simulate a new archive
        // action (which deliberately closes an unarchived live peer on main).
        shell.chat_split_live.clear();
        shell.state.update(cx, |state, cx| {
            state.chats_synced = true;
            for chat in &mut state.chats {
                chat.archived = true;
            }
            cx.notify();
        });
    });
    draw(cx);
}

fn assert_archived(shell: &Entity<Shell>, cx: &gpui::VisualTestContext) {
    shell.read_with(cx, |shell, cx| {
        assert_eq!(shell.state.read(cx).chats.len(), 2);
        assert!(shell.state.read(cx).chats.iter().all(|chat| chat.archived));
    });
}

#[gpui::test]
fn archived_word_and_paragraph_selection_survive_held_press_redraws(cx: &mut gpui::TestAppContext) {
    let _selection = selection::test_state_lock();
    for count in [2, 3] {
        with_panes(
            cx,
            vec![reply("Selectable response text.")],
            |shell, peer, current, cx| {
                archive_both(&shell, cx);
                let start = text_point(&peer, "reply#text.0:0");
                cx.simulate_mouse_move(start, None, Default::default());
                cx.simulate_event(MouseDownEvent {
                    button: MouseButton::Left,
                    position: start,
                    click_count: count,
                    ..Default::default()
                });
                let expected = if count == 2 {
                    "Selectable"
                } else {
                    "Selectable response text."
                };
                assert_eq!(selection::selected_text().as_deref(), Some(expected));
                // A frame/hover refresh during a held multi-click is not a
                // user drag and must not collapse the immediate selection.
                for _ in 0..3 {
                    draw(cx);
                    cx.simulate_mouse_move(start, Some(MouseButton::Left), Default::default());
                    assert_eq!(selection::selected_text().as_deref(), Some(expected));
                }
                cx.simulate_event(MouseUpEvent {
                    button: MouseButton::Left,
                    position: start,
                    click_count: count,
                    ..Default::default()
                });
                assert_peer_active(&shell, &peer, &current, cx);
                assert_eq!(selection::selected_text().as_deref(), Some(expected));
                assert_archived(&shell, cx);
                selection::clear_if_owner(&key(&peer, "reply#text.0:0"));
            },
        );
    }
}

#[gpui::test]
fn archived_drag_survives_metadata_refresh_and_repeated_focus_handoffs(
    cx: &mut gpui::TestAppContext,
) {
    let _selection = selection::test_state_lock();
    with_panes(
        cx,
        vec![reply("Selectable response text.")],
        |shell, peer, current, cx| {
            archive_both(&shell, cx);
            for round in 0..3 {
                if round > 0 {
                    shell.update(cx, |shell, cx| assert!(shell.select_chat_pane(1, cx)));
                    draw(cx);
                }
                let start = text_point(&peer, "reply#text.0:0");
                let end = start + gpui::point(px(100.0), px(0.0));
                cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
                cx.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
                let selected = selection::selected_text().expect("real drag selects text");
                assert!(selected.starts_with("Selectable"));
                shell.update(cx, |shell, cx| {
                    shell.state.update(cx, |state, cx| {
                        let mut chats = state.chats.clone();
                        chats.reverse();
                        for chat in &mut chats {
                            chat.title = Some(format!("Archived refresh {round}"));
                        }
                        state.apply_chats(chats);
                        cx.notify();
                    });
                });
                draw(cx);
                cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
                assert_peer_active(&shell, &peer, &current, cx);
                assert_eq!(selection::selected_text(), Some(selected));
                assert_archived(&shell, cx);
                selection::clear_if_owner(&key(&peer, "reply#text.0:0"));
            }
        },
    );
}

#[gpui::test]
fn archived_pane_activation_with_each_mouse_button_does_not_restore_it(
    cx: &mut gpui::TestAppContext,
) {
    let _selection = selection::test_state_lock();
    for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
        with_panes(
            cx,
            vec![reply("Selectable response text.")],
            |shell, peer, current, cx| {
                archive_both(&shell, cx);
                let point = text_point(&peer, "reply#text.0:0");
                cx.simulate_mouse_down(point, button, Default::default());
                cx.simulate_mouse_up(point, button, Default::default());
                assert_peer_active(&shell, &peer, &current, cx);
                assert_archived(&shell, cx);
                shell.update(cx, |shell, cx| assert!(shell.select_chat_pane(1, cx)));
                draw(cx);
                assert_archived(&shell, cx);
                selection::end_active_drag();
                selection::clear_if_owner(&key(&peer, "reply#text.0:0"));
            },
        );
    }
}
