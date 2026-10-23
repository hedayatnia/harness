// How a token refresh is judged and retried. Pure Foundation, so it is unit-testable without a device.
//
// CodeGraff refresh tokens are single use. That makes one distinction matter more than any other:
//   - the edge TURNED THE CREDENTIAL DOWN (400/401/403): it is spent or revoked, nothing will revive it,
//     and the person has to sign in again;
//   - the edge was UNREACHABLE or having a bad moment (offline, timeout, 5xx, 429): nothing was
//     decided, and the same credential must be kept and tried again.
// Treating the second like the first signs people out over a bad train tunnel; treating the first like
// the second leaves a dead token redialing forever behind a quiet "connecting" spinner.

import Foundation

extension AuthError {
    /// The edge refused the credential itself (as opposed to failing to answer).
    var isRejection: Bool {
        if case .http(let code, _) = self { return code == 400 || code == 401 || code == 403 }
        return false
    }
}

enum RefreshOutcome: Equatable {
    case refreshed(AuthTokens)
    /// The credential is no good: sign in again.
    case rejected
    /// Undecided: keep the credential, try again later.
    case unavailable
}

/// Why the edge turned a refresh down, from its `{error}` text, as a fixed category (logs name the kind,
/// never the response body). Mirrors the desktop engine (crates/engine/src/auth.rs, #331).
enum RefreshRejection: Equatable {
    /// The edge could not verify the refresh credential at all.
    case invalid
    /// The credential belongs to another account's workspace.
    case workspace
    /// The request itself was malformed.
    case malformed
    /// CodeGraff refused the grant: revoked, or already rotated by an attempt whose answer never arrived.
    case rejected
    case unknown

    init(body: String) {
        let data = Data(body.utf8)
        let reason = (try? JSONSerialization.jsonObject(with: data) as? [String: Any])?["error"] as? String ?? ""
        switch reason {
        case "invalid refresh credential": self = .invalid
        case "workspace does not belong to this account": self = .workspace
        case "missing refreshToken", "bad request": self = .malformed
        case _ where reason.hasPrefix("CodeGraff rejected the grant"): self = .rejected
        default: self = .unknown
        }
    }

    /// Retrying cannot change these answers.
    var definite: Bool { self == .invalid || self == .workspace || self == .malformed }
}

enum TokenRefresh {
    /// How long the edge replays a rotated refresh's answer (`refresh-leeway.ts`). A rejection this soon
    /// after an unanswered attempt may be that rotation's lost answer, not a revoked sign-in.
    static let replayWindow: TimeInterval = 120

    /// What earlier rounds left behind: when an attempt last went unanswered, and whether a rejection
    /// after one was already given its second try. Only a success clears them.
    final class Memory: @unchecked Sendable {
        private let lock = NSLock()
        private var lastUnanswered: Date?
        private var spared = false

        func unanswered(at now: Date) { lock.withLock { lastUnanswered = now } }

        func succeeded() {
            lock.withLock {
                lastUnanswered = nil
                spared = false
            }
        }

        /// Whether this rejection gets one more try: it follows an unanswered attempt inside the replay
        /// window, and no rejection was spared since.
        func spareRejection(at now: Date) -> Bool {
            lock.withLock {
                let recent = lastUnanswered.map { now.timeIntervalSince($0) < TokenRefresh.replayWindow } ?? false
                let spare = recent && !spared
                spared = spared || spare
                return spare
            }
        }
    }

    /// Waits between attempts of one refresh round (so up to three tries). Repeating a request is safe:
    /// the edge answers a repeat of a refresh it already served with the same result for a couple of
    /// minutes, so a reply lost on the way back does not spend the token twice.
    static let retryDelays: [UInt64] = [1_000_000_000, 3_000_000_000]

    static func classify(_ error: Error) -> RefreshOutcome {
        if let auth = error as? AuthError, auth.isRejection { return .rejected }
        return .unavailable
    }

    /// One refresh round: retry `.unavailable` after each delay, stop at the first definite answer. A
    /// rejection right after an unanswered attempt (in this round or a recent one) gets one more try
    /// first: when a reply is lost after the edge rotated the credential, the next attempt can be turned
    /// down although the sign-in is fine. A definite cause, or a second rejection, still stands.
    static func run(
        retryDelays: [UInt64] = TokenRefresh.retryDelays,
        memory: Memory = Memory(),
        now: () -> Date = Date.init,
        send: () async throws -> AuthTokens
    ) async -> RefreshOutcome {
        var attempt = 0
        while true {
            do {
                let tokens = try await send()
                memory.succeeded()
                return .refreshed(tokens)
            } catch {
                if error is CancellationError { return .unavailable }
                let outcome = classify(error)
                if outcome == .unavailable {
                    memory.unanswered(at: now())
                } else if case .http(_, let body) = error as? AuthError,
                          !RefreshRejection(body: body).definite, memory.spareRejection(at: now()) {
                    try? await Task.sleep(nanoseconds: retryDelays.first ?? 0)
                    continue
                }
                guard outcome == .unavailable, attempt < retryDelays.count else { return outcome }
                try? await Task.sleep(nanoseconds: retryDelays[attempt])
                attempt += 1
            }
        }
    }
}
