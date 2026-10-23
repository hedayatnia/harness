// GitHub CI runners as a host engine reports them (`CodegraffGithubRunners`,
// crates/engine/src/codegraff_auth/github_runners.rs, camelCase on the wire),
// and the changes the page sends back (`CodegraffGithubRunnerAction`). Like
// cloud sandboxes, the phone asks one of its computers over the relay; that
// computer's engine is signed in to Codegraff and calls the gateway.
//
// Every text and rule mirrors the desktop's crates/ui/src/codegraff_runners.rs.

import Foundation

struct GithubRunners: Decodable, Equatable {
    /// The gateway knows runners at all.
    var available: Bool = false
    /// Runners are open to this account (`false`: coming soon).
    var enabled: Bool = false
    var installUrl: String?
    var installations: [RunnerInstallation] = []
    var recentJobs: [RunnerJob] = []
    var sizes: [RunnerSize] = []

    enum CodingKeys: String, CodingKey { case available, enabled, installUrl, installations, recentJobs, sizes }

    init(available: Bool = false, enabled: Bool = false, installUrl: String? = nil,
         installations: [RunnerInstallation] = [], recentJobs: [RunnerJob] = [], sizes: [RunnerSize] = []) {
        self.available = available
        self.enabled = enabled
        self.installUrl = installUrl
        self.installations = installations
        self.recentJobs = recentJobs
        self.sizes = sizes
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        available = (try? c.decodeIfPresent(Bool.self, forKey: .available)) ?? false
        enabled = (try? c.decodeIfPresent(Bool.self, forKey: .enabled)) ?? false
        installUrl = try? c.decodeIfPresent(String.self, forKey: .installUrl)
        installations = (try? c.decodeIfPresent([RunnerInstallation].self, forKey: .installations)) ?? []
        recentJobs = (try? c.decodeIfPresent([RunnerJob].self, forKey: .recentJobs)) ?? []
        sizes = (try? c.decodeIfPresent([RunnerSize].self, forKey: .sizes)) ?? []
    }
}

struct RunnerInstallation: Decodable, Equatable, Identifiable {
    var id: String { installationId }
    var installationId: String = ""
    var account: String = ""
    /// `User` or `Organization`.
    var accountType: String?
    var suspended: Bool = false
    var monthlyBudgetMicroUsd: Int64?
    var untrustedBudgetMicroUsd: Int64?
    var usedThisMonthMicroUsd: Int64?
    var maxConcurrent: Int?
    var repos: [RunnerRepo] = []

    enum CodingKeys: String, CodingKey {
        case installationId, account, accountType, suspended, monthlyBudgetMicroUsd
        case untrustedBudgetMicroUsd, usedThisMonthMicroUsd, maxConcurrent, repos
    }

    init(installationId: String, account: String, accountType: String? = nil, suspended: Bool = false,
         monthlyBudgetMicroUsd: Int64? = nil, untrustedBudgetMicroUsd: Int64? = nil,
         usedThisMonthMicroUsd: Int64? = nil, maxConcurrent: Int? = nil, repos: [RunnerRepo] = []) {
        self.installationId = installationId
        self.account = account
        self.accountType = accountType
        self.suspended = suspended
        self.monthlyBudgetMicroUsd = monthlyBudgetMicroUsd
        self.untrustedBudgetMicroUsd = untrustedBudgetMicroUsd
        self.usedThisMonthMicroUsd = usedThisMonthMicroUsd
        self.maxConcurrent = maxConcurrent
        self.repos = repos
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        // The engine sends the id as text; a number reads the same.
        if let text = try? c.decodeIfPresent(String.self, forKey: .installationId) {
            installationId = text
        } else if let number = try? c.decodeIfPresent(Int64.self, forKey: .installationId) {
            installationId = String(number)
        }
        account = (try? c.decodeIfPresent(String.self, forKey: .account)) ?? ""
        accountType = try? c.decodeIfPresent(String.self, forKey: .accountType)
        suspended = (try? c.decodeIfPresent(Bool.self, forKey: .suspended)) ?? false
        monthlyBudgetMicroUsd = try? c.decodeIfPresent(Int64.self, forKey: .monthlyBudgetMicroUsd)
        untrustedBudgetMicroUsd = try? c.decodeIfPresent(Int64.self, forKey: .untrustedBudgetMicroUsd)
        usedThisMonthMicroUsd = try? c.decodeIfPresent(Int64.self, forKey: .usedThisMonthMicroUsd)
        maxConcurrent = try? c.decodeIfPresent(Int.self, forKey: .maxConcurrent)
        repos = (try? c.decodeIfPresent([RunnerRepo].self, forKey: .repos)) ?? []
    }
}

