use super::*;

async fn start(
    feeds: Vec<Feed>,
    resume: bool,
) -> (tempfile::TempDir, EngineCore, Arc<ControlledHarness>) {
    let dir = tempfile::tempdir().unwrap();
    let harness = Arc::new(ControlledHarness {
        feeds: Mutex::new(feeds.into()),
        requests: Mutex::new(Vec::new()),
        fail_startup: None,
    });
    let core = assemble(dir.path(), harness.clone());
    create_chat(&core).await;
    if resume {
        core.workspace
            .set_chat_harness_session(CHAT, "stored-session", "/tmp");
    }
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
    (dir, core, harness)
}

fn queue(core: &EngineCore) -> Vec<harness_doc::QueuedMessage> {
    core.doc_host
        .open(CHAT)
        .unwrap()
        .doc()
        .read_queue()
        .unwrap()
}

fn queue_ids(core: &EngineCore) -> Vec<String> {
    queue(core).into_iter().map(|row| row.id).collect()
}

#[tokio::test]
async fn partial_confirmation_preserves_only_the_unconfirmed_suffix_and_edit_gate() {
    let (tx, rx) = mpsc::unbounded_channel();
    let (_dir, core, harness) = start(vec![rx], false).await;
    for id in ["confirmed", "second", "third"] {
        accept(&core, false, id, id, vec![]).await;
    }
    let tail = core
        .doc_host
        .queue_message(CHAT, "protected tail", vec![])
        .unwrap();
    core.doc_host
        .begin_queued_message_edit(CHAT, &tail, "editor-device", "editor-instance")
        .await
        .unwrap();
    let protected = queue(&core).pop().unwrap();
    assert!(protected.delivery_gate.is_some());
    tx.send(Ok(AgentEvent::Steered {
        assistant_message_id: None,
        next_assistant_message_id: None,
    }))
    .unwrap();
    tx.send(Ok(done(DoneStatus::Errored))).unwrap();
    wait_for(|| status(&core) == Some(SessionStatus::Errored)).await;
    assert_eq!(queue_ids(&core), vec!["second", "third", tail.as_str()]);
    assert_eq!(queue(&core)[2], protected);
    for id in ["confirmed", "second", "third"] {
        assert_single_user_entry(&core, id);
    }
    core.doc_host
        .drain_queue(&core.doc_host.open(CHAT).unwrap())
        .await;
    assert_eq!(harness.requests.lock().unwrap().len(), 1);
    core.shutdown().await;
}

#[tokio::test]
async fn duplicate_accepted_ids_restore_only_one_queue_row() {
    let (tx, rx) = mpsc::unbounded_channel();
    let (_dir, core, harness) = start(vec![rx], false).await;
    for _ in 0..2 {
        accept(&core, false, "same prompt", "same-id", vec![]).await;
    }
    tx.send(Ok(done(DoneStatus::Errored))).unwrap();
    wait_for(|| status(&core) == Some(SessionStatus::Errored)).await;
    assert_eq!(queue_ids(&core), vec!["same-id"]);
    assert_single_user_entry(&core, "same-id");
    assert_eq!(harness.requests.lock().unwrap().len(), 1);
    core.shutdown().await;
}

#[tokio::test]
async fn stream_error_after_a_parked_steer_is_not_a_clean_handoff() {
    for routed in [false, true] {
        let (tx, rx) = mpsc::unbounded_channel();
        let (_dir, core, harness) = start(vec![rx], false).await;
        tx.send(Ok(done(DoneStatus::Completed))).unwrap();
        wait_for(|| status(&core) == Some(SessionStatus::Idle)).await;
        accept(&core, routed, "next turn", "next-id", vec![]).await;
        tx.send(Err(HarnessError::Protocol("connection lost".into())))
            .unwrap();
        wait_for(|| {
            status(&core) == Some(SessionStatus::Errored)
                || harness.requests.lock().unwrap().len() > 1
        })
        .await;
        assert_eq!(status(&core), Some(SessionStatus::Errored));
        assert_eq!(queue_ids(&core), vec!["next-id"]);
        core.doc_host
            .drain_queue(&core.doc_host.open(CHAT).unwrap())
            .await;
        assert_eq!(harness.requests.lock().unwrap().len(), 1);
        assert_single_user_entry(&core, "next-id");
        core.shutdown().await;
    }
}

