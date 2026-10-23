//! Why the engine interrupted a chat's run, for drivers whose protocol can
//! say so. An ACP `session/cancel` carries no reason, so graff labelled every
//! cancel as the user's, including the engine replacing a run or shutting
//! down (justrach/codegraff#1611). The engine notes the reason here just
//! before cancelling [`crate::RunControls::interrupt`]; the driver takes it
//! when it sends the cancel.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

static REASONS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);

/// Record why `chat_id`'s run is about to be interrupted.
pub fn note(chat_id: &str, reason: &str) {
    REASONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(chat_id.to_owned(), reason.to_owned());
}

/// The reason noted for `chat_id`, once.
pub fn take(chat_id: &str) -> Option<String> {
    REASONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(chat_id)
}

/// A Stop or a send-now is the user's; everything else (replacing a run,
/// shutdown, a deleted project) is the app's own lifecycle.
pub fn by_user(reason: &str) -> bool {
    matches!(reason, "stop" | "send now")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_noted_reason_is_taken_once_per_chat() {
        note("chat-reason-a", "stop");
        note("chat-reason-b", "replace: mailbox closed");
        assert_eq!(take("chat-reason-a").as_deref(), Some("stop"));
        assert_eq!(take("chat-reason-a"), None);
        assert_eq!(
            take("chat-reason-b").as_deref(),
            Some("replace: mailbox closed")
        );
        assert!(by_user("stop") && by_user("send now"));
        assert!(!by_user("replace: mailbox closed") && !by_user("engine shutdown"));
    }
}