struct RunnerRepo: Decodable, Equatable, Identifiable {
    var id: String { fullName }
    var fullName: String = ""
    var enabled: Bool = false
    var defaultSize: String?
    var isPrivate: Bool = false

    enum CodingKeys: String, CodingKey {
        case fullName, enabled, defaultSize
        case isPrivate = "private"
    }

    init(fullName: String, enabled: Bool = false, defaultSize: String? = nil, isPrivate: Bool = false) {
        self.fullName = fullName
        self.enabled = enabled
        self.defaultSize = defaultSize
        self.isPrivate = isPrivate
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        fullName = (try? c.decodeIfPresent(String.self, forKey: .fullName)) ?? ""
        enabled = (try? c.decodeIfPresent(Bool.self, forKey: .enabled)) ?? false
        defaultSize = try? c.decodeIfPresent(String.self, forKey: .defaultSize)
        isPrivate = (try? c.decodeIfPresent(Bool.self, forKey: .isPrivate)) ?? false
    }
}

struct RunnerJob: Decodable, Equatable {
    var repo: String = ""
    var runId: Int64?
    var jobId: Int64?
    var workflow: String?
    var jobName: String?
    var tier: String?
    var size: String?
    var state: String = ""
    var queuedAt: JSONValue?
    var startedAt: JSONValue?
    var completedAt: JSONValue?
    var durationSeconds: Double?
    var costMicroUsd: Int64?
    var conclusion: String?
    var htmlUrl: String?

    enum CodingKeys: String, CodingKey {
        case repo, runId, jobId, workflow, jobName, tier, size, state, queuedAt, startedAt
        case completedAt, durationSeconds, costMicroUsd, conclusion, htmlUrl
    }

    init(repo: String = "", runId: Int64? = nil, jobId: Int64? = nil, workflow: String? = nil,
         jobName: String? = nil, tier: String? = nil, size: String? = nil, state: String = "",
         queuedAt: JSONValue? = nil, startedAt: JSONValue? = nil, completedAt: JSONValue? = nil,
         durationSeconds: Double? = nil, costMicroUsd: Int64? = nil, conclusion: String? = nil,
         htmlUrl: String? = nil) {
        self.repo = repo
        self.runId = runId
        self.jobId = jobId
        self.workflow = workflow
        self.jobName = jobName
        self.tier = tier
        self.size = size
        self.state = state
        self.queuedAt = queuedAt
        self.startedAt = startedAt
        self.completedAt = completedAt
        self.durationSeconds = durationSeconds
        self.costMicroUsd = costMicroUsd
        self.conclusion = conclusion
        self.htmlUrl = htmlUrl
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let moment = { (key: CodingKeys) -> JSONValue? in
            (try? c.decodeIfPresent(JSONValue.self, forKey: key)).flatMap { $0 == .null ? nil : $0 }
        }
        repo = (try? c.decodeIfPresent(String.self, forKey: .repo)) ?? ""
        runId = try? c.decodeIfPresent(Int64.self, forKey: .runId)
        jobId = try? c.decodeIfPresent(Int64.self, forKey: .jobId)
        workflow = try? c.decodeIfPresent(String.self, forKey: .workflow)
        jobName = try? c.decodeIfPresent(String.self, forKey: .jobName)
        tier = try? c.decodeIfPresent(String.self, forKey: .tier)
        size = try? c.decodeIfPresent(String.self, forKey: .size)
        state = (try? c.decodeIfPresent(String.self, forKey: .state)) ?? ""
        queuedAt = moment(.queuedAt)
        startedAt = moment(.startedAt)
        completedAt = moment(.completedAt)
        durationSeconds = try? c.decodeIfPresent(Double.self, forKey: .durationSeconds)
        costMicroUsd = try? c.decodeIfPresent(Int64.self, forKey: .costMicroUsd)
        conclusion = try? c.decodeIfPresent(String.self, forKey: .conclusion)
        htmlUrl = try? c.decodeIfPresent(String.self, forKey: .htmlUrl)
    }
}

struct RunnerSize: Decodable, Equatable, Identifiable {
    var id: String { key }
    var key: String = ""
    var cpu: Double?
    var memoryGiB: Double?
    var diskGiB: Double?
    var pricePerHourMicroUsd: Int64?

