import XCTest

/// The opt-in phone split: with it on, the session list and the open session are on screen together.
@MainActor
final class PhoneSplitUITests: XCTestCase {
    private func launch(_ mode: String, orientation: UIDeviceOrientation) -> XCUIApplication {
        continueAfterFailure = false
        XCUIDevice.shared.orientation = orientation
        Thread.sleep(forTimeInterval: 1)  // let a rotation from the previous test finish
        let app = XCUIApplication()
        // Opens the session straight away (the route the screenshot tests use), so nothing depends on a row's
        // position in a list that may be scrolled.
        app.launchArguments = ["-demo", "-sethomefilter", "", "-route", "chat:chat-tabs", "-phoneSplit", mode]
        app.launch()
        return app
    }

    private func header(_ app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any)["session-header"]
    }

    /// The list pane's own button: present only while the list is on screen.
    private func listButton(_ app: XCUIApplication) -> XCUIElement {
        app.buttons["new-session"]
    }

    /// A Max-class iPhone is regular width in landscape: it gets the sidebar and the two-pane split screen
    /// (SplitScreenUITests), never the compact phone split, so the landscape cases here do not apply.
    private func skipIfRegularWidthInLandscape(_ app: XCUIApplication) throws {
        let width = app.windows.firstMatch.frame.width
        try XCTSkipIf(width >= 900, "a Max-class iPhone in landscape is regular width (window is \(width)pt wide)")
    }

    private func attachScreenshot(named name: String) {
        let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }

    func testSideBySideKeepsTheListBesideTheOpenSession() throws {
        let app = launch("sideBySide", orientation: .landscapeLeft)
        try skipIfRegularWidthInLandscape(app)
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        Thread.sleep(forTimeInterval: 1)
        attachScreenshot(named: "side-by-side")
        XCTAssertTrue(listButton(app).exists, "the list is still on screen next to the session")
        XCTAssertLessThan(listButton(app).frame.maxX, header(app).frame.minX, "the list sits left of the session")
    }

    func testStackedKeepsTheListAboveTheOpenSession() {
        let app = launch("stacked", orientation: .portrait)
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        Thread.sleep(forTimeInterval: 1)
        attachScreenshot(named: "stacked")
        XCTAssertTrue(listButton(app).exists, "the list is still on screen above the session")
        XCTAssertLessThan(listButton(app).frame.maxY, header(app).frame.minY, "the list sits above the session")
    }

    private func launchRollout(_ half: String, onboardingSeen: Bool,
                               chat: String = "chat-tabs") -> XCUIApplication {
        continueAfterFailure = false
        XCUIDevice.shared.orientation = .portrait
        Thread.sleep(forTimeInterval: 1)
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-sethomefilter", "", "-route", "chat:\(chat)", "-phoneSplit", "stacked",
                               "-rollout.stackedSplitDrag", half,
                               "-onboarding.stackedSplitDrag.seen", onboardingSeen ? "YES" : "NO"]
        app.launch()
        return app
    }

    func testTheOnHalfMeetsTheHandleOnceAndGotItPutsTheCalloutAway() {
        let app = launchRollout("on", onboardingSeen: false)
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        let callout = app.descendants(matching: .any)["stacked-drag-onboarding"]
        XCTAssertTrue(callout.waitForExistence(timeout: 5), "the on half is introduced to the handle")
        attachScreenshot(named: "stacked-drag-onboarding")
        app.buttons["stacked-drag-onboarding-dismiss"].tap()
        XCTAssertFalse(callout.waitForExistence(timeout: 2), "Got it puts the callout away")
        XCTAssertTrue(app.buttons["stacked-split-handle"].exists, "the handle stays")
    }

    func testTheOffHalfKeepsThePlainDivider() {
        let app = launchRollout("off", onboardingSeen: false)
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        XCTAssertTrue(listButton(app).isHittable, "the stacked split is there")
        XCTAssertFalse(app.buttons["stacked-split-handle"].exists, "no handle in the off half")
        XCTAssertFalse(app.descendants(matching: .any)["stacked-drag-onboarding"].exists, "and no introduction")
        attachScreenshot(named: "stacked-off-half")
    }

    func testDraggingTheStackedHandleUpGivesTheSessionTheScreenAndItsIconBringsTheListBack() {
        let app = launchRollout("on", onboardingSeen: true)
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        let handle = app.buttons["stacked-split-handle"]
        XCTAssertTrue(handle.waitForExistence(timeout: 5), "no split handle")
        XCTAssertTrue(listButton(app).isHittable, "the list starts open")

        // Drag the handle to the top of the screen.
        let start = handle.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        let top = app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.08))
        start.press(forDuration: 0.1, thenDragTo: top)
        Thread.sleep(forTimeInterval: 0.8)
        attachScreenshot(named: "stacked-folded")
        XCTAssertFalse(listButton(app).isHittable, "the list folds away")
        XCTAssertTrue(header(app).isHittable, "the session fills the screen")
        XCTAssertEqual(handle.label, "Show the session list", "folded, the handle offers the list back")

        handle.tap()
        Thread.sleep(forTimeInterval: 0.8)
        XCTAssertTrue(listButton(app).isHittable, "tapping the icon brings the list back")
        XCTAssertEqual(handle.label, "Hide the session list")
    }

    /// Back from a session that had the whole screen returns to the list, not to an empty pane with the
    /// list folded away.
    func testBackFromAFullScreenSessionReturnsToTheList() {
        let app = launchRollout("on", onboardingSeen: true)
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        let handle = app.buttons["stacked-split-handle"]
        XCTAssertTrue(handle.waitForExistence(timeout: 5), "no split handle")
        let start = handle.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        let top = app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.08))
        start.press(forDuration: 0.1, thenDragTo: top)
        Thread.sleep(forTimeInterval: 0.8)
        XCTAssertFalse(listButton(app).isHittable, "the session has the screen")

        // The session's own Back: the folded list keeps a navigation bar of its own.
        let headerY = header(app).frame.midY
        let back = app.navigationBars.buttons.allElementsBoundByIndex.first {
            $0.isHittable && abs($0.frame.midY - headerY) < 30 && $0.frame.maxX < header(app).frame.minX
        }
        XCTAssertNotNil(back, "no Back button beside the session header")
        back?.tap()
        Thread.sleep(forTimeInterval: 0.8)
        attachScreenshot(named: "stacked-back-to-list")
        XCTAssertFalse(header(app).exists, "the session closed")
        XCTAssertTrue(listButton(app).isHittable, "Back returns to the list")
    }

    /// Typing an answer to a question gives the session the screen above the keyboard, as the composer
    /// does, so the answer field is not hidden behind the keyboard.
    func testTypingAnAnswerGivesTheSessionTheScreen() {
        let app = launchRollout("on", onboardingSeen: true, chat: "chat-picker")
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        XCTAssertTrue(listButton(app).isHittable, "the list starts open")
        let answer = app.textFields["Or type your own answer"]
        XCTAssertTrue(answer.waitForExistence(timeout: 5), "no question panel")
        answer.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), "the keyboard came up")
        answer.typeText("both, but local first")
        Thread.sleep(forTimeInterval: 0.8)
        attachScreenshot(named: "stacked-question-typing")
        XCTAssertFalse(listButton(app).isHittable, "the list gets out of the way")
        XCTAssertTrue(answer.isHittable, "the answer field is above the keyboard")
        XCTAssertLessThan(answer.frame.maxY, app.keyboards.firstMatch.frame.minY)
    }

    func testDraggingTheStackedHandleDownGivesTheListTheScreenAndItsIconBringsTheSessionBack() {
        let app = launchRollout("on", onboardingSeen: true)
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        let handle = app.buttons["stacked-split-handle"]
        XCTAssertTrue(handle.waitForExistence(timeout: 5), "no split handle")

        let start = handle.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        let bottom = app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.95))
        start.press(forDuration: 0.1, thenDragTo: bottom)
        Thread.sleep(forTimeInterval: 0.8)
        attachScreenshot(named: "stacked-list-full")
        XCTAssertTrue(listButton(app).isHittable, "the list has the screen")
        XCTAssertFalse(header(app).isHittable, "the session folds away below it")
        XCTAssertEqual(handle.label, "Show the session", "folded down, the handle offers the session back")
        XCTAssertGreaterThan(handle.frame.minY, app.windows.firstMatch.frame.height * 0.8,
                             "the handle waits at the bottom")

        handle.tap()
        Thread.sleep(forTimeInterval: 0.8)
        XCTAssertTrue(header(app).isHittable, "tapping the icon brings the session back")
        XCTAssertTrue(listButton(app).isHittable, "in the split, under the list")
        XCTAssertEqual(handle.label, "Hide the session list")
    }

    /// Typing in a stacked split gives the session the screen above the keyboard; sending puts the keyboard
    /// away and the list comes back. Both halves of the rollout get this.
    private func typeAndSend(_ half: String) {
        let app = launchRollout(half, onboardingSeen: true)
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        XCTAssertTrue(listButton(app).isHittable, "the list starts open")

        let input = app.textViews["composer-input"]
        XCTAssertTrue(input.waitForExistence(timeout: 5), "no composer")
        input.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), "the keyboard came up")
        input.typeText("does the list get out of the way")
        Thread.sleep(forTimeInterval: 0.8)
        attachScreenshot(named: "stacked-typing-\(half)")
        XCTAssertFalse(listButton(app).isHittable, "typing gives the session the screen")
        XCTAssertTrue(header(app).isHittable, "the session's header is still on screen")
        XCTAssertGreaterThan(app.keyboards.count, 0, "and the keyboard stays up while typing")

        app.buttons["composer-send"].tap()
        Thread.sleep(forTimeInterval: 1)
        attachScreenshot(named: "stacked-sent-\(half)")
        XCTAssertEqual(app.keyboards.count, 0, "sending puts the keyboard away")
        XCTAssertTrue(listButton(app).isHittable, "and the list comes back")
    }

    func testTypingInTheStackedSplitGivesTheSessionTheScreenAndSendingBringsTheListBack() {
        typeAndSend("on")
    }

    func testTheOffHalfAlsoGivesTheSessionTheScreenWhileTyping() {
        typeAndSend("off")
    }

    func testOffKeepsTheSingleStackAndTheSessionReplacesTheList() {
        let app = launch("off", orientation: .portrait)
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        XCTAssertFalse(listButton(app).isHittable, "with the split off, an open session replaces the list")
    }

    // MARK: the one-time offer

    /// Home with the split off, and the offer's memory seeded as "three sessions opened in the last minute".
    private func launchJuggling(orientation: UIDeviceOrientation) -> XCUIApplication {
        continueAfterFailure = false
        XCUIDevice.shared.orientation = orientation
        Thread.sleep(forTimeInterval: 1)
        let now = Date().timeIntervalSince1970
        // Launch arguments carry plain strings only, so the memory is a comma-separated list of times.
        let opens = "\(now - 30),\(now - 20),\(now - 10)"
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-sethomefilter", "", "-phoneSplit", "off",
                               "-splitSuggestion.opens", opens, "-splitSuggestion.shown", "0",
                               // An earlier test's answer is saved in the app's defaults; pin it to "long ago".
                               "-splitSuggestion.lastShownAt", "0"]
        app.launch()
        return app
    }

    private func offer(_ app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any)["split-offer"]
    }

    func testTheOfferAppearsWithRoomAndTryingItSplitsTheScreen() throws {
        let app = launchJuggling(orientation: .landscapeLeft)
        try skipIfRegularWidthInLandscape(app)
        XCTAssertTrue(offer(app).waitForExistence(timeout: 10), "juggling with room should offer the split")
        attachScreenshot(named: "split-offer")
        app.buttons["split-offer-try"].tap()
        XCTAssertTrue(app.staticTexts["Pick a session, or start one with +"].waitForExistence(timeout: 5),
                      "Try it should turn the split on")
        XCTAssertFalse(offer(app).exists, "the offer goes away once answered")
    }

    func testNotNowHidesTheOfferAndLeavesTheSplitOff() throws {
        let app = launchJuggling(orientation: .landscapeLeft)
        try skipIfRegularWidthInLandscape(app)
        XCTAssertTrue(offer(app).waitForExistence(timeout: 10))
        app.buttons["split-offer-later"].tap()
        XCTAssertFalse(offer(app).waitForExistence(timeout: 2))
        XCTAssertFalse(app.staticTexts["Pick a session, or start one with +"].exists, "the split stays off")
    }

    /// Upright, the offer is the stacked split: trying it keeps the list above the next session opened.
    func testPortraitOffersTheStackedSplit() {
        let app = launchJuggling(orientation: .portrait)
        XCTAssertTrue(offer(app).waitForExistence(timeout: 10), "juggling in portrait should offer the stacked split")
        attachScreenshot(named: "split-offer-portrait")
        app.buttons["split-offer-try"].tap()
        XCTAssertFalse(offer(app).waitForExistence(timeout: 2), "the offer goes away once answered")
        app.staticTexts["Tool group header colors"].firstMatch.tap()
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        XCTAssertTrue(listButton(app).isHittable, "the list stays on screen above the session")
        XCTAssertLessThan(listButton(app).frame.maxY, header(app).frame.minY, "the list sits above the session")
    }

    /// With the stacked split on and nothing open, the list has the whole screen: no empty placeholder below it,
    /// at launch and after Back from a session.
    private func nothingOpenGivesTheListTheScreen(_ half: String) {
        let app = launchRollout(half, onboardingSeen: true)
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        let headerY = header(app).frame.midY
        let back = app.navigationBars.buttons.allElementsBoundByIndex.first {
            $0.isHittable && abs($0.frame.midY - headerY) < 30 && $0.frame.maxX < header(app).frame.minX
        }
        XCTAssertNotNil(back, "no Back button beside the session header")
        back?.tap()
        Thread.sleep(forTimeInterval: 0.8)
        attachScreenshot(named: "stacked-nothing-open-\(half)")
        XCTAssertTrue(listButton(app).isHittable, "Back returns to the list")
        XCTAssertFalse(app.staticTexts["Pick a session, or start one with +"].isHittable,
                       "no empty placeholder below the list")
        XCTAssertFalse(app.buttons["stacked-split-handle"].exists, "no handle with nothing below it")
    }

    func testWithNothingOpenTheListHasTheScreen() { nothingOpenGivesTheListTheScreen("on") }
    func testWithNothingOpenTheListHasTheScreenWithoutTheHandle() { nothingOpenGivesTheListTheScreen("off") }

    /// With nothing open, the list has the whole screen. A new session opened from "+" used to land in the
    /// detail below it and stay folded away, so "+" looked like it did nothing.
    func testNewSessionFromAFullScreenListOpensBelowIt() {
        continueAfterFailure = false
        XCUIDevice.shared.orientation = .portrait
        Thread.sleep(forTimeInterval: 1)
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-sethomefilter", "", "-phoneSplit", "stacked",
                               "-rollout.stackedSplitDrag", "on", "-onboarding.stackedSplitDrag.seen", "YES"]
        app.launch()
        XCTAssertTrue(listButton(app).waitForExistence(timeout: 10), "the list is on screen")
        listButton(app).tap()
        let project = app.buttons["new-session-space-space-harness"]
        XCTAssertTrue(project.waitForExistence(timeout: 5), "the project picker opened")
        project.tap()
        let context = app.descendants(matching: .any)["new-session-context"]
        XCTAssertTrue(context.waitForExistence(timeout: 5), "the new session opened")
        Thread.sleep(forTimeInterval: 0.8)
        attachScreenshot(named: "stacked-new-session-from-list")
        XCTAssertTrue(context.isHittable, "the new session is on screen, not folded away below the list")
    }

    /// The lower pane's list has Home's archived shelf too, and an archived session opens there.
    func testTheLowerPaneOffersArchivedSessions() {
        let app = launchRollout("on", onboardingSeen: true)
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        let split = app.buttons["stacked-split-open"]
        XCTAssertTrue(split.waitForExistence(timeout: 5), "the session offers a split")
        XCTAssertTrue(split.label.contains("below"), "the split button says what it does")
        split.tap()

        let picker = app.descendants(matching: .any)["stacked-session-picker"]
        XCTAssertTrue(picker.waitForExistence(timeout: 5), "the session list opened below")
        let archived = picker.buttons.matching(NSPredicate(format: "label CONTAINS %@", "OKLCH conversion drift")).firstMatch
        for _ in 0..<3 where !archived.isHittable { picker.swipeUp() }
        XCTAssertTrue(archived.isHittable, "the archived shelf is in the list")
        attachScreenshot(named: "stacked-picker-archived")
        archived.tap()

        let headers = app.descendants(matching: .any).matching(identifier: "session-header")
        XCTAssertTrue(headers.element(boundBy: 1).waitForExistence(timeout: 5), "the archived session opened below")
    }

    /// ⌘D on the phone: a session alone in the stacked split opens the session list below it (Home's rows).
    /// Picking one shows both sessions; × closes either pane.
    func testStackedSplitPicksAnExistingSessionBelowAndEitherPaneCloses() {
        let app = launchRollout("on", onboardingSeen: true)
        XCTAssertTrue(header(app).waitForExistence(timeout: 10), "the session did not open")
        let split = app.buttons["stacked-split-open"]
        XCTAssertTrue(split.waitForExistence(timeout: 5), "the session offers a split")
        split.tap()

        let picker = app.descendants(matching: .any)["stacked-session-picker"]
        XCTAssertTrue(picker.waitForExistence(timeout: 5), "the session list opened below")
        let other = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Streaming veil")).firstMatch
        XCTAssertTrue(other.waitForExistence(timeout: 5), "the list shows the other sessions")
        XCTAssertEqual(picker.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Tool group header")).count, 0,
                       "the open session is not offered again")
        attachScreenshot(named: "stacked-picker")
        other.tap()

        let headers = app.descendants(matching: .any).matching(identifier: "session-header")
        XCTAssertTrue(headers.element(boundBy: 1).waitForExistence(timeout: 5), "two sessions are on screen")
        Thread.sleep(forTimeInterval: 0.8)
        attachScreenshot(named: "stacked-two-sessions")
        XCTAssertFalse(app.buttons["stacked-split-open"].exists, "two panes at most")
        let closes = app.buttons.matching(identifier: "stacked-pane-close")
        XCTAssertEqual(closes.count, 2, "each pane has its own ×")

        // × on the lower pane: back to the first session with the list above.
        closes.element(boundBy: 1).tap()
        Thread.sleep(forTimeInterval: 0.8)
        XCTAssertEqual(headers.count, 1, "one session left")
        XCTAssertTrue(listButton(app).isHittable, "the list is back above it")

        // Split again, pick, then × on the upper pane: the lower session takes its place.
        app.buttons["stacked-split-open"].tap()
        XCTAssertTrue(other.waitForExistence(timeout: 5))
        other.tap()
        XCTAssertTrue(headers.element(boundBy: 1).waitForExistence(timeout: 5))
        closes.element(boundBy: 0).tap()
        Thread.sleep(forTimeInterval: 0.8)
        XCTAssertEqual(headers.count, 1, "one session left")
        XCTAssertTrue(header(app).label.contains("Streaming veil"), "the lower session moved up: \(header(app).label)")
    }
}
