use super::*;

#[gpui::test]
fn sidebar_search_trigger_completes_initial_presses_with_mixed_repeats(
    cx: &mut gpui::TestAppContext,
) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    for key in ["enter", "space"] {
        cx.update(|window, cx| {
            window.focus(&shell.read(cx).sidebar_search_trigger_focus.clone(), cx)
        });
        key_down(key, cx);
        for is_held in [true, false, true] {
            cx.update(|window, cx| {
                window.dispatch_event(
                    gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                        keystroke: Keystroke::parse(key).unwrap(),
                        is_held,
                        prefer_character_input: false,
                    }),
                    cx,
                );
            });
            cx.run_until_parked();
        }
        shell.read_with(cx, |shell, _| {
            assert!(
                shell.sidebar_search.is_none(),
                "activation waits for the physical release"
            )
        });
        key_up(key, cx);
        shell.read_with(cx, |shell, _| {
            assert!(
                shell.sidebar_search.is_some(),
                "ignoring repeats must not cancel the original press"
            )
        });
        press_keys(cx, "escape");
    }
}

#[gpui::test]
fn sidebar_search_trigger_cancels_when_release_becomes_modified(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    for (key, modified) in [("enter", "ctrl-enter"), ("space", "shift-space")] {
        cx.update(|window, cx| {
            window.focus(&shell.read(cx).sidebar_search_trigger_focus.clone(), cx)
        });
        key_down(key, cx);
        key_up(modified, cx);
        key_up(key, cx);
        shell.read_with(cx, |shell, _| {
            assert!(
                shell.sidebar_search.is_none(),
                "a modified release cancels pending activation"
            )
        });
        press_keys(cx, key);
        shell.read_with(cx, |shell, _| {
            assert!(
                shell.sidebar_search.is_some(),
                "a cancelled press must not block the next complete press"
            )
        });
        press_keys(cx, "escape");
    }
}

#[gpui::test]
fn sidebar_search_trigger_ignores_orphan_releases_and_flagged_repeats(
    cx: &mut gpui::TestAppContext,
) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    for key in ["enter", "space"] {
        cx.update(|window, cx| {
            window.focus(&shell.read(cx).sidebar_search_trigger_focus.clone(), cx);
        });
        key_up(key, cx);
        shell.read_with(cx, |shell, _| assert!(shell.sidebar_search.is_none()));
        held_key(key, cx);
        shell.read_with(cx, |shell, _| {
            assert!(
                shell.sidebar_search.is_none(),
                "a repeat without an initial press must not activate the trigger"
            );
        });
        press_keys(cx, key);
        shell.read_with(cx, |shell, _| assert!(shell.sidebar_search.is_some()));
        press_keys(cx, "escape");
    }
}

#[gpui::test]
fn sidebar_search_trigger_cancels_a_press_interrupted_by_focus_or_another_key(
    cx: &mut gpui::TestAppContext,
) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    let input = open_search(&shell, cx);
    for key in ["enter", "space"] {
        let focus = shell.read_with(cx, |shell, _| shell.sidebar_search_trigger_focus.clone());
        cx.update(|window, cx| window.focus(&focus, cx));
        key_down(key, cx);
        cx.update(|window, cx| window.focus(&input.focus_handle(cx), cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.focus(&focus, cx));
        key_up(key, cx);
        shell.read_with(cx, |shell, _| {
            assert!(
                shell.sidebar_search.is_some(),
                "focus away and back cancels a pending activation"
            )
        });
        key_down(key, cx);
        key_down("right", cx);
        key_up("right", cx);
        key_up(key, cx);
        shell.read_with(cx, |shell, _| {
            assert!(
                shell.sidebar_search.is_some(),
                "an intervening key cancels a pending activation"
            )
        });
    }
}

#[gpui::test]
fn sidebar_search_key_latch_recovers_after_window_deactivation(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let _input = open_search(&shell, cx);
    let focus = shell.read_with(cx, |shell, _| {
        shell.sidebar_search.as_ref().unwrap().archive_focus.clone()
    });
    cx.update(|window, cx| window.focus(&focus, cx));
    key_down("space", cx);
    cx.deactivate_window();
    shell.read_with(cx, |shell, _| {
        assert!(
            shell
                .sidebar_search
                .as_ref()
                .unwrap()
                .pressed_keys
                .is_empty(),
            "release can happen in another application"
        );
        assert!(shell.sidebar_search.as_ref().unwrap().include_archived);
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    press_keys(cx, "space");
    shell.read_with(cx, |shell, _| {
        assert!(!shell.sidebar_search.as_ref().unwrap().include_archived)
    });
}

#[gpui::test]
fn sidebar_search_pointer_release_does_not_open_a_reordered_replacement(
    cx: &mut gpui::TestAppContext,
) {
    for change in ["insert", "delete", "rename"] {
        let (_dir, shell, cx) = fixture(cx, 280.0);
        seed_matches(&shell, cx);
        let input = open_search(&shell, cx);
        query(&input, "sidebar", cx);
        let point = cx.debug_bounds("palette-chat-b").unwrap().center();
        cx.simulate_mouse_down(point, MouseButton::Left, Default::default());
        cx.update(|_, cx| {
            shell.update(cx, |shell, cx| {
                shell.state.update(cx, |state, cx| {
                    match change {
                        "insert" => state
                            .chats
                            .push(chat("00", "Sidebar incoming", false, "one")),
                        "delete" => state.chats.retain(|chat| chat.id != "b"),
                        "rename" => {
                            state
                                .chats
                                .iter_mut()
                                .find(|chat| chat.id == "b")
                                .unwrap()
                                .title = Some("No longer matching".into())
                        }
                        _ => unreachable!(),
                    }
                    cx.notify();
                })
            })
        });
        cx.run_until_parked();
        cx.simulate_mouse_up(point, MouseButton::Left, Default::default());
        cx.run_until_parked();
        shell.read_with(cx, |shell, cx| {
            assert_eq!(shell.state.read(cx).selected_chat.as_deref(), Some("current"), "{change} between pointer down and up must not open the replacement under the pointer");
        });
    }
}