    enum CodingKeys: String, CodingKey { case key, cpu, memoryGiB, diskGiB, pricePerHourMicroUsd }

    init(key: String, cpu: Double? = nil, memoryGiB: Double? = nil, diskGiB: Double? = nil,
         pricePerHourMicroUsd: Int64? = nil) {
        self.key = key
        self.cpu = cpu
        self.memoryGiB = memoryGiB
        self.diskGiB = diskGiB
        self.pricePerHourMicroUsd = pricePerHourMicroUsd
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        key = (try? c.decodeIfPresent(String.self, forKey: .key)) ?? ""
        cpu = try? c.decodeIfPresent(Double.self, forKey: .cpu)
        memoryGiB = try? c.decodeIfPresent(Double.self, forKey: .memoryGiB)
        diskGiB = try? c.decodeIfPresent(Double.self, forKey: .diskGiB)
        pricePerHourMicroUsd = try? c.decodeIfPresent(Int64.self, forKey: .pricePerHourMicroUsd)
    }
}

/// One change to an installation, in the engine's `RunnerAction` shape
/// (`action` tag in kebab-case, camelCase fields; absent fields are left as they are).
enum GithubRunnerAction: Equatable {
    case installation(installationId: String, monthlyBudgetMicroUsd: Int64? = nil,
                      untrustedBudgetMicroUsd: Int64? = nil, maxConcurrent: Int? = nil)
    case repo(installationId: String, fullName: String, enabled: Bool? = nil, defaultSize: String? = nil)
    case disconnect(installationId: String)

    /// The RPC params.
    var params: [String: Any] {
        switch self {
        case let .installation(id, monthly, untrusted, max):
            var params: [String: Any] = ["action": "installation", "installationId": id]
            if let monthly { params["monthlyBudgetMicroUsd"] = monthly }
            if let untrusted { params["untrustedBudgetMicroUsd"] = untrusted }
            if let max { params["maxConcurrent"] = max }
            return params
        case let .repo(id, fullName, enabled, defaultSize):
            var params: [String: Any] = ["action": "repo", "installationId": id, "fullName": fullName]
            if let enabled { params["enabled"] = enabled }
            if let defaultSize { params["defaultSize"] = defaultSize }
            return params
        case let .disconnect(id):
            return ["action": "disconnect", "installationId": id]
        }
    }

    /// Which control is busy while it runs ("<installation>:<what>", as the desktop names them).
    var busyKey: String {
        switch self {
        case let .installation(id, monthly, untrusted, _):
            if monthly != nil { return "\(id):\(RunnerBudget.monthly.key)" }
            if untrusted != nil { return "\(id):\(RunnerBudget.untrusted.key)" }
            return "\(id):concurrent"
        case let .repo(id, fullName, _, _): return "\(id):repo:\(fullName)"
        case let .disconnect(id): return "\(id):disconnect"
        }
    }
}

/// Which budget an input edits.
enum RunnerBudget: CaseIterable, Hashable {
    case monthly
    /// Jobs from outside contributors (fork pull requests).
    case untrusted

    var key: String {
        switch self {
        case .monthly: return "monthlyBudgetMicroUsd"
        case .untrusted: return "untrustedBudgetMicroUsd"
        }
    }

    var title: String {
        switch self {
        case .monthly: return "Monthly budget"
        case .untrusted: return "Fork pull request budget"
        }
    }

    func current(_ installation: RunnerInstallation) -> Int64? {
        switch self {
        case .monthly: return installation.monthlyBudgetMicroUsd
        case .untrusted: return installation.untrustedBudgetMicroUsd
        }
    }
}

/// Which panel the page shows.
enum RunnerPanel: Equatable {
    /// The computer is signed out of Codegraff.
    case signedOut
    /// The gateway has no runners yet (404).
    case unavailable
    /// Runners exist but aren't open to this account yet.
    case comingSoon
    /// Open, nothing installed: explain and offer Connect.
    case notConnected
    case connected
}

/// How a job's state reads, and whether it is good, bad or neither.
enum RunnerJobTone: Equatable {
    case working, good, bad, quiet
}

