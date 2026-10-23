//! Accepted steers must survive failed runs without automatically restarting.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use harness_adapters::{Harness, HarnessError, RunControls};
use harness_engine::{EngineCore, HarnessRegistry, SteerOutcome};
use harness_proto::{
    AgentEvent, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel,
    SessionStatus, SteeringMode,
};
use tokio::sync::{Notify, mpsc};

const CHAT: &str = "orphaned-steers";
type Feed = mpsc::UnboundedReceiver<Result<AgentEvent, HarnessError>>;

#[path = "orphaned_steers/edge_cases.rs"]
mod edge_cases;

struct ControlledHarness {
    feeds: Mutex<VecDeque<Feed>>,
    requests: Mutex<Vec<RunRequest>>,
    fail_startup: Option<Arc<Notify>>,
}

#[async_trait]
impl Harness for ControlledHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Controlled"
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
        let first = {
            let mut requests = self.requests.lock().unwrap();
            requests.push(request);
            requests.len() == 1
        };
        let feed = self.feeds.lock().unwrap().pop_front();
        if first && let Some(fail) = &self.fail_startup {
            // Accept steers while startup is pending, then fail before a stream exists.
            fail.notified().await;
            return Err(HarnessError::Protocol("offline during startup".into()));
        }
        Ok(
            futures::stream::unfold((feed, controls), |(mut feed, controls)| async move {
                let event = match &mut feed {
                    Some(feed) => feed.recv().await,
                    None => std::future::pending().await,
                };
                event.map(|event| (event, (feed, controls)))
            })
            .boxed(),
        )
    }
}

fn request(prompt: &str) -> RunRequest {
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
        error: (status == DoneStatus::Errored).then(|| "offline".into()),
        session_id: None,
    }
}

fn assemble(path: &std::path::Path, harness: Arc<ControlledHarness>) -> EngineCore {
    let registry = HarnessRegistry::new();
    registry.register(harness);
    EngineCore::assemble(path, Arc::new(registry), HarnessId::Mock, None).unwrap()
}

async fn create_chat(core: &EngineCore) {
    harness_rpc::memory_client(core.rpc_service())
        .call(
            harness_rpc::methods::MUTATE,
            serde_json::json!({
                "op": "createChat",
                "chatId": CHAT,
                "deviceId": core.device_id,
            }),
        )
        .await
        .unwrap();
    // No auto-titler side runs in the request count.
    core.workspace.rename_chat(CHAT, "Orphan recovery").unwrap();
}

async fn wait_for(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("engine transition");
}

fn status(core: &EngineCore) -> Option<SessionStatus> {
    core.sessions.session_status(CHAT).map(|s| s.status)
}

async fn accept(core: &EngineCore, routed: bool, prompt: &str, id: &str, files: Vec<String>) {
    if routed {
        core.sessions
            .dispatch(CHAT, HarnessId::Mock, request(prompt), Some(id.into()))
            .await
            .unwrap();
    } else {
        assert_eq!(
            core.sessions
                .steer_with_attachments(CHAT, prompt, Some(id.into()), files)
                .await
                .unwrap(),
            SteerOutcome::Accepted
        );
    }
}

fn assert_single_user_entry(core: &EngineCore, id: &str) {
    let entries = core
        .doc_host
        .open(CHAT)
        .unwrap()
        .doc()
        .read_entries()
        .unwrap();
    assert_eq!(entries.iter().filter(|entry| entry.id == id).count(), 1);
}

#[derive(Clone, Copy)]
enum Failure {
    Done,
    Stream,
    Eof,
    Startup,
}

