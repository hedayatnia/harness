import XCTest

/// "+" → New chat: a plain chat on a device, offered only the Auto tiers.
@MainActor
final class ChatModeUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        continueAfterFailure = false
        app = XCUIApplication()
        XCUIDevice.shared.orientation = .portrait
        app.launchArguments = ["-demo", "-sethomefilter", ""]
        app.launch()
    }

    private var composer: XCUIElement {
        app.descendants(matching: .any)["composer-input"].firstMatch
    }

    private func chip(_ label: String) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label CONTAINS %@", label)).firstMatch
    }

    func testANewChatOffersOnlyTheAutoTiersAndKeepsItsMode() {
        let newSession = app.buttons["new-session"]
        XCTAssertTrue(newSession.waitForExistence(timeout: 5))
        newSession.tap()
        app.buttons["new-chat"].tap()
        XCTAssertTrue(app.navigationBars["Select a device"].waitForExistence(timeout: 5))
        app.buttons["session-host-dev-mac"].tap()

        let context = app.staticTexts["new-session-context"]
        XCTAssertTrue(context.waitForExistence(timeout: 5))
        XCTAssertTrue(context.label.hasPrefix("Chat"), context.label)
        XCTAssertFalse(app.buttons["session-checkout"].exists, "a chat has no checkout")
        // The model chip opens the three tiers and nothing else. (A chat's
        // last pick is remembered, so start from Auto (low) explicitly.)
        let modelChip = chip("Auto (")
        XCTAssertTrue(modelChip.waitForExistence(timeout: 5))
        modelChip.tap()
        XCTAssertTrue(app.staticTexts["Auto (smart)"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Auto (high)"].exists)
        XCTAssertFalse(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "Claude")).firstMatch.exists)
        app.staticTexts["Auto (low)"].tap()
        app.buttons["Close"].tap()
        XCTAssertTrue(chip("Auto (low)").waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["chat-mode-note"].exists, "no cost note on Auto (low)")

        // Picking smart shows the cost note.
        chip("Auto (low)").tap()
        XCTAssertTrue(app.staticTexts["Auto (smart)"].waitForExistence(timeout: 5))
        app.staticTexts["Auto (smart)"].tap()
        app.buttons["Close"].tap()
        XCTAssertTrue(app.staticTexts["chat-mode-note"].waitForExistence(timeout: 5))
        XCTAssertTrue(chip("Auto (smart)").waitForExistence(timeout: 5))

        let prompt = "What should I cook tonight?"
        composer.tap()
        composer.typeText(prompt)
        app.buttons["composer-send"].tap()
        let transcript = app.otherElements["transcript"]
        XCTAssertTrue(transcript.waitForExistence(timeout: 5))
        XCTAssertTrue(transcript.staticTexts[prompt].waitForExistence(timeout: 5))

        // The open chat keeps its mode: the same three choices and the note
        // (the composer shows its chips once it expands).
        composer.tap()
        XCTAssertTrue(chip("Auto (smart)").waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["chat-mode-note"].exists)
        chip("Auto (smart)").tap()
        XCTAssertTrue(app.staticTexts["Auto (low)"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "Claude")).firstMatch.exists)
    }
}
