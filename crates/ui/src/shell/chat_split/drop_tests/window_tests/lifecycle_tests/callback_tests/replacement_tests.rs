use super::*;

#[derive(Clone, Copy, PartialEq)]
enum During {
    Validation,
    Feedback,
}

#[derive(Clone, Default)]
struct MutationState {
    armed: Rc<Cell<Option<During>>>,
    replace: Rc<Cell<bool>>,
    stops: Rc<RefCell<Vec<bool>>>,
    later_feedback: Rc<Cell<usize>>,
    paints: Rc<Cell<usize>>,
}

impl MutationState {
    fn mutate(&self, during: During, window: &mut Window, cx: &mut gpui::App) -> bool {
        if self.armed.get() != Some(during) {
            return false;
        }
        // Disarm before dispatch: replacement enters through the same public
        // native event API as a real platform handoff, not private drag fields.
        self.armed.set(None);
        self.stops.borrow_mut().push(cx.stop_active_drag(window));
        if self.replace.get() {
            window.dispatch_event(
                PlatformInput::FileDrop(FileDropEvent::Entered {
                    position: position(),
                    paths: ExternalPaths(vec![PathBuf::from("replacement.png")].into()),
                }),
                cx,
            );
        }
        true
    }
}

struct MutationZones {
    state: MutationState,
    accepts: bool,
    child_files: Vec<Vec<PathBuf>>,
    parent_files: Vec<Vec<PathBuf>>,
    internal_drops: usize,
}

impl Render for MutationZones {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let validation = self.state.clone();
        let file_feedback = self.state.clone();
        let internal_feedback = self.state.clone();
        let later_files = self.state.later_feedback.clone();
        let later_internal = self.state.later_feedback.clone();
        let paints = self.state.paints.clone();
        let accepts = self.accepts;
        div()
            .id("mutation-parent")
            .size_full()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, _| {
                this.parent_files.push(paths.paths().to_vec());
            }))
            .on_drop(cx.listener(|this, _: &(), _, _| this.internal_drops += 1))
            .child(
                div()
                    .id("mutation-child")
                    .size_full()
                    .can_drop(move |_, window, cx| {
                        validation.mutate(During::Validation, window, cx);
                        accepts
                    })
                    .drag_over::<ExternalPaths>(move |style, _, window, cx| {
                        if file_feedback.mutate(During::Feedback, window, cx) {
                            style.invisible()
                        } else {
                            style
                        }
                    })
                    .drag_over::<()>(move |style, _, window, cx| {
                        if internal_feedback.mutate(During::Feedback, window, cx) {
                            style.invisible()
                        } else {
                            style
                        }
                    })
                    .drag_over::<ExternalPaths>(move |style, _, _, _| {
                        later_files.set(later_files.get() + 1);
                        style
                    })
                    .drag_over::<()>(move |style, _, _, _| {
                        later_internal.set(later_internal.get() + 1);
                        style
                    })
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, _| {
                        this.child_files.push(paths.paths().to_vec());
                    }))
                    .on_drop(cx.listener(|this, _: &(), _, _| this.internal_drops += 1))
                    .child(
                        div()
                            .id("mutation-source")
                            .size(px(60.0))
                            .on_drag((), |_, _, _, cx| cx.new(|_| DragPreview)),
                    )
                    .child(gpui::canvas(
                        |_, _, _| {},
                        move |_, _, _, _| paints.set(paints.get() + 1),
                    )),
            )
    }
}

fn mutation_window(
    cx: &mut gpui::TestAppContext,
    accepts: bool,
    internal: bool,
) -> (
    Entity<MutationZones>,
    gpui::VisualTestContext,
    MutationState,
) {
    let state = MutationState::default();
    let (view, cx) = cx.add_window_view(|_, _| MutationZones {
        state: state.clone(),
        accepts,
        child_files: Vec::new(),
        parent_files: Vec::new(),
        internal_drops: 0,
    });
    let mut cx = cx.clone();
    draw(&mut cx);
    if internal {
        start_internal(&mut cx);
        cx.simulate_mouse_move(position(), Some(MouseButton::Left), Default::default());
        draw(&mut cx);
    } else {
        enter(&mut cx, position(), "original.png".into());
    }
    (view, cx, state)
}

fn assert_no_mutation_drop(view: &Entity<MutationZones>, cx: &gpui::VisualTestContext) {
    view.read_with(cx, |view, _| {
        assert!(
            view.child_files.is_empty(),
            "old release delivered to child"
        );
        assert!(
            view.parent_files.is_empty(),
            "old release delivered replacement to ancestor"
        );
        assert_eq!(view.internal_drops, 0);
    });
}

fn assert_fresh_drag(
    view: &Entity<MutationZones>,
    cx: &mut gpui::VisualTestContext,
    accepts: bool,
) {
    cx.update(|window, cx| {
        cx.stop_active_drag(window);
    });
    enter(cx, position(), "fresh.png".into());
    submit(cx);
    submit(cx);
    view.read_with(cx, |view, _| {
        let expected = vec![vec![PathBuf::from("fresh.png")]];
        if accepts {
            assert_eq!(view.child_files, expected);
            assert!(view.parent_files.is_empty());
        } else {
            assert_eq!(view.parent_files, expected);
            assert!(view.child_files.is_empty());
        }
        assert_eq!(view.internal_drops, 0);
    });
    cx.update(|_, cx| assert!(!cx.has_active_drag()));
}

