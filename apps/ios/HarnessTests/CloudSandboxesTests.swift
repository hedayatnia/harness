// The cloud sandbox rules beyond the shared vector (ParityTests runs that), ported from
// crates/ui/src/codegraff_sandboxes.rs's own tests, plus the engine's wire shape, the
// relay params and demo mode's account.

import XCTest
@testable import Harness

@MainActor
final class CloudSandboxesTests: XCTestCase {
    private let noon = ISO8601DateFormatter().date(from: "2026-10-01T12:00:00Z")!

    private func sandbox(_ state: String, harness: Bool = true, expires: String? = nil) -> CodegraffSandbox {
        CodegraffSandbox(id: "cnd_1", provider: "fleet", name: "Harness cloud", state: state,
                         expiresAt: expires.map(JSONValue.string), harness: harness)
    }

    private func kept(_ expires: String, renewError: String? = nil) -> CodegraffSandbox {
        var s = sandbox("running", expires: expires)
        s.keepRunning = renewError == nil
        s.renewError = renewError
        return s
    }

    private func priced(_ id: String, _ state: String, _ price: Int64?) -> CodegraffSandbox {
        var s = sandbox(state)
        s.id = id
        s.keepRunning = true
        s.pricePerHourMicroUsd = price
        s.cpu = 2
        s.memoryGiB = 4
        return s
    }

    func testKeptRunningRowsSaySoInsteadOfCountingDown() {
        XCTAssertEqual(CloudSandboxRules.detail(kept("2026-10-01T12:24:00Z"), now: noon),
                       "Running · Harness ready · keeps running until you turn it off")
        var off = kept("2026-10-01T12:24:00Z")
        off.keepRunning = false
        XCTAssertEqual(CloudSandboxRules.detail(off, now: noon), "Running · Harness ready · pauses in 24 min")
    }

    func testFailedRenewalsSayWhyAndWhenItPauses() {
        for (error, words) in [("insufficient_credit", "Out of credit"), ("key_budget", "Key budget reached"),
                               ("fleet_unavailable", "Couldn't renew"), ("something_new", "Couldn't renew")] {
            XCTAssertEqual(CloudSandboxRules.detail(kept("2026-10-01T12:04:00Z", renewError: error), now: noon),
                           "Running · Harness ready · \(words) — pauses in 4 min", error)
        }
        var noExpiry = kept("x", renewError: "insufficient_credit")
        noExpiry.expiresAt = nil
        XCTAssertEqual(CloudSandboxRules.detail(noExpiry, now: noon), "Running · Harness ready · Out of credit")
        // A renew error wins over a keep-running flag that hasn't caught up.
        var both = kept("2026-10-01T12:04:00Z", renewError: "key_budget")
        both.keepRunning = true
        XCTAssertTrue(CloudSandboxRules.detail(both, now: noon).hasSuffix("Key budget reached — pauses in 4 min"))
    }

    func testOnlyRunningFleetSandboxesThatWillPauseOfferKeepRunning() {
        var off = kept("2026-10-01T12:04:00Z", renewError: "insufficient_credit")
        XCTAssertTrue(CloudSandboxRules.canKeepRunning(off))
        off.provider = "daytona"
        XCTAssertFalse(CloudSandboxRules.canKeepRunning(off))
        XCTAssertFalse(CloudSandboxRules.canKeepRunning(kept("2026-10-01T12:04:00Z")))
        XCTAssertFalse(CloudSandboxRules.canKeepRunning(sandbox("running")), "older gateways say nothing")
        var paused = sandbox("paused")
        paused.keepRunning = false
        XCTAssertFalse(CloudSandboxRules.canKeepRunning(paused))
    }

