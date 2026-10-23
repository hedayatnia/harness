import Foundation

/// A ref request belongs to a specific repository and request generation.
/// Completed empty/failure states are not loading, and stale replies never win.
struct RefCatalogState {
    struct Request: Equatable {
        let scope: String
        let generation: UInt64
    }

    private(set) var scope: String?
    private(set) var items: [RepoRef] = []
    private(set) var loading = false
    private(set) var error: String?
    private var generation: UInt64 = 0

    mutating func begin(scope: String) -> Request {
        generation &+= 1
        if self.scope != scope { items = [] }
        self.scope = scope
        loading = true
        error = nil
        return Request(scope: scope, generation: generation)
    }

    @discardableResult
    mutating func finish(_ request: Request, items: [RepoRef]?, failure: String) -> Bool {
        guard request.scope == scope, request.generation == generation else { return false }
        loading = false
        if let items {
            self.items = items
            error = nil
        } else {
            error = failure
        }
        return true
    }

    mutating func clear() {
        generation &+= 1
        scope = nil
        items = []
        loading = false
        error = nil
    }
}
