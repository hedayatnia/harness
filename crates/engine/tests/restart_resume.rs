//! Restart round-trip + harness resume continuity (the "chats forget everything
//! after an app restart" regression): the EMBED assembly (`EngineCore::assemble`)
//! is run twice over one data dir, asserting
//! - chats + transcripts survive a graceful shutdown → relaunch;
//! - the next run in an existing chat carries the chat's stored harness-native
//!   session id as `RunRequest.resume` (engine-owned, harness sessions.ts:736);
//! - a kill -9 style crash recovers the session id from the run journal
//!   (harness recoverDraft, sessions.ts:538-552) and stamps streaming entries
//!   `aborted`;
//! - resume is cwd-scoped (harness session stores are keyed by cwd);
//! - a startup crash retries once with the resume kept, and a helper that is
//!   down hard never tombstones the stored session id;
//! - a steer with no live run after a restart dispatches as a new turn that
//!   still resumes the prior conversation.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;

use harness_doc::{
    MessagePart, MessageRole, MessageStatus, SessionCommandPayload, SessionDoc, SessionMessageEntry,
};
use harness_engine::{EngineCore, HarnessRegistry, RunJournal};
use harness_adapters::{Harness, HarnessError, RunControls};
use harness_proto::{
    AgentEvent, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel,
    SteeringMode,
};
use harness_sync::DocsStore;

const CHAT: &str = "chat-restart";

type RequestLog = Arc<Mutex<Vec<RunRequest>>>;

fn run_request(prompt: &str, cwd: &str) -> RunRequest {
    RunRequest {
        prompt: prompt.into(),
        harness: None,
        model: None,
        reasoning: None,
        model_options: Default::default(),
        cwd: cwd.into(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: true,
        attachments: Vec::new(),
        worktree: None,
        resume: None,
    }
}

/// Records every `RunRequest` it receives (the resume-injection probe). A
/// successful run emits `SessionStarted{session_id}` … `Done{session_id}`;
/// while `fail_starts` is positive, a run dies the way a crashed agent child
/// does — an errored Done before any session starts — decrementing the
/// counter (so a transient spawn blip is one failure, a helper that is down
/// hard is `u32::MAX`).
struct RecordingHarness {
    requests: RequestLog,
    session_id: String,
    fail_starts: Arc<Mutex<u32>>,
}

#[async_trait]
impl Harness for RecordingHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Recording"
    }
    fn supports_steering(&self) -> bool {
        false
    }
    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::TurnBoundary
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
        _controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        self.requests
            .lock()
            .expect("request log")
            .push(request.clone());
        let fail = {
            let mut left = self.fail_starts.lock().expect("fail counter");
            if *left > 0 {
                *left -= 1;
                true
            } else {
                false
            }
        };
        let events: Vec<Result<AgentEvent, HarnessError>> = if fail {
            vec![Ok(AgentEvent::Done {
                status: DoneStatus::Errored,
                result: None,
                error: Some("Recording exited unexpectedly (exit code 1): boom".into()),
                session_id: None,
            })]
        } else {
            vec![
                Ok(AgentEvent::SessionStarted {
                    harness: HarnessId::Mock,
                    model: "mock-1".into(),
                    tools: vec![],
                    cwd: request.cwd.clone(),
                    session_id: self.session_id.clone(),
                    assistant_message_id: "a-1".into(),
                }),
                Ok(AgentEvent::TextDelta {
                    text: format!("ack: {}", request.prompt),
                }),
                Ok(AgentEvent::Done {
                    status: DoneStatus::Completed,
                    result: None,
                    error: None,
                    session_id: Some(self.session_id.clone()),
                }),
            ]
        };
        Ok(futures::stream::iter(events).boxed())
    }
}

fn assemble(dir: &std::path::Path, harness: RecordingHarness) -> EngineCore {
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(harness));
    EngineCore::assemble(dir, Arc::new(registry), HarnessId::Mock, None)
        .expect("engine core assembles")
}

fn queue_run(core: &EngineCore, prompt: &str, cwd: &str, message_id: &str) {
    core.doc_host
        .queue_command(
            CHAT,
            SessionCommandPayload::Run {
                request: run_request(prompt, cwd),
                message_id: message_id.into(),
            },
        )
        .expect("queue run command");
}

async fn wait_for<F>(predicate: F, what: &str)
where
    F: FnMut() -> bool,
{
    wait_for_within(predicate, what, Duration::from_secs(10)).await;
}

async fn wait_for_within<F>(mut predicate: F, what: &str, deadline: Duration)
where
    F: FnMut() -> bool,
{
    let deadline = tokio::time::Instant::now() + deadline;
    while !predicate() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
}

/// Tolerant read for hot-polling predicates (mirrors e2e.rs `entries_now`).
fn entries_now(core: &EngineCore) -> Vec<SessionMessageEntry> {
    core.doc_host
        .open(CHAT)
        .ok()
        .and_then(|h| h.doc().read_entries().ok())
        .unwrap_or_default()
}