    func testTheSummaryShowsOnlyWhileSomethingRuns() {
        XCTAssertNil(CloudSandboxRules.runningSummary(CodegraffSandboxes(enabled: true), now: noon))
        XCTAssertNil(CloudSandboxRules.runningSummary(CodegraffSandboxes(enabled: true, sandboxes: [sandbox("paused")]), now: noon))
        let off = CodegraffSandboxes(enabled: false, sandboxes: [kept("2026-10-01T12:24:00Z")])
        XCTAssertNil(CloudSandboxRules.runningSummary(off, now: noon), "feature off")
        XCTAssertFalse(CloudSandboxRules.needsRefresh(off))

        let one = CloudSandboxRules.runningSummary(CodegraffSandboxes(enabled: true, sandboxes: [kept("2026-10-01T12:24:00Z")]), now: noon)
        XCTAssertEqual(one, CloudRunningSummary(label: "Cloud sandbox running",
                                                detail: "Billed per second until you turn it off",
                                                warning: false, runningIds: ["cnd_1"]))
    }

    func testTheSummaryPutsTheMostPressingWordFirst() throws {
        var other = kept("2026-10-01T12:24:00Z")
        other.id = "cnd_2"
        let failing = kept("2026-10-01T12:04:00Z", renewError: "insufficient_credit")
        let two = try XCTUnwrap(CloudSandboxRules.runningSummary(CodegraffSandboxes(enabled: true, sandboxes: [other, failing]), now: noon))
        XCTAssertEqual(two.label, "2 cloud sandboxes running")
        XCTAssertEqual(two.detail, "Out of credit — pauses in 4 min")
        XCTAssertTrue(two.warning)
        XCTAssertEqual(two.runningIds, ["cnd_2", "cnd_1"])
        XCTAssertEqual(CloudSandboxRules.compactTooltip(two), "2 cloud sandboxes running — Out of credit — pauses in 4 min")

        let old = try XCTUnwrap(CloudSandboxRules.runningSummary(
            CodegraffSandboxes(enabled: true, sandboxes: [sandbox("running", expires: "2026-10-01T12:24:00Z")]), now: noon))
        XCTAssertEqual(old.detail, "Pauses in 24 min")
        XCTAssertFalse(old.warning)

        let starting = try XCTUnwrap(CloudSandboxRules.runningSummary(
            CodegraffSandboxes(enabled: true, sandboxes: [sandbox("starting", harness: false)]), now: noon))
        XCTAssertEqual(starting.label, "Cloud sandbox starting")
        XCTAssertTrue(starting.runningIds.isEmpty, "nothing to turn off yet")
    }

    func testTheListIsReReadOnlyWhileSomethingCanChangeOnItsOwn() {
        XCTAssertFalse(CloudSandboxRules.needsRefresh(CodegraffSandboxes(enabled: true, sandboxes: [sandbox("paused")])))
        for state in ["running", "starting", "stopping"] {
            XCTAssertTrue(CloudSandboxRules.needsRefresh(CodegraffSandboxes(enabled: true, sandboxes: [sandbox(state)])), state)
        }
    }

    func testRunningRowsShowHarnessAndWhenTheLeaseEnds() {
        XCTAssertTrue(CloudSandboxRules.detail(sandbox("running", expires: "2026-10-01T14:05:00Z"), now: noon)
            .hasSuffix("pauses in 2h 5m"))
        XCTAssertEqual(CloudSandboxRules.detail(sandbox("running", harness: false, expires: "2026-10-01T11:00:00Z"), now: noon),
                       "Running · Harness not set up · lease ended")
        // Fractional-second timestamps read too.
        XCTAssertTrue(CloudSandboxRules.detail(sandbox("running", expires: "2026-10-01T12:10:00.250Z"), now: noon)
            .hasSuffix("pauses in 10 min"))
    }

