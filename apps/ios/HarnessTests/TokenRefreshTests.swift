import XCTest
import Security
@testable import Harness

final class TokenRefreshTests: XCTestCase {
    private let tokens = AuthTokens(accessToken: "access-2", refreshToken: "refresh-2")

    private func counter() -> Counter { Counter() }

    func testRejectionIsOnlyWhatTheEdgeTurnsDown() {
        XCTAssertTrue(AuthError.http(401, "").isRejection)
        XCTAssertTrue(AuthError.http(403, "").isRejection)
        XCTAssertTrue(AuthError.http(400, "").isRejection)
        XCTAssertFalse(AuthError.http(503, "").isRejection)
        XCTAssertFalse(AuthError.http(429, "").isRejection)
        XCTAssertFalse(AuthError.http(500, "").isRejection)
        XCTAssertFalse(AuthError.invalidResponse.isRejection)
    }

    func testSuccessNeedsNoRetry() async {
        let calls = counter()
        let outcome = await TokenRefresh.run(retryDelays: [0, 0]) {
            calls.bump()
            return self.tokens
        }
        XCTAssertEqual(outcome, .refreshed(tokens))
        XCTAssertEqual(calls.value, 1)
    }

    func testAnOfflineBlipIsRetriedWithTheSameCredential() async {
        let calls = counter()
        let outcome = await TokenRefresh.run(retryDelays: [0, 0]) {
            calls.bump()
            if calls.value < 3 { throw URLError(.notConnectedToInternet) }
            return self.tokens
        }
        XCTAssertEqual(outcome, .refreshed(tokens))
        XCTAssertEqual(calls.value, 3)
    }

    func testAServerHavingABadMomentIsNotASignOut() async {
        let calls = counter()
        let outcome = await TokenRefresh.run(retryDelays: [0, 0]) {
            calls.bump()
            throw AuthError.http(503, "{\"retryable\":true}")
        }
        XCTAssertEqual(outcome, .unavailable)
        XCTAssertEqual(calls.value, 3)  // tried, gave up, kept the credential
    }

    func testARejectedCredentialStopsAtOnce() async {
        let calls = counter()
        let outcome = await TokenRefresh.run(retryDelays: [0, 0]) {
            calls.bump()
            throw AuthError.http(401, "invalid refresh credential")
        }
        XCTAssertEqual(outcome, .rejected)
        XCTAssertEqual(calls.value, 1)  // replaying a spent token only invites a family revocation
    }

    func testASecondRejectionAfterABlipStillCounts() async {
        let calls = counter()
        let outcome = await TokenRefresh.run(retryDelays: [0, 0]) {
            calls.bump()
            if calls.value == 1 { throw URLError(.timedOut) }
            throw AuthError.http(400, #"{"error":"CodeGraff rejected the grant (400)"}"#)
        }
        XCTAssertEqual(outcome, .rejected)
        XCTAssertEqual(calls.value, 3)  // the blip, the spared rejection, the one that stands
    }

    /// The reply to a refresh is lost after the edge rotated the credential (walking out of Wi-Fi), and
    /// the next attempt is turned down although the sign-in is fine: one more try, not a sign-out.
    func testARejectionRightAfterALostReplyGetsOneMoreTry() async {
        let calls = counter()
        let outcome = await TokenRefresh.run(retryDelays: [0, 0]) {
            calls.bump()
            switch calls.value {
            case 1: throw URLError(.networkConnectionLost)
            case 2: throw AuthError.http(400, #"{"error":"CodeGraff rejected the grant (400)"}"#)
            default: return self.tokens
            }
        }
        XCTAssertEqual(outcome, .refreshed(tokens))
        XCTAssertEqual(calls.value, 3)
    }

    func testTheLostReplyIsRememberedAcrossRoundsWithinTheReplayWindow() async {
        let memory = TokenRefresh.Memory()
        let start = Date()
        let first = await TokenRefresh.run(retryDelays: [], memory: memory, now: { start }) {
            throw URLError(.timedOut)
        }
        XCTAssertEqual(first, .unavailable)
        let calls = counter()
        let second = await TokenRefresh.run(retryDelays: [], memory: memory, now: { start + 60 }) {
            calls.bump()
            if calls.value == 1 { throw AuthError.http(401, "") }
            return self.tokens
        }
        XCTAssertEqual(second, .refreshed(tokens))
        // Past the window a rejection stands at once.
        let stale = TokenRefresh.Memory()
        _ = await TokenRefresh.run(retryDelays: [], memory: stale, now: { start }) { throw URLError(.timedOut) }
        let late = await TokenRefresh.run(retryDelays: [], memory: stale, now: { start + 121 }) {
            throw AuthError.http(401, "")
        }
        XCTAssertEqual(late, .rejected)
    }

    func testADefiniteRejectionAfterABlipStandsAtOnce() async {
        for body in [#"{"error":"invalid refresh credential"}"#,
                     #"{"error":"workspace does not belong to this account"}"#,
                     #"{"error":"missing refreshToken"}"#] {
            let calls = counter()
            let outcome = await TokenRefresh.run(retryDelays: [0, 0]) {
                calls.bump()
                if calls.value == 1 { throw URLError(.timedOut) }
                throw AuthError.http(401, body)
            }
            XCTAssertEqual(outcome, .rejected, body)
            XCTAssertEqual(calls.value, 2, body)
        }
    }

    func testRejectionsAreCategorizedFromTheEdgeText() {
        XCTAssertEqual(RefreshRejection(body: #"{"error":"invalid refresh credential"}"#), .invalid)
        XCTAssertEqual(RefreshRejection(body: #"{"error":"workspace does not belong to this account"}"#), .workspace)
        XCTAssertEqual(RefreshRejection(body: #"{"error":"bad request"}"#), .malformed)
        XCTAssertEqual(RefreshRejection(body: #"{"error":"CodeGraff rejected the grant (400)"}"#), .rejected)
        XCTAssertEqual(RefreshRejection(body: ""), .unknown)
        XCTAssertEqual(RefreshRejection(body: "<html>"), .unknown)
        XCTAssertFalse(RefreshRejection.rejected.definite)
        XCTAssertFalse(RefreshRejection.unknown.definite)
    }

    func testKeychainUpdatesInPlaceAndKeepsBothHalves() throws {
        // An unsigned simulator test host has no keychain entitlement (errSecMissingEntitlement): the
        // keychain is simply not there to test, which says nothing about the code.
        let probe: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "harness.keychain-probe",
            kSecAttrAccount as String: UUID().uuidString,
            kSecValueData as String: Data("x".utf8),
        ]
        let status = SecItemAdd(probe as CFDictionary, nil)
        try XCTSkipIf(status == errSecMissingEntitlement, "no keychain entitlement in this test host")
        SecItemDelete(probe as CFDictionary)
        let key = "testToken-\(UUID().uuidString)"
        defer { Keychain.delete(key: key) }
        XCTAssertEqual(Keychain.read(key: key), .missing, "absent is not the same as unreadable")
        XCTAssertTrue(Keychain.save("one", key: key))
        XCTAssertTrue(Keychain.save("two", key: key))
        XCTAssertEqual(Keychain.load(key: key), "two")
        XCTAssertEqual(Keychain.read(key: key), .found("two"))
        Keychain.delete(key: key)
        XCTAssertEqual(Keychain.read(key: key), .missing)
    }

    final class Counter: @unchecked Sendable {
        private let lock = NSLock()
        private var n = 0
        func bump() { lock.lock(); n += 1; lock.unlock() }
        var value: Int { lock.lock(); defer { lock.unlock() }; return n }
    }
}
