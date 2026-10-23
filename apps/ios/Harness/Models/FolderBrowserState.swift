import Foundation

/// Folder paths and Git metadata belong to a particular host and successful
/// request. A reply from a superseded request must never mutate that selection.
struct FolderBrowserState {
    struct Selection {
        let deviceId: String
        let listing: FolderListing
        let isRepo: Bool
    }

    struct Request: Equatable {
        let id = UUID()
        let deviceId: String
        let path: String?
        let isRepo: Bool
    }

    private(set) var deviceId: String?
    private(set) var pendingRequest: Request?
    private(set) var error: String?
    private var selection: Selection?

    var isLoading: Bool { pendingRequest != nil }

    mutating func selectDevice(_ id: String?) {
        guard deviceId != id else { return }
        deviceId = id
        pendingRequest = nil
        selection = nil
        error = nil
    }

    /// Called synchronously by the navigation action, before starting its Task,
    /// so creation is blocked even before the new RPC gets scheduled.
    mutating func begin(path: String?, isRepo: Bool = false) -> Request? {
        guard let deviceId else { return nil }
        let request = Request(deviceId: deviceId, path: path, isRepo: isRepo)
        pendingRequest = request
        error = nil
        return request
    }

    /// The view's current host is checked as well: its device list can change
    /// before SwiftUI starts the replacement host task.
    @discardableResult
    mutating func finish(_ request: Request, listing: FolderListing?,
                         currentDeviceId: String?, cancelled: Bool = false,
                         failureMessage: String) -> Bool {
        guard pendingRequest == request, deviceId == request.deviceId,
              currentDeviceId == request.deviceId else { return false }
        pendingRequest = nil
        guard !cancelled else { return false }
        if let listing {
            selection = Selection(deviceId: request.deviceId, listing: listing, isRepo: request.isRepo)
        } else {
            // Keep the last successful path AND its Git flag when navigation
            // fails. Only a successful reply may replace either value.
            error = failureMessage
        }
        return true
    }

    mutating func cancelPendingRequest() {
        pendingRequest = nil
    }

    func selection(for currentDeviceId: String?) -> Selection? {
        guard let currentDeviceId, deviceId == currentDeviceId,
              selection?.deviceId == currentDeviceId else { return nil }
        return selection
    }

    func creationTarget(for currentDeviceId: String?) -> Selection? {
        guard !isLoading else { return nil }
        return selection(for: currentDeviceId)
    }
}
