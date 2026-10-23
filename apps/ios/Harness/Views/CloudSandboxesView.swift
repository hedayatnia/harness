// Settings > Cloud sandboxes. The Codegraff account behind cloud sandboxes is
// the one a computer's engine is signed in with, so like Settings → Usage this
// asks one of the computers over its relay (`CodegraffSandboxes`,
// `CodegraffCreateSandbox`, `CodegraffSandboxAction`), the calls the desktop's
// Settings → Accounts card and device picker make. Running sandboxes show their
// machine and hourly rate; a create offers the sizes with their price, the
// default one picked.

import SwiftUI

/// What the Codegraff pages share: which computers to ask, and how a failed call reads.
enum CodegraffHosts {
    /// Computers that can run agents, online ones first.
    @MainActor
    static func hosts(_ model: AppModel) -> [DeviceRow] {
        let candidates = model.devices.filter(\.canHostSessions)
        return candidates.filter { model.deviceOnline($0.id) }
            + candidates.filter { !model.deviceOnline($0.id) }
    }

    /// A load that failed: an older computer, or the relay's reason.
    static func loadFailure(_ error: Error, deviceName: String) -> String {
        if case RelayError.rpc(let message) = error, message.contains("unknown method") {
            return "Update Harness on \(deviceName) to manage this from your phone."
        }
        let reason = (error as? RelayError)?.errorDescription ?? describeTransportError(error)
        return "Couldn't reach \(deviceName): \(reason)"
    }

    /// An action that failed: the engine's own sentence ("Your Codegraff credit is used up…"),
    /// else as a load failure reads.
    static func actionFailure(_ error: Error, deviceName: String) -> String {
        if case RelayError.rpc(let message) = error, !message.contains("unknown method") {
            return message
        }
        return loadFailure(error, deviceName: deviceName)
    }
}

/// The computer picker both Codegraff pages show when there is more than one.
struct CodegraffHostPicker: View {
    @Environment(AppModel.self) private var model
    let hosts: [DeviceRow]
    @Binding var deviceId: String?
    let identifier: String

    var body: some View {
        Section {
            Picker("Computer", selection: Binding(
                get: { deviceId ?? "" },
                set: { deviceId = $0 }
            )) {
                ForEach(hosts) { host in
                    Text(model.deviceOnline(host.id) ? host.name : "\(host.name) (offline)")
                        .tag(host.id)
                }
            }
            .accessibilityIdentifier(identifier)
        } footer: {
            Text("Uses the Codegraff account this computer is signed in with.")
        }
        .harnessListRow()
    }
}

struct CloudSandboxesView: View {
    @Environment(AppModel.self) private var model
    @State private var selectedDeviceId: String?
    @State private var state: LoadState = .idle
    /// The computer `state` describes; another one shows a spinner, not a stale list.
    @State private var loadedDeviceId: String?
    @State private var now = Date()
    /// The sandbox with an in-flight action, and which.
    @State private var busy: (id: String, action: CloudSandboxAction)?
    @State private var turningOffAll = false
    @State private var creating = false
    /// The size picked for the next sandbox (nil: the default one).
    @State private var pickedSize: String?
    @State private var actionError: String?
    @State private var confirmDelete: CodegraffSandbox?

    enum LoadState {
        case idle
        case loading
        case loaded(CodegraffSandboxes?)
        case failed(String)
    }

    private var hosts: [DeviceRow] { CodegraffHosts.hosts(model) }

    private var deviceId: String? {
        if let selectedDeviceId, hosts.contains(where: { $0.id == selectedDeviceId }) {
            return selectedDeviceId
        }
        return hosts.first?.id
    }

    private var list: CodegraffSandboxes? {
        if case .loaded(let list) = state { return list }
        return nil
    }

    /// Re-read every minute while something can change on its own.
    private var refreshKey: String {
        "\(deviceId ?? "")|\(list.map(CloudSandboxRules.needsRefresh) ?? false)"
    }