fn replace_during_release(cx: &mut gpui::TestAppContext, accepts: bool, internal: bool) {
    let (view, mut cx, state) = mutation_window(cx, accepts, internal);
    state.replace.set(true);
    state.armed.set(Some(During::Validation));
    if internal {
        cx.simulate_mouse_up(position(), MouseButton::Left, Default::default());
        draw(&mut cx);
    } else {
        submit(&mut cx);
    }
    assert_eq!(*state.stops.borrow(), vec![true]);
    assert_no_mutation_drop(&view, &cx);
    assert_fresh_drag(&view, &mut cx, accepts);
}

#[gpui::test]
fn accepting_validation_replacement_cannot_bubble_into_ancestor(cx: &mut gpui::TestAppContext) {
    replace_during_release(cx, true, false);
}

#[gpui::test]
fn rejecting_validation_replacement_cannot_bubble_into_ancestor(cx: &mut gpui::TestAppContext) {
    replace_during_release(cx, false, false);
}

#[gpui::test]
fn accepting_validation_replacement_with_different_type_cannot_use_old_release(
    cx: &mut gpui::TestAppContext,
) {
    replace_during_release(cx, true, true);
}

#[gpui::test]
fn rejecting_validation_replacement_with_different_type_cannot_use_old_release(
    cx: &mut gpui::TestAppContext,
) {
    replace_during_release(cx, false, true);
}

fn mutate_during_feedback(cx: &mut gpui::TestAppContext, replace: bool, internal: bool) {
    let (view, mut cx, state) = mutation_window(cx, true, internal);
    let paints = state.paints.get();
    let later_feedback = state.later_feedback.get();
    state.replace.set(replace);
    state.armed.set(Some(During::Feedback));
    // Draw exactly one frame, so a subsequent clean repaint cannot conceal
    // stale invisible feedback returned by the callback that cancelled the drag.
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear();
    });
    assert_eq!(*state.stops.borrow(), vec![true]);
    assert_eq!(
        state.paints.get(),
        paints + 1,
        "feedback returned for a cancelled/replaced payload hid the child"
    );
    assert_eq!(
        state.later_feedback.get(),
        later_feedback,
        "later drag_over callbacks ran with the obsolete payload"
    );
    assert_no_mutation_drop(&view, &cx);
    if replace {
        // A fresh release of the replacement is valid, unlike the old release
        // that was already in flight in the validation tests above.
        draw(&mut cx);
        submit(&mut cx);
        view.read_with(&cx, |view, _| {
            assert_eq!(
                view.child_files,
                vec![vec![PathBuf::from("replacement.png")]]
            );
            assert!(view.parent_files.is_empty());
            assert_eq!(view.internal_drops, 0);
        });
        view.update(&mut cx, |view, _| view.child_files.clear());
    } else {
        cx.update(|_, cx| assert!(!cx.has_active_drag()));
        submit(&mut cx);
        cx.simulate_mouse_up(position(), MouseButton::Left, Default::default());
        assert_no_mutation_drop(&view, &cx);
    }
    assert_fresh_drag(&view, &mut cx, true);
}

#[gpui::test]
fn drag_over_cancellation_discards_its_style_and_skips_later_callbacks(
    cx: &mut gpui::TestAppContext,
) {
    mutate_during_feedback(cx, false, false);
}

#[gpui::test]
fn drag_over_same_type_replacement_discards_obsolete_feedback(cx: &mut gpui::TestAppContext) {
    mutate_during_feedback(cx, true, false);
}

#[gpui::test]
fn drag_over_different_type_replacement_discards_obsolete_feedback(cx: &mut gpui::TestAppContext) {
    mutate_during_feedback(cx, true, true);
}

#[gpui::test]
fn a_validation_replacement_survives_until_its_own_submit(cx: &mut gpui::TestAppContext) {
    for accepts in [true, false] {
        for internal in [true, false] {
            let (view, mut cx, state) = mutation_window(cx, accepts, internal);
            state.replace.set(true);
            state.armed.set(Some(During::Validation));
            if internal {
                cx.simulate_mouse_up(position(), MouseButton::Left, Default::default());
                draw(&mut cx);
            } else {
                submit(&mut cx);
            }
            assert_no_mutation_drop(&view, &cx);
            cx.update(|_, cx| {
                assert!(
                    cx.has_active_drag(),
                    "old-release cleanup erased the replacement"
                );
            });
            submit(&mut cx);
            view.read_with(&cx, |view, _| {
                let expected = vec![vec![PathBuf::from("replacement.png")]];
                if accepts {
                    assert_eq!(view.child_files, expected);
                    assert!(view.parent_files.is_empty());
                } else {
                    assert_eq!(view.parent_files, expected);
                    assert!(view.child_files.is_empty());
                }
                assert_eq!(view.internal_drops, 0);
            });
            cx.update(|_, cx| assert!(!cx.has_active_drag()));
        }
    }
}