fn complete_assistant_count(core: &EngineCore) -> usize {
    entries_now(core)
        .iter()
        .filter(|e| e.role == MessageRole::Assistant && e.status == Some(MessageStatus::Complete))
        .count()
}

fn stored_harness_session(core: &EngineCore) -> Option<(String, Option<String>)> {
    let chat = core
        .workspace
        .chat(CHAT)
        .expect("read chat row")
        .expect("chat row exists");
    chat.harness_session_id
        .map(|id| (id, chat.harness_session_cwd))
}

/// Create + name the chat row up front so the auto-titler (which runs its own
/// harness request after a completed exchange on an UNTITLED chat) stays out
/// of the recorded request log.
fn pre_title(core: &EngineCore) {
    core.workspace
        .create_space("space-restart", &core.device_id, "/tmp", None, false)
        .expect("create space row");
    core.workspace
        .create_chat(CHAT, Some("space-restart"), None, None, None)
        .expect("create chat row");
    core.workspace
        .rename_chat(CHAT, "Pre-titled")
        .expect("rename chat");
}

/// One full turn in a fresh engine over `dir`, then graceful shutdown — the
/// "before restart" phase shared by the tests below.
async fn run_one_turn_and_shutdown(dir: &std::path::Path, requests: &RequestLog, session: &str) {
    let core = assemble(
        dir,
        RecordingHarness {
            requests: requests.clone(),
            session_id: session.into(),
            fail_starts: Default::default(),
        },
    );
    pre_title(&core);
    queue_run(
        &core,
        "remember the codeword PINEAPPLE",
        "/tmp",
        "msg-user-1",
    );
    wait_for(
        || complete_assistant_count(&core) == 1,
        "first turn to complete",
    )
    .await;
    core.shutdown().await;
    drop(core);
}

#[tokio::test]
async fn restart_roundtrip_restores_chats_transcript_and_resume() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    let requests: RequestLog = Arc::new(Mutex::new(Vec::new()));

    run_one_turn_and_shutdown(&dir, &requests, "hs-restart-1").await;
    assert_eq!(
        requests.lock().unwrap()[0].resume,
        None,
        "a chat's first run must start a fresh harness session"
    );

    // Relaunch over the same data dir (the embedded-engine restart path).
    let core = assemble(
        &dir,
        RecordingHarness {
            requests: requests.clone(),
            session_id: "hs-restart-2".into(),
            fail_starts: Default::default(),
        },
    );

    // Sidebar state survived: the chat row is back with its cwd, preview, and
    // the stored harness session (cwd-scoped).
    let chats = core.workspace.read_chats().expect("read chats");
    assert_eq!(chats.len(), 1, "chat row survives restart: {chats:#?}");
    assert_eq!(chats[0].id, CHAT);
    assert_eq!(chats[0].cwd.as_deref(), Some("/tmp"));
    assert!(chats[0].last_message_preview.is_some());
    assert_eq!(
        stored_harness_session(&core),
        Some(("hs-restart-1".into(), Some("/tmp".into())))
    );

    // Transcript survived: user + completed assistant entry, texts intact.
    let entries = entries_now(&core);
    assert_eq!(
        entries.len(),
        2,
        "transcript survives restart: {entries:#?}"
    );
    assert_eq!(entries[0].id, "msg-user-1");
    assert_eq!(entries[0].role, MessageRole::User);
    assert_eq!(entries[1].role, MessageRole::Assistant);
    assert_eq!(entries[1].status, Some(MessageStatus::Complete));
    assert!(matches!(
        &entries[1].parts[0],
        MessagePart::Text { text, .. } if text.contains("PINEAPPLE")
    ));

    // The next run resumes the SAME harness conversation: the engine injects
    // the stored session id even though the caller sent `resume: None`.
    queue_run(&core, "what was the codeword?", "/tmp", "msg-user-2");
    wait_for(
        || complete_assistant_count(&core) == 2,
        "second turn to complete",
    )
    .await;
    {
        let log = requests.lock().unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(
            log[1].resume.as_deref(),
            Some("hs-restart-1"),
            "post-restart dispatch must resume the stored harness session"
        );
    }
    // The fresh turn's session id replaces the stored one.
    assert_eq!(
        stored_harness_session(&core),
        Some(("hs-restart-2".into(), Some("/tmp".into())))
    );
    core.shutdown().await;
}

