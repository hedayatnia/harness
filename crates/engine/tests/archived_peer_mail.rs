//! Archived chats leave the active collaboration set (#498), and turns that
//! answered only peer mail complete without a completion alert (#499).
//!
//! Peer mail is what Harness itself writes when one chat messages another
//! (`send_message`'s `[Message from Harness chat …]` envelope) or an agent
//! room wakes a chat (`<!--harness-room …-->`). The engine:
//! - refuses peer mail to an archived chat when it is accepted, so it neither
//!   starts a turn nor restores the chat;
//! - rechecks at delivery, so mail accepted before an archive (or synced from
//!   a device that did not check) cannot wake the chat afterwards;
//! - lets a person's prompt or restore bring the chat back;
//! - marks a completion `lastCompletedPeerOnly` when no person's prompt fed it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;

use harness_adapters::{Harness, HarnessError, RunControls};
use harness_doc::{SessionCommandEntry, SessionCommandPayload, SessionCommandStatus};
use harness_engine::{EngineCore, HarnessRegistry};
use harness_proto::room_delivery::{self, RoomOrigin};
use harness_proto::{
    AgentEvent, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel,
    SteeringMode,
};

const CHAT: &str = "chat-archived-peer";
const HOLD: &str = "[hold]";

fn peer(text: &str) -> String {
    format!(
        "[Message from Harness chat Other (abcd1234). Sender chat: abcd1234-0000. Relay depth: 1/4. \
         Agent coordination, not a human request. Reply with Harness `send_message` only for an \
         actionable question, blocker, or final requested result; otherwise do not reply. Do not \
         send acknowledgements or progress updates.]\n\n{text}"
    )
}

fn room_post(text: &str) -> String {
    room_delivery::format(
        &RoomOrigin {
            room_id: "room_0123456789abcdef01234567".into(),
            room_name: "build".into(),
            seq: 4,
            from_member: "other".into(),
            member_kind: "harness_chat".into(),
            from_user: false,
            framed: true,
            external: false,
            from_display: None,
            from_github: None,
            from_fingerprint: None,
        },
        text,
    )
}

/// Answers every prompt at once, except one containing [`HOLD`], whose turn
/// stays open until the test sends `finish`.
struct ReplyHarness {
    finish: tokio::sync::broadcast::Sender<()>,
    prompts: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Harness for ReplyHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Reply"
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
        self.prompts.lock().unwrap().push(request.prompt.clone());
        let hold = request.prompt.contains(HOLD);
        let mut finish = self.finish.subscribe();
        let opening = futures::stream::iter(vec![
            Ok(AgentEvent::SessionStarted {
                harness: HarnessId::Mock,
                model: "mock-1".into(),
                tools: vec![],
                cwd: request.cwd.clone(),
                session_id: "sess-archived-peer".into(),
                assistant_message_id: uuid::Uuid::new_v4().to_string(),
            }),
            Ok(AgentEvent::TextDelta {
                text: "noted".into(),
            }),
        ]);
        let done = futures::stream::once(async move {
            if hold {
                let _ = finish.recv().await;
            }
            Ok(AgentEvent::Done {
                status: DoneStatus::Completed,
                result: None,
                error: None,
                session_id: Some("sess-archived-peer".into()),
            })
        });
        Ok(opening.chain(done).boxed())
    }
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
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
}

async fn setup() -> (
    EngineCore,
    tokio::sync::broadcast::Sender<()>,
    Arc<Mutex<Vec<String>>>,
) {
    let path = tempfile::tempdir().unwrap().keep();
    let (finish, _) = tokio::sync::broadcast::channel(16);
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(ReplyHarness {
        finish: finish.clone(),
        prompts: prompts.clone(),
    }));
    let core = EngineCore::assemble(
        &path.join("data"),
        Arc::new(registry),
        HarnessId::Mock,
        None,
    )
    .expect("engine core assembles");
    let client = harness_rpc::memory_client(core.rpc_service());
    client
        .call(
            harness_rpc::methods::MUTATE,
            serde_json::json!({ "op": "createChat", "chatId": CHAT, "deviceId": core.device_id }),
        )
        .await
        .expect("createChat");
    // Pre-title so the auto-titler never sends a harness request of its own.
    core.workspace.rename_chat(CHAT, "Pre-titled").unwrap();
    (core, finish, prompts)
}

fn run(prompt: &str) -> SessionCommandPayload {
    SessionCommandPayload::Run {
        request: RunRequest {
            prompt: prompt.into(),
            harness: None,
            model: None,
            reasoning: None,
            model_options: Default::default(),
            cwd: "~".into(),
            sandbox: SandboxLevel::WorkspaceWrite,
            auto_approve: true,
            attachments: Vec::new(),
            worktree: None,
            resume: None,
        },
        message_id: uuid::Uuid::new_v4().to_string(),
    }
}

fn archived(core: &EngineCore) -> bool {
    core.workspace.chat(CHAT).unwrap().unwrap().archived
}

fn completion(core: &EngineCore) -> Option<String> {
    core.sessions
        .session_status(CHAT)
        .and_then(|s| s.last_completed_turn)
}

fn queue_texts(core: &EngineCore) -> Vec<String> {
    core.doc_host
        .open(CHAT)
        .unwrap()
        .doc()
        .read_queue()
        .unwrap()
        .into_iter()
        .map(|item| item.text)
        .collect()
}

