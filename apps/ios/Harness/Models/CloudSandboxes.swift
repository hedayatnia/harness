// Cloud sandboxes as a host engine reports them (`CodegraffSandboxes`,
// crates/engine/src/codegraff_auth/sandboxes.rs, camelCase on the wire). The
// phone asks one of its computers over the relay, the same way Settings →
// Usage does: the computer's engine is signed in to Codegraff and talks to the
// gateway, so states arrive normalised and terminated rows are already gone.
//
// Every text and button rule mirrors the desktop's
// crates/ui/src/codegraff_sandboxes.rs, and apps/parity/vectors/cloud-sandbox.json
// pins the cases both must agree on (rows, rates, sizes, the running summary).

import Foundation

struct CodegraffSandbox: Decodable, Equatable, Identifiable {
    var id: String = ""
    /// `fleet` or `daytona`.
    var provider: String = ""
    var name: String?
    /// `running` | `starting` | `paused` | `stopping` | `error` (the engine's words).
    var state: String = ""
    /// RFC 3339 text or unix seconds.
    var expiresAt: JSONValue?
    /// Harness was set up in it, so it can show up as a device.
    var harness: Bool = false
    /// The name its device registers under; the gateway gives fleet sandboxes no name.
    var deviceName: String?
    /// The gateway renews its window until it is turned off. nil when the gateway
    /// doesn't know the setting (read as off).
    var keepRunning: Bool?
    /// Why the last renewal failed; it pauses when its current window ends.
    var renewError: String?
    /// Its machine, when the gateway says.
    var cpu: Double?
    var memoryGiB: Double?
    var diskGiB: Double?
    /// What it costs per hour while running, in micro-USD.
    var pricePerHourMicroUsd: Int64?

    enum CodingKeys: String, CodingKey {
        case id, provider, name, state, expiresAt, harness, deviceName, keepRunning, renewError
        case cpu, memoryGiB, diskGiB, pricePerHourMicroUsd
    }

    init(id: String = "", provider: String = "", name: String? = nil, state: String = "",
         expiresAt: JSONValue? = nil, harness: Bool = false, deviceName: String? = nil,
         keepRunning: Bool? = nil, renewError: String? = nil, cpu: Double? = nil,
         memoryGiB: Double? = nil, diskGiB: Double? = nil, pricePerHourMicroUsd: Int64? = nil) {
        self.id = id
        self.provider = provider
        self.name = name
        self.state = state
        self.expiresAt = expiresAt
        self.harness = harness
        self.deviceName = deviceName
        self.keepRunning = keepRunning
        self.renewError = renewError
        self.cpu = cpu
        self.memoryGiB = memoryGiB
        self.diskGiB = diskGiB
        self.pricePerHourMicroUsd = pricePerHourMicroUsd
    }

    /// Tolerant like the desktop's serde defaults: a missing or mistyped field is its default.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = (try? c.decodeIfPresent(String.self, forKey: .id)) ?? ""
        provider = (try? c.decodeIfPresent(String.self, forKey: .provider)) ?? ""
        name = try? c.decodeIfPresent(String.self, forKey: .name)
        state = (try? c.decodeIfPresent(String.self, forKey: .state)) ?? ""
        expiresAt = (try? c.decodeIfPresent(JSONValue.self, forKey: .expiresAt)).flatMap { $0 == .null ? nil : $0 }
        harness = (try? c.decodeIfPresent(Bool.self, forKey: .harness)) ?? false
        deviceName = try? c.decodeIfPresent(String.self, forKey: .deviceName)
        keepRunning = try? c.decodeIfPresent(Bool.self, forKey: .keepRunning)
        renewError = try? c.decodeIfPresent(String.self, forKey: .renewError)
        cpu = try? c.decodeIfPresent(Double.self, forKey: .cpu)
        memoryGiB = try? c.decodeIfPresent(Double.self, forKey: .memoryGiB)
        diskGiB = try? c.decodeIfPresent(Double.self, forKey: .diskGiB)
        pricePerHourMicroUsd = try? c.decodeIfPresent(Int64.self, forKey: .pricePerHourMicroUsd)
    }
}

/// One size a new sandbox can be.
struct PersonalSize: Decodable, Equatable, Identifiable {
    /// `standard`, `large`, …
    var id: String = ""
    var cpu: Int = 0
    var memoryGiB: Int = 0
    var diskGiB: Int = 0
    var pricePerHourMicroUsd: Int64?
    var isDefault: Bool = false