#[tokio::test]
async fn kill_crash_recovers_resume_from_journal_and_stamps_aborted() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    std::fs::create_dir_all(&dir).unwrap();
    // Pin the device id so the manufactured streaming entry counts as OURS.
    std::fs::write(dir.join("device-id"), "dev-crash").unwrap();

    // Manufacture the on-disk state a kill -9 mid-run leaves behind:
    // - a chat doc snapshot whose assistant entry is still `streaming`;
    // - a journal whose last event is NOT `Done` (run died mid-stream), holding
    //   the only copy of the harness session id (the debounced workspace-row
    //   write never landed).
    {
        let store = DocsStore::open(dir.join("orgs/dev-org/dev-user")).unwrap();
        let doc = SessionDoc::init(CHAT).unwrap();
        doc.push_message(&SessionMessageEntry {
            id: "msg-user-1".into(),
            role: MessageRole::User,
            parts: vec![MessagePart::Text {
                id: "t0".into(),
                text: "long task".into(),
            }],
            created_at: 1,
            device_id: "dev-crash".into(),
            status: Some(MessageStatus::Complete),
            continuation_of: None,
            duration_ms: None,
        })
        .unwrap();
        doc.push_message(&SessionMessageEntry {
            id: "msg-assistant-1".into(),
            role: MessageRole::Assistant,
            parts: vec![MessagePart::Text {
                id: "t0".into(),
                text: "partial…".into(),
            }],
            created_at: 2,
            device_id: "dev-crash".into(),
            status: Some(MessageStatus::Streaming),
            continuation_of: None,
            duration_ms: None,
        })
        .unwrap();
        store
            .save_snapshot(CHAT, &doc.export_snapshot().unwrap())
            .unwrap();

        let journal = RunJournal::open(dir.join("orgs/dev-org/dev-user/journals")).unwrap();
        journal
            .append(
                CHAT,
                &AgentEvent::SessionStarted {
                    harness: HarnessId::Mock,
                    model: "mock-1".into(),
                    tools: vec![],
                    cwd: "/tmp".into(),
                    session_id: "hs-crash".into(),
                    assistant_message_id: "msg-assistant-1".into(),
                },
            )
            .unwrap();
        journal
            .append(
                CHAT,
                &AgentEvent::TextDelta {
                    text: "partial…".into(),
                },
            )
            .unwrap();
    }

    let requests: RequestLog = Arc::new(Mutex::new(Vec::new()));
    let core = assemble(
        &dir,
        RecordingHarness {
            requests: requests.clone(),
            session_id: "hs-after-crash".into(),
            fail_starts: Default::default(),
        },
    );
    assert_eq!(core.device_id, "dev-crash");

    // Boot recovery stamped the abandoned streaming entry `aborted` …
    let entries = entries_now(&core);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].status, Some(MessageStatus::Aborted));
    // … and closed the stale journal with a synthetic Done.
    let journal = RunJournal::open(dir.join("orgs/dev-org/dev-user/journals")).unwrap();
    assert!(matches!(
        journal.last_event(CHAT).unwrap(),
        Some((_, AgentEvent::Done { .. }))
    ));

    // The next run resumes the crashed conversation: the session id was
    // recovered from the journal (its only surviving home).
    pre_title(&core);
    queue_run(&core, "keep going", "/tmp", "msg-user-2");
    wait_for(
        || complete_assistant_count(&core) == 1,
        "post-crash turn to complete",
    )
    .await;
    assert_eq!(
        requests.lock().unwrap()[0].resume.as_deref(),
        Some("hs-crash"),
        "journal-recovered session id must ride the next dispatch"
    );
    core.shutdown().await;
}

/// A harness whose stream stays OPEN after each turn's Done, serving follow-up
/// turns from the steering mailbox — the persistent-session shape (codex; and
/// claude's stream-json stdin). Counts `run()` calls to prove the engine
/// reuses one child across turns instead of respawning.
struct PersistentHarness {
    runs_started: Arc<Mutex<usize>>,
}

#[async_trait]
impl Harness for PersistentHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Persistent"
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
        controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        *self.runs_started.lock().unwrap() += 1;
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<AgentEvent, HarnessError>>(32);
        let mut steering = controls.steering;
        tokio::spawn(async move {
            let turn = |n: usize, prompt: &str| {
                vec![
                    AgentEvent::TextDelta {
                        text: format!("turn {n} ack: {prompt}"),
                    },
                    AgentEvent::Done {
                        status: DoneStatus::Completed,
                        result: None,
                        error: None,
                        session_id: Some("hs-persist".into()),
                    },
                ]
            };
            let first = vec![AgentEvent::SessionStarted {
                harness: HarnessId::Mock,
                model: "mock-1".into(),
                tools: vec![],
                cwd: "/tmp".into(),
                session_id: "hs-persist".into(),
                assistant_message_id: "a-1".into(),
            }];
            for ev in first.into_iter().chain(turn(1, "first")) {
                if tx.send(Ok(ev)).await.is_err() {
                    return;
                }
            }
            // Parked: serve follow-up turns from the mailbox until the
            // engine hangs up (idle reap / interrupt / shutdown).
            let mut n = 1usize;
            while let Some(steer) = steering.recv().await {
                n += 1;
                let boundary = AgentEvent::Steered {
                    assistant_message_id: None,
                    next_assistant_message_id: steer.message_id.map(|id| format!("a-{id}")),
                };
                if tx.send(Ok(boundary)).await.is_err() {
                    return;
                }
                for ev in turn(n, &steer.prompt) {
                    if tx.send(Ok(ev)).await.is_err() {
                        return;
                    }
                }
            }
        });
        Ok(futures::stream::unfold(
            rx,
            |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
        )
        .boxed())
    }
}

