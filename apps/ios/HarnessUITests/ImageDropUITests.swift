import XCTest

/// Uses real press-and-drag gestures and UIKit item-provider transfers, not
/// direct calls to AttachmentDraft.receive or UITextDropDelegate methods.
@MainActor
final class ImageDropUITests: XCTestCase {
    private var app: XCUIApplication!
    private var composer: XCUIElement { app.textViews["composer-input"] }
    private var attachments: XCUIElementQuery {
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "composer-attachment-"))
    }

    override func setUp() {
        continueAfterFailure = false
        XCUIDevice.shared.orientation = .portrait
        app = XCUIApplication()
        app.launchArguments = ["-demo", "-route", "chat:chat-tabs", "-sethomefilter", "", "-image-drop-fixture"]
        app.launch()
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
    }

    override func tearDown() {
        XCUIDevice.shared.orientation = .portrait
        app.terminate()
    }

    private func setDraft(_ text: String) {
        composer.tap()
        let previous = composer.value as? String ?? ""
        composer.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: previous.count) + text)
        XCTAssertEqual(composer.value as? String, text)
    }

    private func expectAttachments(_ count: Int, timeout: TimeInterval = 8) {
        let expected = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            self.attachments.count == count
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [expected], timeout: timeout), .completed,
                       "Expected \(count) image attachments")
    }

    private var transcriptTarget: XCUICoordinate {
        app.otherElements["transcript"].coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.6))
    }

    private func drag(_ kind: String, to target: XCUICoordinate) {
        let source = app.descendants(matching: .any)["image-drop-source-\(kind)"].firstMatch
        XCTAssertTrue(source.waitForExistence(timeout: 5))
        XCTAssertTrue(source.isHittable)
        source.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 1, thenDragTo: target, withVelocity: .slow, thenHoldForDuration: 0.2)
    }

    func testTranscriptAndFocusedEditorDropsPreserveDraftAndCanBeRemovedIndependently() {
        let draft = "Keep this draft before sending"
        setDraft(draft)
        drag("png", to: transcriptTarget)
        expectAttachments(1)
        XCTAssertEqual(composer.value as? String, draft)
        drag("png", to: composer.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)))
        expectAttachments(2)
        XCTAssertEqual(composer.value as? String, draft)
        XCTAssertFalse(app.otherElements["transcript"].staticTexts[draft].exists,
                       "Dropping must not automatically send the draft")
        let remove = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@",
                                                      "composer-remove-attachment-")).firstMatch
        XCTAssertTrue(remove.isHittable)
        remove.tap()
        expectAttachments(1)
        XCTAssertEqual(composer.value as? String, draft)
    }

    func testBatchInvalidAndCanceledDropsKeepExistingAttachments() {
        setDraft("Keep the valid images")
        drag("multiple", to: transcriptTarget)
        expectAttachments(2)
        drag("invalid", to: transcriptTarget)
        XCTAssertTrue(app.staticTexts["One image couldn't be attached (unsupported or over 24 MB)."]
            .waitForExistence(timeout: 5))
        expectAttachments(2)
        // The status-bar region is outside the session's drop target.
        drag("png", to: app.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.02)))
        expectAttachments(2)
        XCTAssertEqual(composer.value as? String, "Keep the valid images")
    }

    func testSlowDropDisablesSendUntilAttachmentFinishesLoading() {
        setDraft("Wait for this image")
        drag("slow", to: composer.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)))
        XCTAssertTrue(app.staticTexts["Importing images…"].waitForExistence(timeout: 2))
        XCTAssertFalse(app.buttons["composer-send"].isEnabled)
        expectAttachments(1)
        XCTAssertTrue(app.buttons["composer-send"].isEnabled)
        XCTAssertEqual(composer.value as? String, "Wait for this image")
    }

    func testLandscapeEditorDropAndPlainTextDropKeepTheirSeparateBehaviors() {
        XCUIDevice.shared.orientation = .landscapeLeft
        setDraft("Landscape draft")
        drag("png", to: composer.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)))
        expectAttachments(1)
        XCTAssertEqual(composer.value as? String, "Landscape draft")
        drag("text", to: composer.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)))
        let inserted = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value CONTAINS %@", "Dragged text"),
                                                object: composer)
        XCTAssertEqual(XCTWaiter.wait(for: [inserted], timeout: 5), .completed)
        expectAttachments(1)
    }

    func testOpeningAnotherSessionDoesNotCarryAttachmentsIntoIt() {
        setDraft("Original session draft")
        drag("png", to: transcriptTarget)
        expectAttachments(1)
        app.navigationBars.buttons.firstMatch.tap()
        let other = app.buttons.matching(NSPredicate(format: "label CONTAINS %@",
                                                     "Streaming veil on transcript rows")).firstMatch
        XCTAssertTrue(other.waitForExistence(timeout: 5))
        other.tap()
        XCTAssertTrue(composer.waitForExistence(timeout: 5))
        expectAttachments(0)
        setDraft("Other session draft")
        drag("png", to: transcriptTarget)
        expectAttachments(1)
        XCTAssertEqual(composer.value as? String, "Other session draft")
    }
}