#[tokio::test]
async fn failed_runs_hold_accepted_steers_until_explicit_retry() {
    for failure in [
        Failure::Done,
        Failure::Stream,
        Failure::Eof,
        Failure::Startup,
    ] {
        for routed in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let (tx, rx) = mpsc::unbounded_channel();
            let fail_startup = Arc::new(Notify::new());
            let harness = Arc::new(ControlledHarness {
                feeds: Mutex::new(VecDeque::from([rx])),
                requests: Mutex::new(Vec::new()),
                fail_startup: matches!(failure, Failure::Startup).then(|| fail_startup.clone()),
            });
            let mut core = assemble(dir.path(), harness.clone());
            create_chat(&core).await;
            core.sessions
                .dispatch(
                    CHAT,
                    HarnessId::Mock,
                    request("opening"),
                    Some("opening".into()),
                )
                .await
                .unwrap();
            wait_for(|| harness.requests.lock().unwrap().len() == 1).await;
            let files = if routed {
                vec![]
            } else {
                vec!["/tmp/held-upload.png".into()]
            };
            accept(
                &core,
                routed,
                "first follow-up",
                "first-steer",
                files.clone(),
            )
            .await;
            accept(&core, routed, "second follow-up", "second-steer", vec![]).await;
            let tail = core
                .doc_host
                .queue_message(CHAT, "later work", vec![])
                .unwrap();
            match failure {
                Failure::Done => tx.send(Ok(done(DoneStatus::Errored))).unwrap(),
                Failure::Stream => tx
                    .send(Err(HarnessError::Protocol("offline".into())))
                    .unwrap(),
                Failure::Startup => fail_startup.notify_one(),
                Failure::Eof => {}
            }
            drop(tx);
            wait_for(|| status(&core) == Some(SessionStatus::Errored)).await;
            let expected = vec![
                "first-steer".to_string(),
                "second-steer".into(),
                tail.clone(),
            ];
            let handle = core.doc_host.open(CHAT).unwrap();
            let held = handle.doc().read_queue().unwrap();
            assert_eq!(
                held.iter().map(|row| row.id.clone()).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(held[0].text, "first follow-up");
            assert_eq!(held[0].attachments, files);
            assert!(held[0].issued_at > 0);
            assert!(held[0].hold_for_turn_end);
            assert_single_user_entry(&core, "first-steer");
            assert_single_user_entry(&core, "second-steer");

            // A doc mutation and explicit automatic drains cannot release held work.
            core.doc_host
                .update_queued_message(CHAT, &tail, "edited later work")
                .unwrap();
            core.doc_host.drain_queue(&handle).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(harness.requests.lock().unwrap().len(), 1);
            let saved = handle.doc().read_queue().unwrap();
            assert_eq!(saved.len(), 3);
            drop(handle);

            if matches!(failure, Failure::Done) && !routed {
                // Recovery is durable, including its stop-until-explicit-send policy.
                core.shutdown().await;
                drop(core);
                core = assemble(dir.path(), harness.clone());
                let handle = core.doc_host.open(CHAT).unwrap();
                assert_eq!(handle.doc().read_queue().unwrap(), saved);
                core.doc_host.drain_queue(&handle).await;
                assert_eq!(harness.requests.lock().unwrap().len(), 1);
            }

            assert!(
                core.doc_host
                    .send_queued_now(CHAT, "first-steer")
                    .await
                    .unwrap()
            );
            wait_for(|| harness.requests.lock().unwrap().len() == 2).await;
            let expected_prompt = if files.is_empty() {
                "first follow-up".to_string()
            } else {
                format!(
                    "first follow-up\n\nAttached images (local files — open them to view):\n- {}",
                    files[0]
                )
            };
            assert_eq!(harness.requests.lock().unwrap()[1].prompt, expected_prompt);
            assert_single_user_entry(&core, "first-steer");
            let held = core
                .doc_host
                .open(CHAT)
                .unwrap()
                .doc()
                .read_queue()
                .unwrap();
            assert_eq!(
                held.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
                vec!["second-steer", tail.as_str()]
            );
            core.shutdown().await;
        }
    }
}

#[tokio::test]
async fn clean_parked_exit_still_recovers_an_unconfirmed_steer() {
    for routed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        let harness = Arc::new(ControlledHarness {
            feeds: Mutex::new(VecDeque::from([rx])),
            requests: Mutex::new(Vec::new()),
            fail_startup: None,
        });
        let core = assemble(dir.path(), harness.clone());
        create_chat(&core).await;
        core.sessions
            .dispatch(CHAT, HarnessId::Mock, request("opening"), None)
            .await
            .unwrap();
        wait_for(|| harness.requests.lock().unwrap().len() == 1).await;
        accept(&core, routed, "follow-up", "steer", vec![]).await;
        tx.send(Ok(done(DoneStatus::Completed))).unwrap();
        wait_for(|| status(&core) == Some(SessionStatus::Idle)).await;
        drop(tx);
        wait_for(|| harness.requests.lock().unwrap().len() == 2).await;
        assert_eq!(harness.requests.lock().unwrap()[1].prompt, "follow-up");
        assert!(
            core.doc_host
                .open(CHAT)
                .unwrap()
                .doc()
                .read_queue()
                .unwrap()
                .is_empty()
        );
        assert_single_user_entry(&core, "steer");
        core.shutdown().await;
    }
}

#[tokio::test]
async fn confirmed_or_interrupted_steers_are_not_recovered_as_failed_work() {
    for confirmed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        let harness = Arc::new(ControlledHarness {
            feeds: Mutex::new(VecDeque::from([rx])),
            requests: Mutex::new(Vec::new()),
            fail_startup: None,
        });
        let core = assemble(dir.path(), harness.clone());
        create_chat(&core).await;
        core.sessions
            .dispatch(CHAT, HarnessId::Mock, request("opening"), None)
            .await
            .unwrap();
        wait_for(|| harness.requests.lock().unwrap().len() == 1).await;
        accept(&core, false, "follow-up", "steer", vec![]).await;
        if confirmed {
            tx.send(Ok(AgentEvent::Steered {
                assistant_message_id: None,
                next_assistant_message_id: None,
            }))
            .unwrap();
            tx.send(Ok(done(DoneStatus::Errored))).unwrap();
            wait_for(|| status(&core) == Some(SessionStatus::Errored)).await;
        } else {
            core.sessions.interrupt(CHAT).await.unwrap();
            assert_eq!(status(&core), Some(SessionStatus::Idle));
        }
        assert!(
            core.doc_host
                .open(CHAT)
                .unwrap()
                .doc()
                .read_queue()
                .unwrap()
                .is_empty()
        );
        assert_eq!(harness.requests.lock().unwrap().len(), 1);
        assert_single_user_entry(&core, "steer");
        core.shutdown().await;
    }
}