#[tokio::test]
async fn persistent_session_serves_multiple_turns_on_one_child() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    std::fs::create_dir_all(&dir).unwrap();

    let runs_started = Arc::new(Mutex::new(0usize));
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(PersistentHarness {
        runs_started: runs_started.clone(),
    }));
    let core = EngineCore::assemble(&dir, Arc::new(registry), HarnessId::Mock, None)
        .expect("engine core assembles");
    pre_title(&core);

    queue_run(&core, "first", "/tmp", "msg-user-1");
    wait_for(
        || complete_assistant_count(&core) == 1,
        "first turn to complete",
    )
    .await;

    // The session PARKS (harness runsBySession): the second message routes into
    // the live child instead of spawning a new one.
    queue_run(&core, "second", "/tmp", "msg-user-2");
    wait_for(
        || complete_assistant_count(&core) == 2,
        "second turn to complete on the same child",
    )
    .await;

    assert_eq!(
        *runs_started.lock().unwrap(),
        1,
        "one harness child must serve both turns"
    );
    let entries = entries_now(&core);
    assert_eq!(
        entries
            .iter()
            .filter(|e| e.role == MessageRole::User)
            .count(),
        2
    );
    core.shutdown().await;
}

#[tokio::test]
async fn fresh_crash_auto_resumes_and_notes_the_interruption() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("device-id"), "dev-crash").unwrap();

    // Same manufactured kill -9 state as above, but FRESH: the streaming entry
    // crashed moments ago, inside the 12h revival window.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    {
        let store = DocsStore::open(dir.join("orgs/dev-org/dev-user")).unwrap();
        let doc = SessionDoc::init(CHAT).unwrap();
        doc.push_message(&SessionMessageEntry {
            id: "msg-user-1".into(),
            role: MessageRole::User,
            parts: vec![MessagePart::Text {
                id: "t0".into(),
                text: "long task".into(),
            }],
            created_at: now - 60_000,
            device_id: "dev-crash".into(),
            status: Some(MessageStatus::Complete),
            continuation_of: None,
            duration_ms: None,
        })
        .unwrap();
        doc.push_message(&SessionMessageEntry {
            id: "msg-assistant-1".into(),
            role: MessageRole::Assistant,
            parts: vec![MessagePart::Text {
                id: "t0".into(),
                text: "partial…".into(),
            }],
            created_at: now - 30_000,
            device_id: "dev-crash".into(),
            status: Some(MessageStatus::Streaming),
            continuation_of: None,
            duration_ms: None,
        })
        .unwrap();
        store
            .save_snapshot(CHAT, &doc.export_snapshot().unwrap())
            .unwrap();

        let journal = RunJournal::open(dir.join("orgs/dev-org/dev-user/journals")).unwrap();
        journal
            .append(
                CHAT,
                &AgentEvent::SessionStarted {
                    harness: HarnessId::Mock,
                    model: "mock-1".into(),
                    tools: vec![],
                    cwd: "/tmp".into(),
                    session_id: "hs-crash".into(),
                    assistant_message_id: "msg-assistant-1".into(),
                },
            )
            .unwrap();
        journal
            .append(
                CHAT,
                &AgentEvent::TextDelta {
                    text: "partial…".into(),
                },
            )
            .unwrap();
    }

    let requests: RequestLog = Arc::new(Mutex::new(Vec::new()));
    let core = assemble(
        &dir,
        RecordingHarness {
            requests: requests.clone(),
            session_id: "hs-after-crash".into(),
            fail_starts: Default::default(),
        },
    );

    // The run is PICKED BACK UP without any user action (harness: "not just
    // eulogized"): recovery re-dispatches the crashed prompt itself.
    wait_for(
        || complete_assistant_count(&core) == 1,
        "auto-resumed turn to complete",
    )
    .await;

    let entries = entries_now(&core);
    // The aborted entry SAYS why it ended — and that the run is resuming.
    let aborted = entries
        .iter()
        .find(|e| e.status == Some(MessageStatus::Aborted))
        .expect("crashed entry stays, stamped aborted");
    assert!(
        aborted.parts.iter().any(|p| matches!(
            p,
            MessagePart::Error { message, .. }
                if message.contains("engine restart") && message.contains("resuming")
        )),
        "aborted entry carries the visible interruption note"
    );
    // Re-dispatch reuses the original user message id — never a duplicate.
    assert_eq!(
        entries
            .iter()
            .filter(|e| e.role == MessageRole::User)
            .count(),
        1
    );
    // The revived run continues the journal-recovered harness conversation.
    // (An auto-title request may precede it — titling fires at dispatch.)
    let recorded = requests.lock().unwrap().clone();
    let revived = recorded
        .iter()
        .find(|r| r.prompt == "long task")
        .expect("auto-resumed dispatch reached the harness");
    assert_eq!(
        revived.resume.as_deref(),
        Some("hs-crash"),
        "auto-resume must reattach the crashed harness session"
    );
    core.shutdown().await;
}