#[tokio::test]
async fn resumed_startup_retry_preserves_steers_across_both_failed_attempts() {
    let (tx, rx) = mpsc::unbounded_channel();
    let (retry_tx, retry_rx) = mpsc::unbounded_channel();
    let (_dir, core, harness) = start(vec![rx, retry_rx], true).await;
    assert_eq!(
        harness.requests.lock().unwrap()[0].resume.as_deref(),
        Some("stored-session")
    );
    for id in ["first", "second"] {
        accept(&core, false, id, id, vec![]).await;
    }
    // Unlike Harness::run -> Err, this is an errored stream before SessionStarted.
    tx.send(Ok(done(DoneStatus::Errored))).unwrap();
    wait_for(|| harness.requests.lock().unwrap().len() == 2).await;
    assert_eq!(queue_ids(&core), vec!["first", "second"]);
    assert_eq!(
        harness.requests.lock().unwrap()[1].resume.as_deref(),
        Some("stored-session")
    );
    retry_tx.send(Ok(done(DoneStatus::Errored))).unwrap();
    wait_for(|| status(&core) == Some(SessionStatus::Errored)).await;
    core.doc_host
        .drain_queue(&core.doc_host.open(CHAT).unwrap())
        .await;
    assert_eq!(queue_ids(&core), vec!["first", "second"]);
    assert_eq!(harness.requests.lock().unwrap().len(), 2);
    for id in ["opening", "first", "second"] {
        assert_single_user_entry(&core, id);
    }
    core.shutdown().await;
}

#[tokio::test]
async fn recovered_rows_can_be_edited_removed_and_reordered_before_another_failure() {
    let (tx, rx) = mpsc::unbounded_channel();
    let (retry_tx, retry_rx) = mpsc::unbounded_channel();
    let (_dir, core, harness) = start(vec![rx, retry_rx], false).await;
    for id in ["first", "discard", "third"] {
        accept(&core, false, id, id, vec![]).await;
    }
    tx.send(Ok(done(DoneStatus::Errored))).unwrap();
    wait_for(|| status(&core) == Some(SessionStatus::Errored)).await;
    assert!(
        core.doc_host
            .remove_queued_message(CHAT, "discard")
            .await
            .unwrap()
    );
    assert!(
        core.doc_host
            .update_queued_message(CHAT, "third", "edited third")
            .unwrap()
    );
    assert!(core.doc_host.move_queued_message(CHAT, "third", 0).unwrap());
    core.doc_host
        .drain_queue(&core.doc_host.open(CHAT).unwrap())
        .await;
    assert_eq!(queue_ids(&core), vec!["third", "first"]);
    assert_eq!(harness.requests.lock().unwrap().len(), 1);
    assert!(core.doc_host.send_queued_now(CHAT, "third").await.unwrap());
    wait_for(|| harness.requests.lock().unwrap().len() == 2).await;
    assert_eq!(harness.requests.lock().unwrap()[1].prompt, "edited third");
    accept(&core, false, "new follow-up", "new-id", vec![]).await;
    retry_tx.send(Ok(done(DoneStatus::Errored))).unwrap();
    wait_for(|| status(&core) == Some(SessionStatus::Errored)).await;
    core.doc_host
        .drain_queue(&core.doc_host.open(CHAT).unwrap())
        .await;
    assert_eq!(queue_ids(&core), vec!["new-id", "first"]);
    assert_eq!(harness.requests.lock().unwrap().len(), 2);
    for id in ["first", "discard", "third", "new-id"] {
        assert_single_user_entry(&core, id);
    }
    core.shutdown().await;
}

#[tokio::test]
async fn successful_explicit_retry_resumes_normal_queue_delivery() {
    let (tx, rx) = mpsc::unbounded_channel();
    let (retry_tx, retry_rx) = mpsc::unbounded_channel();
    let (_dir, core, harness) = start(vec![rx, retry_rx], false).await;
    for id in ["first", "second"] {
        accept(&core, false, id, id, vec![]).await;
    }
    tx.send(Ok(done(DoneStatus::Errored))).unwrap();
    wait_for(|| status(&core) == Some(SessionStatus::Errored)).await;
    assert!(core.doc_host.send_queued_now(CHAT, "first").await.unwrap());
    wait_for(|| harness.requests.lock().unwrap().len() == 2).await;
    retry_tx.send(Ok(done(DoneStatus::Completed))).unwrap();
    // A clean turn boundary now releases the remaining row into the warm runtime.
    wait_for(|| queue(&core).is_empty()).await;
    retry_tx
        .send(Ok(AgentEvent::Steered {
            assistant_message_id: None,
            next_assistant_message_id: Some("second-answer".into()),
        }))
        .unwrap();
    retry_tx.send(Ok(done(DoneStatus::Completed))).unwrap();
    wait_for(|| {
        core.sessions
            .session_status(CHAT)
            .and_then(|session| session.last_completed_turn)
            .as_deref()
            == Some("second-answer")
    })
    .await;
    assert_eq!(status(&core), Some(SessionStatus::Idle));
    assert_eq!(harness.requests.lock().unwrap().len(), 2);
    for id in ["first", "second"] {
        assert_single_user_entry(&core, id);
    }
    drop(retry_tx);
    core.shutdown().await;
}

