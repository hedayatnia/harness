use super::*;

#[derive(Clone, Copy)]
enum EarlyChoice {
    NewSession,
    NewTab,
    Split(SplitAxis),
}

pub(super) fn cold_start(
    cx: &mut gpui::TestAppContext,
    saved: settings::UiSettings,
) -> (tempfile::TempDir, gpui::WindowHandle<Shell>) {
    let (dir, window) = fixture(cx, false, None);
    window
        .update(cx, |shell, _, cx| {
            shell.settings = saved;
            shell.boot_restored = false;
            shell.state.update(cx, |state, cx| {
                state.select_chat(None, cx);
                // Model the untouched boot canvas, not an explicit user choice.
                state.auto_selected = false;
                state.chats_synced = false;
                for chat in &mut state.chats {
                    chat.archived = true;
                }
            });
        })
        .unwrap();
    (dir, window)
}

pub(super) fn saved_reference() -> settings::UiSettings {
    let mut saved = settings::UiSettings::default();
    saved.last_chat_id = Some("current".into());
    saved.chat_layout = Some(
        ChatSplit::split(None, SplitAxis::Horizontal, Some("peer".into()), None)
            .unwrap()
            .to_saved(),
    );
    saved
}

fn assert_early_choice_wins(cx: &mut gpui::TestAppContext, choice: EarlyChoice) {
    let (_dir, window) = cold_start(cx, saved_reference());
    let (layout, tabs) = window
        .update(cx, |shell, window, cx| {
            match choice {
                EarlyChoice::NewSession => shell.open_new_session(cx),
                EarlyChoice::NewTab => shell.new_chat_tab(None, window, cx),
                EarlyChoice::Split(axis) => shell.split_chat(axis, window, cx),
            }
            assert!(!shell.boot_restored);
            let expected = (shell.chat_split.clone(), shell.chat_tabs.len());
            shell.state.update(cx, |state, cx| {
                state.apply_chats(state.chats.clone());
                cx.notify();
            });
            expected
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |shell, _, cx| {
            assert!(shell.boot_restored);
            assert!(
                shell.state.read(cx).selected_chat.is_none(),
                "late startup restoration must not override an explicit canvas choice"
            );
            assert_eq!(
                shell.chat_split, layout,
                "late sync must keep the layout the user just opened"
            );
            assert_eq!(shell.chat_tabs.len(), tabs);
            assert!(shell.peer_chat_views.is_empty());
            assert!(shell.state.read(cx).chats.iter().all(|chat| chat.archived));
        })
        .unwrap();
}

#[gpui::test]
fn new_session_before_initial_sync_overrides_saved_archive(cx: &mut gpui::TestAppContext) {
    assert_early_choice_wins(cx, EarlyChoice::NewSession);
}

#[gpui::test]
fn new_tab_before_initial_sync_overrides_saved_archive(cx: &mut gpui::TestAppContext) {
    assert_early_choice_wins(cx, EarlyChoice::NewTab);
}

#[gpui::test]
fn new_split_before_initial_sync_overrides_saved_archive(cx: &mut gpui::TestAppContext) {
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        assert_early_choice_wins(cx, EarlyChoice::Split(axis));
    }
}

#[gpui::test]
fn selecting_archived_reference_before_sync_overrides_saved_layout(cx: &mut gpui::TestAppContext) {
    let (_dir, window) = cold_start(cx, saved_reference());
    window
        .update(cx, |shell, window, cx| {
            shell.open_chat_in_tab("target".into(), window, cx);
            shell.state.update(cx, |state, cx| {
                state.apply_chats(state.chats.clone());
                cx.notify();
            });
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |shell, _, cx| {
            assert_eq!(
                shell.state.read(cx).selected_chat.as_deref(),
                Some("target")
            );
            assert!(shell.state.read(cx).selected_chat_row().unwrap().archived);
            assert!(shell.chat_split.is_none());
            assert!(shell.peer_chat_views.is_empty());
        })
        .unwrap();
}

#[gpui::test]
fn archived_layout_round_trips_through_disk_at_every_focus(cx: &mut gpui::TestAppContext) {
    let ids = [Some("current"), Some("peer"), Some("target"), None];
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        for (focus, selected) in ids.into_iter().enumerate() {
            let (dir, source) = fixture(cx, false, None);
            source
                .update(cx, |shell, window, cx| {
                    shell.boot_restored = true;
                    shell.state.update(cx, |state, _| {
                        state.chats_synced = true;
                        for chat in &mut state.chats {
                            chat.archived = true;
                        }
                    });
                    for open in [Some("peer"), Some("target"), None] {
                        shell.split_chat_opening(axis, open.map(str::to_owned), window, cx);
                    }
                    shell.focus_chat_pane(focus, window, cx);
                    let split = shell.chat_split.as_mut().unwrap();
                    split.drag_divider(&[0.25; 4], 1, 0.08);
                    shell.persist_chat_layout(cx);
                    shell.state.update(cx, |_, cx| cx.notify());
                })
                .unwrap();
            cx.run_until_parked();
            let saved = source
                .update(cx, |shell, _, cx| {
                    assert_eq!(shell.settings.last_chat_id.as_deref(), selected);
                    assert_eq!(shell.settings.chat_layout.as_ref().unwrap().focus, focus);
                    assert_eq!(
                        shell.settings.chat_layout,
                        shell.chat_split.as_ref().map(ChatSplit::to_saved)
                    );
                    let published = settings::with_current(cx, Clone::clone);
                    assert_eq!(published.last_chat_id, shell.settings.last_chat_id);
                    assert_eq!(published.chat_layout, shell.settings.chat_layout);
                    shell.settings.clone()
                })
                .unwrap();
            saved.save(dir.path()).unwrap();
            let loaded = settings::UiSettings::load(dir.path());
            assert_eq!(loaded.last_chat_id, saved.last_chat_id);
            assert_eq!(loaded.chat_layout, saved.chat_layout);
            let (_restart_dir, restored) = cold_start(cx, loaded);
            restored
                .update(cx, |shell, _, cx| {
                    shell.state.update(cx, |state, cx| {
                        state.apply_chats(state.chats.clone());
                        cx.notify();
                    });
                })
                .unwrap();
            cx.run_until_parked();
            restored
                .update(cx, |shell, _, cx| {
                    assert_eq!(shell.state.read(cx).selected_chat.as_deref(), selected);
                    let layout = shell.chat_split.as_ref().unwrap();
                    assert_eq!(layout.axis, axis);
                    assert_eq!(layout.focus, focus);
                    assert_eq!(layout.panes.len(), 4);
                    for (ix, expected) in ids.into_iter().enumerate() {
                        assert_eq!(
                            layout.panes[ix].as_deref(),
                            if ix == focus { None } else { expected }
                        );
                    }
                    for (actual, expected) in layout
                        .shares
                        .iter()
                        .zip(&saved.chat_layout.as_ref().unwrap().shares)
                    {
                        assert!((actual - expected).abs() < 1e-6);
                    }
                    assert!(shell.state.read(cx).chats.iter().all(|chat| chat.archived));
                    assert_eq!(shell.state.read(cx).visible_chats().count(), 0);
                })
                .unwrap();
        }
    }
}
