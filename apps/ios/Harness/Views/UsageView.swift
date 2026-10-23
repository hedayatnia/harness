// Settings > Usage. Spend and limits live with the computers that run the
// agents, so this asks one of them over its relay: the CodeGraff account its
// engine is signed in with (credits, 30-day spend, the monthly key budget)
// and each agent CLI's plan windows (Claude Code, Codex, …). The desktop's
// Settings → Accounts shows the same numbers, and like it every load asks the
// engine to read the limits afresh: an unforced list only returns limits some
// other client read in the last few minutes, and otherwise none.

import SwiftUI

struct UsageView: View {
    @Environment(AppModel.self) private var model
    @State private var selectedDeviceId: String?
    @State private var state = UsageLoadState<CodegraffUsage?, [AgentAccount]>()
    @State private var now = Date()

    /// Computers that can run agents, online ones first.
    private var hosts: [DeviceRow] {
        let candidates = model.devices.filter(\.canHostSessions)
        return candidates.filter { model.deviceOnline($0.id) }
            + candidates.filter { !model.deviceOnline($0.id) }
    }

    private var deviceId: String? {
        if let selectedDeviceId, hosts.contains(where: { $0.id == selectedDeviceId }) {
            return selectedDeviceId
        }
        return hosts.first?.id
    }

    var body: some View {
        List {
            if hosts.isEmpty {
                ContentUnavailableView("No computers yet", systemImage: "desktopcomputer",
                    description: Text("Usage comes from the computers that run your agents. Open Harness on one and sign in."))
            } else {
                if hosts.count > 1 {
                    Section {
                        Picker("Computer", selection: Binding(
                            get: { deviceId ?? "" },
                            set: { selectedDeviceId = $0 }
                        )) {
                            ForEach(hosts) { host in
                                Text(model.deviceOnline(host.id) ? host.name : "\(host.name) (offline)")
                                    .tag(host.id)
                            }
                        }
                        .accessibilityIdentifier("usage-device")
                    }
                    .harnessListRow()
                }
                content
            }
        }
        .harnessGroupedList()
        .refreshable { await reload() }
        .task(id: deviceId) { await reload() }
        .navigationTitle("Usage")
        .navigationBarTitleDisplayMode(.inline)
    }

    @ViewBuilder
    private var content: some View {
        // Host selection can change before its .task starts. Never render the
        // previous computer's account names (or signed-out claims) in that gap.
        if state.deviceId != deviceId {
            Section { ProgressView() }
                .harnessListRow()
        } else {
            endpointSection("CodeGraff", state: state.codegraff) { codegraffSection($0) }
            endpointSection("Agent plans", state: state.accounts) { agentSections($0) }
        }
    }

    @ViewBuilder
    private func endpointSection<Value, Content: View>(
        _ title: String, state: UsageSectionState<Value>,
        @ViewBuilder content: (Value) -> Content
    ) -> some View {
        switch state {
        case .loaded(let value):
            content(value)
        case .loading:
            Section(title) { ProgressView() }
                .harnessListRow()
        case .idle:
            Section(title) {
                Text("Usage wasn't loaded.").foregroundStyle(.secondary)
                Button("Try again") { Task { await reload() } }
            }
            .harnessListRow()
        case .failed(let message):
            Section(title) {
                Text("Couldn't load usage: \(message)").foregroundStyle(.secondary)
                Button("Try again") { Task { await reload() } }
            }
            .harnessListRow()
        }
    }