#[tokio::test]
async fn parked_stream_error_without_new_work_still_settles_cleanly() {
    let (tx, rx) = mpsc::unbounded_channel();
    let (_dir, core, harness) = start(vec![rx], false).await;
    tx.send(Ok(done(DoneStatus::Completed))).unwrap();
    wait_for(|| status(&core) == Some(SessionStatus::Idle)).await;
    tx.send(Err(HarnessError::Protocol("parked child exited".into())))
        .unwrap();
    wait_for(|| tx.is_closed()).await;
    assert_eq!(status(&core), Some(SessionStatus::Idle));
    assert!(queue(&core).is_empty());
    assert_eq!(harness.requests.lock().unwrap().len(), 1);
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_acceptance_and_failure_preserve_every_accepted_id_once() {
    for _ in 0..12 {
        let (tx, rx) = mpsc::unbounded_channel();
        let (_dir, core, harness) = start(vec![rx], false).await;
        let mut accepted = Vec::new();
        for n in 0..4 {
            let id = format!("before-{n}");
            accept(&core, false, &id, &id, vec![]).await;
            accepted.push(id);
        }
        let barrier = Arc::new(tokio::sync::Barrier::new(13));
        let mut tasks = Vec::new();
        for n in 0..12 {
            let sessions = core.sessions.clone();
            let barrier = barrier.clone();
            tasks.push(tokio::spawn(async move {
                let id = format!("racing-{n}");
                barrier.wait().await;
                let outcome = sessions.steer(CHAT, &id, Some(id.clone())).await.unwrap();
                (id, outcome)
            }));
        }
        barrier.wait().await;
        tx.send(Ok(done(DoneStatus::Errored))).unwrap();
        for task in tasks {
            let (id, outcome) = task.await.unwrap();
            if outcome == SteerOutcome::Accepted {
                accepted.push(id);
            }
        }
        wait_for(|| status(&core) == Some(SessionStatus::Errored)).await;
        core.doc_host
            .drain_queue(&core.doc_host.open(CHAT).unwrap())
            .await;
        let mut actual = queue_ids(&core);
        actual.sort();
        accepted.sort();
        assert_eq!(actual, accepted);
        assert_eq!(harness.requests.lock().unwrap().len(), 1);
        for id in accepted {
            assert_single_user_entry(&core, &id);
        }
        core.shutdown().await;
    }
}

#[tokio::test]
async fn full_steering_mailbox_rejects_without_losing_or_reordering_accepted_work() {
    let (tx, rx) = mpsc::unbounded_channel();
    let (_dir, core, harness) = start(vec![rx], false).await;
    let mut expected = Vec::new();
    for n in 0..32 {
        let id = format!("accepted-{n:02}");
        accept(&core, false, &id, &id, vec![]).await;
        expected.push(id);
    }
    assert_eq!(
        core.sessions
            .steer(CHAT, "overflow", Some("overflow-id".into()))
            .await
            .unwrap(),
        SteerOutcome::NotSteerable
    );
    let entries = core
        .doc_host
        .open(CHAT)
        .unwrap()
        .doc()
        .read_entries()
        .unwrap();
    assert!(!entries.iter().any(|entry| entry.id == "overflow-id"));
    tx.send(Ok(done(DoneStatus::Errored))).unwrap();
    wait_for(|| status(&core) == Some(SessionStatus::Errored)).await;
    assert_eq!(queue_ids(&core), expected);
    core.doc_host
        .drain_queue(&core.doc_host.open(CHAT).unwrap())
        .await;
    assert_eq!(harness.requests.lock().unwrap().len(), 1);
    for id in expected {
        assert_single_user_entry(&core, &id);
    }
    core.shutdown().await;
}
