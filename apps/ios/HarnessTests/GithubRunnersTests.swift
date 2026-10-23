// GitHub CI rules, ported from crates/ui/src/codegraff_runners.rs's tests, plus the action
// params the engine's `RunnerAction` reads and demo mode's account.

import XCTest
@testable import Harness

@MainActor
final class GithubRunnersTests: XCTestCase {
    private func listed() throws -> GithubRunners {
        let json = """
        {"available":true,"enabled":true,
         "installUrl":"https://github.com/apps/codegraff/installations/new",
         "installations":[{"installationId":"4242","account":"octo-org","accountType":"Organization",
           "suspended":false,"monthlyBudgetMicroUsd":50000000,"usedThisMonthMicroUsd":12500000,
           "maxConcurrent":3,"repos":[
             {"fullName":"octo-org/app","enabled":true,"defaultSize":"large","private":true},
             {"fullName":"octo-org/docs","enabled":false}]}],
         "recentJobs":[],
         "sizes":[{"key":"standard","cpu":2,"memoryGiB":4,"diskGiB":40,"pricePerHourMicroUsd":100000},
                  {"key":"large","cpu":4,"memoryGiB":8,"diskGiB":40,"pricePerHourMicroUsd":200000}]}
        """
        return try JSONDecoder().decode(GithubRunners.self, from: Data(json.utf8))
    }

    func testThePanelFollowsSignInAvailabilityAndInstalls() throws {
        XCTAssertEqual(GithubRunnerRules.panel(nil), .signedOut)
        XCTAssertEqual(GithubRunnerRules.panel(GithubRunners()), .unavailable)
        XCTAssertEqual(GithubRunnerRules.panel(GithubRunners(available: true)), .comingSoon)
        XCTAssertEqual(GithubRunnerRules.panel(GithubRunners(available: true, enabled: true)), .notConnected)
        XCTAssertEqual(GithubRunnerRules.panel(try listed()), .connected)
        var insecure = try listed()
        insecure.installUrl = "http://example.invalid"
        XCTAssertNil(GithubRunnerRules.installURL(insecure))
        XCTAssertNotNil(GithubRunnerRules.installURL(try listed()))
        XCTAssertNil(try JSONDecoder().decode(GithubRunners?.self, from: Data("null".utf8)), "signed out")
    }

    func testInstallationIdsReadAsTextEvenWhenNumbers() throws {
        let json = #"{"available":true,"enabled":true,"installations":[{"installationId":4242,"account":"me"}]}"#
        let runners = try JSONDecoder().decode(GithubRunners.self, from: Data(json.utf8))
        XCTAssertEqual(runners.installations.first?.installationId, "4242")
    }

    func testDollarsReadPlainly() {
        XCTAssertEqual(GithubRunnerRules.formatDollars(50_000_000), "$50")
        XCTAssertEqual(GithubRunnerRules.formatDollars(12_500_000), "$12.50")
        XCTAssertEqual(GithubRunnerRules.formatDollars(5_000), "$0.005")
        XCTAssertEqual(GithubRunnerRules.formatDollars(4_200), "$0.0042")
        XCTAssertEqual(GithubRunnerRules.formatDollars(0), "$0")
        XCTAssertEqual(GithubRunnerRules.formatDollars(-2_000_000), "-$2")
        XCTAssertEqual(GithubRunnerRules.budgetInputText(12_500_000), "12.50")
        XCTAssertEqual(GithubRunnerRules.budgetInputText(nil), "")
    }

    func testBudgetsParseAndRefuseNonsense() {
        XCTAssertEqual(GithubRunnerRules.parseBudget("50"), .success(50_000_000))
        XCTAssertEqual(GithubRunnerRules.parseBudget(" $1,250.5 "), .success(1_250_500_000))
        XCTAssertEqual(GithubRunnerRules.parseBudget("0.25"), .success(250_000))
        XCTAssertEqual(GithubRunnerRules.parseBudget(".5"), .success(500_000))
        XCTAssertEqual(GithubRunnerRules.parseBudget("0"), .success(0))
        XCTAssertEqual(GithubRunnerRules.parseBudget(""), .failure(BudgetInputError("Enter a budget in dollars.")))
        for bad in ["-5", "abc", "1.2.3", "."] {
            XCTAssertEqual(GithubRunnerRules.parseBudget(bad),
                           .failure(BudgetInputError("Enter a dollar amount, like 50 or 12.50.")), bad)
        }
        XCTAssertEqual(GithubRunnerRules.parseBudget("1.234"), .failure(BudgetInputError("Use at most two decimal places.")))
        XCTAssertEqual(GithubRunnerRules.parseBudget("100001"), .failure(BudgetInputError("Budgets go up to $100,000 a month.")))
        XCTAssertEqual(GithubRunnerRules.parseBudget("99999999999999999999"), .failure(BudgetInputError("That budget is too large.")))
    }