#[tokio::test]
async fn resume_is_cwd_scoped() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    let requests: RequestLog = Arc::new(Mutex::new(Vec::new()));

    run_one_turn_and_shutdown(&dir, &requests, "hs-cwd-1").await;

    let core = assemble(
        &dir,
        RecordingHarness {
            requests: requests.clone(),
            session_id: "hs-cwd-2".into(),
            fail_starts: Default::default(),
        },
    );
    // Same chat, different launch directory: claude session stores are keyed
    // by cwd, so the stored id must NOT be injected.
    queue_run(
        &core,
        "now from another project",
        "/elsewhere",
        "msg-user-2",
    );
    wait_for(
        || complete_assistant_count(&core) == 2,
        "cross-cwd turn to complete",
    )
    .await;
    assert_eq!(
        requests.lock().unwrap()[1].resume,
        None,
        "a session created under /tmp must not resume from /elsewhere"
    );
    core.shutdown().await;
}

#[tokio::test]
async fn startup_crash_retries_once_with_resume_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    let requests: RequestLog = Arc::new(Mutex::new(Vec::new()));

    run_one_turn_and_shutdown(&dir, &requests, "hs-live").await;

    // Relaunch with a harness whose child dies at startup ONCE (a transient
    // spawn blip). Since the ACP conversion a stale id falls back inside the
    // harness (`session/load` → `session/new`), so a startup death never
    // indicts the stored id: the retry must carry the SAME session id, not
    // start fresh — and never tombstone it.
    let core = assemble(
        &dir,
        RecordingHarness {
            requests: requests.clone(),
            session_id: "hs-next".into(),
            fail_starts: Arc::new(Mutex::new(1)),
        },
    );
    queue_run(&core, "second turn", "/tmp", "msg-user-2");
    wait_for(
        || complete_assistant_count(&core) == 2,
        "retried turn to complete",
    )
    .await;

    // The crashed attempt, then exactly one retry — resume kept both times.
    {
        let log = requests.lock().unwrap();
        assert_eq!(log.len(), 3, "one crashed attempt + one retry");
        assert_eq!(log[1].resume.as_deref(), Some("hs-live"));
        assert_eq!(
            log[2].resume.as_deref(),
            Some("hs-live"),
            "the retry must keep the stored conversation"
        );
        assert_eq!(log[2].prompt, "second turn");
    }
    // The retry reused the same user entry — no duplicates, no error turn.
    let entries = entries_now(&core);
    let users: Vec<_> = entries
        .iter()
        .filter(|e| e.role == MessageRole::User)
        .collect();
    assert_eq!(users.len(), 2, "retry must not duplicate the user entry");
    assert_eq!(entries.len(), 4, "user+assistant per turn: {entries:#?}");
    // The successful turn's session id replaces the stored one as usual.
    assert_eq!(
        stored_harness_session(&core),
        Some(("hs-next".into(), Some("/tmp".into())))
    );
    core.shutdown().await;
}

#[tokio::test]
async fn persistent_startup_crash_keeps_stored_session_id() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    let requests: RequestLog = Arc::new(Mutex::new(Vec::new()));

    run_one_turn_and_shutdown(&dir, &requests, "hs-live").await;

    // A helper that is down hard: every spawn dies at startup. The old guess
    // logic read this as "bad resume id" and tombstoned a perfectly good
    // session — permanently, surviving restarts (user incident 2026-08-13).
    let core = assemble(
        &dir,
        RecordingHarness {
            requests: requests.clone(),
            session_id: "hs-unused".into(),
            fail_starts: Arc::new(Mutex::new(u32::MAX)),
        },
    );
    queue_run(&core, "second turn", "/tmp", "msg-user-2");
    // Crashed attempt + its single retry — then it must STOP (no spawn loop).
    wait_for(
        || requests.lock().unwrap().len() == 3,
        "crashed attempt and retry to be recorded",
    )
    .await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    {
        let log = requests.lock().unwrap();
        assert_eq!(log.len(), 3, "exactly one retry, no revival loop");
        assert_eq!(log[1].resume.as_deref(), Some("hs-live"));
        assert_eq!(log[2].resume.as_deref(), Some("hs-live"));
    }
    // THE fix: the stored id survives the startup failures — the next send
    // (say, after the restart that heals the helper) resumes the same
    // conversation.
    assert_eq!(
        stored_harness_session(&core),
        Some(("hs-live".into(), Some("/tmp".into())))
    );
    core.shutdown().await;
}