    enum CodingKeys: String, CodingKey {
        case id, cpu, memoryGiB, diskGiB, pricePerHourMicroUsd
        case isDefault = "default"
    }

    init(id: String, cpu: Int, memoryGiB: Int, diskGiB: Int = 0,
         pricePerHourMicroUsd: Int64? = nil, isDefault: Bool = false) {
        self.id = id
        self.cpu = cpu
        self.memoryGiB = memoryGiB
        self.diskGiB = diskGiB
        self.pricePerHourMicroUsd = pricePerHourMicroUsd
        self.isDefault = isDefault
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = (try? c.decodeIfPresent(String.self, forKey: .id)) ?? ""
        cpu = (try? c.decodeIfPresent(Int.self, forKey: .cpu)) ?? 0
        memoryGiB = (try? c.decodeIfPresent(Int.self, forKey: .memoryGiB)) ?? 0
        diskGiB = (try? c.decodeIfPresent(Int.self, forKey: .diskGiB)) ?? 0
        pricePerHourMicroUsd = try? c.decodeIfPresent(Int64.self, forKey: .pricePerHourMicroUsd)
        isDefault = (try? c.decodeIfPresent(Bool.self, forKey: .isDefault)) ?? false
    }
}

/// `enabled: false` means the computer's Harness doesn't offer cloud sandboxes.
struct CodegraffSandboxes: Decodable, Equatable {
    var enabled: Bool = false
    var sandboxes: [CodegraffSandbox] = []
    /// The sizes a new sandbox can be; empty on gateways without sizes (no size choice shown).
    var sizes: [PersonalSize] = []

    enum CodingKeys: String, CodingKey { case enabled, sandboxes, sizes }

    init(enabled: Bool = false, sandboxes: [CodegraffSandbox] = [], sizes: [PersonalSize] = []) {
        self.enabled = enabled
        self.sandboxes = sandboxes
        self.sizes = sizes
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        enabled = (try? c.decodeIfPresent(Bool.self, forKey: .enabled)) ?? false
        sandboxes = (try? c.decodeIfPresent([CodegraffSandbox].self, forKey: .sandboxes)) ?? []
        sizes = (try? c.decodeIfPresent([PersonalSize].self, forKey: .sizes)) ?? []
    }
}

/// `CodegraffSandboxAction`'s `action` words.
enum CloudSandboxAction: String, CaseIterable {
    case start
    case stop
    case delete
    case keepRunningOn = "keep-running-on"
    case keepRunningOff = "keep-running-off"
}

/// What the always-visible summary says while sandboxes cost credit.
struct CloudRunningSummary: Equatable {
    /// "Cloud sandbox running · $0.10/h" / "2 cloud sandboxes running · $0.20/h".
    var label: String
    /// "Billed per second until you turn it off", "Pauses in 24 min", or a failed renewal.
    var detail: String
    /// The detail is a problem (a failed renewal): show it in the warning color.
    var warning: Bool
    /// What "Turn off" pauses: every running sandbox.
    var runningIds: [String]
}

/// Pure row text and button rules (crates/ui/src/codegraff_sandboxes.rs).
enum CloudSandboxRules {
    /// What a create says it costs, under the size choice.
    static let createCopy = "Keeps running until you turn it off. Billed per second from your Codegraff credit."
    /// The create row's description.
    static let createDescription = "A machine that runs Harness for you and shows up in your devices. \(createCopy)"
    /// The turn-off button's label, its in-progress label, and what it does.
    static let turnOff = (label: "Turn off", busy: "Turning off…")
    static let turnOffHint = "Pauses it. Memory and files are kept; billing stops."
    static let keepRunningHint = "Keep it running until you turn it off, billed per second from your Codegraff credit."
    static let billed = "Billed per second until you turn it off"

    /// An hourly price: "$0.10/h", "$0.125/h", "$1.50/h".
    static func formatRate(_ microUsd: Int64) -> String {
        let dollars = Double(max(microUsd, 0)) / 1_000_000
        var text = String(format: "%.3f", dollars)
        // Cents always show; a third decimal only when it says something.
        if text.hasSuffix("0") { text.removeLast() }
        return "$\(text)/h"
    }

    /// "4 GB", "0.5 GB".
    static func formatGB(_ gib: Double) -> String {
        if gib.rounded(.towardZero) == gib {
            return String(format: "%.0f GB", gib)
        }
        return "\(number((gib * 10).rounded() / 10)) GB"
    }

