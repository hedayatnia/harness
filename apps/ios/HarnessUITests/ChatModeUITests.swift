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

    private func attachScreenshot(named name: String) {
        let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
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
        XCTAssertTrue(app.buttons["new-chat"].waitForExistence(timeout: 5))
        attachScreenshot(named: "1-plus-sheet")
        app.buttons["new-chat"].tap()
        XCTAssertTrue(app.navigationBars["Select a device"].waitForExistence(timeout: 5))
        app.buttons["session-host-dev-mac"].tap()

        let context = app.staticTexts["new-session-context"]
        XCTAssertTrue(context.waitForExistence(timeout: 5))
        XCTAssertTrue(context.label.hasPrefix("Chat"), context.label)
        XCTAssertFalse(app.buttons["session-checkout"].exists, "a chat has no checkout")
        XCTAssertEqual(composer.label, "Ask anything…")
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
        attachScreenshot(named: "2-new-chat")

        // Picking smart shows the cost note.
        chip("Auto (low)").tap()
        XCTAssertTrue(app.staticTexts["Auto (smart)"].waitForExistence(timeout: 5))
        attachScreenshot(named: "3-model-picker")
        app.staticTexts["Auto (smart)"].tap()
        app.buttons["Close"].tap()
        XCTAssertTrue(app.staticTexts["chat-mode-note"].waitForExistence(timeout: 5))
        XCTAssertTrue(chip("Auto (smart)").waitForExistence(timeout: 5))
        attachScreenshot(named: "4-smart-cost-note")

        let prompt = "What should I cook tonight?"
        composer.tap()
        composer.typeText(prompt)
        app.buttons["composer-send"].tap()
        let transcript = app.otherElements["transcript"]
        XCTAssertTrue(transcript.waitForExistence(timeout: 5))
        XCTAssertTrue(transcript.staticTexts[prompt].waitForExistence(timeout: 5))

        // The open chat keeps its mode: it names itself a chat, keeps the note
        // on its own line, and offers the same three choices (the composer
        // shows its chips once it expands).
        let header = app.descendants(matching: .any)["session-header"]
        XCTAssertTrue(header.waitForExistence(timeout: 5))
        XCTAssertTrue(header.label.contains("Chat @ MacBook Pro"), header.label)
        let note = app.staticTexts["chat-mode-note"]
        XCTAssertTrue(note.exists)
        XCTAssertEqual(note.label, "Uses Gemini 4 Argon and may incur API costs.")
        composer.tap()
        XCTAssertTrue(chip("Auto (smart)").waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["chat-mode-note"].exists)
        attachScreenshot(named: "5-open-chat")
        chip("Auto (smart)").tap()
        XCTAssertTrue(app.staticTexts["Auto (low)"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "Claude")).firstMatch.exists)
    }
}
