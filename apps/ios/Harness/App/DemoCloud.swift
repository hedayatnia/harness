// Demo mode's Codegraff account: made-up cloud sandboxes and GitHub CI runners,
// so Settings → Cloud sandboxes and Settings → GitHub CI can be explored and
// screenshotted offline. Actions change this state the way the gateway would.
// Launch flag `-github-ci-new` starts GitHub CI with nothing connected.

import Foundation

@MainActor
final class DemoCloud {
    var sandboxes: CodegraffSandboxes
    var runners: GithubRunners

    init(sandboxes: CodegraffSandboxes, runners: GithubRunners) {
        self.sandboxes = sandboxes
        self.runners = runners
    }

    /// The sizes the shared parity vector pins.
    static let sizes = [
        PersonalSize(id: "standard", cpu: 2, memoryGiB: 4, diskGiB: 40, pricePerHourMicroUsd: 100_000, isDefault: true),
        PersonalSize(id: "large", cpu: 4, memoryGiB: 8, diskGiB: 40, pricePerHourMicroUsd: 200_000),
    ]

    static func standard(now: Date = Date(), connected: Bool = true) -> DemoCloud {
        let iso = { (offset: TimeInterval) in
            ISO8601DateFormatter().string(from: now.addingTimeInterval(offset))
        }
        let sandboxes = CodegraffSandboxes(enabled: true, sandboxes: [
            CodegraffSandbox(id: "cnd_7f3a91c0d2e4", provider: "fleet", state: "running",
                             expiresAt: .string(iso(24 * 60)), harness: true,
                             deviceName: "Harness cloud 7f3a", keepRunning: true,
                             cpu: 2, memoryGiB: 4, diskGiB: 40, pricePerHourMicroUsd: 100_000),
            CodegraffSandbox(id: "cnd_51c2b8e07a19", provider: "fleet", state: "paused",
                             harness: true, deviceName: "Harness cloud 51c2", keepRunning: false,
                             cpu: 4, memoryGiB: 8, diskGiB: 40, pricePerHourMicroUsd: 200_000),
        ], sizes: sizes)

        let job = { (workflow: String, name: String, repo: String, size: String, state: String,
                     conclusion: String?, startedAgo: TimeInterval, seconds: TimeInterval?,
                     cost: Int64?, tier: String) in
            RunnerJob(repo: repo, workflow: workflow, jobName: name, tier: tier, size: size, state: state,
                      startedAt: .string(iso(-startedAgo)),
                      completedAt: seconds.map { .string(iso(-startedAgo + $0)) },
                      costMicroUsd: cost, conclusion: conclusion,
                      htmlUrl: "https://github.com/octo-org/\(repo.split(separator: "/").last ?? "")/actions")
        }
        let runners = GithubRunners(
            available: true, enabled: true,
            installUrl: "https://github.com/apps/codegraff/installations/new",
            installations: connected ? [
                RunnerInstallation(installationId: "4242", account: "octo-org", accountType: "Organization",
                                   monthlyBudgetMicroUsd: 50_000_000, untrustedBudgetMicroUsd: 10_000_000,
                                   usedThisMonthMicroUsd: 12_500_000, maxConcurrent: 3, repos: [
                                       RunnerRepo(fullName: "octo-org/app", enabled: true, defaultSize: "large", isPrivate: true),
                                       RunnerRepo(fullName: "octo-org/site", enabled: true, defaultSize: "standard"),
                                       RunnerRepo(fullName: "octo-org/docs", enabled: false),
                                   ]),
            ] : [],
            recentJobs: connected ? [
                job("CI", "test", "octo-org/app", "large", "in_progress", nil, 90, nil, nil, "trusted"),
                job("CI", "build", "octo-org/app", "large", "completed", "success", 25 * 60, 185, 10_300, "trusted"),
                job("Deploy", "preview", "octo-org/site", "standard", "completed", "failure", 3 * 3600, 48, 1_300, "untrusted"),
            ] : [],
            sizes: sizes.map {
                RunnerSize(key: $0.id, cpu: Double($0.cpu), memoryGiB: Double($0.memoryGiB),
                           diskGiB: Double($0.diskGiB), pricePerHourMicroUsd: $0.pricePerHourMicroUsd)
            })
        return DemoCloud(sandboxes: sandboxes, runners: runners)
    }

    /// A create: a new running sandbox of the picked size (the default one without).
    func create(size: String?) {
        let picked = CloudSandboxRules.selectedSize(sandboxes.sizes, picked: size)
        let suffix = String(UUID().uuidString.lowercased().replacingOccurrences(of: "-", with: "").prefix(12))
        sandboxes.sandboxes.append(CodegraffSandbox(
            id: "cnd_\(suffix)", provider: "fleet", state: "running", harness: true,
            deviceName: "Harness cloud \(suffix.prefix(4))", keepRunning: true,
            cpu: picked.map { Double($0.cpu) }, memoryGiB: picked.map { Double($0.memoryGiB) },
            diskGiB: picked.map { Double($0.diskGiB) }, pricePerHourMicroUsd: picked?.pricePerHourMicroUsd))
    }

    func act(id: String, action: CloudSandboxAction) {
        guard let ix = sandboxes.sandboxes.firstIndex(where: { $0.id == id }) else { return }
        switch action {
        case .start:
            sandboxes.sandboxes[ix].state = "running"
            sandboxes.sandboxes[ix].harness = true
            sandboxes.sandboxes[ix].keepRunning = true
            sandboxes.sandboxes[ix].renewError = nil
        case .stop:
            sandboxes.sandboxes[ix].state = "paused"
        case .delete:
            sandboxes.sandboxes.remove(at: ix)
        case .keepRunningOn:
            sandboxes.sandboxes[ix].keepRunning = true
            sandboxes.sandboxes[ix].renewError = nil
        case .keepRunningOff:
            sandboxes.sandboxes[ix].keepRunning = false
        }
    }

    func act(_ action: GithubRunnerAction) {
        runners = runners.applying(action)
    }
}
