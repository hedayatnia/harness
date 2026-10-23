//! A runaway self-continued turn (#341): the agent resumes a parked session
//! on its own and then streams without ever ending its turn. Two bounds:
//!
//! - The chain's wall-clock cap holds while the resumed turn is still
//!   streaming, not only when it tries to resume again. A turn that never
//!   goes quiet never parks, so a cap checked only at resume never fired.
//!   Past the cap the run ends with a visible error.
//! - Streaming deltas are coalesced in the run journal, so one long turn is
//!   a handful of rows rather than one row per fragment.
//!
//! A separate file because the env knobs are process-global.

use std::sync::{Arc, Once};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use tokio::sync::{Mutex, mpsc};

use harness_adapters::{Harness, HarnessError, RunControls};
use harness_doc::{MessagePart, SessionMessageEntry};
use harness_engine::{EngineCore, HarnessRegistry};
use harness_proto::{
    AgentEvent, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel,
    SessionStatus, SteeringMode,
};

const CHAT: &str = "chat-runaway";
/// Normal window: far beyond the test horizon.
const QUIESCE_MS: u64 = 600_000;
const SELF_QUIESCE_MS: u64 = 300;
/// Wall-clock cap on a self-continued chain under test.
const SELF_CHAIN_MAX_MS: u64 = 1_500;

fn init_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: called before any engine (and thus any reader of the vars)
        // exists in this test process.
        unsafe {
            std::env::set_var("HARNESS_TURN_QUIESCE_MS", QUIESCE_MS.to_string());
            std::env::set_var("HARNESS_SELF_TURN_QUIESCE_MS", SELF_QUIESCE_MS.to_string());
            std::env::set_var("HARNESS_SELF_CHAIN_MAX_MS", SELF_CHAIN_MAX_MS.to_string());
        }
    });
}

fn run_request(prompt: &str) -> RunRequest {
    RunRequest {
        prompt: prompt.into(),
        harness: None,
        model: None,
        reasoning: None,
        model_options: Default::default(),
        cwd: "/tmp".into(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: true,
        attachments: Vec::new(),
        worktree: None,
        resume: None,
    }
}

fn done(status: DoneStatus) -> AgentEvent {
    AgentEvent::Done {
        status,
        result: None,
        error: None,
        session_id: Some("hs-runaway".into()),
    }
}

fn session_started() -> AgentEvent {
    AgentEvent::SessionStarted {
        harness: HarnessId::Mock,
        model: "mock-1".into(),
        tools: vec![],
        cwd: "/tmp".into(),
        session_id: "hs-runaway".into(),
        assistant_message_id: "a-runaway".into(),
    }
}

fn text(t: &str) -> AgentEvent {
    AgentEvent::TextDelta { text: t.into() }
}

fn reasoning(t: &str) -> AgentEvent {
    AgentEvent::ReasoningDelta { text: t.into() }
}

/// Feed-by-hand harness (see `self_continue_cap.rs`).
struct FeedHarness {
    main_prompt: String,
    feed: Mutex<Option<mpsc::UnboundedReceiver<AgentEvent>>>,
}

#[async_trait]
impl Harness for FeedHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Feed"
    }
    fn supports_steering(&self) -> bool {
        true
    }
    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::StepBoundary
    }
    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        &[ReasoningLevel::Medium]
    }
    async fn models(&self) -> Result<Vec<Model>, HarnessError> {
        Ok(vec![])
    }
    async fn run(
        &self,
        request: RunRequest,
        mut controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        if request.prompt != self.main_prompt {
            let events = vec![Ok(done(DoneStatus::Completed))];
            return Ok(futures::stream::iter(events).boxed());
        }
        let mut feed = self
            .feed
            .lock()
            .await
            .take()
            .expect("FeedHarness serves the main dispatch once per test");
        let (tx, rx) = mpsc::channel::<Result<AgentEvent, HarnessError>>(64);
        tokio::spawn(async move {
            let mut steering_open = true;
            loop {
                tokio::select! {
                    biased;
                    steer = controls.steering.recv(), if steering_open => match steer {
                        Some(_) => {
                            let boundary = AgentEvent::Steered {
                                assistant_message_id: None,
                                next_assistant_message_id: None,
                            };
                            if tx.send(Ok(boundary)).await.is_err() {
                                return;
                            }
                        }
                        None => steering_open = false,
                    },
                    event = feed.recv() => match event {
                        Some(event) => {
                            if tx.send(Ok(event)).await.is_err() {
                                return;
                            }
                        }
                        None => return,
                    },
                }
            }
        });
        Ok(futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|event| (event, rx))
        })
        .boxed())
    }
}

struct Rig {
    core: EngineCore,
    feed: mpsc::UnboundedSender<AgentEvent>,
    _dir: tempfile::TempDir,
}

fn assemble(main_prompt: &str) -> Rig {
    init_env();
    let (feed, rx) = mpsc::unbounded_channel();
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(FeedHarness {
        main_prompt: main_prompt.into(),
        feed: Mutex::new(Some(rx)),
    }));
    let dir = tempfile::tempdir().unwrap();
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Mock, None)
        .expect("engine core assembles");
    Rig {
        core,
        feed,
        _dir: dir,
    }
}

fn status(core: &EngineCore) -> Option<SessionStatus> {
    core.sessions.session_status(CHAT).map(|s| s.status)
}

