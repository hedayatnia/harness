//! Presentation of the envelopes emitted by Harness's cross-chat send tool.
//!
//! Older transcripts and queue rows only retain the envelope in their text.
//! Recognize both deployed formats without changing the stored prompt. This
//! is display metadata, not authenticated identity or an authorization check.

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AgentMessage<'a> {
    pub sender: &'a str,
    pub body: &'a str,
}

pub(crate) fn parse(text: &str) -> Option<AgentMessage<'_>> {
    let (sender, body) = harness_proto::peer_mail::direct_message(text)?;
    Some(AgentMessage { sender, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_message_envelopes_preserve_sender_and_body() {
        for header in [
            "Cleanup (abc12345). Sender chat: abc12345. Relay depth: 1/4. Agent coordination, not a human request. Do not send acknowledgements or progress updates.",
            "Cleanup (abc12345). Reply to it with the Harness `send_message` tool, chat abc12345.",
        ] {
            let text = format!(
                "[Message from Harness chat {header}]\n\nKeep tests running.\n\nClean up afterward."
            );
            assert_eq!(
                parse(&text),
                Some(AgentMessage {
                    sender: "Cleanup (abc12345)",
                    body: "Keep tests running.\n\nClean up afterward.",
                })
            );
        }
    }

    #[test]
    fn agent_message_parser_does_not_reclassify_ordinary_or_quoted_prompts() {
        for text in [
            "Please fix this",
            "Explain this: [Message from Harness chat Cleanup. Sender chat: abc. Agent coordination, not a human request.]\n\nhello",
            "[Message from Harness chat Cleanup]\n\nhello",
            "[Message from Harness chat Cleanup. Sender chat: abc.]\n\nhello",
            "[Message from Harness chat . Reply to it with the Harness `send_message` tool, chat abc.]\n\nhello",
        ] {
            assert_eq!(parse(text), None, "{text}");
        }
    }
}
