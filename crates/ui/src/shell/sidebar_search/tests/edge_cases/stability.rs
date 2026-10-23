use super::*;

mod activation;
mod lifecycle;

fn seed_matches(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) {
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, cx| {
                state.chats = vec![
                    chat("current", "Current session", false, "one"),
                    chat("a", "Sidebar alpha", false, "one"),
                    chat("b", "Sidebar beta", false, "one"),
                    chat("c", "Sidebar gamma", false, "one"),
                ];
                cx.notify();
            })
        })
    });
    cx.run_until_parked();
}

fn highlighted(shell: &Entity<Shell>, cx: &gpui::VisualTestContext) -> String {
    shell.read_with(cx, |shell, cx| {
        shell.sidebar_search_ids(cx)[shell.sidebar_search.as_ref().unwrap().active].clone()
    })
}

fn key_down(key: &str, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                keystroke: Keystroke::parse(key).unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
    });
    cx.run_until_parked();
}

fn key_up(key: &str, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::KeyUp(gpui::KeyUpEvent {
                keystroke: Keystroke::parse(key).unwrap(),
            }),
            cx,
        );
    });
    cx.run_until_parked();
}

#[gpui::test]
fn sidebar_search_preserves_highlight_when_a_match_arrives(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    seed_matches(&shell, cx);
    let input = open_search(&shell, cx);
    query(&input, "sidebar", cx);
    press_keys(cx, "down");
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, cx| {
                state
                    .chats
                    .push(chat("00", "Sidebar incoming", false, "one"));
                cx.notify();
            })
        })
    });
    cx.run_until_parked();
    assert_eq!(
        highlighted(&shell, cx),
        "b",
        "incoming results must not change the chosen session"
    );
    press_keys(cx, "enter");
    shell.read_with(cx, |shell, cx| {
        assert_eq!(shell.state.read(cx).selected_chat.as_deref(), Some("b"))
    });
}

#[gpui::test]
fn sidebar_search_preserves_highlight_when_an_earlier_match_is_renamed(
    cx: &mut gpui::TestAppContext,
) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    seed_matches(&shell, cx);
    let input = open_search(&shell, cx);
    query(&input, "sidebar", cx);
    press_keys(cx, "down");
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, cx| {
                state
                    .chats
                    .iter_mut()
                    .find(|chat| chat.id == "a")
                    .unwrap()
                    .title = Some("Renamed away".into());
                cx.notify();
            })
        })
    });
    cx.run_until_parked();
    assert_eq!(highlighted(&shell, cx), "b");
}

#[gpui::test]
fn sidebar_search_preserves_highlight_when_sort_changes(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    seed_matches(&shell, cx);
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, cx| {
                let newest = Utc::now();
                for (i, chat) in state
                    .chats
                    .iter_mut()
                    .filter(|chat| chat.id != "current")
                    .enumerate()
                {
                    chat.created_at = newest + chrono::Duration::seconds(i as i64);
                    chat.last_message_at = Some(newest - chrono::Duration::seconds(i as i64));
                }
                cx.notify();
            })
        })
    });
    cx.run_until_parked();
    let input = open_search(&shell, cx);
    query(&input, "sidebar", cx);
    assert_eq!(highlighted(&shell, cx), "a");
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.settings.sidebar_sort = SidebarSort::Created;
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert_eq!(
        highlighted(&shell, cx),
        "a",
        "sorting changes positions, not session identity"
    );
}

#[gpui::test]
fn sidebar_search_enter_reconciles_an_update_before_redraw(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    seed_matches(&shell, cx);
    let input = open_search(&shell, cx);
    query(&input, "sidebar", cx);
    press_keys(cx, "down");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, _| {
                state
                    .chats
                    .push(chat("00", "Sidebar incoming", false, "one"))
            });
            shell.sidebar_search_key(
                &gpui::KeyDownEvent {
                    keystroke: Keystroke::parse("enter").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            );
            assert_eq!(shell.state.read(cx).selected_chat.as_deref(), Some("b"));
        })
    });
}

#[gpui::test]
fn sidebar_search_enter_does_not_open_a_deleted_targets_replacement(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    seed_matches(&shell, cx);
    let input = open_search(&shell, cx);
    query(&input, "sidebar", cx);
    press_keys(cx, "down");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell
                .state
                .update(cx, |state, _| state.chats.retain(|chat| chat.id != "b"));
            shell.sidebar_search_key(
                &gpui::KeyDownEvent {
                    keystroke: Keystroke::parse("enter").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            );
            assert_eq!(
                shell.state.read(cx).selected_chat.as_deref(),
                Some("current"),
                "a disappeared target must not silently turn into the next session"
            );
        })
    });
}

#[gpui::test]
fn sidebar_search_scope_reset_applies_before_keyboard_activation(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    seed_matches(&shell, cx);
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, cx| {
                state.chats.push(chat("x", "Sidebar X", false, "two"));
                state.chats.push(chat("y", "Sidebar Y", false, "two"));
                cx.notify();
            })
        })
    });
    cx.run_until_parked();
    let input = open_search(&shell, cx);
    query(&input, "sidebar", cx);
    press_keys(cx, "down down");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_space_filter(Some("two".into()), cx);
            shell.sidebar_search_key(
                &gpui::KeyDownEvent {
                    keystroke: Keystroke::parse("enter").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            );
            assert_eq!(shell.state.read(cx).selected_chat.as_deref(), Some("x"));
        })
    });
}

