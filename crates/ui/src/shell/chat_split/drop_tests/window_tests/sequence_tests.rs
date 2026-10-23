use super::*;

#[derive(Clone, Copy, Debug)]
enum Payload {
    Real,
    Empty,
    Blank,
    Mixed,
}

#[derive(Clone, Copy, Debug)]
enum Action {
    Enter(Payload),
    Pending,
    Submit,
    Exit,
    Move(Option<MouseButton>),
    Release(MouseButton),
    Stop,
}

const ACTIONS: [Action; 13] = [
    Action::Enter(Payload::Real),
    Action::Enter(Payload::Empty),
    Action::Enter(Payload::Blank),
    Action::Enter(Payload::Mixed),
    Action::Pending,
    Action::Submit,
    Action::Exit,
    Action::Move(None),
    Action::Move(Some(MouseButton::Left)),
    Action::Release(MouseButton::Left),
    Action::Release(MouseButton::Right),
    Action::Release(MouseButton::Middle),
    Action::Stop,
];

#[derive(Clone, Copy, Debug)]
struct Event {
    window: usize,
    action: Action,
}

fn events() -> Vec<Event> {
    (0..2)
        .flat_map(|window| ACTIONS.map(|action| Event { window, action }))
        .collect()
}

/// The contract, not GPUI's private ownership bookkeeping: latest Enter wins,
/// only its window can finish it, and stopping from either window cancels it.
/// FileWindow deliberately accepts even empty ExternalPaths; rejecting empty
/// batches at the application layer is tested by payload_tests instead.
#[derive(Default)]
struct Oracle {
    owned: Option<(usize, Vec<PathBuf>)>,
    deliveries: [Vec<Vec<PathBuf>>; 2],
    releases: [usize; 2],
    moves: [usize; 2],
}

impl Oracle {
    fn apply(&mut self, event: Event, payload: Vec<PathBuf>) {
        let window = event.window;
        let owner = self.owned.as_ref().map(|(owner, _)| *owner);
        match event.action {
            Action::Enter(_) => {
                self.owned = Some((window, payload));
                self.moves[window] += 1;
            }
            Action::Pending | Action::Move(_) if owner == Some(window) => {
                self.moves[window] += 1;
            }
            Action::Submit if owner == Some(window) => self.release(window, true),
            Action::Release(button) if owner.is_none() || owner == Some(window) => {
                self.release(window, button == MouseButton::Left);
            }
            Action::Exit if owner == Some(window) => self.owned = None,
            Action::Stop => self.owned = None,
            _ => {}
        }
    }

    fn release(&mut self, window: usize, primary: bool) {
        self.releases[window] += 1;
        if primary && let Some((owner, payload)) = self.owned.take() {
            assert_eq!(owner, window);
            self.deliveries[window].push(payload);
        }
    }
}

struct Sweep<'a> {
    views: [Entity<FileWindow>; 2],
    windows: [&'a mut gpui::VisualTestContext; 2],
    oracle: Oracle,
    case: usize,
    trace: Vec<Event>,
    steps: usize,
}

impl<'a> Sweep<'a> {
    fn new(
        first: Entity<FileWindow>,
        first_cx: &'a mut gpui::VisualTestContext,
        second: Entity<FileWindow>,
        second_cx: &'a mut gpui::VisualTestContext,
    ) -> Self {
        Self {
            views: [first, second],
            windows: [first_cx, second_cx],
            oracle: Oracle::default(),
            case: 0,
            trace: Vec::new(),
            steps: 0,
        }
    }

    /// Keep the same two real GPUI windows throughout a sweep, but never carry
    /// an active payload or observations across independent sequences.
    fn reset(&mut self) {
        self.windows[0].update(|window, cx| {
            cx.stop_active_drag(window);
            assert!(!cx.has_active_drag());
        });
        for window in 0..2 {
            self.views[window].update(self.windows[window], |view, _| {
                view.drops.clear();
                view.moves = 0;
                view.releases = 0;
            });
            draw(self.windows[window]);
        }
        self.oracle = Oracle::default();
        self.trace.clear();
        self.case += 1;
        self.check();
    }

