//! Real ACP subprocess coverage: protocol errors, EOF, crashes, and turn boundaries.
#![cfg(unix)]

use std::{path::Path, sync::Arc, time::Duration};

use harness_adapters::AcpHarness;
use harness_doc::{MessagePart, QueuedMessage};
use harness_engine::{EngineCore, HarnessRegistry, SteerOutcome};
use harness_proto::{HarnessId, RunRequest, SandboxLevel, SessionStatus};

const CHAT: &str = "acp-orphan-recovery";

struct Rig {
    core: EngineCore,
    dir: tempfile::TempDir,
}

impl Rig {
    async fn new(mode: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("failure-mode"), mode).unwrap();
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/orphaned-steers-acp.py");
        let registry = HarnessRegistry::new();
        registry.register(Arc::new(AcpHarness::grok().with_executable(fixture)));
        let core =
            EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Grok, None).unwrap();
        harness_rpc::memory_client(core.rpc_service())
            .call(
                harness_rpc::methods::MUTATE,
                serde_json::json!({
                    "op": "createChat", "chatId": CHAT, "deviceId": core.device_id,
                }),
            )
            .await
            .unwrap();
        core.workspace
            .set_chat_cwd(CHAT, &dir.path().display().to_string())
            .unwrap();
        core.workspace
            .rename_chat(CHAT, "ACP recovery fixture")
            .unwrap();
        let rig = Self { core, dir };
        rig.core
            .sessions
            .dispatch(
                CHAT,
                HarnessId::Grok,
                rig.request("opening"),
                Some("opening-id".into()),
            )
            .await
            .unwrap();
        wait_for(|| rig.has_text("working: opening")).await;
        rig
    }

    fn request(&self, prompt: &str) -> RunRequest {
        RunRequest {
            prompt: prompt.into(),
            harness: None,
            model: None,
            reasoning: None,
            model_options: Default::default(),
            cwd: self.dir.path().display().to_string(),
            sandbox: SandboxLevel::WorkspaceWrite,
            auto_approve: true,
            attachments: Vec::new(),
            worktree: None,
            resume: None,
        }
    }

    fn status(&self) -> Option<SessionStatus> {
        self.core
            .sessions
            .session_status(CHAT)
            .map(|session| session.status)
    }

    fn queue(&self) -> Vec<QueuedMessage> {
        self.core
            .doc_host
            .open(CHAT)
            .unwrap()
            .doc()
            .read_queue()
            .unwrap()
    }

    fn prompt_texts(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("acp-events.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| event["event"] == "prompt")
            .filter_map(|event| event["text"].as_str().map(str::to_owned))
            .collect()
    }

    fn has_text(&self, expected: &str) -> bool {
        self.core
            .doc_host
            .open(CHAT)
            .unwrap()
            .doc()
            .read_entries()
            .unwrap_or_default()
            .iter()
            .any(|entry| {
                entry.parts.iter().any(|part| {
                matches!(part, MessagePart::Text { text, .. } if text.contains(expected))
            })
            })
    }

    fn assert_single_entry(&self, id: &str) {
        let entries = self
            .core
            .doc_host
            .open(CHAT)
            .unwrap()
            .doc()
            .read_entries()
            .unwrap();
        assert_eq!(entries.iter().filter(|entry| entry.id == id).count(), 1);
    }

    fn release(&self, gate: &str) {
        std::fs::write(self.dir.path().join(gate), "released").unwrap();
    }

    async fn accept(&self, text: &str, id: &str, routed: bool) {
        if routed {
            self.core
                .sessions
                .dispatch(CHAT, HarnessId::Grok, self.request(text), Some(id.into()))
                .await
                .unwrap();
        } else {
            assert_eq!(
                self.core
                    .sessions
                    .steer(CHAT, text, Some(id.into()))
                    .await
                    .unwrap(),
                SteerOutcome::Accepted
            );
        }
    }
}

async fn wait_for(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("ACP fixture transition");
}