    func testSpendAndTheMeterFollowTheBudget() throws {
        let install = try XCTUnwrap(try listed().installations.first)
        XCTAssertEqual(GithubRunnerRules.spendLine(install), "$12.50 of $50 used this month")
        XCTAssertEqual(GithubRunnerRules.budgetFraction(install), 0.25)
        XCTAssertEqual(GithubRunnerRules.installationMeta(install), "Organization · 1 of 2 repositories on")
        var openEnded = install
        openEnded.monthlyBudgetMicroUsd = nil
        openEnded.suspended = true
        openEnded.repos = Array(openEnded.repos.prefix(1))
        XCTAssertNil(GithubRunnerRules.budgetFraction(openEnded))
        XCTAssertEqual(GithubRunnerRules.spendLine(openEnded), "$12.50 used this month · no monthly budget")
        XCTAssertEqual(GithubRunnerRules.installationMeta(openEnded), "Organization · 1 of 1 repository on · Suspended on GitHub")
        var zero = install
        zero.monthlyBudgetMicroUsd = 0
        XCTAssertEqual(GithubRunnerRules.budgetFraction(zero), 1)
        var empty = install
        empty.repos = []
        empty.accountType = " "
        XCTAssertEqual(GithubRunnerRules.installationMeta(empty), "No repositories")
    }

    func testConcurrencyStaysBetweenOneAndEight() {
        XCTAssertEqual(GithubRunnerRules.stepConcurrent(3, delta: 1), 4)
        XCTAssertEqual(GithubRunnerRules.stepConcurrent(8, delta: 1), 8)
        XCTAssertEqual(GithubRunnerRules.stepConcurrent(1, delta: -1), 1)
        XCTAssertEqual(GithubRunnerRules.stepConcurrent(nil, delta: 1), 2)
        XCTAssertEqual(GithubRunnerRules.stepConcurrent(40, delta: -1), 7)
    }

    func testSizesShowTheirMachineAndRate() throws {
        let runners = try listed()
        XCTAssertEqual(GithubRunnerRules.sizeOptionLabel(runners.sizes[0]), "Standard · 2 vCPU · 4 GB · $0.10/h")
        let repos = runners.installations[0].repos
        XCTAssertEqual(GithubRunnerRules.repoSizeLabel(repos[0], sizes: runners.sizes), "Large · $0.20/h")
        XCTAssertEqual(GithubRunnerRules.repoSizeLabel(repos[1], sizes: runners.sizes), "Standard · $0.10/h",
                       "no size named: the first")
        XCTAssertEqual(GithubRunnerRules.repoSizeLabel(RunnerRepo(fullName: "x", defaultSize: "xl"), sizes: runners.sizes), "Xl")
        XCTAssertEqual(GithubRunnerRules.repoSizeLabel(RunnerRepo(fullName: "x"), sizes: []), "Default size")
    }

    func testJobsSayWhatRanHowItWentAndWhatItCost() throws {
        let now = try XCTUnwrap(ISO8601DateFormatter().date(from: "2026-10-22T10:08:05Z"))
        let json = """
        {"repo":"octo-org/app","workflow":"CI","jobName":"test","size":"standard",
         "state":"completed","conclusion":"success",
         "startedAt":"2026-10-22T10:00:05Z","completedAt":"2026-10-22T10:03:05Z",
         "costMicroUsd":5000,"tier":"untrusted",
         "htmlUrl":"https://github.com/octo-org/app/actions/runs/1/job/2"}
        """
        let job = try JSONDecoder().decode(RunnerJob.self, from: Data(json.utf8))
        XCTAssertEqual(GithubRunnerRules.jobTitle(job), "CI · test")
        XCTAssertEqual(GithubRunnerRules.jobStatus(job).text, "Succeeded")
        XCTAssertEqual(GithubRunnerRules.jobStatus(job).tone, .good)
        XCTAssertEqual(GithubRunnerRules.jobMeta(job, now: now), "Standard · 3m 0s · $0.005 · 5 min ago · Fork, isolated")
        XCTAssertEqual(GithubRunnerRules.jobLine(job, now: now), "Succeeded · Standard · 3m 0s · $0.005 · 5 min ago · Fork, isolated")
        XCTAssertNotNil(GithubRunnerRules.jobURL(job))

        let queued = RunnerJob(state: "queued")
        XCTAssertEqual(GithubRunnerRules.jobStatus(queued).text, "Queued")
        XCTAssertEqual(GithubRunnerRules.jobTitle(queued), "Job")
        XCTAssertEqual(GithubRunnerRules.jobMeta(queued, now: now), "")
        XCTAssertEqual(GithubRunnerRules.jobLine(queued, now: now), "Queued")
        let failed = RunnerJob(state: "completed", durationSeconds: 3725, conclusion: "failure",
                               htmlUrl: "javascript:alert(1)")
        XCTAssertEqual(GithubRunnerRules.jobStatus(failed).text, "Failed")
        XCTAssertEqual(GithubRunnerRules.jobStatus(failed).tone, .bad)
        XCTAssertEqual(GithubRunnerRules.jobMeta(failed, now: now), "1h 2m")
        XCTAssertNil(GithubRunnerRules.jobURL(failed))
        XCTAssertEqual(GithubRunnerRules.jobStatus(RunnerJob(state: "action_required")).text, "Action required")
        XCTAssertEqual(GithubRunnerRules.jobStatus(RunnerJob()).text, "Unknown")
        // Millisecond timestamps read too.
        let millis = RunnerJob(state: "completed", completedAt: .int(Int64(now.timeIntervalSince1970 * 1000) - 3 * 3_600_000))
        XCTAssertEqual(GithubRunnerRules.jobMeta(millis, now: now), "3h ago")

        var runners = try listed()
        XCTAssertFalse(GithubRunnerRules.jobsWorking(runners))
        runners.recentJobs.append(queued)
        XCTAssertTrue(GithubRunnerRules.jobsWorking(runners))
        XCTAssertEqual(GithubRunnerRules.formatDuration(45.4), "45s")
    }