#[gpui::test]
fn sidebar_search_archive_toggle_ignores_unflagged_repeat_until_release(
    cx: &mut gpui::TestAppContext,
) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    let _input = open_search(&shell, cx);
    cx.update(|window, cx| {
        let focus = shell
            .read(cx)
            .sidebar_search
            .as_ref()
            .unwrap()
            .archive_focus
            .clone();
        window.focus(&focus, cx);
    });
    key_down("space", cx);
    key_down("space", cx);
    shell.read_with(cx, |shell, _| {
        assert!(
            shell.sidebar_search.as_ref().unwrap().include_archived,
            "X11 can repeat keydown without is_held"
        )
    });
    key_up("space", cx);
    shell.read_with(cx, |shell, _| {
        assert!(
            shell.sidebar_search.as_ref().unwrap().include_archived,
            "the native keyup click must not toggle a second time"
        )
    });
    key_down("space", cx);
    shell.read_with(cx, |shell, _| {
        assert!(!shell.sidebar_search.as_ref().unwrap().include_archived)
    });
    key_up("space", cx);
    shell.read_with(cx, |shell, _| {
        assert!(!shell.sidebar_search.as_ref().unwrap().include_archived)
    });
}

#[gpui::test]
fn sidebar_search_escape_ignores_unflagged_repeat_until_release(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    let input = open_search(&shell, cx);
    query(&input, "sidebar", cx);
    key_down("escape", cx);
    key_down("escape", cx);
    shell.read_with(cx, |shell, _| {
        assert!(
            shell.sidebar_search.is_some(),
            "one physical Escape must not both clear and collapse search"
        )
    });
    key_up("escape", cx);
    key_down("escape", cx);
    key_up("escape", cx);
    shell.read_with(cx, |shell, _| assert!(shell.sidebar_search.is_none()));
}

#[gpui::test]
fn sidebar_search_keyboard_clear_restores_input_and_archive_defaults(
    cx: &mut gpui::TestAppContext,
) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    let input = open_search(&shell, cx);
    query(&input, "sidebar", cx);
    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.toggle_sidebar_search_archived(cx)));
    press_keys(cx, "down tab space");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(input.focus_handle(cx).is_focused(window));
        assert_eq!(input.read(cx).text(), "");
        assert!(
            !shell
                .read(cx)
                .sidebar_search
                .as_ref()
                .unwrap()
                .include_archived
        );
    });
    assert!(cx.debug_bounds("sidebar-search-results").is_none());
}

#[gpui::test]
fn sidebar_search_grapheme_editing_and_undo_keep_query_intact(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    let input = open_search(&shell, cx);
    query(&input, "ab👩‍💻cd", cx);
    press_keys(cx, "left left backspace");
    assert_eq!(
        input.read_with(cx, |input, _| input.text().to_string()),
        "abcd"
    );
    press_keys(cx, &platform_combo("mod-z"));
    assert_eq!(
        input.read_with(cx, |input, _| input.text().to_string()),
        "ab👩‍💻cd"
    );
}

#[gpui::test]
fn sidebar_search_missing_keyup_does_not_stick_after_focus_leaves(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    // Test windows start inactive; GPUI intentionally omits focus-out paths
    // for inactive windows. Exercise this lifecycle in a foreground window.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let _input = open_search(&shell, cx);
    let archive_focus = shell.read_with(cx, |shell, _| {
        shell.sidebar_search.as_ref().unwrap().archive_focus.clone()
    });
    cx.update(|window, cx| {
        window.focus(&archive_focus, cx);
        window.draw(cx).clear();
    });
    cx.run_until_parked();
    key_down("space", cx);
    cx.update(|window, cx| {
        let trigger = shell.read(cx).sidebar_search_trigger_focus.clone();
        window.focus(&trigger, cx);
        // Focus observers are delivered at the completed frame boundary.
        window.draw(cx).clear();
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert!(!shell.sidebar_search_has_focus(window, cx));
        assert!(
            shell
                .sidebar_search
                .as_ref()
                .unwrap()
                .pressed_keys
                .is_empty(),
            "leaving the controls must release their key latch"
        );
    });
    // The key was released elsewhere; no keyup reaches the search controls.
    cx.update(|window, cx| window.focus(&archive_focus, cx));
    key_down("space", cx);
    key_up("space", cx);
    shell.read_with(cx, |shell, _| {
        assert!(!shell.sidebar_search.as_ref().unwrap().include_archived)
    });
}

#[gpui::test]
fn sidebar_search_query_and_archive_changes_reset_stable_selection(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    seed_matches(&shell, cx);
    let input = open_search(&shell, cx);
    query(&input, "sidebar", cx);
    press_keys(cx, "down");
    assert_eq!(highlighted(&shell, cx), "b");
    query(&input, "sidebar ", cx);
    assert_eq!(
        highlighted(&shell, cx),
        "a",
        "editing the query intentionally starts a fresh choice"
    );
    press_keys(cx, "down");
    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.toggle_sidebar_search_archived(cx)));
    cx.run_until_parked();
    assert_eq!(
        highlighted(&shell, cx),
        "a",
        "changing archive scope intentionally starts a fresh choice"
    );
}

#[gpui::test]
fn sidebar_search_trigger_closes_once_per_complete_keyboard_press(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    for key in ["enter", "space"] {
        cx.update(|window, cx| {
            let focus = shell.read(cx).sidebar_search_trigger_focus.clone();
            window.focus(&focus, cx);
        });
        press_keys(cx, key);
        shell.read_with(cx, |shell, _| assert!(shell.sidebar_search.is_some()));
        cx.update(|window, cx| {
            let focus = shell.read(cx).sidebar_search_trigger_focus.clone();
            window.focus(&focus, cx);
        });
        press_keys(cx, key);
        shell.read_with(cx, |shell, _| {
            assert!(
                shell.sidebar_search.is_none(),
                "key release must not reopen the search just closed on keydown"
            )
        });
    }
}