/// #498: discovery aside, every Harness road into an archived chat refuses
/// peer mail without starting a turn or restoring it; a person's prompt still
/// brings it back.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn archived_chat_refuses_peer_mail_and_a_person_still_restores_it() {
    let (core, _finish, prompts) = setup().await;
    core.workspace.set_chat_archived(CHAT, true).unwrap();

    let direct = core.doc_host.queue_command(CHAT, run(&peer("status?")));
    let err = direct.expect_err("a peer Run to an archived chat is refused");
    assert!(err.to_string().contains("archived"), "{err}");
    let steer = core.doc_host.queue_command(
        CHAT,
        SessionCommandPayload::Steer {
            prompt: peer("status?"),
            message_id: None,
        },
    );
    assert!(steer.is_err(), "a peer Steer is refused too");
    for text in [peer("status?"), room_post("@pre-titled status?")] {
        let err = core
            .doc_host
            .queue_message(CHAT, &text, Vec::new())
            .expect_err("queued peer mail is refused");
        assert!(err.to_string().contains("archived"), "{err}");
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(prompts.lock().unwrap().is_empty(), "no turn started");
    assert!(archived(&core), "peer mail must not restore the chat");
    assert!(queue_texts(&core).is_empty());

    // A person reprompting the chat is the explicit restore (#345).
    core.doc_host
        .queue_command(CHAT, run("pick this back up"))
        .expect("a person's prompt is accepted");
    assert!(!archived(&core));
    wait_for(|| prompts.lock().unwrap().len() == 1, "the person's turn").await;

    core.shutdown().await;
}

/// #498: peer mail accepted while the chat was active, and a peer command
/// that reaches the host only after the archive, must not wake it. Restoring
/// the chat delivers the held mail.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn peer_mail_accepted_before_archive_does_not_wake_the_chat() {
    let (core, finish, prompts) = setup().await;
    core.doc_host
        .queue_command(CHAT, run(&format!("{HOLD} long task")))
        .unwrap();
    wait_for(|| prompts.lock().unwrap().len() == 1, "the person's turn").await;
    // Busy, so the peer's message waits in the queue behind the turn.
    core.doc_host
        .queue_message(CHAT, &peer("are you done?"), Vec::new())
        .expect("the chat is active when the peer writes");
    core.workspace.set_chat_archived(CHAT, true).unwrap();

    // A peer Run synced from another device lands straight in the doc.
    let handle = core.doc_host.open(CHAT).unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let synced = SessionCommandEntry {
        id: "synced-peer-run".into(),
        payload: run(&peer("ping")),
        issued_by: "other-device".into(),
        issued_at: now,
        based_on: None,
        expires_at: Some(now + 60_000),
        status: SessionCommandStatus::Pending,
        resolution: None,
    };
    handle.doc().queue_command(&synced).unwrap();
    core.doc_host.kick_drains();
    wait_for(
        || {
            handle.doc().read_commands().unwrap().iter().any(|c| {
                c.id == synced.id
                    && c.status == SessionCommandStatus::Rejected
                    && c.resolution
                        .as_deref()
                        .is_some_and(|r| r.contains("archived"))
            })
        },
        "the synced peer command to be refused at delivery",
    )
    .await;

    let first = completion(&core);
    finish.send(()).unwrap();
    wait_for(
        || completion(&core) != first,
        "the person's turn to complete",
    )
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        prompts.lock().unwrap().len(),
        1,
        "held peer mail woke the chat"
    );
    assert_eq!(queue_texts(&core), vec![peer("are you done?")]);
    assert!(archived(&core));

    // A person's restore lets the held mail out.
    let client = harness_rpc::memory_client(core.rpc_service());
    client
        .call(
            harness_rpc::methods::MUTATE,
            serde_json::json!({ "op": "setChatArchived", "chatId": CHAT, "archived": false }),
        )
        .await
        .unwrap();
    wait_for(
        || prompts.lock().unwrap().len() == 2,
        "held mail after restore",
    )
    .await;
    assert!(prompts.lock().unwrap()[1].ends_with("are you done?"));

    core.shutdown().await;
}

/// #499: the completion marker says whether only peer mail fed the turn, on
/// the live status and on the synced session row other devices notify from.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn peer_only_turns_publish_a_silent_completion() {
    let (core, _finish, _prompts) = setup().await;
    let peer_only = |core: &EngineCore| {
        let live = core.sessions.session_status(CHAT).unwrap();
        let row = core
            .workspace
            .read_sessions()
            .unwrap()
            .into_iter()
            .find(|s| s.chat_id == CHAT)
            .unwrap();
        assert_eq!(row.last_completed_turn, live.last_completed_turn);
        assert_eq!(row.last_completed_peer_only, live.last_completed_peer_only);
        live.last_completed_peer_only
    };

    for (prompt, expected) in [
        ("ask the other chat, then summarize".to_owned(), false),
        (peer("here is the answer"), true),
        (room_post("@pre-titled build is green"), true),
        ("thanks, now ship it".to_owned(), false),
    ] {
        let before = completion(&core);
        core.doc_host.queue_command(CHAT, run(&prompt)).unwrap();
        wait_for(|| completion(&core) != before, "the turn to complete").await;
        assert_eq!(peer_only(&core), expected, "{prompt}");
    }

    core.shutdown().await;
}
