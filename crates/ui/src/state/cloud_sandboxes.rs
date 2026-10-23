//! The account's Codegraff cloud sandboxes, shared by every view that shows them: the
//! always-visible "Cloud sandbox running" strip, the Settings → Accounts card and the device
//! picker.
//!
//! A sandbox started from Harness keeps running (and costs credit) until it is turned off, so
//! the list is read when the engine attaches and re-read while anything is running: every
//! [`REFRESH_EVERY`], plus right after any create, wake or turn-off. With nothing running
//! there is no timer at all.

use std::time::Duration;

use gpui::{Context, Task};
use harness_rpc::methods;

use super::AppState;
use crate::codegraff_sandboxes::{CodegraffSandboxes, needs_refresh};

/// How often a running sandbox's state is re-read (renewals, failed renewals, pauses made
/// elsewhere).
pub const REFRESH_EVERY: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct CloudSandboxesState {
    /// The engine's last answer: `None` before the first one, while signed out of Codegraff,
    /// or from an engine that doesn't know the call.
    list: Option<CodegraffSandboxes>,
    /// Bumped on every answer, so views can tell a fresh read from a repeat.
    revision: u64,
    read: Option<Task<()>>,
    next_read: Option<Task<()>>,
}

impl AppState {
    pub fn cloud_sandboxes(&self) -> Option<&CodegraffSandboxes> {
        self.cloud_sandboxes.list.as_ref()
    }

    /// Bumped on every answer the engine gives, changed or not.
    pub fn cloud_sandboxes_revision(&self) -> u64 {
        self.cloud_sandboxes.revision
    }

    /// Read the list now (dropping any read already under way). A failed read keeps the last
    /// answer, so a sandbox that was running stays on screen, and tries again on the timer.
    pub fn refresh_cloud_sandboxes(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = self.engine().cloned() else {
            return;
        };
        self.cloud_sandboxes.next_read = None;
        self.cloud_sandboxes.read = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call(methods::CODEGRAFF_SANDBOXES, serde_json::json!({}))
                .await;
            this.update(cx, |state, cx| match result {
                Ok(value) => match serde_json::from_value::<Option<CodegraffSandboxes>>(value) {
                    Ok(list) => state.apply_cloud_sandboxes(list, cx),
                    Err(error) => {
                        tracing::debug!(%error, "cloud sandbox list did not parse");
                        state.schedule_cloud_sandboxes_read(cx);
                    }
                },
                Err(error) => {
                    tracing::debug!(%error, "cloud sandbox list unavailable");
                    state.schedule_cloud_sandboxes_read(cx);
                }
            })
            .ok();
        }));
    }

    /// Take an answer read elsewhere (the device picker reads the same call).
    pub fn apply_cloud_sandboxes_value(
        &mut self,
        value: &serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        if let Ok(list) = serde_json::from_value::<Option<CodegraffSandboxes>>(value.clone()) {
            self.apply_cloud_sandboxes(list, cx);
        }
    }

    pub(crate) fn apply_cloud_sandboxes(
        &mut self,
        list: Option<CodegraffSandboxes>,
        cx: &mut Context<Self>,
    ) {
        self.cloud_sandboxes.revision = self.cloud_sandboxes.revision.wrapping_add(1);
        // While one runs, its "pauses in N min" moves with the clock even when the answer
        // doesn't, so a periodic read repaints too.
        let changed = self.cloud_sandboxes.list != list;
        if changed || list.as_ref().is_some_and(needs_refresh) {
            self.cloud_sandboxes.list = list;
            cx.notify();
        }
        self.schedule_cloud_sandboxes_read(cx);
    }

    /// One timer while anything is running; none otherwise.
    fn schedule_cloud_sandboxes_read(&mut self, cx: &mut Context<Self>) {
        if !self
            .cloud_sandboxes
            .list
            .as_ref()
            .is_some_and(needs_refresh)
        {
            self.cloud_sandboxes.next_read = None;
            return;
        }
        self.cloud_sandboxes.next_read = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_EVERY).await;
            this.update(cx, |state, cx| state.refresh_cloud_sandboxes(cx))
                .ok();
        }));
    }

    /// Forget everything (the runtime is being replaced).
    pub(super) fn reset_cloud_sandboxes(&mut self) {
        self.cloud_sandboxes = CloudSandboxesState::default();
    }
}

#[cfg(test)]
mod tests {
    use gpui::AppContext as _;

    use super::*;
    use crate::codegraff_sandboxes::CodegraffSandbox;

    fn list(state: &str) -> Option<CodegraffSandboxes> {
        Some(CodegraffSandboxes {
            enabled: true,
            sandboxes: vec![CodegraffSandbox {
                id: "cnd_1".into(),
                provider: "fleet".into(),
                state: state.into(),
                ..Default::default()
            }],
            ..Default::default()
        })
    }

    #[gpui::test]
    fn a_timer_runs_only_while_something_runs(cx: &mut gpui::TestAppContext) {
        let state = cx.new(|_| AppState::new());
        let timer = |cx: &mut gpui::TestAppContext| {
            state.read_with(cx, |state, _| state.cloud_sandboxes.next_read.is_some())
        };
        state.update(cx, |state, cx| {
            state.apply_cloud_sandboxes(list("running"), cx)
        });
        assert!(timer(cx), "running: re-read on the timer");
        state.update(cx, |state, cx| {
            state.apply_cloud_sandboxes(list("paused"), cx)
        });
        assert!(!timer(cx), "nothing running: no timer");
        state.update(cx, |state, cx| {
            state.apply_cloud_sandboxes(list("stopping"), cx)
        });
        assert!(timer(cx), "a pause under way is watched until it lands");
        state.update(cx, |state, cx| state.apply_cloud_sandboxes(None, cx));
        assert!(!timer(cx), "signed out: no timer");
        assert_eq!(
            state.read_with(cx, |state, _| state.cloud_sandboxes().cloned()),
            None
        );
        let revision = state.read_with(cx, |state, _| state.cloud_sandboxes_revision());
        assert_eq!(revision, 4, "every answer counts");
    }
}
