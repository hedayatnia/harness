import XCTest

@MainActor
final class ModelCatalogLoadingUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        continueAfterFailure = false
        app = XCUIApplication()
        XCUIDevice.shared.orientation = .portrait
    }

    private func launchNewSession(modelId: String, arguments: [String] = []) {
        app.launchArguments = ["-demo", "-route", "space:space-harness", "-sheet", "newsession",
                               "-setnewsessionharness", "codex", "-setnewsessionmodel", modelId,
                               "-setnewsessionreasoning", "high"] + arguments
        app.launch()
    }

    private var composer: XCUIElement {
        app.descendants(matching: .any)["composer-input"].firstMatch
    }

    private func expectNoInventedModelRows() {
        XCTAssertFalse(app.scrollViews.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Fable 5.1,")).firstMatch.exists)
        XCTAssertFalse(app.scrollViews.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "GPT-6-Astra,")).firstMatch.exists)
    }

    func testNewSessionPickerPreservesIdentityButBlocksPositivelyUnavailableSelection() {
        launchNewSession(modelId: "saved-live-only", arguments: ["-slow-catalog"])
        let saved = app.buttons["saved-live-only"]
        XCTAssertTrue(saved.waitForExistence(timeout: 5))
        saved.tap()
        XCTAssertTrue(app.descendants(matching: .any)["model-picker-loading"].firstMatch.exists)
        XCTAssertTrue(app.staticTexts["model-picker-current"].label.contains("saved-live-only"))
        expectNoInventedModelRows()
        let confirm = app.buttons["model-picker-confirm"]
        XCTAssertEqual(confirm.label, "Close")

        let discovered = app.scrollViews.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "GPT-6-Astra,")).firstMatch
        XCTAssertTrue(discovered.waitForExistence(timeout: 40))
        XCTAssertTrue(app.staticTexts["model-picker-current"].label.contains("saved-live-only"))
        XCTAssertEqual(confirm.label, "Close", "catalog hydration is not a user selection")
        confirm.tap()
        XCTAssertTrue(saved.exists)
        XCTAssertFalse(app.buttons["200K"].exists)
        composer.tap()
        composer.typeText("Preserve the saved model.")
        XCTAssertFalse(app.buttons["composer-send"].isEnabled)
        XCTAssertTrue(app.staticTexts["model-selection-unavailable"].exists)
        saved.tap()
        discovered.tap()
        confirm.tap()
        XCTAssertTrue(app.buttons["composer-send"].isEnabled)
        app.buttons["composer-send"].tap()
        XCTAssertTrue(app.otherElements["transcript"].waitForExistence(timeout: 5))
        composer.tap()
        XCTAssertTrue(app.buttons["gpt-6-astra"].waitForExistence(timeout: 3)
                      || app.buttons["GPT-6-Astra"].exists)
    }

    func testSavedSelectionCanSendBeforeDiscoveryWithoutSubstitutingADefault() {
        launchNewSession(modelId: "gpt-5.6-terra", arguments: ["-slow-catalog"])
        XCTAssertTrue(app.buttons["gpt-5.6-terra"].waitForExistence(timeout: 5))
        composer.tap()
        composer.typeText("Send with the saved identity.")
        XCTAssertTrue(app.buttons["composer-send"].isEnabled)
        app.buttons["composer-send"].tap()
        XCTAssertTrue(app.otherElements["transcript"].waitForExistence(timeout: 5))
        composer.tap()
        XCTAssertTrue(app.buttons["gpt-5.6-terra"].waitForExistence(timeout: 3))
        XCTAssertFalse(app.buttons["Fable 5.1"].exists)
    }

    func testLiveSessionPickerWaitsForItsHostInsteadOfOfferingStaticModels() {
        app.launchArguments = ["-demo", "-route", "chat:chat-tabs", "-sheet", "config", "-slow-catalog"]
        app.launch()
        let current = app.staticTexts["model-picker-current"]
        XCTAssertTrue(current.waitForExistence(timeout: 5))
        XCTAssertTrue(current.label.contains("gpt-5.6-terra"))
        expectNoInventedModelRows()
        XCTAssertTrue(app.descendants(matching: .any)["model-picker-loading"].firstMatch.exists)
        let confirm = app.buttons["model-picker-confirm"]
        XCTAssertEqual(confirm.label, "Close")
        let discovered = app.scrollViews.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "GPT-5.6-Terra,")).firstMatch
        XCTAssertTrue(discovered.waitForExistence(timeout: 25))
        XCTAssertEqual(confirm.label, "Close")
        confirm.tap()
        composer.tap()
        XCTAssertTrue(app.buttons["GPT-5.6-Terra"].waitForExistence(timeout: 3))
        XCTAssertTrue(app.buttons["High"].exists)
    }

    func testExistingChatCatalogCanRetryAfterFailureWithoutReopening() {
        app.launchArguments = ["-demo", "-route", "chat:chat-tabs", "-sheet", "config", "-models-fail-once"]
        app.launch()
        XCTAssertTrue(app.staticTexts["model-picker-unavailable"].waitForExistence(timeout: 5))
        let retry = app.buttons["model-picker-retry"]
        XCTAssertTrue(retry.exists)
        retry.tap()
        let saved = app.scrollViews.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "GPT-5.6-Terra,")).firstMatch
        XCTAssertTrue(saved.waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["model-picker-unavailable"].exists)
        XCTAssertEqual(app.buttons["model-picker-confirm"].label, "Close")
    }

    func testEmptyAgentDiscoveryDoesNotOfferDefaultsOrEnableUnconfiguredSend() {
        launchNewSession(modelId: "", arguments: ["-no-agents"])
        let select = app.buttons["Select model"]
        XCTAssertTrue(select.waitForExistence(timeout: 5))
        composer.tap()
        composer.typeText("Wait for an explicit model.")
        XCTAssertFalse(app.buttons["composer-send"].isEnabled)
        select.tap()
        XCTAssertTrue(app.staticTexts["model-picker-unavailable"].waitForExistence(timeout: 3))
        expectNoInventedModelRows()
        XCTAssertEqual(app.buttons["model-picker-confirm"].label, "Close")
    }
}