    var body: some View {
        List {
            if hosts.isEmpty {
                ContentUnavailableView("No computers yet", systemImage: "desktopcomputer",
                    description: Text("Cloud sandboxes are managed through the computers that run your agents. Open Harness on one and sign in."))
            } else {
                if hosts.count > 1 {
                    CodegraffHostPicker(hosts: hosts, deviceId: Binding(
                        get: { deviceId }, set: { selectedDeviceId = $0 }
                    ), identifier: "cloud-sandboxes-device")
                }
                if let actionError {
                    Section {
                        Text(actionError)
                            .foregroundStyle(Theme.danger)
                            .accessibilityIdentifier("cloud-sandboxes-error")
                    }
                    .harnessListRow()
                }
                content
            }
        }
        .harnessGroupedList()
        .refreshable { await reload() }
        .task(id: deviceId) { await reload() }
        .task(id: refreshKey) {
            guard let list, CloudSandboxRules.needsRefresh(list) else { return }
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: CloudSandboxRules.refreshEverySeconds * 1_000_000_000)
                guard !Task.isCancelled else { return }
                await reload()
            }
        }
        .navigationTitle("Cloud sandboxes")
        .navigationBarTitleDisplayMode(.inline)
        .confirmationDialog(
            "Delete \(confirmDelete.map(CloudSandboxRules.title) ?? "this sandbox")?",
            isPresented: Binding(get: { confirmDelete != nil }, set: { if !$0 { confirmDelete = nil } }),
            titleVisibility: .visible,
            presenting: confirmDelete
        ) { sandbox in
            Button("Delete for good", role: .destructive) {
                Task { await act(sandbox.id, .delete) }
            }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text("Its memory and files are gone for good.")
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
        case .loaded(nil):
            Section {
                Text("This computer isn't signed in to Codegraff. Sign in with Codegraff in Harness on it to use cloud sandboxes.")
                    .foregroundStyle(.secondary)
            }
            .harnessListRow()
            .accessibilityIdentifier("cloud-sandboxes-signed-out")
        case .loaded(let list?):
            if !list.enabled {
                Section {
                    Text("Harness on this computer doesn't offer cloud sandboxes.")
                        .foregroundStyle(.secondary)
                }
                .harnessListRow()
            } else {
                summarySection(list)
                sandboxesSection(list)
                createSection(list)
            }
        }
    }

    /// What everything running costs, and one Turn off for all of it (the desktop's strip).
    @ViewBuilder
    private func summarySection(_ list: CodegraffSandboxes) -> some View {
        if let summary = CloudSandboxRules.runningSummary(list, now: now) {
            Section {
                HStack(alignment: .center, spacing: Theme.spaceMD) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(summary.label)
                            .font(.body.weight(.medium))
                            .accessibilityIdentifier("cloud-summary-label")
                        Text(summary.detail)
                            .font(.footnote)
                            .foregroundStyle(summary.warning ? Theme.warning : .secondary)
                    }
                    Spacer(minLength: 0)
                    if !summary.runningIds.isEmpty {
                        Button(turningOffAll ? CloudSandboxRules.turnOff.busy : CloudSandboxRules.turnOff.label) {
                            Task { await turnOffAll(summary.runningIds) }
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        .disabled(turningOffAll || busy != nil)
                        .accessibilityIdentifier("cloud-turn-off-all")
                    }
                }
                .padding(.vertical, 2)
            } footer: {
                Text(CloudSandboxRules.turnOffHint)
            }
            .harnessListRow()
        }
    }

    @ViewBuilder
    private func sandboxesSection(_ list: CodegraffSandboxes) -> some View {
        Section("Sandboxes") {
            if list.sandboxes.isEmpty {
                Text("No cloud sandboxes yet.")
                    .foregroundStyle(.secondary)
            }
            ForEach(Array(list.sandboxes.enumerated()), id: \.element.id) { ix, sandbox in
                sandboxRow(sandbox)
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("cloud-sandbox-\(ix)")
            }
        }
        .harnessListRow()
    }

    private func sandboxRow(_ sandbox: CodegraffSandbox) -> some View {
        let inFlight = busy?.id == sandbox.id ? busy?.action : nil
        let locked = busy != nil || turningOffAll || creating
        return VStack(alignment: .leading, spacing: 6) {
            Text(CloudSandboxRules.title(sandbox))
            Text(CloudSandboxRules.detail(sandbox, now: now))
                .font(.footnote)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("cloud-sandbox-detail")
            HStack(spacing: Theme.spaceSM) {
                if CloudSandboxRules.canStart(sandbox) {
                    let labels = CloudSandboxRules.startLabels(sandbox)
                    Button(inFlight == .start ? labels.busy : labels.label) {
                        Task { await act(sandbox.id, .start) }
                    }
                    .accessibilityIdentifier("cloud-sandbox-start")
                }
                if CloudSandboxRules.canKeepRunning(sandbox) {
                    Button(inFlight == .keepRunningOn ? "Keeping running…" : "Keep running") {
                        Task { await act(sandbox.id, .keepRunningOn) }
                    }
                    .accessibilityHint(CloudSandboxRules.keepRunningHint)
                }
                if CloudSandboxRules.canStop(sandbox) {
                    Button(inFlight == .stop ? CloudSandboxRules.turnOff.busy : CloudSandboxRules.turnOff.label) {
                        Task { await act(sandbox.id, .stop) }
                    }
                    .accessibilityHint(CloudSandboxRules.turnOffHint)
                    .accessibilityIdentifier("cloud-sandbox-stop")
                }
                Button(inFlight == .delete ? "Deleting…" : "Delete", role: .destructive) {
                    confirmDelete = sandbox
                }
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .disabled(locked)
            .padding(.top, 2)
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder
    private func createSection(_ list: CodegraffSandboxes) -> some View {
        let picked = CloudSandboxRules.selectedSize(list.sizes, picked: pickedSize)
        Section {
            // Standard / Large with what each is and costs; the default one preselected.
            ForEach(list.sizes) { size in
                Button {
                    pickedSize = size.id
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(CloudSandboxRules.sizeTitle(size))
                                .foregroundStyle(Theme.text)
                            Text(CloudSandboxRules.sizeDetail(size))
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        if picked?.id == size.id {
                            Image(systemName: "checkmark")
                                .foregroundStyle(Theme.accent)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(creating)
                .accessibilityAddTraits(picked?.id == size.id ? .isSelected : [])
                .accessibilityIdentifier("cloud-size-\(size.id)")
            }
            Button {
                Task { await create(size: picked?.id) }
            } label: {
                HStack(spacing: Theme.spaceSM) {
                    if creating { ProgressView() }
                    Text(creating ? "Creating…" : "Create")
                }
            }
            .disabled(creating || busy != nil || turningOffAll)
            .accessibilityIdentifier("cloud-create")
        } header: {
            Text("Harness cloud")
        } footer: {
            Text(CloudSandboxRules.createDescription)
        }
        .harnessListRow()
    }

    // MARK: Calls

    private func reload() async {
        guard let deviceId else { return }
        if case .loaded = state, loadedDeviceId == deviceId {} else { state = .loading }
        let result: Result<CodegraffSandboxes?, Error>
        do { result = .success(try await model.cloudSandboxes(deviceId: deviceId)) } catch { result = .failure(error) }
        guard self.deviceId == deviceId else { return }
        now = Date()
        switch result {
        case .success(let list):
            loadedDeviceId = deviceId
            state = .loaded(list)
        case .failure(let error):
            // A list on screen stays through a failed refresh; the reason shows above it.
            if case .loaded = state, loadedDeviceId == deviceId {
                actionError = CodegraffHosts.loadFailure(error, deviceName: model.deviceName(deviceId))
            } else {
                loadedDeviceId = deviceId
                state = .failed(CodegraffHosts.loadFailure(error, deviceName: model.deviceName(deviceId)))
            }
        }
    }

    private func act(_ id: String, _ action: CloudSandboxAction) async {
        guard let deviceId, busy == nil else { return }
        busy = (id, action)
        actionError = nil
        do {
            try await model.cloudSandboxAction(deviceId: deviceId, id: id, action: action)
        } catch {
            actionError = CodegraffHosts.actionFailure(error, deviceName: model.deviceName(deviceId))
        }
        busy = nil
        await reload()
    }

    private func turnOffAll(_ ids: [String]) async {
        guard let deviceId, !turningOffAll else { return }
        turningOffAll = true
        actionError = nil
        for id in ids {
            do {
                try await model.cloudSandboxAction(deviceId: deviceId, id: id, action: .stop)
            } catch {
                actionError = CodegraffHosts.actionFailure(error, deviceName: model.deviceName(deviceId))
                break
            }
        }
        turningOffAll = false
        await reload()
    }

    private func create(size: String?) async {
        guard let deviceId, !creating else { return }
        creating = true
        actionError = nil
        do {
            try await model.createCloudSandbox(deviceId: deviceId, size: size)
        } catch {
            actionError = CodegraffHosts.actionFailure(error, deviceName: model.deviceName(deviceId))
        }
        creating = false
        await reload()
    }
}