    /// "2 vCPU · 4 GB" when both are known.
    static func machineSpec(cpu: Double?, memoryGiB: Double?) -> String? {
        guard let cpu, cpu > 0, let memory = memoryGiB, memory > 0 else { return nil }
        let cpuText = cpu.rounded(.towardZero) == cpu ? String(format: "%.0f", cpu) : number(cpu)
        return "\(cpuText) vCPU · \(formatGB(memory))"
    }

    /// "Standard", "Large", or the id capitalised.
    static func sizeTitle(_ size: PersonalSize) -> String {
        capitalize(size.id.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    /// "2 vCPU · 4 GB · 40 GB · $0.10/h".
    static func sizeDetail(_ size: PersonalSize) -> String {
        var parts = [machineSpec(cpu: Double(size.cpu), memoryGiB: Double(size.memoryGiB)) ?? ""]
        if size.diskGiB > 0 { parts.append(formatGB(Double(size.diskGiB))) }
        if let price = size.pricePerHourMicroUsd { parts.append(formatRate(price)) }
        return parts.filter { !$0.isEmpty }.joined(separator: " · ")
    }

    /// The size a create uses: the one picked, else the default (else the first).
    /// nil when the gateway offers no sizes.
    static func selectedSize(_ sizes: [PersonalSize], picked: String?) -> PersonalSize? {
        if let picked, let size = sizes.first(where: { $0.id == picked }) { return size }
        return sizes.first(where: \.isDefault) ?? sizes.first
    }

    private static func isLive(_ sandbox: CodegraffSandbox) -> Bool {
        sandbox.state == "running" || sandbox.state == "starting"
    }

    /// What the running (and starting) sandboxes cost together per hour, when every one of them says.
    static func runningRate(_ list: CodegraffSandboxes) -> Int64? {
        let live = list.sandboxes.filter(isLive)
        guard !live.isEmpty else { return nil }
        var total: Int64 = 0
        for sandbox in live {
            guard let price = sandbox.pricePerHourMicroUsd else { return nil }
            total += price
        }
        return total
    }

    static func title(_ sandbox: CodegraffSandbox) -> String {
        [sandbox.name, sandbox.deviceName]
            .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty } ?? "Cloud sandbox"
    }

    /// Only our own (fleet) sandboxes have Harness set up in them: a paused one is started, and a
    /// running one that never got Harness (a create that half-failed) is set up again.
    static func canStart(_ sandbox: CodegraffSandbox) -> Bool {
        sandbox.provider == "fleet"
            && (sandbox.state == "paused" || (sandbox.state == "running" && !sandbox.harness))
    }

    /// The start button's label and its in-progress label.
    static func startLabels(_ sandbox: CodegraffSandbox) -> (label: String, busy: String) {
        sandbox.state == "running" ? ("Set up Harness", "Setting up…") : ("Start", "Starting…")
    }

    static func canStop(_ sandbox: CodegraffSandbox) -> Bool {
        sandbox.state == "running"
    }

    /// A running fleet sandbox the gateway says will pause when its window ends can be kept
    /// running again. Gateways that don't know the setting say nothing, so nothing is offered.
    static func canKeepRunning(_ sandbox: CodegraffSandbox) -> Bool {
        sandbox.provider == "fleet" && sandbox.state == "running" && sandbox.keepRunning == false
    }

    private static func keepsRunning(_ sandbox: CodegraffSandbox) -> Bool {
        sandbox.keepRunning == true && renewProblem(sandbox) == nil
    }

    /// The renewal failure in the app's words.
    private static func renewProblem(_ sandbox: CodegraffSandbox) -> String? {
        guard let error = sandbox.renewError?.trimmingCharacters(in: .whitespacesAndNewlines),
              !error.isEmpty else { return nil }
        switch error {
        case "insufficient_credit", "insufficient_credits": return "Out of credit"
        case "key_budget": return "Key budget reached"
        default: return "Couldn't renew"
        }
    }

    /// When a running sandbox stops on its own; nil when unknown.
    private static func runningOutlook(_ sandbox: CodegraffSandbox, now: Date) -> String? {
        let lease = sandbox.expiresAt.flatMap { leaseRemaining($0, now: now) }
        if let problem = renewProblem(sandbox) {
            if let lease { return "\(problem) — \(lease)" }
            return problem
        }
        if keepsRunning(sandbox) { return "keeps running until you turn it off" }
        return lease
    }

    /// "Running · Harness ready · keeps running until you turn it off".
    static func detail(_ sandbox: CodegraffSandbox, now: Date) -> String {
        var parts: [String]
        switch sandbox.state {
        case "running": parts = ["Running"]
        case "starting": parts = ["Starting…"]
        // A fleet pause keeps the machine's memory and disk.
        case "paused": parts = [sandbox.provider == "fleet" ? "Paused — memory and files kept" : "Paused"]
        case "stopping": parts = ["Stopping…"]
        case "error": parts = ["Error"]
        case "", "unknown": parts = ["Unknown state"]
        default: parts = [sandbox.state]
        }
        if sandbox.state == "running" {
            let spec = machineSpec(cpu: sandbox.cpu, memoryGiB: sandbox.memoryGiB)
            let rate = sandbox.pricePerHourMicroUsd.map(formatRate)
            // With its machine and rate to say, a ready sandbox goes without "Harness ready".
            if !sandbox.harness {
                parts.append("Harness not set up")
            } else if spec == nil && rate == nil {
                parts.append("Harness ready")
            }
            if let spec { parts.append(spec) }
            if let rate { parts.append(rate) }
            if let outlook = runningOutlook(sandbox, now: now) { parts.append(outlook) }
        }
        return parts.joined(separator: " · ")
    }

    /// nil while nothing is running (or starting). Every provider counts, since every running
    /// sandbox is billed.
    static func runningSummary(_ list: CodegraffSandboxes, now: Date) -> CloudRunningSummary? {
        guard list.enabled else { return nil }
        let live = list.sandboxes.filter(isLive)
        guard !live.isEmpty else { return nil }
        var label: String
        switch live.count {
        case 1 where live[0].state == "starting": label = "Cloud sandbox starting"
        case 1: label = "Cloud sandbox running"
        default: label = "\(live.count) cloud sandboxes running"
        }
        if let rate = runningRate(list) { label += " · \(formatRate(rate))" }
        let running = live.filter(canStop)
        // The most pressing word wins: a failed renewal, then a window about to end, then billing.
        let problem = running.first { renewProblem($0) != nil }.flatMap { runningOutlook($0, now: now) }
        var detail = billed
        var warning = false
        if let problem {
            detail = problem
            warning = true
        } else if let lease = running.first(where: { !keepsRunning($0) }).flatMap({ runningOutlook($0, now: now) }),
                  lease.hasPrefix("pauses") {
            detail = capitalize(lease)
        }
        return CloudRunningSummary(label: label, detail: detail, warning: warning,
                                   runningIds: running.map(\.id))
    }

    /// The short form: "Cloud sandbox running — billed per second", or the summary's own news.
    static func compactTooltip(_ summary: CloudRunningSummary) -> String {
        summary.detail == billed
            ? "\(summary.label) — billed per second"
            : "\(summary.label) — \(summary.detail)"
    }

    /// Anything running, starting or stopping changes on its own, so the list is re-read while one is.
    static func needsRefresh(_ list: CodegraffSandboxes) -> Bool {
        list.enabled && list.sandboxes.contains { ["running", "starting", "stopping"].contains($0.state) }
    }

    /// The desktop re-reads a changing list this often (state/cloud_sandboxes.rs REFRESH_EVERY).
    static let refreshEverySeconds: UInt64 = 60

    // MARK: helpers

    static func capitalize(_ text: String) -> String {
        guard let first = text.first else { return "" }
        return first.uppercased() + text.dropFirst()
    }

    /// Shortest decimal text of a number ("0.5", "1.25"), like Rust's `{}` for f64.
    private static func number(_ value: Double) -> String {
        var text = "\(value)"
        if text.hasSuffix(".0") { text.removeLast(2) }
        return text
    }

    static func parseMoment(_ text: String) -> Date? {
        let plain = ISO8601DateFormatter()
        plain.formatOptions = [.withInternetDateTime]
        if let date = plain.date(from: text) { return date }
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractional.date(from: text)
    }

    private static func leaseRemaining(_ expiresAt: JSONValue, now: Date) -> String? {
        let at: Date
        switch expiresAt {
        case .string(let text):
            guard let date = parseMoment(text) else { return nil }
            at = date
        case .int(let seconds):
            at = Date(timeIntervalSince1970: TimeInterval(seconds))
        default:
            return nil
        }
        // Whole minutes, truncated toward zero (chrono's num_minutes).
        let minutes = Int(at.timeIntervalSince(now) / 60)
        switch minutes {
        case ..<0: return "lease ended"
        case 0: return "pauses in under a minute"
        case 1..<60: return "pauses in \(minutes) min"
        default: return "pauses in \(minutes / 60)h \(minutes % 60)m"
        }
    }
}
