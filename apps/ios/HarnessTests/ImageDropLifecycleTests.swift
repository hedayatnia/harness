import SwiftUI
import UIKit
import UniformTypeIdentifiers
import XCTest
@testable import Harness

@MainActor
final class ImageDropLifecycleTests: XCTestCase {
    private func png(_ color: UIColor = .red) -> Data {
        UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8)).pngData { context in
            color.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 8, height: 8))
        }
    }

    private func finish(_ draft: AttachmentDraft) async {
        let deadline = Date().addingTimeInterval(5)
        while draft.loading, Date() < deadline {
            try? await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertFalse(draft.loading, "Image import should finish")
    }

    func testSlowImportRejectsSecondNativeDropWithoutChangingEditor() async {
        let source = DeferredImageRepresentation()
        let draft = AttachmentDraft()
        draft.enabled = true
        XCTAssertTrue(draft.receive([source.provider]))
        await fulfillment(of: [source.started], timeout: 2)
        XCTAssertTrue(draft.loading)
        XCTAssertFalse(draft.canImport)
        XCTAssertTrue(draft.items.isEmpty)
        let input = UITextView()
        input.text = "Unsent draft 👋"
        input.selectedRange = NSRange(location: 2, length: 3)
        let controller = ComposerEditorController()
        controller.attachmentDraft = draft
        let request = MockTextDropRequest(input: input, providers: [source.provider])
        XCTAssertEqual(controller.textDroppableView(input, proposalForDrop: request).operation, .cancel)
        // Even a stale UIKit callback must not start another import.
        controller.textDroppableView(input, willPerformDrop: request)
        source.complete(png())
        await finish(draft)
        XCTAssertEqual(draft.items.count, 1)
        XCTAssertTrue(draft.canImport)
        XCTAssertEqual(input.text, "Unsent draft 👋")
        XCTAssertEqual(input.selectedRange, NSRange(location: 2, length: 3))
    }

    func testRemovingExistingAttachmentDuringImportDoesNotResurrectIt() async throws {
        let draft = AttachmentDraft()
        draft.enabled = true
        let existing = try XCTUnwrap(StagedAttachment.stage(data: png(.blue)))
        draft.items = [existing]
        let source = DeferredImageRepresentation()
        XCTAssertTrue(draft.receive([source.provider]))
        await fulfillment(of: [source.started], timeout: 2)
        draft.items.removeAll { $0.id == existing.id }
        let incoming = png(.green)
        source.complete(incoming)
        await finish(draft)
        XCTAssertEqual(draft.items.map(\.data), [incoming])
        XCTAssertFalse(draft.items.contains { $0.id == existing.id })
    }

    func testLateImportRemainsInOriginalDraftNotAnotherSession() async {
        let original = AttachmentDraft()
        original.enabled = true
        let source = DeferredImageRepresentation()
        XCTAssertTrue(original.receive([source.provider]))
        await fulfillment(of: [source.started], timeout: 2)
        original.enabled = false
        let nextSession = AttachmentDraft()
        nextSession.enabled = true
        let otherImage = png(.blue)
        XCTAssertTrue(nextSession.receive([NSItemProvider(item: otherImage as NSData,
                                                        typeIdentifier: UTType.png.identifier)]))
        await finish(nextSession)
        let lateImage = png(.green)
        source.complete(lateImage)
        await finish(original)
        XCTAssertEqual(original.items.map(\.data), [lateImage])
        XCTAssertEqual(nextSession.items.map(\.data), [otherImage])
        XCTAssertFalse(original.canImport)
    }

    func testNativeSessionEndAndNonImageHoverClearHighlightWithoutImporting() {
        let draft = AttachmentDraft()
        draft.enabled = true
        let input = UITextView()
        let controller = ComposerEditorController()
        controller.attachmentDraft = draft
        let imageSession = MockImageDropSession([NSItemProvider(item: png() as NSData,
                                                               typeIdentifier: UTType.png.identifier)])
        controller.textDroppableView(input, dropSessionDidUpdate: imageSession)
        XCTAssertTrue(draft.targeted)
        controller.textDroppableView(input, dropSessionDidEnd: imageSession)
        XCTAssertFalse(draft.targeted)
        controller.textDroppableView(input, dropSessionDidUpdate: imageSession)
        controller.textDroppableView(input, dropSessionDidUpdate: MockImageDropSession([
            NSItemProvider(object: "text" as NSString)
        ]))
        XCTAssertFalse(draft.targeted)
        XCTAssertTrue(draft.items.isEmpty)
        XCTAssertFalse(draft.loading)
    }

    func testRepresentableInstallsNativeDropDelegateAndPreservesBinding() async throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        let window = UIWindow(windowScene: scene)
        let controller = ComposerEditorController()
        let draft = AttachmentDraft()
        draft.enabled = true
        var text = "Draft survives a drop"
        let binding = Binding(get: { text }, set: { text = $0 })
        let host = UIHostingController(rootView: ComposerEditor(
            text: binding, focused: .constant(false), placeholder: "Message", maxLines: 7,
            controller: controller, attachmentDraft: draft))
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer { window.isHidden = true; window.rootViewController = nil }
        host.view.layoutIfNeeded()
        let deadline = Date().addingTimeInterval(2)
        while controller.view == nil, Date() < deadline {
            try? await Task.sleep(for: .milliseconds(10))
        }
        let input = try XCTUnwrap(controller.view)
        XCTAssertTrue(input.textDropDelegate === controller)
        XCTAssertTrue(controller.attachmentDraft === draft)
        input.selectedRange = NSRange(location: 4, length: 0)
        let request = MockTextDropRequest(input: input, providers: [
            NSItemProvider(item: png() as NSData, typeIdentifier: UTType.png.identifier)
        ])
        XCTAssertEqual(input.textDropDelegate?.textDroppableView?(input, proposalForDrop: request).dropPerformer,
                       .delegate)
        input.textDropDelegate?.textDroppableView?(input, willPerformDrop: request)
        await finish(draft)
        XCTAssertEqual(draft.items.count, 1)
        XCTAssertEqual(text, "Draft survives a drop")
        XCTAssertEqual(input.text, text)
        XCTAssertEqual(input.selectedRange, NSRange(location: 4, length: 0))
    }
}

/// A source controlled by the test, rather than sleeps or network timing.
private final class DeferredImageRepresentation: @unchecked Sendable {
    let started = XCTestExpectation(description: "Provider asked to load image data")
    let provider = NSItemProvider()
    private let lock = NSLock()
    private var completion: (@Sendable (Data?, Error?) -> Void)?

    init() {
        provider.registerDataRepresentation(forTypeIdentifier: UTType.png.identifier, visibility: .all) { [weak self] completion in
            guard let self else {
                completion(nil, NSError(domain: NSItemProvider.errorDomain, code: -1))
                return nil
            }
            lock.lock()
            self.completion = completion
            lock.unlock()
            started.fulfill()
            return nil
        }
    }

    func complete(_ data: Data) {
        lock.lock()
        let callback = completion
        completion = nil
        lock.unlock()
        callback?(data, nil)
    }
}