    // MARK: Actions

    private func json(_ params: [String: Any]) throws -> NSDictionary {
        let data = try JSONSerialization.data(withJSONObject: params, options: [.sortedKeys])
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? NSDictionary)
    }

    /// The engine reads `{action, installationId, …}` with the tag in kebab-case and camelCase
    /// fields, leaving absent ones as they are.
    func testActionsSendTheEnginesRunnerActionShape() throws {
        XCTAssertEqual(try json(GithubRunnerAction.installation(installationId: "4242", monthlyBudgetMicroUsd: 50_000_000).params),
                       ["action": "installation", "installationId": "4242", "monthlyBudgetMicroUsd": 50_000_000])
        XCTAssertEqual(try json(GithubRunnerAction.installation(installationId: "4242", untrustedBudgetMicroUsd: 0).params),
                       ["action": "installation", "installationId": "4242", "untrustedBudgetMicroUsd": 0])
        XCTAssertEqual(try json(GithubRunnerAction.installation(installationId: "4242", maxConcurrent: 4).params),
                       ["action": "installation", "installationId": "4242", "maxConcurrent": 4])
        XCTAssertEqual(try json(GithubRunnerAction.repo(installationId: "4242", fullName: "octo-org/app", enabled: false).params),
                       ["action": "repo", "installationId": "4242", "fullName": "octo-org/app", "enabled": false])
        XCTAssertEqual(try json(GithubRunnerAction.repo(installationId: "4242", fullName: "octo-org/app", defaultSize: "large").params),
                       ["action": "repo", "installationId": "4242", "fullName": "octo-org/app", "defaultSize": "large"])
        XCTAssertEqual(try json(GithubRunnerAction.disconnect(installationId: "4242").params),
                       ["action": "disconnect", "installationId": "4242"])
    }

    func testEachActionBusiesItsOwnControl() {
        XCTAssertEqual(GithubRunnerAction.installation(installationId: "1", monthlyBudgetMicroUsd: 1).busyKey, "1:monthlyBudgetMicroUsd")
        XCTAssertEqual(GithubRunnerAction.installation(installationId: "1", untrustedBudgetMicroUsd: 1).busyKey, "1:untrustedBudgetMicroUsd")
        XCTAssertEqual(GithubRunnerAction.installation(installationId: "1", maxConcurrent: 2).busyKey, "1:concurrent")
        XCTAssertEqual(GithubRunnerAction.repo(installationId: "1", fullName: "a/b", enabled: true).busyKey, "1:repo:a/b")
        XCTAssertEqual(GithubRunnerAction.disconnect(installationId: "1").busyKey, "1:disconnect")
    }

    func testAppliedActionsChangeOnlyWhatTheyName() throws {
        let runners = try listed()
        let budget = runners.applying(.installation(installationId: "4242", monthlyBudgetMicroUsd: 80_000_000))
        XCTAssertEqual(budget.installations[0].monthlyBudgetMicroUsd, 80_000_000)
        XCTAssertEqual(budget.installations[0].maxConcurrent, 3)
        let repo = runners.applying(.repo(installationId: "4242", fullName: "octo-org/docs", enabled: true, defaultSize: "large"))
        XCTAssertEqual(repo.installations[0].repos[1], RunnerRepo(fullName: "octo-org/docs", enabled: true, defaultSize: "large"))
        XCTAssertEqual(repo.installations[0].repos[0], runners.installations[0].repos[0])
        XCTAssertEqual(runners.applying(.repo(installationId: "nope", fullName: "octo-org/docs", enabled: true)), runners)
        let gone = runners.applying(.disconnect(installationId: "4242"))
        XCTAssertEqual(GithubRunnerRules.panel(gone), .notConnected)
    }

    func testDemoAccountStartsConnectedOrNot() {
        XCTAssertEqual(GithubRunnerRules.panel(DemoCloud.standard().runners), .connected)
        XCTAssertEqual(GithubRunnerRules.panel(DemoCloud.standard(connected: false).runners), .notConnected)
        XCTAssertTrue(GithubRunnerRules.jobsWorking(DemoCloud.standard().runners), "the demo has a running job")
    }
}
