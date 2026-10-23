use super::*;

mod delivery_tests;
mod replacement_tests;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Default)]
struct ValidationState {
    cancel: Rc<Cell<bool>>,
    stop_results: Rc<RefCell<Vec<bool>>>,
    feedback: Rc<Cell<usize>>,
}

struct CallbackZones {
    validation: ValidationState,
    accepts: bool,
    parent_drops: Vec<Vec<PathBuf>>,
    child_drops: Vec<Vec<PathBuf>>,
}

impl Render for CallbackZones {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let validation = self.validation.clone();
        let feedback = self.validation.feedback.clone();
        let accepts = self.accepts;
        div()
            .id("callback-parent")
            .size_full()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, _| {
                this.parent_drops.push(paths.paths().to_vec());
            }))
            .child(
                div()
                    .id("callback-child")
                    .size_full()
                    .can_drop(move |_, window, cx| {
                        if validation.cancel.get() {
                            validation
                                .stop_results
                                .borrow_mut()
                                .push(cx.stop_active_drag(window));
                        }
                        accepts
                    })
                    .drag_over::<ExternalPaths>(move |style, _, _, _| {
                        feedback.set(feedback.get() + 1);
                        style
                    })
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, _| {
                        this.child_drops.push(paths.paths().to_vec());
                    })),
            )
    }
}

fn callback_window(
    cx: &mut gpui::TestAppContext,
    accepts: bool,
) -> (
    Entity<CallbackZones>,
    gpui::VisualTestContext,
    ValidationState,
) {
    let validation = ValidationState::default();
    let (view, cx) = cx.add_window_view(|_, _| CallbackZones {
        validation: validation.clone(),
        accepts,
        parent_drops: Vec::new(),
        child_drops: Vec::new(),
    });
    let mut cx = cx.clone();
    draw(&mut cx);
    enter(&mut cx, position(), "cancelled.png".into());
    (view, cx, validation)
}

fn submit(cx: &mut gpui::VisualTestContext) {
    file_event(
        cx,
        FileDropEvent::Submit {
            position: position(),
        },
    );
}

fn assert_cancelled(view: &Entity<CallbackZones>, cx: &mut gpui::VisualTestContext) {
    cx.update(|_, cx| {
        assert!(
            !cx.has_active_drag(),
            "validation must not resurrect a cancelled drag"
        )
    });
    // Neither another native Submit nor an ordinary release may deliver it later.
    submit(cx);
    cx.simulate_mouse_up(position(), MouseButton::Left, Default::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.child_drops.is_empty(),
            "cancelled payload reached the target"
        );
        assert!(
            view.parent_drops.is_empty(),
            "cancelled payload reached an ancestor"
        );
    });
}

fn assert_recovery(
    view: &Entity<CallbackZones>,
    cx: &mut gpui::VisualTestContext,
    validation: &ValidationState,
    accepts: bool,
) {
    validation.cancel.set(false);
    enter(cx, position(), "replacement.png".into());
    submit(cx);
    view.read_with(cx, |view, _| {
        let expected = vec![vec![PathBuf::from("replacement.png")]];
        if accepts {
            assert_eq!(view.child_drops, expected);
            assert!(view.parent_drops.is_empty());
        } else {
            assert_eq!(view.parent_drops, expected);
            assert!(view.child_drops.is_empty());
        }
    });
    cx.update(|_, cx| assert!(!cx.has_active_drag()));
}

fn cancel_during_submit(cx: &mut gpui::TestAppContext, accepts: bool) {
    let (view, mut cx, validation) = callback_window(cx, accepts);
    // Arm after Enter has painted: this isolates mouse-up validation from
    // the can_drop invocation used to compute drag-over feedback.
    validation.cancel.set(true);
    submit(&mut cx);
    assert_cancelled(&view, &mut cx);
    assert_eq!(*validation.stop_results.borrow(), vec![true]);
    assert_recovery(&view, &mut cx, &validation, accepts);
}

