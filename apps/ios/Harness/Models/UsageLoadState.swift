import Foundation

/// Independent endpoint results: only a successful response can mean signed out
/// or empty. Each refresh clears old values rather than presenting stale usage.
struct UsageLoadFailure: Error {
    let message: String
}

enum UsageSectionState<Value> {
    case idle
    case loading
    case loaded(Value)
    case failed(String)
}

/// Both endpoints share a host/request identity, but finish independently.
/// Checking the current host also covers the interval before SwiftUI starts its
/// replacement task. A generation prevents A → B → A and same-host retry races.
struct UsageLoadState<Codegraff, Accounts> {
    struct Request: Equatable {
        let deviceId: String
        let generation: UInt64
    }

    private(set) var deviceId: String?
    private(set) var codegraff: UsageSectionState<Codegraff> = .idle
    private(set) var accounts: UsageSectionState<Accounts> = .idle
    private var generation: UInt64 = 0
    private var request: Request?

    mutating func begin(deviceId: String) -> Request {
        generation &+= 1
        let request = Request(deviceId: deviceId, generation: generation)
        self.deviceId = deviceId
        self.request = request
        codegraff = .loading
        accounts = .loading
        return request
    }

    @discardableResult
    mutating func finishCodegraff(_ request: Request, currentDeviceId: String?,
                                 result: Result<Codegraff, UsageLoadFailure>, cancelled: Bool = false) -> Bool {
        guard accepts(request, currentDeviceId: currentDeviceId, cancelled: cancelled) else { return false }
        switch result {
        case .success(let value): codegraff = .loaded(value)
        case .failure(let error): codegraff = .failed(error.message)
        }
        return true
    }

    @discardableResult
    mutating func finishAccounts(_ request: Request, currentDeviceId: String?,
                                result: Result<Accounts, UsageLoadFailure>, cancelled: Bool = false) -> Bool {
        guard accepts(request, currentDeviceId: currentDeviceId, cancelled: cancelled) else { return false }
        switch result {
        case .success(let value): accounts = .loaded(value)
        case .failure(let error): accounts = .failed(error.message)
        }
        return true
    }

    private mutating func accepts(_ request: Request, currentDeviceId: String?, cancelled: Bool) -> Bool {
        guard self.request == request, deviceId == request.deviceId,
              currentDeviceId == request.deviceId else { return false }
        guard !cancelled else {
            // A cancelled refresh must neither publish its replies nor strand
            // a visible section on a spinner. A completed sibling stays useful.
            self.request = nil
            if case .loading = codegraff { codegraff = .idle }
            if case .loading = accounts { accounts = .idle }
            return false
        }
        return true
    }

    mutating func clear() {
        generation &+= 1
        request = nil
        deviceId = nil
        codegraff = .idle
        accounts = .idle
    }
}
