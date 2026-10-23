import XCTest

/// #424: a refused checkout near the top of a long ref list must be visible
/// where the user is looking, keep the old selection, and survive closing.
@MainActor
final class RefCheckoutFailureUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        continueAfterFailure = false
        app = XCUIApplication()
        XCUIDevice.shared.orientation = .portrait
    }

    func testRefusedCheckoutIsPinnedAboveLongListAndKeepsSelection() {
        app.launchArguments = ["-demo", "-route", "space:space-harness", "-sheet", "newsession",
                               "-many-refs", "-checkout-refused"]
        app.launch()
        let chip = app.buttons["session-ref"]
        XCTAssertTrue(chip.waitForExistence(timeout: 5))
        let loaded = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label CONTAINS %@", "main"),
                                               object: chip)
        XCTAssertEqual(XCTWaiter.wait(for: [loaded], timeout: 5), .completed)
        chip.tap()

        // A plain non-current ref needs a real checkout; the fixture refuses it.
        let target = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "feature/diff-pane")).firstMatch
        XCTAssertTrue(target.waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "chore/batch-30")).firstMatch.exists)
        target.tap()

        let banner = app.descendants(matching: .any)["ref-checkout-error"].firstMatch
        XCTAssertTrue(banner.waitForExistence(timeout: 5))
        XCTAssertTrue(banner.isHittable, "the refusal must be on screen, not below the list")
        XCTAssertTrue(banner.label.contains("Couldn't switch to feature/diff-pane"), banner.label)
        XCTAssertTrue(banner.label.contains("git: synthetic checkout refused"), banner.label)
        XCTAssertTrue(banner.label.contains("main is still selected"), banner.label)
        // The sheet stays open and the row is pickable again (spinner stopped).
        XCTAssertTrue(app.navigationBars["Select ref"].exists)
        XCTAssertTrue(target.isEnabled)

        app.buttons["Close"].tap()
        XCTAssertTrue(chip.waitForExistence(timeout: 5))
        XCTAssertTrue(chip.label.contains("main"), chip.label)
        XCTAssertFalse(chip.label.contains("feature/diff-pane"), chip.label)
        let note = app.staticTexts["session-ref-error"]
        XCTAssertTrue(note.waitForExistence(timeout: 2))
        XCTAssertTrue(note.label.contains("git: synthetic checkout refused"), note.label)
    }
}
