import XCTest
@testable import Harness

final class RefCatalogStateTests: XCTestCase {
    private let main = RepoRef(name: "main", current: true, worktreePath: nil)

    func testEmptyAndFailedRequestsFinishLoading() {
        var state = RefCatalogState()
        let empty = state.begin(scope: "host/repo")
        XCTAssertTrue(state.loading)
        XCTAssertTrue(state.finish(empty, items: [], failure: "unreachable"))
        XCTAssertFalse(state.loading)
        XCTAssertTrue(state.items.isEmpty)
        XCTAssertNil(state.error)
        let failure = state.begin(scope: "host/repo")
        XCTAssertTrue(state.finish(failure, items: nil, failure: "unreachable"))
        XCTAssertFalse(state.loading)
        XCTAssertEqual(state.error, "unreachable")
        let retry = state.begin(scope: "host/repo")
        XCTAssertNil(state.error)
        state.finish(retry, items: [main], failure: "unused")
        XCTAssertEqual(state.items, [main])
    }

    func testOlderSameScopeReplyCannotReplaceNewerRequest() {
        var state = RefCatalogState()
        let older = state.begin(scope: "host/repo")
        let newer = state.begin(scope: "host/repo")
        XCTAssertFalse(state.finish(older, items: [main], failure: "old"))
        XCTAssertTrue(state.loading)
        state.finish(newer, items: [], failure: "unused")
        XCTAssertTrue(state.items.isEmpty)
        XCTAssertFalse(state.loading)
    }

    func testChangingScopeAndClearingInvalidatesReplies() {
        var state = RefCatalogState()
        let old = state.begin(scope: "host-a/repo")
        state.finish(old, items: [main], failure: "unused")
        let current = state.begin(scope: "host-b/repo")
        XCTAssertTrue(state.items.isEmpty)
        XCTAssertFalse(state.finish(old, items: [main], failure: "old"))
        state.clear()
        XCTAssertFalse(state.finish(current, items: [main], failure: "stale"))
        XCTAssertNil(state.scope)
        XCTAssertTrue(state.items.isEmpty)
    }

    func testFailedRefreshPreservesSameRepositoryRows() {
        var state = RefCatalogState()
        let initial = state.begin(scope: "host/repo")
        state.finish(initial, items: [main], failure: "unused")
        let refresh = state.begin(scope: "host/repo")
        state.finish(refresh, items: nil, failure: "offline")
        XCTAssertEqual(state.items, [main])
        XCTAssertEqual(state.error, "offline")
        XCTAssertFalse(state.loading)
    }
}
