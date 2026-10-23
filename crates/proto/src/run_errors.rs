//! What a failed run's error means for the person reading it: the agent's
//! provider needs a sign-in, its usage limit was reached, or something else.
//! The desktop and the phone read the same error text the adapters and the
//! engine write, and both name the first two the same way.
//!
//! This only labels plain error text. A provider-typed failure
//! ([`crate::ReauthProvider`]) keeps its own recovery card, and no sign-in
//! route or command is ever taken from the message: a client that offers
//! "Log in" routes it by the chat's own agent. The cases both apps must agree
//! on live in `apps/parity/vectors/run-errors.json`.

/// A provider-side reason a turn couldn't run that signing in can fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderProblem {
    /// Not signed in, signed out, or an expired or rejected login.
    SignedOut,
    /// The account's usage or rate limit was hit (another account helps).
    LimitReached,
}

impl ProviderProblem {
    /// The error card's label.
    pub fn label(self) -> &'static str {
        match self {
            Self::SignedOut => "Signed out",
            Self::LimitReached => "Limit reached",
        }
    }

    /// The button that fixes it.
    pub fn action_label(self) -> &'static str {
        "Log in"
    }

    /// The vector's name for it (`apps/parity/vectors/run-errors.json`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SignedOut => "signedOut",
            Self::LimitReached => "limitReached",
        }
    }
}

const SIGNED_OUT: &[&str] = &[
    "isn't signed in",
    "is not signed in",
    "not signed in",
    "not logged in",
    "please run /login",
    "run `claude login`",
    "run codex login",
    "authentication failed",
    "authentication_error",
    "sign in to claude again",
    "please sign in again",
    "sign in again",
    "log in again",
    "invalid api key",
    "invalid x-api-key",
    "oauth token has expired",
    "token has expired",
    "token expired",
    "unauthorized",
    "http 401",
    "status 401",
    "(401)",
    "401 ",
    "logged out",
];

const LIMIT_REACHED: &[&str] = &[
    "usage limit",
    "limit reached",
    "rate limit",
    "rate-limit",
    "you've hit your",
    "you have hit your",
    "hit your limit",
    "quota",
    "http 429",
    "status 429",
    "(429)",
    "too many requests",
];

/// Failures that read like a sign-in or limit but that signing in to the
/// agent's provider doesn't fix: hosted credit, GitHub's own API, and
/// Harness's own account.
const NOT_THE_PROVIDER: &[&str] = &["credit", "github", "graphql:", "harness login"];

/// The problem `message` reports, if signing in to the agent's provider can
/// fix it.
pub fn provider_problem(message: &str) -> Option<ProviderProblem> {
    // Curly apostrophes read as straight ones ("isn’t signed in").
    let text = message.to_ascii_lowercase().replace('\u{2019}', "'");
    if NOT_THE_PROVIDER.iter().any(|needle| text.contains(needle)) {
        return None;
    }
    if SIGNED_OUT.iter().any(|needle| text.contains(needle)) {
        return Some(ProviderProblem::SignedOut);
    }
    if LIMIT_REACHED.iter().any(|needle| text.contains(needle)) {
        return Some(ProviderProblem::LimitReached);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_errors_the_adapters_and_engine_write() {
        for signed_out in [
            "Claude Code isn't signed in. Use Settings → Agents → Sign in.",
            "Authentication failed — sign in to Claude again.",
            "Invalid API key · Please run /login",
            "OAuth token has expired. Please obtain a new token or refresh your existing token.",
            "Not logged in · Please run /login",
            "Request failed (401) Unauthorized",
            "Codex isn\u{2019}t signed in on this computer.",
        ] {
            assert_eq!(
                provider_problem(signed_out),
                Some(ProviderProblem::SignedOut),
                "{signed_out}"
            );
        }
        for limited in [
            "Claude usage limit reached — try again after the limit resets.",
            "Claude 5-hour limit reached — the turn was blocked. Try again after it resets.",
            "You've hit your usage limit. Upgrade to Pro or try again later.",
            "429 Too Many Requests",
            "rate limited (429): quota/billing cap",
        ] {
            assert_eq!(
                provider_problem(limited),
                Some(ProviderProblem::LimitReached),
                "{limited}"
            );
        }
        for other in [
            "Claude is overloaded right now — try again shortly.",
            "The selected model isn't available.",
            "Your Codegraff credit is used up. Add credit and try again.",
            "GitHub API rate limit reached",
            "GraphQL: API rate limit exceeded",
            "not signed in — run `harness login` on this machine first",
            "Harness exited with status 1",
            "Listening on port 4012",
            "",
        ] {
            assert_eq!(provider_problem(other), None, "{other}");
        }
    }

    #[test]
    fn labels_name_the_problem_and_the_fix() {
        assert_eq!(ProviderProblem::SignedOut.label(), "Signed out");
        assert_eq!(ProviderProblem::LimitReached.label(), "Limit reached");
        assert_eq!(ProviderProblem::SignedOut.action_label(), "Log in");
        assert_eq!(ProviderProblem::LimitReached.action_label(), "Log in");
    }

    /// The cases the phone checks too (`ParityTests.swift`): both apps label
    /// the same messages the same way.
    #[test]
    fn matches_the_shared_parity_vector() {
        let vector: serde_json::Value =
            serde_json::from_str(include_str!("../../../apps/parity/vectors/run-errors.json"))
                .unwrap();
        let cases = vector["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let message = case["message"].as_str().unwrap();
            let expect = case["expect"].as_str();
            assert_eq!(
                provider_problem(message).map(ProviderProblem::as_str),
                expect,
                "{message}"
            );
            if let Some(problem) = provider_problem(message) {
                assert_eq!(
                    Some(problem.label()),
                    vector["labels"][problem.as_str()].as_str()
                );
                assert_eq!(Some(problem.action_label()), vector["actionLabel"].as_str());
            }
        }
    }
}
