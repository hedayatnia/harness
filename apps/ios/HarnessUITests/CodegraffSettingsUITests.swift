import XCTest

/// Settings → Cloud sandboxes and Settings → GitHub CI against demo mode's made-up account.
@MainActor
final class CodegraffSettingsUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        continueAfterFailure = false
        app = XCUIApplication()
        XCUIDevice.shared.orientation = .portrait
    }

    private func openSettings(_ extra: [String] = []) {
        app.launchArguments = ["-demo", "-sethomefilter", "", "-phoneSplit", "off"] + extra
        app.launch()
        let settings = app.buttons["home-settings"]
        XCTAssertTrue(settings.waitForExistence(timeout: 10))
        settings.tap()
        XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 5))
    }

    private func keep(_ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }

    private func label(_ text: String, timeout: TimeInterval = 5) -> Bool {
        app.staticTexts[text].waitForExistence(timeout: timeout)
    }

    /// Running sandboxes say what they are and cost; a create offers the sizes with their
    /// price, and the new one adds to the running rate.
    func testCloudSandboxesShowSizesAndRates() {
        openSettings()
        keep("settings")
        app.buttons["settings-cloud-sandboxes"].tap()
        XCTAssertTrue(app.navigationBars["Cloud sandboxes"].waitForExistence(timeout: 5))
        XCTAssertTrue(label("Cloud sandbox running · $0.10/h"))
        XCTAssertTrue(label("Running · 2 vCPU · 4 GB · $0.10/h · keeps running until you turn it off"))
        XCTAssertTrue(label("Paused — memory and files kept"))
        keep("cloud-sandboxes")

        let large = app.buttons["cloud-size-large"]
        large.swipeUpUntilHittable(in: app)
        XCTAssertTrue(app.staticTexts["4 vCPU · 8 GB · 40 GB · $0.20/h"].exists)
        large.tap()
        XCTAssertTrue(large.isSelected)
        keep("cloud-sandboxes-create")
        let create = app.buttons["cloud-create"]
        create.swipeUpUntilHittable(in: app)
        create.tap()
        app.swipeDown(velocity: .fast)
        app.swipeDown(velocity: .fast)
        XCTAssertTrue(label("2 cloud sandboxes running · $0.30/h", timeout: 10))
        keep("cloud-sandboxes-created")
    }

    /// Budgets, concurrency and a repository's switch and size, then recent jobs.
    func testGithubCIConnectedAccount() {
        openSettings()
        app.buttons["settings-github-ci"].tap()
        XCTAssertTrue(app.navigationBars["GitHub CI"].waitForExistence(timeout: 5))
        XCTAssertTrue(label("Organization · 2 of 3 repositories on"))
        XCTAssertTrue(label("$12.50 of $50 used this month"))
        XCTAssertTrue(label("3"))
        keep("github-ci")

        let docs = app.switches["github-ci-repo-switch-octo-org/docs"]
        docs.swipeUpUntilHittable(in: app)
        XCTAssertEqual(docs.value as? String, "0")
        // The switch's knob, not the row's middle, takes the tap.
        docs.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)).tap()
        let allOn = app.staticTexts["Organization · 3 of 3 repositories on"]
        app.swipeDown()
        XCTAssertTrue(allOn.waitForExistence(timeout: 5))

        let jobs = app.staticTexts["Recent jobs"]
        jobs.swipeUpUntilHittable(in: app)
        XCTAssertTrue(app.staticTexts["octo-org/app · CI · test"].exists)
        keep("github-ci-jobs")
    }

    func testGithubCIOffersConnectWhenNothingIsConnected() {
        openSettings(["-github-ci-new"])
        app.buttons["settings-github-ci"].tap()
        XCTAssertTrue(label("Run GitHub Actions on Codegraff"))
        let connect = app.buttons["github-ci-connect"]
        XCTAssertTrue(connect.exists)
        XCTAssertTrue(connect.isEnabled)
        XCTAssertEqual(connect.label, "Connect GitHub")
        XCTAssertTrue(app.buttons["github-ci-copy-snippet"].exists)
        keep("github-ci-connect")
    }
}

private extension XCUIElement {
    /// Scroll the list until this element can be tapped.
    func swipeUpUntilHittable(in app: XCUIApplication, attempts: Int = 6) {
        var left = attempts
        while (!exists || !isHittable) && left > 0 {
            app.swipeUp()
            left -= 1
        }
    }
}