    @ViewBuilder
    private func codegraffSection(_ usage: CodegraffUsage?) -> some View {
        Section {
            if let usage {
                LabeledContent("Account", value: usage.email)
                LabeledContent("Plan", value: usage.tier.capitalized)
                LabeledContent("Credits available", value: formatMicroUSD(usage.creditsMicroUSD))
                LabeledContent("Spent in 30 days", value: formatMicroUSD(usage.spend30dMicroUSD))
                LabeledContent("Requests in 30 days", value: usage.requests30d.formatted())
                if let budget = usage.budgetWindow {
                    UsageMeterRow(window: budget, now: now,
                                  detail: "\(formatMicroUSD(usage.keySpendMonthlyMicroUSD)) of \(formatMicroUSD(usage.keyBudgetMonthlyMicroUSD ?? 0))")
                }
            } else {
                Text("This computer isn't signed in to CodeGraff.")
                    .foregroundStyle(.secondary)
            }
        } header: {
            Text("CodeGraff")
        }
        .harnessListRow()
        .accessibilityIdentifier("usage-codegraff")
    }

    @ViewBuilder
    private func agentSections(_ accounts: [AgentAccount]) -> some View {
        let shown = accounts.filter { $0.active || !$0.usageWindows.isEmpty }
        if shown.isEmpty {
            Section("Agent plans") {
                Text("No agent sign-ins with plan limits on this computer.")
                    .foregroundStyle(.secondary)
            }
            .harnessListRow()
        }
        ForEach(shown) { account in
            Section {
                if account.usageWindows.isEmpty {
                    Text("No plan limits reported.").foregroundStyle(.secondary)
                }
                ForEach(account.usageWindows) { window in
                    UsageMeterRow(window: window, now: now)
                }
            } header: {
                Text(HarnessCatalog.label(for: account.harness))
            } footer: {
                Text([account.title, account.planLabel, account.active ? "In use" : nil]
                    .compactMap { $0 }.joined(separator: " · "))
            }
            .harnessListRow()
        }
    }

    @MainActor
    private func reload() async {
        guard !Task.isCancelled else { return }
        guard let deviceId else {
            state.clear()
            return
        }
        // Pin the initial/fallback choice: presence sorting must not switch
        // the computer being inspected while a usage request is in flight.
        if selectedDeviceId != deviceId { selectedDeviceId = deviceId }
        let request = state.begin(deviceId: deviceId)
        // Publish each result as soon as it arrives. A slow or failing endpoint
        // must not hide the other endpoint's successful response.
        async let codegraff: Void = reloadCodegraff(request)
        async let accounts: Void = reloadAccounts(request)
        _ = await (codegraff, accounts)
    }

    @MainActor
    private func reloadCodegraff(_ request: UsageLoadState<CodegraffUsage?, [AgentAccount]>.Request) async {
        let result = await capture { try await model.codegraffUsage(deviceId: request.deviceId) }
        if state.finishCodegraff(request, currentDeviceId: deviceId, result: result,
                                cancelled: Task.isCancelled) {
            now = Date()
        }
    }

    @MainActor
    private func reloadAccounts(_ request: UsageLoadState<CodegraffUsage?, [AgentAccount]>.Request) async {
        let result = await capture {
            try await model.agentAccounts(deviceId: request.deviceId, forceUsage: true).accounts
        }
        if state.finishAccounts(request, currentDeviceId: deviceId, result: result,
                                cancelled: Task.isCancelled) {
            now = Date()
        }
    }

    @MainActor
    private func capture<T>(_ body: () async throws -> T) async -> Result<T, UsageLoadFailure> {
        do { return .success(try await body()) }
        catch { return .failure(UsageLoadFailure(message: describeTransportError(error))) }
    }
}

private struct UsageMeterRow: View {
    let window: UsageWindow
    let now: Date
    var detail: String? = nil

    private var tint: Color {
        switch window.level {
        case .normal: return Theme.accent
        case .warn: return Theme.warning
        case .critical: return Theme.danger
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(window.label)
                Spacer()
                Text("\(Int((window.usedFraction * 100).rounded()))% used")
                    .monospacedDigit()
                    .foregroundStyle(window.level == .normal ? .secondary : tint)
            }
            ProgressView(value: window.usedFraction)
                .tint(tint)
            let notes = [detail, formatResetsIn(window.resetsAt, now: now)].compactMap { $0 }
            if !notes.isEmpty {
                Text(notes.joined(separator: " · "))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }
}
