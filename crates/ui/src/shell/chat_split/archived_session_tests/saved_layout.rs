use super::persistence::{cold_start, saved_reference};
use super::*;

#[test]
fn saved_layout_normalizes_large_and_tiny_finite_weights() {
    for weights in [
        vec![f32::MAX; 2],
        vec![f32::from_bits(1); 2],
        vec![2.0, 3.0],
    ] {
        let mut saved = saved_reference().chat_layout.unwrap();
        saved.shares = weights.clone();
        let restored = ChatSplit::from_saved(&saved, |_| true).unwrap();
        assert!(
            restored
                .shares
                .iter()
                .all(|share| share.is_finite() && *share > 0.0)
        );
        assert!((restored.shares.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        let total: f64 = weights.iter().map(|weight| f64::from(*weight)).sum();
        for (actual, weight) in restored.shares.iter().zip(weights) {
            assert!((f64::from(*actual) - f64::from(weight) / total).abs() < 1e-6);
        }
    }
}

#[test]
fn saved_layout_never_restores_zero_sized_or_nonfinite_panes() {
    for weight in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut saved = saved_reference().chat_layout.unwrap();
        saved.shares[0] = weight;
        assert!(ChatSplit::from_saved(&saved, |_| true).is_none());
    }
    let mut saved = saved_reference().chat_layout.unwrap();
    saved.shares = vec![f32::MAX, f32::from_bits(1)];
    // Both source weights are positive, but their ratio cannot be
    // represented as positive f32 pane geometry. Reject instead of hiding it.
    assert!(ChatSplit::from_saved(&saved, |_| true).is_none());
}

#[gpui::test]
fn malformed_disk_layout_preserves_a_valid_archived_selection(cx: &mut gpui::TestAppContext) {
    for case in 0..6 {
        let mut saved = saved_reference();
        let layout = saved.chat_layout.as_mut().unwrap();
        match case {
            0 => layout.focus = layout.panes.len(),
            1 => {
                layout.shares.pop();
            }
            2 => layout.shares[0] = 0.0,
            3 => layout.shares[0] = -0.1,
            4 => {
                layout.panes = vec![None];
                layout.shares = vec![1.0];
                layout.focus = 0;
            }
            _ => {
                layout.panes = vec![None; MAX_CHAT_PANES + 1];
                layout.shares = vec![1.0; MAX_CHAT_PANES + 1];
            }
        }
        let disk = tempfile::tempdir().unwrap();
        saved.save(disk.path()).unwrap();
        let (_dir, window) = cold_start(cx, settings::UiSettings::load(disk.path()));
        window
            .update(cx, |shell, _, cx| {
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
                    Some("current")
                );
                assert!(shell.state.read(cx).selected_chat_row().unwrap().archived);
                assert!(shell.chat_split.is_none());
                assert!(shell.settings.chat_layout.is_none());
                assert!(shell.peer_chat_views.is_empty());
            })
            .unwrap();
    }
}
