import XCTest
@testable import Harness

final class FolderBrowserStateTests: XCTestCase {
    private func listing(_ path: String) -> FolderListing {
        FolderListing(path: path, entries: [], truncated: false)
    }

    @discardableResult
    private func reply(_ request: FolderBrowserState.Request, path: String?,
                       host: String?, cancelled: Bool = false,
                       to state: inout FolderBrowserState) -> Bool {
        state.finish(request, listing: path.map(listing), currentDeviceId: host,
                     cancelled: cancelled, failureMessage: "Folder unavailable")
    }

    private func openedRepo() -> FolderBrowserState {
        var state = FolderBrowserState()
        state.selectDevice("host-a")
        let request = state.begin(path: "/projects/repo", isRepo: true)!
        reply(request, path: "/projects/repo", host: "host-a", to: &state)
        return state
    }

    func testOutOfOrderHostReplyCannotReplaceNewHostListingOrMetadata() {
        var state = FolderBrowserState()
        state.selectDevice("host-a")
        let a = state.begin(path: "/a/repo", isRepo: true)!
        state.selectDevice("host-b")
        let b = state.begin(path: nil)!
        XCTAssertTrue(reply(b, path: "/b/home", host: "host-b", to: &state))
        XCTAssertFalse(reply(a, path: "/a/repo", host: "host-b", to: &state))
        let target = state.creationTarget(for: "host-b")
        XCTAssertEqual(target?.deviceId, "host-b")
        XCTAssertEqual(target?.listing.path, "/b/home")
        XCTAssertEqual(target?.isRepo, false)
    }

    func testSwitchingHostImmediatelyInvalidatesExistingListingAndGitFlag() {
        var state = openedRepo()
        state.selectDevice("host-b")
        XCTAssertNil(state.selection(for: "host-a"))
        XCTAssertNil(state.creationTarget(for: "host-b"))
        let request = state.begin(path: nil)!
        reply(request, path: "/home", host: "host-b", to: &state)
        XCTAssertEqual(state.creationTarget(for: "host-b")?.isRepo, false)
    }

    func testReturningToSameHostDoesNotReviveItsOldRequest() {
        var state = FolderBrowserState()
        state.selectDevice("host-a")
        let oldA = state.begin(path: nil)!
        state.selectDevice("host-b")
        state.selectDevice("host-a")
        let newA = state.begin(path: nil)!
        XCTAssertFalse(reply(oldA, path: "/old", host: "host-a", to: &state))
        XCTAssertTrue(state.isLoading)
        XCTAssertNil(state.creationTarget(for: "host-a"))
        reply(newA, path: "/new", host: "host-a", to: &state)
        XCTAssertEqual(state.creationTarget(for: "host-a")?.listing.path, "/new")
    }

    func testOutOfOrderNavigationKeepsLatestPathAndGitMetadata() {
        var state = openedRepo()
        let first = state.begin(path: "/projects/plain", isRepo: false)!
        let second = state.begin(path: "/projects/other-repo", isRepo: true)!
        reply(second, path: "/projects/other-repo", host: "host-a", to: &state)
        XCTAssertFalse(reply(first, path: "/projects/plain", host: "host-a", to: &state))
        let target = state.creationTarget(for: "host-a")
        XCTAssertEqual(target?.listing.path, "/projects/other-repo")
        XCTAssertEqual(target?.isRepo, true)
    }

    func testRepeatedIdenticalNavigationStillHasDistinctRequestOwnership() {
        var state = openedRepo()
        let first = state.begin(path: "/same", isRepo: false)!
        let second = state.begin(path: "/same", isRepo: false)!
        XCTAssertNotEqual(first, second)
        XCTAssertFalse(reply(first, path: "/same", host: "host-a", to: &state))
        XCTAssertTrue(state.isLoading)
        reply(second, path: "/same", host: "host-a", to: &state)
        XCTAssertFalse(state.isLoading)
    }

    func testPendingNavigationBlocksCreationButKeepsLastSuccessfulBreadcrumb() {
        var state = openedRepo()
        let request = state.begin(path: "/projects", isRepo: false)!
        XCTAssertTrue(state.isLoading)
        XCTAssertEqual(state.selection(for: "host-a")?.listing.path, "/projects/repo")
        XCTAssertEqual(state.selection(for: "host-a")?.isRepo, true)
        XCTAssertNil(state.creationTarget(for: "host-a"))
        reply(request, path: "/projects", host: "host-a", to: &state)
        XCTAssertEqual(state.creationTarget(for: "host-a")?.listing.path, "/projects")
        XCTAssertEqual(state.creationTarget(for: "host-a")?.isRepo, false)
    }