/// Real-CLI proof of the whole regression fix: tell claude a codeword, restart
/// the engine (fresh `EngineCore::assemble` over the same data dir), ask for
/// the codeword back — the reply can only contain it if the second run resumed
/// the first run's harness session. Ignored by default: needs an installed,
/// authenticated `claude` CLI and spends real tokens (haiku, two tiny turns).
/// Run with: `cargo test -p harness-engine --test restart_resume -- --ignored`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires installed+authenticated claude CLI; spends tokens"]
async fn real_claude_remembers_codeword_across_engine_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    let cwd = tmp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().to_string();

    let real_request = |prompt: &str| RunRequest {
        prompt: prompt.into(),
        harness: None,
        model: Some("haiku".into()),
        reasoning: None,
        model_options: Default::default(),
        cwd: cwd.clone(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: false,
        attachments: Vec::new(),
        worktree: None,
        resume: None,
    };
    let assemble_real = || {
        EngineCore::assemble(
            &dir,
            Arc::new(harness_engine::default_registry()),
            HarnessId::ClaudeCode,
            None,
        )
        .expect("engine core assembles")
    };

    let core = assemble_real();
    pre_title(&core); // keep the auto-titler from spending a second model call
    core.doc_host
        .queue_command(
            CHAT,
            SessionCommandPayload::Run {
                request: real_request(
                    "Remember the codeword: PINEAPPLE. Reply with exactly: stored",
                ),
                message_id: "msg-user-1".into(),
            },
        )
        .expect("queue first real run");
    wait_for_within(
        || complete_assistant_count(&core) == 1,
        "first real claude turn",
        Duration::from_secs(120),
    )
    .await;
    assert!(
        stored_harness_session(&core).is_some(),
        "claude session id must be stored on the chat row"
    );
    core.shutdown().await;
    drop(core);

    // "App restart": a brand-new engine over the same data dir.
    let core = assemble_real();
    core.doc_host
        .queue_command(
            CHAT,
            SessionCommandPayload::Run {
                request: real_request(
                    "What was the codeword I told you earlier? Reply with just the codeword.",
                ),
                message_id: "msg-user-2".into(),
            },
        )
        .expect("queue second real run");
    wait_for_within(
        || complete_assistant_count(&core) == 2,
        "post-restart real claude turn",
        Duration::from_secs(120),
    )
    .await;

    let entries = entries_now(&core);
    let last_assistant_text: String = entries
        .iter()
        .rev()
        .find(|e| e.role == MessageRole::Assistant)
        .into_iter()
        .flat_map(|e| {
            e.parts.iter().filter_map(|p| match p {
                MessagePart::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
        })
        .collect();
    assert!(
        last_assistant_text.to_uppercase().contains("PINEAPPLE"),
        "post-restart reply must recall the codeword (got: {last_assistant_text:?})"
    );
    core.shutdown().await;
}

#[tokio::test]
async fn steer_after_restart_dispatches_new_turn_with_resume() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    let requests: RequestLog = Arc::new(Mutex::new(Vec::new()));

    run_one_turn_and_shutdown(&dir, &requests, "hs-steer").await;

    // Relaunch: no live run, no in-process `last_request`. A steer must fall
    // back to a new turn built from the chat's workspace row, resuming the
    // prior harness conversation.
    let core = assemble(
        &dir,
        RecordingHarness {
            requests: requests.clone(),
            session_id: "hs-steer-2".into(),
            fail_starts: Default::default(),
        },
    );
    core.doc_host
        .queue_command(
            CHAT,
            SessionCommandPayload::Steer {
                prompt: "actually, also add tests".into(),
                message_id: Some("msg-user-2".into()),
            },
        )
        .expect("queue steer command");
    wait_for(
        || complete_assistant_count(&core) == 2,
        "steer-as-new-turn to complete",
    )
    .await;

    {
        let log = requests.lock().unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[1].prompt, "actually, also add tests");
        assert_eq!(log[1].cwd, "/tmp", "run config rebuilt from the chat row");
        assert_eq!(
            log[1].resume.as_deref(),
            Some("hs-steer"),
            "steer-turned-run must resume the stored harness session"
        );
    }
    core.shutdown().await;
}

/// A persistent harness whose FIRST run plays `script` and then stays open
/// (as a live agent process would) until the run is interrupted. Later runs
/// complete at once, like `RecordingHarness`.
struct ScriptedHarness {
    requests: RequestLog,
    session_id: String,
    script: Mutex<Option<Vec<AgentEvent>>>,
}