/// Pure reading of the engine's answer and every text the page shows
/// (crates/ui/src/codegraff_runners.rs).
enum GithubRunnerRules {
    /// The most concurrent jobs an installation may run.
    static let maxConcurrent = 8
    /// After Connect, how often the page re-reads while the user installs the app in the browser,
    /// and for how long.
    static let connectPollSeconds: UInt64 = 10
    static let connectWatchSeconds: TimeInterval = 5 * 60
    /// While a job is queued or running.
    static let jobsPollSeconds: UInt64 = 20
    /// How many recent jobs the page lists.
    static let jobsShown = 12

    /// What a workflow says to run on Codegraff.
    static let yamlSnippet = "jobs:\n  build:\n    runs-on: codegraff\n    steps:\n      - uses: actions/checkout@v4\n"

    /// The not-connected explanation.
    static let explainer = "Jobs with runs-on: codegraff run in a fresh Codegraff sandbox per job, billed per second from your Codegraff credit. Pull requests from forks run in a smaller, isolated tier. For public repositories, require approval for workflows from outside contributors."

    static func panel(_ runners: GithubRunners?) -> RunnerPanel {
        guard let runners else { return .signedOut }
        if !runners.available { return .unavailable }
        if !runners.enabled { return .comingSoon }
        if runners.installations.isEmpty { return .notConnected }
        return .connected
    }

    /// The link Connect opens: only an https one.
    static func installURL(_ runners: GithubRunners) -> URL? {
        guard let text = runners.installUrl?.trimmingCharacters(in: .whitespacesAndNewlines),
              text.hasPrefix("https://") else { return nil }
        return URL(string: text)
    }

    /// "$50", "$12.50", "$0.0042" (sub-cent amounts keep their digits).
    static func formatDollars(_ microUsd: Int64) -> String {
        let sign = microUsd < 0 ? "-" : ""
        let micro = microUsd.magnitude
        if micro == 0 { return "$0" }
        if micro % 1_000_000 == 0 { return "\(sign)$\(micro / 1_000_000)" }
        let dollars = Double(micro) / 1_000_000
        if micro < 10_000 {
            var text = String(format: "%.4f", dollars)
            while text.hasSuffix("0") { text.removeLast() }
            return "\(sign)$\(text)"
        }
        return "\(sign)$\(String(format: "%.2f", dollars))"
    }

    /// A budget as typed ("50", "$50", "1,250.50") in micro-USD.
    static func parseBudget(_ text: String) -> Result<Int64, BudgetInputError> {
        var trimmed = Substring(text.trimmingCharacters(in: .whitespacesAndNewlines))
        while trimmed.first == "$" { trimmed = trimmed.dropFirst() }
        let cleaned = String(trimmed.filter { $0 != "," && !$0.isWhitespace })
        if cleaned.isEmpty { return .failure(.init("Enter a budget in dollars.")) }
        let pieces = cleaned.split(separator: ".", maxSplits: 1, omittingEmptySubsequences: false)
        let whole = String(pieces[0])
        let cents = pieces.count > 1 ? String(pieces[1]) : ""
        let digits = { (s: String) in s.allSatisfy { $0.isASCII && $0.isNumber } }
        if !digits(whole) || !digits(cents) || (whole.isEmpty && cents.isEmpty) {
            return .failure(.init("Enter a dollar amount, like 50 or 12.50."))
        }
        if cents.count > 2 { return .failure(.init("Use at most two decimal places.")) }
        let dollars: Int64
        if whole.isEmpty {
            dollars = 0
        } else if let parsed = Int64(whole) {
            dollars = parsed
        } else {
            return .failure(.init("That budget is too large."))
        }
        if dollars > 100_000 { return .failure(.init("Budgets go up to $100,000 a month.")) }
        let padded = cents.padding(toLength: 2, withPad: "0", startingAt: 0)
        return .success(dollars * 1_000_000 + (Int64(padded) ?? 0) * 10_000)
    }

    /// The budget as the input shows it: "50", "12.50".
    static func budgetInputText(_ microUsd: Int64?) -> String {
        guard let microUsd else { return "" }
        var text = formatDollars(microUsd)
        while text.first == "$" { text.removeFirst() }
        return text
    }

    /// How much of the month's budget is used, for the meter (nil without a budget).
    static func budgetFraction(_ installation: RunnerInstallation) -> Double? {
        guard let budget = installation.monthlyBudgetMicroUsd else { return nil }
        let used = max(installation.usedThisMonthMicroUsd ?? 0, 0)
        if budget <= 0 { return 1 }
        return min(max(Double(used) / Double(budget), 0), 1)
    }

