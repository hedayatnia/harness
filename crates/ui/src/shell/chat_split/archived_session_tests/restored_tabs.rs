use super::persistence::cold_start;
use super::*;

#[gpui::test]
fn archived_tabs_restore_and_deduplicate_their_focused_sessions(cx: &mut gpui::TestAppContext) {
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        for active in 0..2 {
            let selections = ["current", "target"];
            let peers = ["peer", "parked"];
            let tabs: Vec<_> = (0..2)
                .map(|ix| {
                    // An older save can repeat the selected session in a
                    // peer slot. Keep the distinct peer and the pane geometry.
                    let split = ChatSplit::split(None, axis, Some(selections[ix].into()), None);
                    let split =
                        ChatSplit::split(split, axis, Some(peers[ix].into()), None).unwrap();
                    settings::SavedChatTab {
                        selected: Some(selections[ix].into()),
                        project: None,
                        layout: Some(split.to_saved()),
                    }
                })
                .collect();
            let mut saved = settings::UiSettings::default();
            saved.last_chat_id = tabs[active].selected.clone();
            saved.chat_layout = tabs[active].layout.clone();
            saved.chat_tabs = tabs;
            saved.chat_tab = active;
            let (_dir, window) = cold_start(cx, saved);
            window
                .update(cx, |shell, _, cx| {
                    shell.state.update(cx, |state, cx| {
                        state.apply_chats(state.chats.clone());
                        cx.notify();
                    });
                    shell.boot_select_chat(cx);
                })
                .unwrap();
            cx.run_until_parked();
            window
                .update(cx, |shell, window, cx| {
                    assert_eq!(shell.chat_tabs.len(), 2);
                    assert_eq!(shell.chat_tab, active);
                    for ix in [active, 1 - active, active] {
                        shell.switch_chat_tab(ix, window, cx);
                        assert_eq!(
                            shell.state.read(cx).selected_chat.as_deref(),
                            Some(selections[ix])
                        );
                        let split = shell.chat_split.as_ref().unwrap();
                        assert_eq!(split.axis, axis);
                        assert_eq!(split.focus, 2);
                        assert!(
                            split.panes[0].is_none(),
                            "the focused session must not also be a peer"
                        );
                        assert_eq!(split.panes[1].as_deref(), Some(peers[ix]));
                        assert!(split.panes[2].is_none());
                    }
                    assert!(shell.state.read(cx).chats.iter().all(|chat| chat.archived));
                    assert_eq!(shell.state.read(cx).visible_chats().count(), 0);
                })
                .unwrap();
        }
    }
}