#[async_trait]
impl Harness for ScriptedHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Scripted"
    }
    fn supports_steering(&self) -> bool {
        true
    }
    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::StepBoundary
    }
    fn deterministic_turn_end(&self) -> bool {
        true
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
        controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        self.requests
            .lock()
            .expect("request log")
            .push(request.clone());
        let started = AgentEvent::SessionStarted {
            harness: HarnessId::Mock,
            model: "mock-1".into(),
            tools: vec![],
            cwd: request.cwd.clone(),
            session_id: self.session_id.clone(),
            assistant_message_id: "a-1".into(),
        };
        let done = |status| AgentEvent::Done {
            status,
            result: None,
            error: None,
            session_id: None,
        };
        match self.script.lock().expect("script").take() {
            Some(script) => {
                let token = controls.interrupt.clone();
                let events = std::iter::once(started).chain(script).map(Ok);
                Ok(futures::stream::iter(events)
                    .chain(futures::stream::once(async move {
                        let _keep_steering_open = &controls;
                        token.cancelled().await;
                        Ok(done(DoneStatus::Interrupted))
                    }))
                    .boxed())
            }
            None => Ok(futures::stream::iter(vec![
                Ok(started),
                Ok(AgentEvent::TextDelta {
                    text: format!("ack: {}", request.prompt),
                }),
                Ok(done(DoneStatus::Completed)),
            ])
            .boxed()),
        }
    }
}

fn scripted(requests: &RequestLog, script: Vec<AgentEvent>) -> ScriptedHarness {
    ScriptedHarness {
        requests: requests.clone(),
        session_id: "hs-before-quit".into(),
        script: Mutex::new(Some(script)),
    }
}

fn assemble_with(dir: &std::path::Path, harness: impl Harness + 'static) -> EngineCore {
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(harness));
    EngineCore::assemble(dir, Arc::new(registry), HarnessId::Mock, None)
        .expect("engine core assembles")
}

fn session_status(core: &EngineCore) -> Option<harness_proto::SessionStatus> {
    core.sessions.session_status(CHAT).map(|s| s.status)
}

/// Quit mid-something, relaunch over the same data dir, and return what the
/// relaunched engine dispatched on its own.
async fn quit_and_relaunch(
    script: Vec<AgentEvent>,
    quit_when: harness_proto::SessionStatus,
) -> Vec<RunRequest> {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("device-id"), "dev-quit").unwrap();
    // The chat's project row (`pre_title`) is `/tmp`: the run happens there,
    // as a real chat runs in its project folder, so resume (keyed by cwd)
    // finds its session.
    let cwd = "/tmp";

    let first: RequestLog = Arc::new(Mutex::new(Vec::new()));
    let core = assemble_with(&dir, scripted(&first, script));
    pre_title(&core);
    queue_run(&core, "long task", cwd, "msg-user-1");
    wait_for(
        || session_status(&core) == Some(quit_when),
        "the state to quit in",
    )
    .await;
    core.shutdown().await;
    drop(core);

    let second: RequestLog = Arc::new(Mutex::new(Vec::new()));
    let core = assemble_with(
        &dir,
        ScriptedHarness {
            requests: second.clone(),
            session_id: "hs-after-quit".into(),
            script: Mutex::new(None),
        },
    );
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let dispatched = second
        .lock()
        .unwrap()
        .iter()
        .filter(|r| !r.prompt.is_empty())
        .cloned()
        .collect();
    core.shutdown().await;
    dispatched
}

/// #482: quitting (or updating) while a turn runs used to settle it as
/// interrupted for good. The next boot re-sends it on the same session.
#[tokio::test]
async fn a_quit_mid_turn_resumes_the_turn_on_the_next_boot() {
    let dispatched = quit_and_relaunch(
        vec![AgentEvent::TextDelta {
            text: "working on it…".into(),
        }],
        harness_proto::SessionStatus::Working,
    )
    .await;
    let resumed = dispatched
        .iter()
        .find(|r| r.prompt == "long task")
        .unwrap_or_else(|| panic!("the cut-short turn was not resumed: {dispatched:?}"));
    assert_eq!(resumed.resume.as_deref(), Some("hs-before-quit"));
}

/// #517: a parked session whose agent had background work running lost that
/// work with the quit, silently. The next boot tells the agent, on the same
/// session, so it can start it again.
#[tokio::test]
async fn a_quit_with_background_work_running_tells_the_agent_on_the_next_boot() {
    let dispatched = quit_and_relaunch(
        vec![
            AgentEvent::BackgroundTask {
                id: "watch-ci".into(),
                running: true,
            },
            AgentEvent::TextDelta {
                text: "Watching CI in the background.".into(),
            },
            AgentEvent::Done {
                status: DoneStatus::Completed,
                result: None,
                error: None,
                session_id: None,
            },
        ],
        harness_proto::SessionStatus::Idle,
    )
    .await;
    let note = dispatched
        .iter()
        .find(|r| r.prompt.contains("background work"))
        .unwrap_or_else(|| panic!("the agent was not told about its stopped work: {dispatched:?}"));
    assert!(note.prompt.contains("Harness restarted"), "{}", note.prompt);
    assert_eq!(note.resume.as_deref(), Some("hs-before-quit"));
}