    /// "$12.50 of $50 used this month" / "$12.50 used this month · no monthly budget".
    static func spendLine(_ installation: RunnerInstallation) -> String {
        let used = formatDollars(installation.usedThisMonthMicroUsd ?? 0)
        if let budget = installation.monthlyBudgetMicroUsd {
            return "\(used) of \(formatDollars(budget)) used this month"
        }
        return "\(used) used this month · no monthly budget"
    }

    /// What each budget row says under its title.
    static func budgetDescription(_ budget: RunnerBudget, _ installation: RunnerInstallation) -> String {
        switch budget {
        case .monthly: return spendLine(installation)
        case .untrusted: return "Jobs from outside contributors stop at this amount each month."
        }
    }

    /// "Organization · 1 of 2 repositories on · Suspended on GitHub", under the account name.
    static func installationMeta(_ installation: RunnerInstallation) -> String {
        var parts: [String] = []
        if let kind = installation.accountType?.trimmingCharacters(in: .whitespacesAndNewlines), !kind.isEmpty {
            parts.append(kind)
        }
        let total = installation.repos.count
        let on = installation.repos.filter(\.enabled).count
        parts.append(total == 0 ? "No repositories" : "\(on) of \(total) repositor\(total == 1 ? "y" : "ies") on")
        if installation.suspended { parts.append("Suspended on GitHub") }
        return parts.joined(separator: " · ")
    }

    /// The concurrent-job cap, kept within 1…maxConcurrent.
    static func stepConcurrent(_ current: Int?, delta: Int) -> Int {
        let current = min(max(current ?? 1, 1), maxConcurrent)
        return min(max(current + delta, 1), maxConcurrent)
    }

    /// "Standard", "Large", or the key capitalised.
    static func sizeName(_ key: String) -> String {
        let trimmed = key.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "Default" : CloudSandboxRules.capitalize(trimmed)
    }

    /// "Standard · 2 vCPU · 4 GB · $0.10/h".
    static func sizeOptionLabel(_ size: RunnerSize) -> String {
        var parts = [sizeName(size.key)]
        if let spec = CloudSandboxRules.machineSpec(cpu: size.cpu, memoryGiB: size.memoryGiB) { parts.append(spec) }
        if let price = size.pricePerHourMicroUsd { parts.append(CloudSandboxRules.formatRate(price)) }
        return parts.joined(separator: " · ")
    }

    /// The repository's size as its menu shows it: the named size, else the first offered.
    static func repoSizeLabel(_ repo: RunnerRepo, sizes: [RunnerSize]) -> String {
        let key = repo.defaultSize?.trimmingCharacters(in: .whitespacesAndNewlines)
        let named = key.flatMap { $0.isEmpty ? nil : $0 }
        // A named size the list no longer offers reads as its own name, not as the first one.
        let size: RunnerSize?
        if let named {
            size = sizes.first { $0.key == named }
        } else {
            size = sizes.first
        }
        if let size {
            var text = sizeName(size.key)
            if let price = size.pricePerHourMicroUsd { text += " · \(CloudSandboxRules.formatRate(price))" }
            return text
        }
        if let named { return sizeName(named) }
        return "Default size"
    }

    /// "CI · test", falling back to whichever is known.
    static func jobTitle(_ job: RunnerJob) -> String {
        let parts = [job.workflow, job.jobName]
            .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        return parts.isEmpty ? "Job" : parts.joined(separator: " · ")
    }

    /// "Queued", "Running", "Succeeded", "Failed", "Cancelled", …
    static func jobStatus(_ job: RunnerJob) -> (text: String, tone: RunnerJobTone) {
        let conclusion = job.conclusion?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let state = job.state.trimmingCharacters(in: .whitespacesAndNewlines)
        switch state {
        case "queued", "waiting", "pending": return ("Queued", .working)
        case "in_progress", "running", "started": return ("Running", .working)
        default: break
        }
        switch conclusion {
        case "success": return ("Succeeded", .good)
        case "failure", "timed_out", "startup_failure": return ("Failed", .bad)
        case "cancelled": return ("Cancelled", .quiet)
        case "skipped", "neutral": return ("Skipped", .quiet)
        case "":
            if state.isEmpty { return ("Unknown", .quiet) }
            return (sizeName(state.replacingOccurrences(of: "_", with: " ")), .quiet)
        default:
            return (sizeName(conclusion.replacingOccurrences(of: "_", with: " ")), .quiet)
        }
    }

