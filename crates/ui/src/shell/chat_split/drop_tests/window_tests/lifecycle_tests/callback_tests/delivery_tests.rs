use super::*;

#[derive(Clone, Copy)]
enum AfterDelivery {
    Replace,
    CancelReplacement,
    Finish,
}

struct DeliveryZones {
    accepts: bool,
    after: AfterDelivery,
    armed: bool,
    child_files: Vec<Vec<PathBuf>>,
    parent_files: Vec<Vec<PathBuf>>,
    stops: Vec<bool>,
}

impl DeliveryZones {
    fn after_delivery(&mut self, window: &mut Window, cx: &mut gpui::App) {
        if !std::mem::replace(&mut self.armed, false) {
            return;
        }
        // The original payload has already been consumed. Nested duplicate
        // platform submissions must not deliver it again, even before cleanup.
        for _ in 0..2 {
            window.dispatch_event(
                PlatformInput::FileDrop(FileDropEvent::Submit {
                    position: position(),
                }),
                cx,
            );
        }
        self.stops.push(cx.stop_active_drag(window));
        if !matches!(self.after, AfterDelivery::Finish) {
            window.dispatch_event(
                PlatformInput::FileDrop(FileDropEvent::Entered {
                    position: position(),
                    paths: ExternalPaths(vec![PathBuf::from("next.png")].into()),
                }),
                cx,
            );
            if matches!(self.after, AfterDelivery::CancelReplacement) {
                self.stops.push(cx.stop_active_drag(window));
            }
        }
    }
}

impl Render for DeliveryZones {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let accepts = self.accepts;
        div()
            .size_full()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.parent_files.push(paths.paths().to_vec());
                this.after_delivery(window, cx);
            }))
            .child(
                div()
                    .size_full()
                    .can_drop(move |_, _, _| accepts)
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                        this.child_files.push(paths.paths().to_vec());
                        this.after_delivery(window, cx);
                    })),
            )
    }
}

fn delivery_case(cx: &mut gpui::TestAppContext, after: AfterDelivery) {
    for accepts in [true, false] {
        let (view, cx) = cx.add_window_view(|_, _| DeliveryZones {
            accepts,
            after,
            armed: true,
            child_files: Vec::new(),
            parent_files: Vec::new(),
            stops: Vec::new(),
        });
        let mut cx = cx.clone();
        draw(&mut cx);
        enter(&mut cx, position(), "original.png".into());
        submit(&mut cx);
        let replacement = matches!(after, AfterDelivery::Replace);
        cx.update(|_, cx| assert_eq!(cx.has_active_drag(), replacement));
        // A surviving replacement belongs to this new submit, not the old
        // release. A cancelled/finished drag makes all late submits no-ops.
        submit(&mut cx);
        submit(&mut cx);
        view.read_with(&cx, |view, _| {
            let mut expected = vec![vec![PathBuf::from("original.png")]];
            if replacement {
                expected.push(vec![PathBuf::from("next.png")]);
            }
            if accepts {
                assert_eq!(view.child_files, expected);
                assert!(view.parent_files.is_empty());
            } else {
                assert_eq!(view.parent_files, expected);
                assert!(view.child_files.is_empty());
            }
            let expected_stops = if matches!(after, AfterDelivery::CancelReplacement) {
                vec![false, true]
            } else {
                vec![false]
            };
            assert_eq!(view.stops, expected_stops);
        });
        cx.update(|_, cx| assert!(!cx.has_active_drag()));
    }
}

#[gpui::test]
fn a_drop_handler_can_start_a_separate_drag_without_duplicate_delivery(
    cx: &mut gpui::TestAppContext,
) {
    delivery_case(cx, AfterDelivery::Replace);
}

#[gpui::test]
fn a_drop_handler_can_cancel_its_replacement_without_reviving_the_original(
    cx: &mut gpui::TestAppContext,
) {
    delivery_case(cx, AfterDelivery::CancelReplacement);
}

#[gpui::test]
fn late_submits_inside_delivery_cannot_reenter_the_consumed_payload(cx: &mut gpui::TestAppContext) {
    delivery_case(cx, AfterDelivery::Finish);
}
