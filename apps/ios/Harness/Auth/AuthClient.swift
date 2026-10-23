// Edge auth client — CodeGraff OAuth exchange, refresh, and personal workspace.
// - Dev (AUTH_MODE=dev edge): the bearer string IS the user id; "user@org"
//   supplies a fake org claim.

import Foundation
import UIKit

struct AuthUser: Codable, Equatable {
    var id: String
    var email: String?
    var firstName: String?
    var lastName: String?
}

struct AuthOrg: Codable, Identifiable, Equatable {
    var id: String
    var organizationId: String
    var name: String
}

struct AuthTokens: Codable, Equatable {
    var accessToken: String
    var refreshToken: String
}

enum AuthError: LocalizedError {
    case http(Int, String)
    case invalidResponse

    var errorDescription: String? {
        switch self {
        case .http(let code, let body): return "Auth failed (\(code)): \(body)"
        case .invalidResponse: return "Unexpected auth response"
        }
    }
}

struct AuthClient {
    var baseURL: URL

    func exchange(code: String, codeVerifier: String, redirectURI: String,
                  nonce: String) async throws -> (AuthUser, AuthTokens) {
        struct Response: Codable {
            var user: AuthUser
            var accessToken: String
            var refreshToken: String
        }
        let r: Response = try await post("auth/exchange", body: [
            "code": code, "codeVerifier": codeVerifier,
            "redirectUri": redirectURI, "nonce": nonce,
        ])
        return (r.user, AuthTokens(accessToken: r.accessToken, refreshToken: r.refreshToken))
    }

    func refresh(refreshToken: String, organizationId: String? = nil) async throws -> AuthTokens {
        var body: [String: String] = ["refreshToken": refreshToken]
        if let organizationId { body["organizationId"] = organizationId }
        return try await post("auth/refresh", body: body)
    }

    func orgs(accessToken: String) async throws -> [AuthOrg] {
        struct Response: Codable { var orgs: [AuthOrg] }
        var request = URLRequest(url: baseURL.appending(path: "auth/orgs"))
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        let (data, response) = try await URLSession.shared.data(for: request)
        try Self.check(data: data, response: response)
        return try JSONDecoder().decode(Response.self, from: data).orgs
    }

    /// Both halves of the sign-in go along: the edge refuses a delete unless
    /// the access token and refresh credential belong to the same person.
    func deleteAccount(accessToken: String, refreshToken: String) async throws -> AccountDeletion {
        var request = URLRequest(url: baseURL.appending(path: "auth/account/delete"))
        request.httpMethod = "POST"
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(["refreshToken": refreshToken, "confirm": "delete"])
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse else { throw AuthError.invalidResponse }
        return AccountDeletion(status: http.statusCode, data: data)
    }

    private func post<T: Decodable>(_ path: String, body: [String: String]) async throws -> T {
        var request = URLRequest(url: baseURL.appending(path: path))
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(body)
        let (data, response) = try await URLSession.shared.data(for: request)
        try Self.check(data: data, response: response)
        return try JSONDecoder().decode(T.self, from: data)
    }

    private static func check(data: Data, response: URLResponse) throws {
        guard let http = response as? HTTPURLResponse else { throw AuthError.invalidResponse }
        guard (200..<300).contains(http.statusCode) else {
            throw AuthError.http(http.statusCode, String(data: data, encoding: .utf8) ?? "")
        }
    }
}

// MARK: - Keychain storage

enum Keychain {
    private static let service = "harness.codegraff.ios"

    /// What a read found. `unavailable` is not `missing`: before the first unlock after a restart (iOS may
    /// launch the app in the background then), or for an item still under an older accessibility class
    /// while the phone is locked, the Keychain cannot be read, and a sign-in that is there must not be
    /// taken for one that isn't.
    enum Read: Equatable {
        case found(String)
        case missing
        case unavailable
    }

    /// Writes the Keychain refused (the phone was locked), newest value per key, written again as soon as
    /// it can be. A rotated refresh token that never reaches the Keychain is a spent credential on the
    /// next launch, and a signed-out phone.
    private static let pending = PendingWrites()

    /// Update in place. Delete-then-add left a gap where a kill (or a failed add) lost the item, and a
    /// lost refresh token is a signed-out phone. `false` when the value is only held in memory for now.
    @discardableResult
    static func save(_ value: String, key: String) -> Bool {
        if write(value, key: key) {
            pending.remove(key, ifValue: value)
            return true
        }
        pending.set(value, for: key)
        watchForUnlock()
        return false
    }

    /// The pair a sign-in is made of. The refresh token goes first: it is the one that cannot be
    /// replaced, while a missing access token is simply refreshed.
    static func saveTokens(_ tokens: AuthTokens) {
        save(tokens.refreshToken, key: "codegraffRefreshToken")
        save(tokens.accessToken, key: "codegraffAccessToken")
    }

    static func load(key: String) -> String? {
        if case .found(let value) = read(key: key) { return value }
        return nil
    }

    static func read(key: String) -> Read {
        if let held = pending.value(for: key) { return .found(held) }
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: AnyObject?
        switch SecItemCopyMatching(query as CFDictionary, &result) {
        case errSecSuccess:
            guard let data = result as? Data, let value = String(data: data, encoding: .utf8) else { return .missing }
            return .found(value)
        case errSecItemNotFound:
            return .missing
        default:
            return .unavailable
        }
    }

    static func delete(key: String) {
        pending.remove(key)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
        ]
        SecItemDelete(query as CFDictionary)
    }

    /// Write what the Keychain refused earlier. Called on unlock and when the app comes to the front.
    static func flushPending() {
        for (key, value) in pending.all() where write(value, key: key) {
            pending.remove(key, ifValue: value)
        }
    }

    private static func write(_ value: String, key: String) -> Bool {
        let data = Data(value.utf8)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
        ]
        // The accessibility class goes along on every update too: items saved by older builds under the
        // default (readable only while unlocked) move to after-first-unlock, so a refresh while the
        // phone is locked can read and replace them.
        let status = SecItemUpdate(query as CFDictionary, [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlock,
        ] as CFDictionary)
        if status == errSecSuccess { return true }
        guard status == errSecItemNotFound else { return false }
        var add = query
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
        return SecItemAdd(add as CFDictionary, nil) == errSecSuccess
    }

    private static func watchForUnlock() {
        DispatchQueue.main.async {
            guard unlockObserver == nil else { return }
            unlockObserver = NotificationCenter.default.addObserver(
                forName: UIApplication.protectedDataDidBecomeAvailableNotification, object: nil, queue: .main
            ) { _ in flushPending() }
        }
    }

    /// Main-queue only.
    nonisolated(unsafe) private static var unlockObserver: NSObjectProtocol?

    private final class PendingWrites: @unchecked Sendable {
        private let lock = NSLock()
        private var values: [String: String] = [:]

        func set(_ value: String, for key: String) { lock.withLock { values[key] = value } }
        func value(for key: String) -> String? { lock.withLock { values[key] } }
        func all() -> [(String, String)] { lock.withLock { values.map { ($0.key, $0.value) } } }
        func remove(_ key: String) { _ = lock.withLock { values.removeValue(forKey: key) } }
        /// A newer value saved meanwhile stays pending.
        func remove(_ key: String, ifValue value: String) {
            lock.withLock { if values[key] == value { values.removeValue(forKey: key) } }
        }
    }
}