    /// "45s", "3m 5s", "1h 2m".
    static func formatDuration(_ seconds: Double) -> String {
        let s = Int64(max(seconds, 0).rounded())
        if s < 60 { return "\(s)s" }
        if s < 3600 { return "\(s / 60)m \(s % 60)s" }
        return "\(s / 3600)h \((s % 3600) / 60)m"
    }

    static func moment(_ value: JSONValue) -> Date? {
        switch value {
        case .string(let text):
            return CloudSandboxRules.parseMoment(text)
        case .int(let n):
            // Milliseconds or seconds.
            return n > 100_000_000_000
                ? Date(timeIntervalSince1970: TimeInterval(n) / 1000)
                : Date(timeIntervalSince1970: TimeInterval(n))
        default:
            return nil
        }
    }

    /// "just now", "5 min ago", "3h ago", "2d ago".
    static func ago(_ at: Date, now: Date) -> String {
        let minutes = Int(now.timeIntervalSince(at) / 60)
        if minutes < 1 { return "just now" }
        if minutes < 60 { return "\(minutes) min ago" }
        if minutes < 60 * 24 { return "\(minutes / 60)h ago" }
        return "\(minutes / (60 * 24))d ago"
    }

    /// "Large · 3m 5s · $0.0050 · 5 min ago · Fork, isolated".
    static func jobMeta(_ job: RunnerJob, now: Date) -> String {
        var parts: [String] = []
        if let size = job.size, !size.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            parts.append(sizeName(size))
        }
        var duration = job.durationSeconds
        if duration == nil, let start = job.startedAt.flatMap(moment), let end = job.completedAt.flatMap(moment) {
            // Whole seconds, like chrono's num_seconds.
            duration = Double(Int64(end.timeIntervalSince(start)))
        }
        if let duration { parts.append(formatDuration(duration)) }
        if let cost = job.costMicroUsd { parts.append(formatDollars(cost)) }
        if let at = [job.completedAt, job.startedAt, job.queuedAt].lazy.compactMap({ $0.flatMap(moment) }).first {
            parts.append(ago(at, now: now))
        }
        if job.tier == "untrusted" { parts.append("Fork, isolated") }
        return parts.joined(separator: " · ")
    }

    /// The job's row: "<status> · <meta>", or the status alone.
    static func jobLine(_ job: RunnerJob, now: Date) -> String {
        let status = jobStatus(job).text
        let meta = jobMeta(job, now: now)
        return meta.isEmpty ? status : "\(status) · \(meta)"
    }

    /// The job's page on GitHub, when it is an https link.
    static func jobURL(_ job: RunnerJob) -> URL? {
        guard let text = job.htmlUrl?.trimmingCharacters(in: .whitespacesAndNewlines),
              text.hasPrefix("https://") else { return nil }
        return URL(string: text)
    }

    /// Anything still queued or running moves on its own, so the page re-reads while one is.
    static func jobsWorking(_ runners: GithubRunners) -> Bool {
        runners.recentJobs.contains { jobStatus($0).tone == .working }
    }
}

/// Why a typed budget can't be saved, in the words the page shows.
struct BudgetInputError: Error, Equatable {
    var message: String
    init(_ message: String) { self.message = message }
}

extension GithubRunners {
    /// The answer after `action` lands, the way the gateway applies it (demo mode, and tests).
    func applying(_ action: GithubRunnerAction) -> GithubRunners {
        var next = self
        switch action {
        case let .installation(id, monthly, untrusted, max):
            guard let ix = next.installations.firstIndex(where: { $0.installationId == id }) else { return next }
            if let monthly { next.installations[ix].monthlyBudgetMicroUsd = monthly }
            if let untrusted { next.installations[ix].untrustedBudgetMicroUsd = untrusted }
            if let max { next.installations[ix].maxConcurrent = max }
        case let .repo(id, fullName, enabled, defaultSize):
            guard let ix = next.installations.firstIndex(where: { $0.installationId == id }),
                  let repoIx = next.installations[ix].repos.firstIndex(where: { $0.fullName == fullName })
            else { return next }
            if let enabled { next.installations[ix].repos[repoIx].enabled = enabled }
            if let defaultSize { next.installations[ix].repos[repoIx].defaultSize = defaultSize }
        case let .disconnect(id):
            next.installations.removeAll { $0.installationId == id }
        }
        return next
    }
}