    fn step(&mut self, event: Event) {
        self.trace.push(event);
        self.steps += 1;
        // Every real Enter is distinguishable, including same-window reentry:
        // stale delivery cannot accidentally compare equal to the latest batch.
        let real = PathBuf::from(format!("case-{}-step-{}.png", self.case, self.trace.len()));
        let other = real.with_extension("jpg");
        let (input, expected) = match event.action {
            Action::Enter(Payload::Real) => (vec![real.clone()], vec![real]),
            Action::Enter(Payload::Empty) => (vec![], vec![]),
            Action::Enter(Payload::Blank) => (vec![PathBuf::new(); 3], vec![]),
            Action::Enter(Payload::Mixed) => (
                vec![PathBuf::new(), real.clone(), PathBuf::new(), other.clone()],
                vec![real, other],
            ),
            _ => (vec![], vec![]),
        };
        let was_active = self.oracle.owned.is_some();
        self.oracle.apply(event, expected);
        let cx = &mut *self.windows[event.window];
        match event.action {
            Action::Enter(_) => enter_many(cx, position(), input),
            Action::Pending => file_event(
                cx,
                FileDropEvent::Pending {
                    position: position(),
                },
            ),
            Action::Submit => file_event(
                cx,
                FileDropEvent::Submit {
                    position: position(),
                },
            ),
            Action::Exit => file_event(cx, FileDropEvent::Exited),
            Action::Move(button) => {
                cx.simulate_mouse_move(position(), button, Default::default());
                draw(cx);
            }
            Action::Release(button) => {
                cx.simulate_mouse_up(position(), button, Default::default());
                draw(cx);
            }
            Action::Stop => {
                let stopped = cx.update(|window, cx| cx.stop_active_drag(window));
                assert_eq!(stopped, was_active, "case {}: {:?}", self.case, self.trace);
                draw(cx);
            }
        }
        self.check();
    }

    fn check(&mut self) {
        for window in 0..2 {
            self.views[window].read_with(self.windows[window], |view, _| {
                assert_eq!(
                    view.drops, self.oracle.deliveries[window],
                    "deliveries in window {window}, case {}: {:?}",
                    self.case, self.trace
                );
                assert_eq!(
                    view.releases, self.oracle.releases[window],
                    "releases in window {window}, case {}: {:?}",
                    self.case, self.trace
                );
                assert_eq!(
                    view.moves, self.oracle.moves[window],
                    "moves in window {window}, case {}: {:?}",
                    self.case, self.trace
                );
            });
            self.windows[window].update(|_, cx| {
                assert_eq!(
                    cx.has_active_drag(),
                    self.oracle.owned.is_some(),
                    "active drag from window {window}, case {}: {:?}",
                    self.case,
                    self.trace
                );
            });
        }
    }

    /// Observing only has_active_drag cannot reveal a stale or wrong-owner
    /// payload. Attempt delivery in both windows after every sequence, then
    /// repeat it in both: exactly the current owner's batch may arrive once.
    fn probe_delivery(&mut self) {
        for window in [0, 1, 0, 1] {
            self.step(Event {
                window,
                action: Action::Submit,
            });
        }
        assert!(self.oracle.owned.is_none());
    }
}

#[gpui::test]
fn exhaustive_native_drag_event_pairs_from_idle_and_both_owners(cx: &mut gpui::TestAppContext) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        let mut sweep = Sweep::new(first, first_cx, second, second_cx);
        let events = events();
        // Idle plus every payload shape owned by either window. Starting with
        // mixed/empty payloads also exercises replacement and cancellation of
        // those shapes, not just their delivery as the final event in a pair.
        let starts = std::iter::once(None).chain(
            events
                .iter()
                .copied()
                .filter(|event| matches!(event.action, Action::Enter(_)))
                .map(Some),
        );
        for start in starts {
            for &first in &events {
                for &second in &events {
                    sweep.reset();
                    if let Some(start) = start {
                        sweep.step(start);
                    }
                    sweep.step(first);
                    sweep.step(second);
                    sweep.probe_delivery();
                }
            }
        }
        assert_eq!(sweep.case, 6_084);
        assert_eq!(sweep.steps, 41_912);
    });
}

#[gpui::test]
fn seeded_native_drag_lifecycle_sequences_keep_only_the_latest_owner(
    cx: &mut gpui::TestAppContext,
) {
    with_windows(cx, |first, first_cx, second, second_cx| {
        let mut sweep = Sweep::new(first, first_cx, second, second_cx);
        let events = events();
        // Fixed local LCG: no dependencies, executor randomness, clock, or
        // process seed can alter the generated trace printed on failure.
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        for _ in 0..512 {
            sweep.reset();
            for _ in 0..64 {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                let index = ((state >> 32) % events.len() as u64) as usize;
                sweep.step(events[index]);
            }
            sweep.probe_delivery();
        }
        assert_eq!(sweep.case, 512);
        assert_eq!(sweep.steps, 34_816);
    });
}
