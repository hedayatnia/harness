use super::*;
use navigation_tests::fixture;

fn selected_tabs(shell: &Shell, cx: &App) -> Vec<Option<String>> {
    if shell.chat_tabs.is_empty() {
        return vec![shell.state.read(cx).selected_chat.clone()];
    }
    shell
        .chat_tabs
        .iter()
        .enumerate()
        .map(|(ix, tab)| {
            if ix == shell.chat_tab {
                shell.state.read(cx).selected_chat.clone()
            } else {
                tab.selected.clone()
            }
        })
        .collect()
}

#[gpui::test]
fn reopen_closed_chat_tab_restores_position_layout_and_draft(cx: &mut gpui::TestAppContext) {
    let (_dir, window) = fixture(cx, true, None);
    window
        .update(cx, |shell, window, cx| {
            let original = shell.park_chat_tab(cx);
            shell.new_chat_tab(Some("target".into()), window, cx);
            shell.new_chat_tab(Some("parked".into()), window, cx);
            assert!(shell.close_chat_tab_at(0, window, cx));
            assert_eq!(shell.chat_tab, 1);
            shell.reopen_closed_chat_tab(window, cx);
            assert_eq!(shell.chat_tab, 0);
            assert_eq!(shell.park_chat_tab(cx), original);
            assert_eq!(
                selected_tabs(shell, cx),
                ["current", "target", "parked"].map(|id| Some(id.into()))
            );
            // The insertion must also reindex visit history: closing the
            // restored tab returns to the previously focused parked tab.
            assert!(shell.close_chat_tab(window, cx));
            assert_eq!(shell.chat_tab, 1);
            assert_eq!(
                shell.state.read(cx).selected_chat.as_deref(),
                Some("parked")
            );
        })
        .unwrap();
}

#[gpui::test]
fn reopen_closed_chat_tab_shortcut_restores_in_reverse_order(cx: &mut gpui::TestAppContext) {
    let (_dir, window) = fixture(cx, false, None);
    window
        .update(cx, |shell, window, cx| {
            shell.new_chat_tab(Some("target".into()), window, cx);
            shell.new_chat_tab(Some("parked".into()), window, cx);
            shell.switch_chat_tab(1, window, cx);
            assert!(shell.close_focused_chat_pane_without_archiving(window, cx));
            assert!(shell.close_chat_tab_at(1, window, cx));
            assert!(shell.chat_tabs.is_empty());
        })
        .unwrap();
    let keystroke = gpui::Keystroke::parse(&platform_combo(
        crate::settings::ShortcutId::ReopenClosedTab.default_combo(),
    ))
    .unwrap();
    for expected in [
        vec!["current", "parked"],
        vec!["current", "target", "parked"],
    ] {
        cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear();
            window.dispatch_event(
                gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                    keystroke: keystroke.clone(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
            window.dispatch_event(
                gpui::PlatformInput::KeyUp(gpui::KeyUpEvent {
                    keystroke: keystroke.clone(),
                }),
                cx,
            );
        })
        .unwrap();
        window
            .update(cx, |shell, _, cx| {
                assert_eq!(
                    selected_tabs(shell, cx),
                    expected
                        .into_iter()
                        .map(|id| Some(id.into()))
                        .collect::<Vec<_>>()
                );
                assert_eq!(shell.chat_tab, 1);
            })
            .unwrap();
    }
    window
        .update(cx, |shell, window, cx| {
            let before = shell.park_chat_tab(cx);
            shell.reopen_closed_chat_tab(window, cx);
            assert_eq!(shell.park_chat_tab(cx), before);
            assert_eq!(shell.chat_tabs.len(), 3);
        })
        .unwrap();
}

#[gpui::test]
fn reopen_closed_chat_tab_preserves_canvas_and_clamps_old_position(cx: &mut gpui::TestAppContext) {
    let (_dir, window) = fixture(cx, false, None);
    window
        .update(cx, |shell, window, cx| {
            shell.new_chat_tab(Some("target".into()), window, cx);
            shell.new_chat_tab(None, window, cx);
            let draft = CanvasDraft {
                input: Some(crate::pickers::CanvasInput {
                    text: "unsent prompt".into(),
                    ..Default::default()
                }),
                ..Default::default()
            };
            shell.adopt_canvas_draft(Some(draft), cx);
            let original = shell.park_chat_tab(cx);
            assert!(shell.close_chat_tab(window, cx));
            // The old position was 2; move another tab out of the way without
            // adding to close history to simulate the layout changing.
            shell.chat_tabs.clear();
            shell.chat_tab = 0;
            shell.chat_tab_history.clear();
            shell.reopen_closed_chat_tab(window, cx);
            assert_eq!(shell.chat_tab, 1);
            assert_eq!(shell.chat_tabs.len(), 2);
            assert_eq!(shell.park_chat_tab(cx), original);
        })
        .unwrap();
}