/// Nothing running, nothing to resume: a quit while parked idle stays quiet.
#[tokio::test]
async fn a_quit_while_idle_with_nothing_running_resumes_nothing() {
    let dispatched = quit_and_relaunch(
        vec![
            AgentEvent::TextDelta {
                text: "All done.".into(),
            },
            AgentEvent::Done {
                status: DoneStatus::Completed,
                result: None,
                error: None,
                session_id: None,
            },
        ],
        harness_proto::SessionStatus::Idle,
    )
    .await;
    assert!(
        dispatched.is_empty(),
        "resumed with nothing cut short: {dispatched:?}"
    );
}

/// Reports each session's cwd with symlinks resolved, as Claude Code does: a
/// session started in `/tmp/x` on macOS reports `/private/tmp/x`.
struct ResolvesCwd<H>(H);

#[async_trait]
impl<H: Harness> Harness for ResolvesCwd<H> {
    fn id(&self) -> HarnessId {
        self.0.id()
    }
    fn display_name(&self) -> &str {
        self.0.display_name()
    }
    fn supports_steering(&self) -> bool {
        self.0.supports_steering()
    }
    fn steering_mode(&self) -> SteeringMode {
        self.0.steering_mode()
    }
    fn deterministic_turn_end(&self) -> bool {
        self.0.deterministic_turn_end()
    }
    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        self.0.reasoning_levels()
    }
    async fn models(&self) -> Result<Vec<Model>, HarnessError> {
        self.0.models().await
    }
    async fn run(
        &self,
        request: RunRequest,
        controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        let resolved = std::fs::canonicalize(&request.cwd)
            .expect("run cwd exists")
            .to_string_lossy()
            .to_string();
        let stream = self.0.run(request, controls).await?;
        Ok(stream
            .map(move |event| match event {
                Ok(AgentEvent::SessionStarted {
                    harness,
                    model,
                    tools,
                    session_id,
                    assistant_message_id,
                    ..
                }) => Ok(AgentEvent::SessionStarted {
                    harness,
                    model,
                    tools,
                    cwd: resolved.clone(),
                    session_id,
                    assistant_message_id,
                }),
                other => other,
            })
            .boxed())
    }
}

/// A chat whose folder is opened through a symlink: the agent records the
/// session under the folder's real path, which differs from the chat's path as
/// a string. Quitting mid-turn must still resume that session on the next
/// boot, not start a new one without the conversation.
#[cfg(unix)]
#[tokio::test]
async fn a_quit_mid_turn_resumes_the_session_of_a_chat_opened_through_a_symlink() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("device-id"), "dev-quit").unwrap();
    let real = tmp.path().join("real-project");
    let link = tmp.path().join("linked-project");
    std::fs::create_dir_all(&real).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let link = link.to_string_lossy().to_string();

    let first: RequestLog = Arc::new(Mutex::new(Vec::new()));
    let core = assemble_with(
        &dir,
        ResolvesCwd(scripted(
            &first,
            vec![AgentEvent::TextDelta {
                text: "working on it…".into(),
            }],
        )),
    );
    core.workspace
        .create_space("space-linked", &core.device_id, &link, None, false)
        .expect("create space row");
    core.workspace
        .create_chat(CHAT, Some("space-linked"), None, None, None)
        .expect("create chat row");
    core.workspace
        .rename_chat(CHAT, "Pre-titled")
        .expect("rename chat");
    queue_run(&core, "long task", &link, "msg-user-1");
    wait_for(
        || session_status(&core) == Some(harness_proto::SessionStatus::Working),
        "the turn to start",
    )
    .await;
    core.shutdown().await;
    drop(core);

    let second: RequestLog = Arc::new(Mutex::new(Vec::new()));
    let core = assemble_with(
        &dir,
        ResolvesCwd(ScriptedHarness {
            requests: second.clone(),
            session_id: "hs-after-quit".into(),
            script: Mutex::new(None),
        }),
    );
    wait_for(
        || {
            second
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.prompt == "long task")
        },
        "the cut-short turn to be resumed",
    )
    .await;
    let resumed = second
        .lock()
        .unwrap()
        .iter()
        .find(|r| r.prompt == "long task")
        .cloned()
        .unwrap();
    assert_eq!(resumed.cwd, link);
    assert_eq!(
        resumed.resume.as_deref(),
        Some("hs-before-quit"),
        "the resumed turn must continue the session it was cut short in"
    );
    core.shutdown().await;
}