    func testFailedUpNavigationPreservesRepoPathAndGitFlag() {
        var state = openedRepo()
        let request = state.begin(path: "/projects", isRepo: false)!
        reply(request, path: nil, host: "host-a", to: &state)
        XCTAssertFalse(state.isLoading)
        XCTAssertEqual(state.error, "Folder unavailable")
        let target = state.creationTarget(for: "host-a")
        XCTAssertEqual(target?.listing.path, "/projects/repo")
        XCTAssertEqual(target?.isRepo, true)
    }

    func testFailedDescentDoesNotMarkPreviousPlainFolderAsRepo() {
        var state = FolderBrowserState()
        state.selectDevice("host-a")
        let home = state.begin(path: nil)!
        reply(home, path: "/home", host: "host-a", to: &state)
        let child = state.begin(path: "/home/repo", isRepo: true)!
        reply(child, path: nil, host: "host-a", to: &state)
        XCTAssertEqual(state.creationTarget(for: "host-a")?.listing.path, "/home")
        XCTAssertEqual(state.creationTarget(for: "host-a")?.isRepo, false)
    }

    func testSupersededFailureOrCancellationCannotClearNewLoadingOrPublishError() {
        var state = openedRepo()
        let old = state.begin(path: "/old")!
        let current = state.begin(path: "/current")!
        XCTAssertFalse(reply(old, path: nil, host: "host-a", to: &state))
        XCTAssertFalse(reply(old, path: "/old", host: "host-a", cancelled: true, to: &state))
        XCTAssertTrue(state.isLoading)
        XCTAssertEqual(state.pendingRequest, current)
        XCTAssertNil(state.error)
        XCTAssertNil(state.creationTarget(for: "host-a"))
    }

    func testCancelledCurrentReplyCannotCommitPathOrGitMetadata() {
        var state = openedRepo()
        let request = state.begin(path: "/projects", isRepo: false)!
        XCTAssertFalse(reply(request, path: "/projects", host: "host-a", cancelled: true, to: &state))
        XCTAssertFalse(state.isLoading)
        XCTAssertNil(state.error)
        XCTAssertEqual(state.creationTarget(for: "host-a")?.listing.path, "/projects/repo")
        XCTAssertEqual(state.creationTarget(for: "host-a")?.isRepo, true)
    }

    func testDismissalInvalidatesUncooperativeLateReply() {
        var state = openedRepo()
        let request = state.begin(path: "/late", isRepo: false)!
        state.cancelPendingRequest()
        XCTAssertFalse(reply(request, path: "/late", host: "host-a", to: &state))
        XCTAssertEqual(state.selection(for: "host-a")?.listing.path, "/projects/repo")
        XCTAssertEqual(state.selection(for: "host-a")?.isRepo, true)
    }

    func testHostChangeBeforeReplacementTaskBlocksDisplayCreationAndCommit() {
        var state = openedRepo()
        let request = state.begin(path: "/late")!
        // The device disappears or changes before .task(id:) can start.
        for currentHost in [nil, "host-b"] as [String?] {
            XCTAssertNil(state.selection(for: currentHost))
            XCTAssertNil(state.creationTarget(for: currentHost))
            XCTAssertFalse(reply(request, path: "/late", host: currentHost, to: &state))
        }
        state.selectDevice(nil)
        XCTAssertFalse(state.isLoading)
        XCTAssertNil(state.begin(path: nil))
        XCTAssertNil(state.creationTarget(for: nil))
    }

    func testInitialFailureCannotCreateAndRetryClearsError() {
        var state = FolderBrowserState()
        state.selectDevice("host-a")
        let initial = state.begin(path: nil)!
        reply(initial, path: nil, host: "host-a", to: &state)
        XCTAssertNil(state.creationTarget(for: "host-a"))
        XCTAssertNotNil(state.error)
        let retry = state.begin(path: nil)!
        XCTAssertNil(state.error)
        reply(retry, path: "/home", host: "host-a", to: &state)
        XCTAssertEqual(state.creationTarget(for: "host-a")?.listing.path, "/home")
    }

    func testCreationTargetCapturesHostPathAndMetadataTogether() {
        var state = openedRepo()
        let captured = state.creationTarget(for: "host-a")!
        state.selectDevice("host-b")
        XCTAssertEqual(captured.deviceId, "host-a")
        XCTAssertEqual(captured.listing.path, "/projects/repo")
        XCTAssertTrue(captured.isRepo)
        XCTAssertNil(state.creationTarget(for: "host-b"))
    }
}