fn journal(core: &EngineCore) -> Vec<AgentEvent> {
    core.sessions
        .subscribe(CHAT, 0)
        .unwrap()
        .0
        .into_iter()
        .map(|e| e.event)
        .collect()
}

/// Tolerant read (see e2e.rs): a snapshot mid-segment-write deserializes with
/// fields missing — treat that instant as "not yet".
fn entries(core: &EngineCore) -> Vec<SessionMessageEntry> {
    core.doc_host
        .open(CHAT)
        .ok()
        .and_then(|h| h.doc().read_entries().ok())
        .unwrap_or_default()
}

async fn wait_for<F>(mut predicate: F, what: &str)
where
    F: FnMut() -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn a_resumed_turn_that_never_goes_quiet_ends_visibly_at_the_chain_cap() {
    let rig = assemble("watch the builds");
    rig.core
        .sessions
        .dispatch(CHAT, HarnessId::Mock, run_request("watch the builds"), None)
        .await
        .expect("dispatch");
    rig.feed.send(session_started()).unwrap();
    rig.feed.send(text("Watching.")).unwrap();
    rig.feed.send(done(DoneStatus::Completed)).unwrap();
    wait_for(
        || status(&rig.core) == Some(SessionStatus::Idle),
        "park after Done",
    )
    .await;

    // The agent wakes itself up past the resume gate…
    tokio::time::sleep(Duration::from_millis(1200)).await;
    rig.feed.send(text("Checking again.")).unwrap();
    wait_for(
        || status(&rig.core) == Some(SessionStatus::Working),
        "self-continued output resumes the parked session",
    )
    .await;

    // …and then streams reasoning fragments forever, never quiet long enough
    // for the watchdog to park it and never sending a turn end.
    let started = tokio::time::Instant::now();
    let streamed = Duration::from_millis(SELF_CHAIN_MAX_MS * 3);
    let mut fragments = 0usize;
    while started.elapsed() < streamed {
        // Once the cap stops the agent its feed closes.
        if rig.feed.send(reasoning("thinking ")).is_err() {
            break;
        }
        fragments += 1;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    wait_for(
        || status(&rig.core) == Some(SessionStatus::Errored),
        "the chain cap ends the runaway turn",
    )
    .await;

    // Visible: the journal closes with an errored Done that says why, and the
    // transcript carries that error.
    let events = journal(&rig.core);
    let rows = events.len();
    match events.last() {
        Some(AgentEvent::Done {
            status: DoneStatus::Errored,
            error: Some(error),
            ..
        }) => assert!(
            error.contains("without ending its turn"),
            "unexpected stop reason: {error}"
        ),
        other => panic!("journal must close with an errored Done, got {other:?}"),
    }
    wait_for(
        || {
            entries(&rig.core).iter().any(|e| {
                e.parts.iter().any(|p| {
                    matches!(p, MessagePart::Error { message, .. }
                        if message.contains("without ending its turn"))
                })
            })
        },
        "the stop reason in the transcript",
    )
    .await;

    // Bounded: nothing the agent still sends lands in the journal, and the
    // fragments that did land were coalesced, not one row each.
    for _ in 0..50 {
        let _ = rig.feed.send(reasoning("still going "));
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        journal(&rig.core).len(),
        rows,
        "the journal stopped growing"
    );
    assert!(
        rows < fragments / 4,
        "{fragments} reasoning fragments took {rows} journal rows"
    );

    rig.core.sessions.shutdown().await;
}

#[tokio::test]
async fn streaming_deltas_are_coalesced_in_the_journal() {
    let rig = assemble("think hard");
    rig.core
        .sessions
        .dispatch(CHAT, HarnessId::Mock, run_request("think hard"), None)
        .await
        .expect("dispatch");
    rig.feed.send(session_started()).unwrap();
    let mut thought = String::new();
    for i in 0..400 {
        let fragment = format!("r{i} ");
        thought.push_str(&fragment);
        rig.feed.send(reasoning(&fragment)).unwrap();
    }
    let mut answer = String::new();
    for i in 0..200 {
        let fragment = format!("t{i} ");
        answer.push_str(&fragment);
        rig.feed.send(text(&fragment)).unwrap();
    }
    rig.feed.send(done(DoneStatus::Completed)).unwrap();
    wait_for(
        || status(&rig.core) == Some(SessionStatus::Idle),
        "park after Done",
    )
    .await;

    let events = journal(&rig.core);
    let reasoning_rows: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::ReasoningDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let text_rows: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        reasoning_rows.len() <= 4 && text_rows.len() <= 4,
        "600 fragments took {} reasoning and {} text rows",
        reasoning_rows.len(),
        text_rows.len()
    );
    // Nothing lost or reordered: the merged rows replay to the same text,
    // reasoning before the answer, and the Done last.
    assert_eq!(reasoning_rows.concat(), thought);
    assert_eq!(text_rows.concat(), answer);
    let first_text = events
        .iter()
        .position(|e| matches!(e, AgentEvent::TextDelta { .. }))
        .unwrap();
    let last_reasoning = events
        .iter()
        .rposition(|e| matches!(e, AgentEvent::ReasoningDelta { .. }))
        .unwrap();
    assert!(last_reasoning < first_text);
    assert!(matches!(events.last(), Some(AgentEvent::Done { .. })));

    rig.core.sessions.shutdown().await;
}