    func testStartAndStopFollowStateAndProvider() {
        XCTAssertTrue(CloudSandboxRules.canStart(sandbox("paused")))
        XCTAssertFalse(CloudSandboxRules.canStart(sandbox("running")))
        let unready = sandbox("running", harness: false)
        XCTAssertTrue(CloudSandboxRules.canStart(unready))
        XCTAssertEqual(CloudSandboxRules.startLabels(unready).label, "Set up Harness")
        XCTAssertEqual(CloudSandboxRules.startLabels(unready).busy, "Setting up…")
        XCTAssertEqual(CloudSandboxRules.startLabels(sandbox("paused")).busy, "Starting…")
        var daytona = sandbox("paused", harness: false)
        daytona.provider = "daytona"
        XCTAssertFalse(CloudSandboxRules.canStart(daytona))
        XCTAssertTrue(CloudSandboxRules.canStop(sandbox("running")))
        XCTAssertFalse(CloudSandboxRules.canStop(sandbox("paused")))
        for state in ["starting", "stopping", "error"] {
            XCTAssertFalse(CloudSandboxRules.canStart(sandbox(state, harness: false)) || CloudSandboxRules.canStop(sandbox(state)), state)
        }
    }

    func testRatesAndMachinesReadPlainly() {
        XCTAssertEqual(CloudSandboxRules.formatRate(2_000_000), "$2.00/h")
        XCTAssertEqual(CloudSandboxRules.formatRate(-5), "$0.00/h")
        XCTAssertEqual(CloudSandboxRules.machineSpec(cpu: 2, memoryGiB: 4), "2 vCPU · 4 GB")
        XCTAssertEqual(CloudSandboxRules.machineSpec(cpu: 0.5, memoryGiB: 0.5), "0.5 vCPU · 0.5 GB")
        XCTAssertNil(CloudSandboxRules.machineSpec(cpu: 2, memoryGiB: nil))
        XCTAssertNil(CloudSandboxRules.machineSpec(cpu: nil, memoryGiB: 4))
        XCTAssertEqual(CloudSandboxRules.sizeDetail(PersonalSize(id: "x", cpu: 1, memoryGiB: 2)), "1 vCPU · 2 GB")
        XCTAssertEqual(CloudSandboxRules.sizeTitle(PersonalSize(id: " large ", cpu: 4, memoryGiB: 8)), "Large")
    }

    func testRunningRowsShowTheMachineAndRateInsteadOfHarnessReady() {
        let row = priced("cnd_1", "running", 100_000)
        XCTAssertEqual(CloudSandboxRules.detail(row, now: noon),
                       "Running · 2 vCPU · 4 GB · $0.10/h · keeps running until you turn it off")
        var unpriced = row
        unpriced.pricePerHourMicroUsd = nil
        XCTAssertEqual(CloudSandboxRules.detail(unpriced, now: noon),
                       "Running · 2 vCPU · 4 GB · keeps running until you turn it off")
    }

    func testTheSummaryAddsTheSummedRateOnlyWhenEveryRunningOneHasAPrice() throws {
        let two = CodegraffSandboxes(enabled: true, sandboxes: [
            priced("a", "running", 100_000), priced("b", "running", 200_000), priced("c", "paused", 900_000),
        ])
        XCTAssertEqual(try XCTUnwrap(CloudSandboxRules.runningSummary(two, now: noon)).label,
                       "2 cloud sandboxes running · $0.30/h")
        let partly = CodegraffSandboxes(enabled: true, sandboxes: [priced("a", "running", 100_000), priced("b", "running", nil)])
        XCTAssertNil(CloudSandboxRules.runningRate(partly))
        let starting = CodegraffSandboxes(enabled: true, sandboxes: [priced("a", "starting", 100_000)])
        XCTAssertEqual(try XCTUnwrap(CloudSandboxRules.runningSummary(starting, now: noon)).label,
                       "Cloud sandbox starting · $0.10/h")
        XCTAssertNil(CloudSandboxRules.runningRate(CodegraffSandboxes(enabled: true)))
    }

    // MARK: Wire

