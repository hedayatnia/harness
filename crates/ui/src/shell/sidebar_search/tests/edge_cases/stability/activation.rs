use super::*;

#[gpui::test]
fn sidebar_search_trigger_ignores_unflagged_repeats(cx: &mut gpui::TestAppContext) {
    for key in ["enter", "space"] {
        let (_dir, shell, cx) = fixture(cx, 280.0);
        let _input = open_search(&shell, cx);
        cx.update(|window, cx| {
            window.focus(&shell.read(cx).sidebar_search_trigger_focus.clone(), cx);
        });
        key_down(key, cx);
        key_down(key, cx);
        key_down(key, cx);
        key_up(key, cx);
        shell.read_with(cx, |shell, _| {
            assert!(
                shell.sidebar_search.is_none(),
                "one physical press must close search only once"
            );
        });
        press_keys(cx, key);
        shell.read_with(cx, |shell, _| assert!(shell.sidebar_search.is_some()));
    }
}

#[gpui::test]
fn sidebar_search_trigger_does_not_consume_modified_button_keys(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    for key in ["ctrl-enter", "alt-space", "shift-enter"] {
        cx.update(|window, cx| {
            window.focus(&shell.read(cx).sidebar_search_trigger_focus.clone(), cx);
        });
        press_keys(cx, key);
        shell.read_with(cx, |shell, _| {
            assert!(
                shell.sidebar_search.is_none(),
                "{key} is not a plain button activation"
            );
        });
    }
}

#[gpui::test]
fn sidebar_search_archive_ignores_modified_button_keys(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    let _input = open_search(&shell, cx);
    for key in ["ctrl-space", "alt-enter", "shift-space"] {
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
        press_keys(cx, key);
        shell.read_with(cx, |shell, _| {
            assert!(
                !shell.sidebar_search.as_ref().unwrap().include_archived,
                "{key} must not change archive scope"
            );
        });
    }
}

#[gpui::test]
fn sidebar_search_clear_ignores_modified_button_keys(cx: &mut gpui::TestAppContext) {
    let (_dir, shell, cx) = fixture(cx, 280.0);
    let input = open_search(&shell, cx);
    query(&input, "sidebar", cx);
    for key in ["ctrl-enter", "alt-space", "shift-enter"] {
        cx.update(|window, cx| {
            let focus = shell
                .read(cx)
                .sidebar_search
                .as_ref()
                .unwrap()
                .clear_focus
                .clone();
            window.focus(&focus, cx);
        });
        press_keys(cx, key);
        assert_eq!(
            input.read_with(cx, |input, _| input.text().to_string()),
            "sidebar",
            "{key} must not erase the query"
        );
    }
}

#[gpui::test]
fn sidebar_search_enter_rejects_a_target_that_leaves_the_scope_before_redraw(
    cx: &mut gpui::TestAppContext,
) {
    for change in ["archive", "project", "rename", "child"] {
        let (_dir, shell, cx) = fixture(cx, 280.0);
        seed_matches(&shell, cx);
        let input = open_search(&shell, cx);
        query(&input, "sidebar", cx);
        cx.update(|_, cx| {
            shell.update(cx, |shell, cx| {
                shell.set_space_filter(Some("one".into()), cx)
            })
        });
        cx.run_until_parked();
        press_keys(cx, "down");
        assert_eq!(highlighted(&shell, cx), "b");
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.state.update(cx, |state, _| {
                    let chat = state.chats.iter_mut().find(|chat| chat.id == "b").unwrap();
                    match change {
                        "archive" => chat.archived = true,
                        "project" => chat.space_id = Some("two".into()),
                        "rename" => chat.title = Some("No longer matching".into()),
                        "child" => chat.parent_chat_id = Some("a".into()),
                        _ => unreachable!(),
                    }
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
                assert_eq!(
                    shell.state.read(cx).selected_chat.as_deref(),
                    Some("current"),
                    "{change} must not retarget Enter to a replacement"
                );
            });
        });
        key_up("enter", cx);
        press_keys(cx, "enter");
        shell.read_with(cx, |shell, cx| {
            assert_eq!(
                shell.state.read(cx).selected_chat.as_deref(),
                Some("c"),
                "a fresh press may choose the now-visible replacement"
            );
        });
    }
}