#[tokio::test]
async fn protocol_error_eof_and_child_crash_preserve_unconfirmed_prompts() {
    for mode in ["rpc_error", "eof", "crash"] {
        for routed in [false, true] {
            let rig = Rig::new(mode).await;
            rig.accept("first", "first-id", routed).await;
            rig.accept("second", "second-id", routed).await;
            rig.release("release-first");
            wait_for(|| rig.status() == Some(SessionStatus::Errored)).await;
            let held = rig.queue();
            assert_eq!(
                held.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
                vec!["first-id", "second-id"],
                "{mode}, routed={routed}"
            );
            assert_eq!(
                held.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
                vec!["first", "second"]
            );
            // Status and document drains cannot translate a failed RPC into a new prompt.
            for _ in 0..3 {
                rig.core
                    .doc_host
                    .drain_queue(&rig.core.doc_host.open(CHAT).unwrap())
                    .await;
            }
            assert_eq!(rig.prompt_texts(), vec!["opening"]);
            for id in ["opening-id", "first-id", "second-id"] {
                rig.assert_single_entry(id);
            }
            assert!(
                rig.core
                    .doc_host
                    .send_queued_now(CHAT, "first-id")
                    .await
                    .unwrap()
            );
            wait_for(|| {
                rig.has_text("ack: second")
                    && rig.queue().is_empty()
                    && rig.status() == Some(SessionStatus::Idle)
            })
            .await;
            assert_eq!(rig.prompt_texts(), vec!["opening", "first", "second"]);
            for id in ["opening-id", "first-id", "second-id"] {
                rig.assert_single_entry(id);
            }
            rig.core.shutdown().await;
        }
    }
}

#[tokio::test]
async fn failure_after_a_real_steered_boundary_holds_only_the_remaining_prompt() {
    for routed in [false, true] {
        let rig = Rig::new("next_error").await;
        rig.accept("first", "first-id", routed).await;
        rig.accept("second", "second-id", routed).await;
        rig.release("release-first");
        wait_for(|| rig.has_text("working: first")).await;
        assert_eq!(rig.prompt_texts(), vec!["opening", "first"]);
        rig.release("release-second");
        wait_for(|| rig.status() == Some(SessionStatus::Errored)).await;
        assert_eq!(
            rig.queue()
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["second-id"]
        );
        rig.core
            .doc_host
            .drain_queue(&rig.core.doc_host.open(CHAT).unwrap())
            .await;
        assert_eq!(rig.prompt_texts(), vec!["opening", "first"]);
        assert!(
            rig.core
                .doc_host
                .send_queued_now(CHAT, "second-id")
                .await
                .unwrap()
        );
        wait_for(|| rig.has_text("ack: second") && rig.status() == Some(SessionStatus::Idle)).await;
        assert_eq!(rig.prompt_texts(), vec!["opening", "first", "second"]);
        assert!(rig.queue().is_empty());
        rig.assert_single_entry("first-id");
        rig.assert_single_entry("second-id");
        rig.core.shutdown().await;
    }
}

#[tokio::test]
async fn image_bytes_and_identity_survive_error_restart_and_explicit_acp_retry() {
    use base64::Engine as _;

    let rig = Rig::new("rpc_error").await;
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    std::fs::create_dir_all(rig.core.uploads.dir()).unwrap();
    let image = rig
        .core
        .uploads
        .dir()
        .join("image with spaces-\u{03bb}.png");
    std::fs::write(&image, png).unwrap();
    let image_path = image.display().to_string();
    assert_eq!(
        rig.core
            .sessions
            .steer_with_attachments(
                CHAT,
                "inspect image",
                Some("image-id".into()),
                vec![image_path.clone()],
            )
            .await
            .unwrap(),
        SteerOutcome::Accepted
    );
    rig.release("release-first");
    wait_for(|| rig.status() == Some(SessionStatus::Errored)).await;
    let held = rig.queue();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].attachments, vec![image_path.clone()]);
    let Rig { core, dir } = rig;
    core.shutdown().await;
    drop(core);
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/orphaned-steers-acp.py");
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(AcpHarness::grok().with_executable(fixture)));
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Grok, None).unwrap();
    let rig = Rig { core, dir };
    assert_eq!(rig.queue(), held);
    rig.core
        .doc_host
        .drain_queue(&rig.core.doc_host.open(CHAT).unwrap())
        .await;
    assert_eq!(rig.prompt_texts(), vec!["opening"]);
    assert!(
        rig.core
            .doc_host
            .send_queued_now(CHAT, "image-id")
            .await
            .unwrap()
    );
    wait_for(|| rig.has_text("ack: inspect image") && rig.status() == Some(SessionStatus::Idle))
        .await;
    let events = std::fs::read_to_string(rig.dir.path().join("acp-events.jsonl")).unwrap();
    let prompts: Vec<serde_json::Value> = events
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|event| event["event"] == "prompt")
        .collect();
    assert_eq!(prompts.len(), 2);
    // Successful inlining removes the local-path trailer from provider text.
    assert_eq!(prompts[1]["text"], "inspect image");
    let images: Vec<_> = prompts[1]["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|part| part["type"] == "image")
        .collect();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0]["mimeType"], "image/png");
    let uri = images[0]["uri"].as_str().unwrap();
    assert!(uri.contains("%20"));
    assert!(uri.contains("%CE%BB"));
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(images[0]["data"].as_str().unwrap())
            .unwrap(),
        png
    );
    rig.assert_single_entry("image-id");
    assert!(rig.queue().is_empty());
    rig.core.shutdown().await;
}