#[gpui::test]
fn accepting_can_drop_can_cancel_before_target_delivery(cx: &mut gpui::TestAppContext) {
    cancel_during_submit(cx, true);
}

#[gpui::test]
fn rejecting_can_drop_cancellation_does_not_bubble_or_resurrect(cx: &mut gpui::TestAppContext) {
    cancel_during_submit(cx, false);
}

fn cancel_during_feedback(cx: &mut gpui::TestAppContext, accepts: bool) {
    let (view, mut cx, validation) = callback_window(cx, accepts);
    let feedback_before = validation.feedback.get();
    validation.cancel.set(true);
    draw(&mut cx);
    assert_cancelled(&view, &mut cx);
    assert_eq!(*validation.stop_results.borrow(), vec![true]);
    assert_eq!(
        validation.feedback.get(),
        feedback_before,
        "cancelled drag must not paint feedback"
    );
    assert_recovery(&view, &mut cx, &validation, accepts);
}

#[gpui::test]
fn accepting_can_drop_can_cancel_during_drag_over_validation(cx: &mut gpui::TestAppContext) {
    cancel_during_feedback(cx, true);
}

#[gpui::test]
fn rejecting_can_drop_can_cancel_during_drag_over_validation(cx: &mut gpui::TestAppContext) {
    cancel_during_feedback(cx, false);
}

#[gpui::test]
fn noncancelling_acceptance_delivers_once_to_child(cx: &mut gpui::TestAppContext) {
    let (view, mut cx, validation) = callback_window(cx, true);
    assert!(validation.feedback.get() > 0);
    submit(&mut cx);
    submit(&mut cx);
    view.read_with(&cx, |view, _| {
        assert_eq!(view.child_drops, vec![vec![PathBuf::from("cancelled.png")]]);
        assert!(view.parent_drops.is_empty());
    });
    assert!(validation.stop_results.borrow().is_empty());
    cx.update(|_, cx| assert!(!cx.has_active_drag()));
}

#[gpui::test]
fn noncancelling_rejection_keeps_parent_fallback(cx: &mut gpui::TestAppContext) {
    let (view, mut cx, validation) = callback_window(cx, false);
    assert_eq!(validation.feedback.get(), 0);
    submit(&mut cx);
    submit(&mut cx);
    view.read_with(&cx, |view, _| {
        assert_eq!(
            view.parent_drops,
            vec![vec![PathBuf::from("cancelled.png")]]
        );
        assert!(view.child_drops.is_empty());
    });
    assert!(validation.stop_results.borrow().is_empty());
    cx.update(|_, cx| assert!(!cx.has_active_drag()));
}

#[gpui::test]
fn a_foreign_windows_validation_cannot_cancel_the_owners_drag(cx: &mut gpui::TestAppContext) {
    let (owner, mut owner_cx, _) = callback_window(cx, true);
    let validation = ValidationState::default();
    let (foreign, foreign_cx) = cx.add_window_view(|_, _| CallbackZones {
        validation: validation.clone(),
        accepts: true,
        parent_drops: Vec::new(),
        child_drops: Vec::new(),
    });
    let mut foreign_cx = foreign_cx.clone();
    validation.cancel.set(true);
    draw(&mut foreign_cx);
    assert!(
        validation.stop_results.borrow().is_empty(),
        "foreign targets must not validate another window's native payload"
    );
    assert_eq!(validation.feedback.get(), 0);
    owner_cx.update(|_, cx| assert!(cx.has_active_drag()));
    submit(&mut owner_cx);
    owner.read_with(&owner_cx, |view, _| {
        assert_eq!(view.child_drops, vec![vec![PathBuf::from("cancelled.png")]]);
    });
    foreign.read_with(&foreign_cx, |view, _| {
        assert!(view.child_drops.is_empty());
        assert!(view.parent_drops.is_empty());
    });
}