    func testTheEngineAnswerDecodesToleratingNewAndMissingFields() throws {
        let json = """
        {"enabled":true,"sandboxes":[
          {"id":"cnd_1","provider":"fleet","name":null,"state":"running","tier":"personal",
           "expiresAt":"2026-10-01T12:24:00Z","harness":true,"deviceName":"Harness cloud ab12",
           "keepRunning":true,"renewError":null,"cpu":2,"memoryGiB":4,"diskGiB":40,
           "pricePerHourMicroUsd":100000,"future":1},
          {"id":"sb_2","provider":"daytona","state":"paused"}],
         "sizes":[{"id":"standard","cpu":2,"memoryGiB":4,"diskGiB":40,"pricePerHourMicroUsd":100000,"default":true}]}
        """
        let list = try XCTUnwrap(try JSONDecoder().decode(CodegraffSandboxes?.self, from: Data(json.utf8)))
        XCTAssertTrue(list.enabled)
        XCTAssertEqual(list.sandboxes.map(\.id), ["cnd_1", "sb_2"])
        XCTAssertEqual(list.sandboxes[0].pricePerHourMicroUsd, 100_000)
        XCTAssertEqual(list.sandboxes[0].expiresAt, .string("2026-10-01T12:24:00Z"))
        XCTAssertNil(list.sandboxes[1].keepRunning)
        XCTAssertEqual(list.sizes.first?.isDefault, true)

        XCTAssertNil(try JSONDecoder().decode(CodegraffSandboxes?.self, from: Data("null".utf8)), "signed out")
        let older = try JSONDecoder().decode(CodegraffSandboxes.self, from: Data(#"{"enabled":true,"sandboxes":[]}"#.utf8))
        XCTAssertTrue(older.sizes.isEmpty, "a gateway without sizes offers no choice")
        XCTAssertFalse(try JSONDecoder().decode(CodegraffSandboxes.self, from: Data("{}".utf8)).enabled)
    }

    func testActionWordsAreTheEngines() {
        XCTAssertEqual(CloudSandboxAction.allCases.map(\.rawValue),
                       ["start", "stop", "delete", "keep-running-on", "keep-running-off"])
    }

    func testFailuresReadAsTheEngineSaysOrAsAnOlderComputer() {
        XCTAssertEqual(CodegraffHosts.actionFailure(RelayError.rpc("Your Codegraff credit is used up. Add credit and try again."),
                                                    deviceName: "Mac"),
                       "Your Codegraff credit is used up. Add credit and try again.")
        XCTAssertEqual(CodegraffHosts.actionFailure(RelayError.rpc("unknown method: CodegraffSandboxes"), deviceName: "Mac"),
                       "Update Harness on Mac to manage this from your phone.")
        XCTAssertEqual(CodegraffHosts.loadFailure(RelayError.hostOffline, deviceName: "Mac"),
                       "Couldn't reach Mac: The device is offline")
    }

    // MARK: Demo account

    func testDemoActionsChangeTheAccountTheWayTheGatewayWould() throws {
        let demo = DemoCloud.standard(now: noon)
        let paused = try XCTUnwrap(demo.sandboxes.sandboxes.first { $0.state == "paused" })
        demo.act(id: paused.id, action: .start)
        XCTAssertEqual(demo.sandboxes.sandboxes.first { $0.id == paused.id }?.state, "running")
        demo.act(id: paused.id, action: .stop)
        XCTAssertEqual(demo.sandboxes.sandboxes.first { $0.id == paused.id }?.state, "paused")
        demo.act(id: paused.id, action: .delete)
        XCTAssertNil(demo.sandboxes.sandboxes.first { $0.id == paused.id })

        let before = demo.sandboxes.sandboxes.count
        demo.create(size: "large")
        let made = try XCTUnwrap(demo.sandboxes.sandboxes.last)
        XCTAssertEqual(demo.sandboxes.sandboxes.count, before + 1)
        XCTAssertEqual(made.pricePerHourMicroUsd, 200_000)
        XCTAssertEqual(CloudSandboxRules.detail(made, now: noon),
                       "Running · 4 vCPU · 8 GB · $0.20/h · keeps running until you turn it off")
        demo.create(size: nil)
        XCTAssertEqual(demo.sandboxes.sandboxes.last?.cpu, 2, "no pick: the default size")
    }
}
