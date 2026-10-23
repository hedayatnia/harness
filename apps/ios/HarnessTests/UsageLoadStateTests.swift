import XCTest
@testable import Harness

final class UsageLoadStateTests: XCTestCase {
    private typealias State = UsageLoadState<Int?, [String]>
    private let offline = UsageLoadFailure(message: "offline")

    func testUsageFailureDoesNotBecomeSignedOutOrHideAccounts() {
        var state = State()
        let request = state.begin(deviceId: "a")
        state.finishAccounts(request, currentDeviceId: "a", result: .success(["account"]))
        guard case .loaded(["account"]) = state.accounts else {
            return XCTFail("Accounts should appear before usage finishes")
        }
        guard case .loading = state.codegraff else { return XCTFail("Usage is still pending") }
        state.finishCodegraff(request, currentDeviceId: "a", result: .failure(offline))
        guard case .failed("offline") = state.codegraff else {
            return XCTFail("Failed usage must not become successful nil")
        }
        guard case .loaded(["account"]) = state.accounts else { return XCTFail("Accounts were lost") }
    }

    func testAccountFailureDoesNotBecomeEmptyOrHideUsage() {
        var state = State()
        let request = state.begin(deviceId: "a")
        state.finishCodegraff(request, currentDeviceId: "a", result: .success(42))
        guard case .loaded(42) = state.codegraff else {
            return XCTFail("Usage should appear before accounts finish")
        }
        guard case .loading = state.accounts else { return XCTFail("Accounts are still pending") }
        state.finishAccounts(request, currentDeviceId: "a", result: .failure(offline))
        guard case .failed("offline") = state.accounts else {
            return XCTFail("Failed accounts must not become a successful empty array")
        }
        guard case .loaded(42) = state.codegraff else { return XCTFail("Usage was lost") }
    }

    func testSuccessfulNilAndEmptyAreLiveResults() {
        var state = State()
        let request = state.begin(deviceId: "a")
        state.finishCodegraff(request, currentDeviceId: "a", result: .success(nil))
        state.finishAccounts(request, currentDeviceId: "a", result: .success([]))
        guard case .loaded(nil) = state.codegraff else { return XCTFail("Successful nil means signed out") }
        guard case .loaded(let accounts) = state.accounts else { return XCTFail("Expected live accounts") }
        XCTAssertTrue(accounts.isEmpty)
    }

    func testBothFailuresRemainRetryableAndSuccessfulRetryReplacesThem() {
        var state = State()
        let failed = state.begin(deviceId: "a")
        state.finishCodegraff(failed, currentDeviceId: "a", result: .failure(offline))
        state.finishAccounts(failed, currentDeviceId: "a", result: .failure(offline))
        guard case .failed = state.codegraff, case .failed = state.accounts else {
            return XCTFail("Both sections must report their failures")
        }
        let retry = state.begin(deviceId: "a")
        guard case .loading = state.codegraff, case .loading = state.accounts else {
            return XCTFail("Retry must independently load both sections")
        }
        state.finishCodegraff(retry, currentDeviceId: "a", result: .success(nil))
        state.finishAccounts(retry, currentDeviceId: "a", result: .success([]))
        guard case .loaded(nil) = state.codegraff, case .loaded = state.accounts else {
            return XCTFail("Retry should replace failures with successful empty results")
        }
    }

    func testRefreshNeverPresentsOldValuesAsLiveAfterFailure() {
        var state = State()
        let initial = state.begin(deviceId: "a")
        state.finishCodegraff(initial, currentDeviceId: "a", result: .success(42))
        state.finishAccounts(initial, currentDeviceId: "a", result: .success(["old"]))
        let refresh = state.begin(deviceId: "a")
        guard case .loading = state.codegraff, case .loading = state.accounts else {
            return XCTFail("Old values must be cleared while refreshing")
        }
        state.finishCodegraff(refresh, currentDeviceId: "a", result: .failure(offline))
        state.finishAccounts(refresh, currentDeviceId: "a", result: .failure(offline))
        guard case .failed = state.codegraff, case .failed = state.accounts else {
            return XCTFail("Failed refresh must not silently reuse old values")
        }
    }

    func testHostChangeRejectsRepliesBeforeReplacementTaskStarts() {
        var state = State()
        let request = state.begin(deviceId: "a")
        XCTAssertFalse(state.finishCodegraff(request, currentDeviceId: "b", result: .success(42)))
        XCTAssertFalse(state.finishAccounts(request, currentDeviceId: "b", result: .success(["private-a"])))
        XCTAssertFalse(state.finishAccounts(request, currentDeviceId: nil, result: .success(["private-a"])))
        let replacement = state.begin(deviceId: "b")
        XCTAssertEqual(state.deviceId, "b")
        guard case .loading = state.codegraff, case .loading = state.accounts else {
            return XCTFail("A new host must not inherit values")
        }
        XCTAssertFalse(state.finishAccounts(request, currentDeviceId: "b", result: .success(["private-a"])))
        XCTAssertTrue(state.finishAccounts(replacement, currentDeviceId: "b", result: .success([])))
    }

    func testSameHostAndRoundTripHostRepliesCannotReplaceNewerRequest() {
        var state = State()
        let old = state.begin(deviceId: "a")
        let newer = state.begin(deviceId: "a")
        XCTAssertFalse(state.finishCodegraff(old, currentDeviceId: "a", result: .success(1)))
        XCTAssertTrue(state.finishCodegraff(newer, currentDeviceId: "a", result: .success(2)))
        _ = state.begin(deviceId: "b")
        let current = state.begin(deviceId: "a")
        XCTAssertFalse(state.finishAccounts(newer, currentDeviceId: "a", result: .success(["old-a"])))
        XCTAssertTrue(state.finishAccounts(current, currentDeviceId: "a", result: .success(["new-a"])))
    }

    func testCancelledRequestCannotPublishAndDoesNotStrandLoadingSibling() {
        var state = State()
        let request = state.begin(deviceId: "a")
        state.finishCodegraff(request, currentDeviceId: "a", result: .success(42))
        XCTAssertFalse(state.finishAccounts(request, currentDeviceId: "a", result: .success(["cancelled"]),
                                            cancelled: true))
        guard case .loaded(42) = state.codegraff else { return XCTFail("Completed sibling should stay visible") }
        guard case .idle = state.accounts else { return XCTFail("Cancelled section must allow retry, not spin") }
        XCTAssertFalse(state.finishAccounts(request, currentDeviceId: "a", result: .success(["late"])))
        let retry = state.begin(deviceId: "a")
        XCTAssertFalse(state.finishCodegraff(request, currentDeviceId: "a", result: .failure(offline),
                                             cancelled: true))
        XCTAssertTrue(state.finishCodegraff(retry, currentDeviceId: "a", result: .success(nil)))
    }

    func testClearingInvalidatesAllOutstandingReplies() {
        var state = State()
        let request = state.begin(deviceId: "a")
        state.clear()
        XCTAssertNil(state.deviceId)
        XCTAssertFalse(state.finishCodegraff(request, currentDeviceId: "a", result: .success(42)))
        XCTAssertFalse(state.finishAccounts(request, currentDeviceId: "a", result: .success(["late"])))
        guard case .idle = state.codegraff, case .idle = state.accounts else {
            return XCTFail("No-host state should have no results")
        }
    }
}
