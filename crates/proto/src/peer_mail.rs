//! Prompts that reached a chat through agent coordination rather than from
//! its own composer: a Harness `send_message` from another chat (the
//! `[Message from Harness chat …]` envelope) or an agent-room delivery
//! (the `<!--harness-room …-->` tag, see [`crate::room_delivery`]).
//!
//! Both envelopes are written by Harness itself, so recognizing them needs no
//! sender cooperation. Recognition is not authentication: a person can type
//! an envelope by hand, and the only effect is that their own prompt is
//! treated as peer mail (it stays silent and cannot wake an archived chat).
//!
//! Two rules use it:
//! - an archived chat takes no peer mail ([`delivery_allowed`]): it is out of
//!   the active collaboration set until a person restores or reprompts it;
//! - a turn that answered only peer mail completes without a completion
//!   alert ([`TurnOrigin::peer_only`]); a person's prompt anywhere in the
//!   turn makes it a human turn again.

const DIRECT_PREFIX: &str = "[Message from Harness chat ";

/// The `(sender, body)` of a Harness cross-chat `send_message` envelope, in
/// either deployed format. `None` for anything else, including an envelope
/// quoted after other text.
pub fn direct_message(text: &str) -> Option<(&str, &str)> {
    let envelope = text.strip_prefix(DIRECT_PREFIX)?;
    let (header, body) = envelope.split_once("]\n\n")?;
    if header.contains(['\r', '\n']) {
        return None;
    }
    let sender = if let Some((sender, rest)) = header.split_once(". Sender chat: ") {
        if !rest.contains("Agent coordination, not a human request.") {
            return None;
        }
        sender
    } else {
        let (sender, _) =
            header.split_once(". Reply to it with the Harness `send_message` tool, chat ")?;
        sender
    };
    if sender.trim().is_empty() {
        return None;
    }
    Some((sender, body))
}

/// Whether `text` arrived as agent coordination (a cross-chat message or a
/// room delivery) rather than as a person's prompt.
pub fn is_peer_mail(text: &str) -> bool {
    direct_message(text).is_some() || crate::room_delivery::is_delivery(text)
}

/// Whether a prompt may be delivered to a chat in its current archive state.
/// An archived chat refuses peer mail, so peer traffic neither starts a turn
/// in it nor restores it; a person's prompt is always delivered (and restores
/// the chat, #345).
pub fn delivery_allowed(chat_archived: bool, peer_mail: bool) -> bool {
    !(chat_archived && peer_mail)
}

/// The reason a refused peer delivery reports, to the sender and in the
/// command's resolution.
pub const ARCHIVED_REFUSAL: &str = "chat is archived: archived chats take no peer messages. \
     Do not retry; ask the person to restore the chat if it is needed again";

/// Who asked for the work of one turn. Accumulates over every prompt the turn
/// accepted (its opening prompt and any steers) until the turn settles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TurnOrigin {
    pub human: bool,
    pub peer: bool,
}

impl TurnOrigin {
    pub fn note_prompt(&mut self, text: &str) {
        if is_peer_mail(text) {
            self.peer = true;
        } else {
            self.human = true;
        }
    }

    /// Only peer mail fed this turn: its completion is coordination, not a
    /// result a person asked to be told about. A turn with no recorded prompt
    /// (an agent continuing on its own) is not peer-only, so it keeps the
    /// ordinary completion alert.
    pub fn peer_only(&self) -> bool {
        self.peer && !self.human
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::room_delivery::{RoomOrigin, format};

    const DIRECT: &str = "[Message from Harness chat Cleanup (abc12345). Sender chat: abc12345. \
        Relay depth: 1/4. Agent coordination, not a human request. Do not send acknowledgements \
        or progress updates.]\n\nKeep tests running.";
    const LEGACY: &str = "[Message from Harness chat Cleanup (abc12345). Reply to it with the \
        Harness `send_message` tool, chat abc12345.]\n\nKeep tests running.";

    fn room() -> String {
        format(
            &RoomOrigin {
                room_id: "room_1".into(),
                room_name: "build".into(),
                seq: 3,
                from_member: "peer".into(),
                member_kind: "harness_chat".into(),
                from_user: false,
                framed: true,
                external: false,
                from_display: None,
                from_github: None,
                from_fingerprint: None,
            },
            "done here",
        )
    }

    #[test]
    fn peer_envelopes_are_recognized_and_ordinary_prompts_are_not() {
        assert_eq!(
            direct_message(DIRECT),
            Some(("Cleanup (abc12345)", "Keep tests running."))
        );
        assert!(is_peer_mail(DIRECT));
        assert!(is_peer_mail(LEGACY));
        assert!(is_peer_mail(&room()));
        for text in [
            "Please fix this",
            "Explain this: [Message from Harness chat Cleanup. Sender chat: abc. Agent coordination, not a human request.]\n\nhello",
            "[Message from Harness chat Cleanup. Sender chat: abc.]\n\nhello",
            "<!--harness-room {not json} -->\nhi",
            "quoted <!--harness-room {} -->",
        ] {
            assert!(!is_peer_mail(text), "{text}");
        }
    }

    #[test]
    fn archived_chats_refuse_only_peer_mail() {
        assert!(delivery_allowed(false, false));
        assert!(delivery_allowed(false, true));
        assert!(delivery_allowed(true, false));
        assert!(!delivery_allowed(true, true));
    }

    #[test]
    fn a_turn_is_peer_only_until_a_person_prompts_it() {
        let mut origin = TurnOrigin::default();
        assert!(
            !origin.peer_only(),
            "a prompt-less agent turn keeps its alert"
        );
        origin.note_prompt(DIRECT);
        origin.note_prompt(&room());
        assert!(origin.peer_only());
        origin.note_prompt("also update the changelog");
        assert!(
            !origin.peer_only(),
            "human work that consults peers still notifies"
        );
        let mut human_first = TurnOrigin::default();
        human_first.note_prompt("ask the other chat and report back");
        human_first.note_prompt(DIRECT);
        assert!(!human_first.peer_only());
    }
}