#[gpui::test]
fn reopen_closed_chat_tab_restores_last_session_without_extra_canvas(
    cx: &mut gpui::TestAppContext,
) {
    let (_dir, window) = fixture(cx, false, None);
    window
        .update(cx, |shell, window, cx| {
            assert!(shell.close_chat_to_new_session(cx));
            assert!(shell.state.read(cx).selected_chat.is_none());
            shell.reopen_closed_chat_tab(window, cx);
            assert!(shell.chat_tabs.is_empty());
            assert_eq!(
                shell.state.read(cx).selected_chat.as_deref(),
                Some("current")
            );
            assert!(shell.close_chat_to_new_session(cx));
            shell.adopt_canvas_draft(
                Some(CanvasDraft {
                    input: Some(crate::pickers::CanvasInput {
                        text: "keep this new draft".into(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                cx,
            );
            shell.reopen_closed_chat_tab(window, cx);
            assert_eq!(shell.chat_tab, 0);
            assert_eq!(shell.chat_tabs.len(), 2);
            assert_eq!(
                shell.chat_tabs[1]
                    .draft
                    .as_ref()
                    .unwrap()
                    .input
                    .as_ref()
                    .unwrap()
                    .text,
                "keep this new draft"
            );
        })
        .unwrap();
}

#[gpui::test]
fn reopen_closed_chat_tab_undoes_pending_archives_without_losing_split_panes(
    cx: &mut gpui::TestAppContext,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = runtime.enter();
    let (out, mut requests) = tokio::sync::mpsc::channel(128);
    let (replies, inbound) = tokio::sync::mpsc::channel(128);
    let engine =
        crate::state::EngineHandle::from_test_client(harness_rpc::RpcClient::new(out, inbound));
    let (_dir, window) = fixture(cx, true, None);
    cx.update(|cx| settings::set_archive_sessions_on_close(true, cx));
    let original = window
        .update(cx, |shell, window, cx| {
            shell
                .state
                .update(cx, |state, _| state.set_test_engine(engine));
            let original = shell.park_chat_tab(cx);
            shell.new_chat_tab(Some("target".into()), window, cx);
            assert!(shell.close_chat_tab_at(0, window, cx));
            let archived = &shell.closed_chat_tabs.last().unwrap().archived;
            assert_eq!(archived, &["current", "peer"]);
            // Reopen before either archive has completed.
            shell.reopen_closed_chat_tab(window, cx);
            original
        })
        .unwrap();
    cx.run_until_parked();

    for archived in [true, false] {
        let mut mutations = Vec::new();
        while let Ok(request) = requests.try_recv() {
            let request: serde_json::Value = serde_json::from_str(&request).unwrap();
            if request["method"] == methods::MUTATE {
                mutations.push(request);
            }
        }
        assert_eq!(
            mutations.len(),
            2,
            "both sessions must be updated independently"
        );
        let mut ids: Vec<_> = mutations
            .iter()
            .map(|request| {
                assert_eq!(request["params"]["op"], "setChatArchived");
                assert_eq!(request["params"]["archived"], archived);
                request["params"]["chatId"].as_str().unwrap()
            })
            .collect();
        ids.sort();
        assert_eq!(ids, ["current", "peer"]);
        if archived {
            // A chats frame from the earlier close must not prune the peer
            // while the queued unarchive is still waiting for its response.
            window
                .update(cx, |shell, _, cx| {
                    shell.state.update(cx, |state, _| {
                        state.chats_synced = true;
                        for chat in &mut state.chats {
                            if ids.contains(&chat.id.as_str()) {
                                chat.archived = true;
                            }
                        }
                    });
                    shell.chat_split_on_state_changed(cx);
                    assert_eq!(shell.chat_split, original.split);
                })
                .unwrap();
        }
        runtime.block_on(async {
            for request in mutations {
                replies
                    .send(serde_json::json!({"id": request["id"], "ok": {}}).to_string())
                    .await
                    .unwrap();
            }
            while replies.capacity() < replies.max_capacity() {
                tokio::task::yield_now().await;
            }
        });
        cx.run_until_parked();
    }
    window
        .update(cx, |shell, _, cx| {
            assert_eq!(shell.park_chat_tab(cx), original);
            assert!(shell.chat_tab_unarchiving.is_empty());
            assert!(shell.state.read(cx).chats.iter().all(|chat| !chat.archived));
        })
        .unwrap();
}
