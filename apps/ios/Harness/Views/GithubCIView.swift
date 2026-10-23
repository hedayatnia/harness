// Settings > GitHub CI: run GitHub Actions jobs on Codegraff sandboxes. The
// desktop's Settings → GitHub CI page on the phone, through one computer's
// engine over its relay (`CodegraffGithubRunners`, `CodegraffGithubRunnerAction`):
// connect the Codegraff app on GitHub, set the monthly and fork pull request
// budgets and how many jobs run at once, switch repositories on and pick their
// default size, and see recent jobs.

import SwiftUI
import UIKit

struct GithubCIView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openURL) private var openURL
    @Environment(\.scenePhase) private var scenePhase
    @State private var selectedDeviceId: String?
    @State private var state: LoadState = .idle
    @State private var loadedDeviceId: String?
    @State private var lastLoad: Date?
    @State private var now = Date()
    /// The control with an in-flight change (`GithubRunnerAction.busyKey`).
    @State private var busy: String?
    @State private var actionError: String?
    @State private var confirmDisconnect: RunnerInstallation?
    /// What each budget field holds, by "<installation>:<budget key>".
    @State private var budgetText: [String: String] = [:]
    @State private var budgetErrors: [String: String] = [:]
    /// After Connect: re-read until the installation arrives or this passes.
    @State private var connectUntil: Date?
    @State private var snippetCopied = false

    enum LoadState {
        case idle
        case loading
        case loaded(GithubRunners?)
        case failed(String)
    }

    static let subtitle = "Run GitHub Actions jobs on Codegraff sandboxes, billed per second from your Codegraff credit."

    private var hosts: [DeviceRow] { CodegraffHosts.hosts(model) }

    private var deviceId: String? {
        if let selectedDeviceId, hosts.contains(where: { $0.id == selectedDeviceId }) {
            return selectedDeviceId
        }
        return hosts.first?.id
    }

    private var runners: GithubRunners? {
        if case .loaded(let runners) = state { return runners }
        return nil
    }

    /// How often to re-read: while waiting for a Connect to land, or while a job is working.
    private var pollSeconds: UInt64? {
        if let connectUntil, connectUntil > Date() { return GithubRunnerRules.connectPollSeconds }
        if let runners, GithubRunnerRules.jobsWorking(runners) { return GithubRunnerRules.jobsPollSeconds }
        return nil
    }

    var body: some View {
        List {
            if hosts.isEmpty {
                ContentUnavailableView("No computers yet", systemImage: "desktopcomputer",
                    description: Text("GitHub CI is set up through the computers that run your agents. Open Harness on one and sign in."))
            } else {
                Section {
                    Text(Self.subtitle)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets(top: 0, leading: Theme.spaceLG, bottom: 0, trailing: Theme.spaceLG))
                if hosts.count > 1 {
                    CodegraffHostPicker(hosts: hosts, deviceId: Binding(
                        get: { deviceId }, set: { selectedDeviceId = $0 }
                    ), identifier: "github-ci-device")
                }
                if let actionError {
                    Section {
                        Text(actionError)
                            .foregroundStyle(Theme.danger)
                            .accessibilityIdentifier("github-ci-error")
                    }
                    .harnessListRow()
                }
                content
            }
        }
        .harnessGroupedList()
        .refreshable { await reload() }
        .task(id: deviceId) { await reload() }
        .task(id: "\(deviceId ?? "")|\(pollSeconds ?? 0)") {
            guard let every = pollSeconds else { return }
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: every * 1_000_000_000)
                guard !Task.isCancelled else { return }
                if let until = connectUntil, until <= Date() { connectUntil = nil }
                await reload()
            }
        }
        // Coming back from the browser (after installing the app) re-reads the page.
        .onChange(of: scenePhase) { _, phase in
            guard phase == .active, lastLoad.map({ Date().timeIntervalSince($0) >= 5 }) ?? true else { return }
            Task { await reload() }
        }
        .navigationTitle("GitHub CI")
        .navigationBarTitleDisplayMode(.inline)
        .confirmationDialog(
            "Disconnect \(confirmDisconnect?.account ?? "this account")?",
            isPresented: Binding(get: { confirmDisconnect != nil }, set: { if !$0 { confirmDisconnect = nil } }),
            titleVisibility: .visible,
            presenting: confirmDisconnect
        ) { installation in
            Button("Disconnect for good", role: .destructive) {
                Task { await act(.disconnect(installationId: installation.installationId)) }
            }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text("Its repositories stop running jobs on Codegraff.")
        }
    }

    @ViewBuilder
    private var content: some View {
        switch state {
        case .idle, .loading:
            Section {
                HStack {
                    Spacer()
                    ProgressView()
                    Spacer()
                }
            }
            .harnessListRow()
        case .failed(let message):
            Section {
                Text(message).foregroundStyle(.secondary)
                Button("Try again") { Task { await reload() } }
            }
            .harnessListRow()
        case .loaded(let runners):
            switch GithubRunnerRules.panel(runners) {
            case .signedOut:
                Section {
                    Text("This computer isn't signed in to Codegraff. Sign in with Codegraff in Harness on it to run GitHub Actions on Codegraff.")
                        .foregroundStyle(.secondary)
                }
                .harnessListRow()
                .accessibilityIdentifier("github-ci-signed-out")
            case let panel where panel != .connected:
                intro(runners, panel: panel)
                snippetSection
            default:
                if let runners {
                    connected(runners)
                }
            }
        }
    }

    // MARK: Not connected yet

    @ViewBuilder
    private func intro(_ runners: GithubRunners?, panel: RunnerPanel) -> some View {
        let url = runners.flatMap(GithubRunnerRules.installURL)
        let waiting = connectUntil != nil
        let label = switch panel {
        case .comingSoon, .unavailable: "Coming soon"
        default: waiting ? "Waiting for GitHub…" : "Connect GitHub"
        }
        let note: String? = switch panel {
        case .unavailable: "GitHub CI isn't available on Codegraff yet. This page fills in once it is."
        case .comingSoon: "GitHub CI is coming soon to your Codegraff account."
        default:
            waiting ? "Finish installing the Codegraff app on GitHub in your browser. This page updates when you come back."
                : url == nil ? "Codegraff didn't send an install link. Try Refresh." : nil
        }
        Section {
            VStack(alignment: .leading, spacing: 6) {
                Text("Run GitHub Actions on Codegraff")
                    .font(.body.weight(.medium))
                Text(GithubRunnerRules.explainer)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                if let note {
                    Text(note)
                        .font(.footnote)
                        .foregroundStyle(Theme.warning)
                }
            }
            .padding(.vertical, 2)
            Button(label) {
                if let url { connect(url) }
            }
            .disabled(panel != .notConnected || url == nil)
            .accessibilityIdentifier("github-ci-connect")
        }
        .harnessListRow()
    }

    /// The `runs-on: codegraff` snippet with Copy.
    private var snippetSection: some View {
        Section {
            Text(GithubRunnerRules.yamlSnippet.trimmingCharacters(in: .newlines))
                .font(.system(.footnote, design: .monospaced))
                .textSelection(.enabled)
                .padding(.vertical, 2)
            Button {
                UIPasteboard.general.string = GithubRunnerRules.yamlSnippet
                snippetCopied = true
                Task {
                    try? await Task.sleep(nanoseconds: 1_500_000_000)
                    snippetCopied = false
                }
            } label: {
                Label(snippetCopied ? "Copied" : "Copy", systemImage: snippetCopied ? "checkmark" : "doc.on.doc")
            }
            .accessibilityIdentifier("github-ci-copy-snippet")
        } header: {
            Text("In a workflow, run a job on Codegraff:")
        }
        .harnessListRow()
    }

    // MARK: Connected

    @ViewBuilder
    private func connected(_ runners: GithubRunners) -> some View {
        ForEach(runners.installations) { installation in
            installationSection(installation, sizes: runners.sizes)
        }
        if let url = GithubRunnerRules.installURL(runners) {
            Section {
                Button(connectUntil != nil ? "Waiting for GitHub…" : "Connect another account") {
                    connect(url)
                }
                .accessibilityIdentifier("github-ci-add-account")
            }
            .harnessListRow()
        }
        jobsSection(runners)
        snippetSection
    }

    private func installationSection(_ installation: RunnerInstallation, sizes: [RunnerSize]) -> some View {
        let id = installation.installationId
        return Section {
            HStack(alignment: .center) {
                Text(GithubRunnerRules.installationMeta(installation))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                Spacer(minLength: Theme.spaceSM)
                Button(busy == "\(id):disconnect" ? "Disconnecting…" : "Disconnect", role: .destructive) {
                    confirmDisconnect = installation
                }
                .buttonStyle(.borderless)
                .disabled(busy != nil)
                .accessibilityIdentifier("github-ci-disconnect")
            }
            ForEach(RunnerBudget.allCases, id: \.self) { budget in
                budgetRow(installation, budget: budget)
            }
            concurrencyRow(installation)
            if installation.repos.isEmpty {
                Text("No repositories yet. Choose them in the Codegraff app's settings on GitHub.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            ForEach(installation.repos) { repo in
                repoRow(installation, repo: repo, sizes: sizes)
            }
        } header: {
            Text(installation.account)
        }
        .harnessListRow()
    }

    private func budgetKey(_ installation: RunnerInstallation, _ budget: RunnerBudget) -> String {
        "\(installation.installationId):\(budget.key)"
    }

    private func budgetRow(_ installation: RunnerInstallation, budget: RunnerBudget) -> some View {
        let key = budgetKey(installation, budget)
        let saving = busy == key
        let problem = budgetErrors[key]
        let fraction = budget == .monthly ? GithubRunnerRules.budgetFraction(installation) : nil
        return VStack(alignment: .leading, spacing: 6) {
            Text(budget.title)
            Text(GithubRunnerRules.budgetDescription(budget, installation))
                .font(.footnote)
                .foregroundStyle(.secondary)
            if let fraction {
                ProgressView(value: fraction)
                    .tint(fraction >= 0.95 ? Theme.danger : fraction >= 0.8 ? Theme.warning : Theme.accent)
            }
            HStack(spacing: Theme.spaceSM) {
                HStack(spacing: 4) {
                    Text("$").foregroundStyle(.secondary)
                    TextField("Amount", text: Binding(
                        get: { budgetText[key] ?? GithubRunnerRules.budgetInputText(budget.current(installation)) },
                        set: {
                            budgetText[key] = $0
                            budgetErrors[key] = nil
                        }
                    ))
                    .keyboardType(.decimalPad)
                    .onSubmit { Task { await saveBudget(installation, budget) } }
                    .accessibilityIdentifier("github-ci-budget-\(budget.key)")
                }
                .padding(.horizontal, Theme.spaceSM)
                .padding(.vertical, 6)
                .background(Theme.bg, in: RoundedRectangle(cornerRadius: Theme.controlRadius))
                .overlay(
                    RoundedRectangle(cornerRadius: Theme.controlRadius)
                        .stroke(problem != nil ? Theme.danger.opacity(0.5) : Theme.border)
                )
                Button(saving ? "Saving…" : "Save") {
                    Task { await saveBudget(installation, budget) }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .disabled(busy != nil)
                .accessibilityIdentifier("github-ci-budget-save-\(budget.key)")
            }
            if let problem {
                Text(problem)
                    .font(.footnote)
                    .foregroundStyle(Theme.danger)
            }
        }
        .padding(.vertical, 2)
    }

    private func concurrencyRow(_ installation: RunnerInstallation) -> some View {
        let id = installation.installationId
        let current = min(max(installation.maxConcurrent ?? 1, 1), GithubRunnerRules.maxConcurrent)
        let saving = busy == "\(id):concurrent"
        return Stepper {
            VStack(alignment: .leading, spacing: 2) {
                HStack {
                    Text("Concurrent jobs")
                    Spacer()
                    Text(saving ? "…" : "\(current)")
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                Text("How many jobs run at once, 1 to \(GithubRunnerRules.maxConcurrent). More wait in the queue.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        } onIncrement: {
            step(installation, current: current, delta: 1)
        } onDecrement: {
            step(installation, current: current, delta: -1)
        }
        .disabled(busy != nil)
        .accessibilityIdentifier("github-ci-concurrent")
    }

    private func step(_ installation: RunnerInstallation, current: Int, delta: Int) {
        let next = GithubRunnerRules.stepConcurrent(current, delta: delta)
        guard next != current else { return }
        Task { await act(.installation(installationId: installation.installationId, maxConcurrent: next)) }
    }

    private func repoRow(_ installation: RunnerInstallation, repo: RunnerRepo, sizes: [RunnerSize]) -> some View {
        let id = installation.installationId
        let saving = busy == "\(id):repo:\(repo.fullName)"
        return VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: Theme.spaceSM) {
                Text(repo.fullName)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if repo.isPrivate {
                    Text("Private")
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 1)
                        .overlay(Capsule().stroke(Theme.border))
                }
                Spacer(minLength: 0)
                Toggle("Run \(repo.fullName) jobs on Codegraff", isOn: Binding(
                    get: { repo.enabled },
                    set: { on in
                        Task { await act(.repo(installationId: id, fullName: repo.fullName, enabled: on)) }
                    }
                ))
                .labelsHidden()
                .disabled(busy != nil)
                .accessibilityIdentifier("github-ci-repo-switch-\(repo.fullName)")
            }
            if !sizes.isEmpty {
                Menu {
                    Section("Default size") {
                        ForEach(sizes) { size in
                            Button {
                                Task { await act(.repo(installationId: id, fullName: repo.fullName, defaultSize: size.key)) }
                            } label: {
                                if repo.defaultSize == size.key {
                                    Label(GithubRunnerRules.sizeOptionLabel(size), systemImage: "checkmark")
                                } else {
                                    Text(GithubRunnerRules.sizeOptionLabel(size))
                                }
                            }
                        }
                    }
                } label: {
                    HStack(spacing: 4) {
                        Text(saving ? "Saving…" : GithubRunnerRules.repoSizeLabel(repo, sizes: sizes))
                        Image(systemName: "chevron.up.chevron.down")
                            .font(.caption2)
                    }
                    .font(.footnote)
                }
                .disabled(busy != nil)
                .accessibilityIdentifier("github-ci-repo-size-\(repo.fullName)")
            }
        }
        .padding(.vertical, 2)
    }

    private func jobsSection(_ runners: GithubRunners) -> some View {
        Section("Recent jobs") {
            if runners.recentJobs.isEmpty {
                Text("No jobs yet. Push a workflow with runs-on: codegraff to start one.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            ForEach(Array(runners.recentJobs.prefix(GithubRunnerRules.jobsShown).enumerated()), id: \.offset) { ix, job in
                jobRow(job)
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("github-ci-job-\(ix)")
            }
        }
        .harnessListRow()
    }

    private func jobRow(_ job: RunnerJob) -> some View {
        let tone = GithubRunnerRules.jobStatus(job).tone
        let color: Color = switch tone {
        case .good: Theme.statusCompleted
        case .bad: Theme.danger
        case .working: Theme.accent
        case .quiet: Theme.textFaint
        }
        return HStack(alignment: .center, spacing: Theme.spaceMD) {
            Circle().fill(color).frame(width: 7, height: 7)
            VStack(alignment: .leading, spacing: 2) {
                Text("\(job.repo) · \(GithubRunnerRules.jobTitle(job))")
                    .lineLimit(2)
                Text(GithubRunnerRules.jobLine(job, now: now))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            if let url = GithubRunnerRules.jobURL(job) {
                Button("Open on GitHub") { openURL(url) }
                    .buttonStyle(.borderless)
                    .font(.footnote)
            }
        }
        .padding(.vertical, 2)
    }

    // MARK: Calls

    private func reload() async {
        guard let deviceId else { return }
        if case .loaded = state, loadedDeviceId == deviceId {} else { state = .loading }
        let result: Result<GithubRunners?, Error>
        do { result = .success(try await model.githubRunners(deviceId: deviceId)) } catch { result = .failure(error) }
        guard self.deviceId == deviceId else { return }
        lastLoad = Date()
        now = Date()
        switch result {
        case .success(let runners):
            if loadedDeviceId != deviceId {
                budgetText = [:]
                budgetErrors = [:]
            }
            loadedDeviceId = deviceId
            // The installation the user went to the browser for has arrived.
            if let runners, !runners.installations.isEmpty { connectUntil = nil }
            state = .loaded(runners)
        case .failure(let error):
            if case .loaded = state, loadedDeviceId == deviceId {
                actionError = CodegraffHosts.loadFailure(error, deviceName: model.deviceName(deviceId))
            } else {
                loadedDeviceId = deviceId
                state = .failed(CodegraffHosts.loadFailure(error, deviceName: model.deviceName(deviceId)))
            }
        }
    }

    /// Run one change, then re-read.
    private func act(_ action: GithubRunnerAction) async {
        guard let deviceId, busy == nil else { return }
        busy = action.busyKey
        actionError = nil
        do {
            try await model.githubRunnerAction(deviceId: deviceId, action: action)
        } catch {
            actionError = CodegraffHosts.actionFailure(error, deviceName: model.deviceName(deviceId))
        }
        busy = nil
        await reload()
    }

    private func saveBudget(_ installation: RunnerInstallation, _ budget: RunnerBudget) async {
        let key = budgetKey(installation, budget)
        let text = budgetText[key] ?? GithubRunnerRules.budgetInputText(budget.current(installation))
        switch GithubRunnerRules.parseBudget(text) {
        case .failure(let problem):
            budgetErrors[key] = problem.message
        case .success(let amount):
            budgetErrors[key] = nil
            budgetText[key] = GithubRunnerRules.budgetInputText(amount)
            let id = installation.installationId
            await act(budget == .monthly
                      ? .installation(installationId: id, monthlyBudgetMicroUsd: amount)
                      : .installation(installationId: id, untrustedBudgetMicroUsd: amount))
        }
    }

    /// Open the Codegraff app's install page and watch for the installation to arrive.
    private func connect(_ url: URL) {
        openURL(url)
        connectUntil = Date().addingTimeInterval(GithubRunnerRules.connectWatchSeconds)
    }
}
