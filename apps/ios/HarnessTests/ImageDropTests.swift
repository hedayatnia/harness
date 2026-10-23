import UIKit
import UniformTypeIdentifiers
import XCTest
@testable import Harness

@MainActor
final class ImageDropTests: XCTestCase {
    private func png(_ color: UIColor = .red) -> Data {
        UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8)).pngData { context in
            color.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 8, height: 8))
        }
    }

    private func provider(_ data: Data, type: UTType = .png) -> NSItemProvider {
        NSItemProvider(item: data as NSData, typeIdentifier: type.identifier)
    }

    private func finish(_ draft: AttachmentDraft) async {
        let deadline = Date().addingTimeInterval(5)
        while draft.loading, Date() < deadline {
            try? await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertFalse(draft.loading, "Image import should finish")
    }

    func testDropAppendsMultipleImagesInSourceOrderWithoutReplacingExistingAttachments() async throws {
        let draft = AttachmentDraft()
        draft.enabled = true
        let existing = try XCTUnwrap(StagedAttachment.stage(data: png(.blue)))
        draft.items = [existing]
        let first = png(.green)
        let second = png(.red)
        XCTAssertTrue(draft.receive([provider(first), provider(second)]))
        XCTAssertTrue(draft.loading)
        XCTAssertFalse(draft.receive([provider(first)]), "Do not race a second import or send")
        await finish(draft)
        XCTAssertEqual(draft.items.map(\.data), [existing.data, first, second])
        XCTAssertEqual(draft.items.first?.id, existing.id)
        XCTAssertNil(draft.error)
    }

    func testUIImageAndFileProvidersUseNormalStagingAndAttachmentTransport() async throws {
        let data = png()
        let image = try XCTUnwrap(UIImage(data: data))
        let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".png")
        try data.write(to: file)
        defer { try? FileManager.default.removeItem(at: file) }
        let fileProvider = try XCTUnwrap(NSItemProvider(contentsOf: file))
        let draft = AttachmentDraft()
        draft.enabled = true
        XCTAssertTrue(draft.receive([NSItemProvider(object: image), fileProvider]))
        await finish(draft)
        XCTAssertEqual(draft.items.count, 2)
        // UIKit may vend a UIImage as JPEG even when it was decoded from PNG.
        let objectAttachment = try XCTUnwrap(draft.items.first)
        XCTAssertTrue(objectAttachment.name.hasSuffix(".png") || objectAttachment.name.hasSuffix(".jpg"))
        XCTAssertEqual(objectAttachment.image.size, image.size)
        XCTAssertTrue(draft.items.last?.name.hasSuffix(".png") == true)
        XCTAssertEqual(draft.items.last?.data, data)
        // Dropped bytes feed exactly the same upload/ref transport as picks.
        let paths = draft.items.map { "/uploads/\($0.name)" }
        let parsed = parseUserMessageImages(withAttachments(text: "Keep this draft", paths: paths))
        XCTAssertEqual(parsed.text, "Keep this draft")
        XCTAssertEqual(parsed.attachments.map(\.path), paths)
    }

    func testBadAndOversizedImagesReportErrorsWithoutLosingValidImages() async {
        let draft = AttachmentDraft()
        draft.enabled = true
        let valid = png()
        XCTAssertTrue(draft.receive([
            provider(Data("not a png".utf8)), provider(valid),
            provider(Data(count: maxAttachmentBytes + 1))
        ]))
        await finish(draft)
        XCTAssertEqual(draft.items.map(\.data), [valid])
        XCTAssertEqual(draft.error, "2 images couldn't be attached (unsupported or over 24 MB).")
        XCTAssertTrue(draft.receive([provider(valid)]))
        await finish(draft)
        XCTAssertNil(draft.error)
        XCTAssertEqual(draft.items.count, 2)
    }

    func testDisabledAndNonImageDropsDoNotChangeDraft() {
        let draft = AttachmentDraft()
        let image = provider(png())
        XCTAssertFalse(draft.receive([image]))
        draft.enabled = true
        XCTAssertFalse(draft.receive([NSItemProvider(object: "plain text" as NSString)]))
        XCTAssertTrue(draft.items.isEmpty)
        XCTAssertFalse(draft.loading)
        XCTAssertNil(draft.error)
    }

    func testNativeEditorRoutesImageDropToAttachmentsWithoutChangingTextOrSelection() async throws {
        let input = UITextView()
        input.text = "Keep this draft"
        input.selectedRange = NSRange(location: 5, length: 4)
        let controller = ComposerEditorController()
        controller.view = input
        let draft = AttachmentDraft()
        draft.enabled = true
        controller.attachmentDraft = draft
        let request = MockTextDropRequest(input: input, providers: [provider(png())])
        let proposal = controller.textDroppableView(input, proposalForDrop: request)
        XCTAssertEqual(proposal.operation, .copy)
        XCTAssertEqual(proposal.dropPerformer, .delegate)
        controller.textDroppableView(input, dropSessionDidUpdate: request.dropSession)
        XCTAssertTrue(draft.targeted)
        controller.textDroppableView(input, dropSessionDidExit: request.dropSession)
        XCTAssertFalse(draft.targeted)
        XCTAssertTrue(draft.items.isEmpty, "Hovering/canceling must not import")
        controller.textDroppableView(input, willPerformDrop: request)
        await finish(draft)
        XCTAssertEqual(draft.items.count, 1)
        XCTAssertEqual(input.text, "Keep this draft")
        XCTAssertEqual(input.selectedRange, NSRange(location: 5, length: 4))
        XCTAssertFalse(draft.targeted)
        draft.enabled = false
        XCTAssertEqual(controller.textDroppableView(input, proposalForDrop: request).operation, .cancel)
    }

    func testNativeTextDropRetainsUIKitProposal() {
        let input = UITextView()
        let controller = ComposerEditorController()
        controller.attachmentDraft = AttachmentDraft()
        let request = MockTextDropRequest(input: input, providers: [NSItemProvider(object: "text" as NSString)])
        XCTAssertTrue(controller.textDroppableView(input, proposalForDrop: request) === request.suggestedProposal)
    }
}

@MainActor
final class MockTextDropRequest: NSObject, UITextDropRequest {
    let dropPosition: UITextPosition
    let suggestedProposal = UITextDropProposal(operation: .copy)
    let isSameView = false
    let dropSession: UIDropSession

    init(input: UITextView, providers: [NSItemProvider]) {
        dropPosition = input.beginningOfDocument
        dropSession = MockImageDropSession(providers)
    }
}

@MainActor
final class MockImageDropSession: NSObject, UIDropSession {
    let items: [UIDragItem]
    let allowsMoveOperation = false
    let isRestrictedToDraggingApplication = false
    let localDragSession: UIDragSession? = nil
    let progress = Progress(totalUnitCount: 1)
    var progressIndicatorStyle: UIDropSessionProgressIndicatorStyle = .none

    init(_ providers: [NSItemProvider]) { items = providers.map { UIDragItem(itemProvider: $0) } }
    func location(in view: UIView) -> CGPoint { .zero }
    func hasItemsConforming(toTypeIdentifiers types: [String]) -> Bool {
        items.contains { item in types.contains { item.itemProvider.hasItemConformingToTypeIdentifier($0) } }
    }
    func canLoadObjects(ofClass aClass: NSItemProviderReading.Type) -> Bool {
        items.contains { $0.itemProvider.canLoadObject(ofClass: aClass) }
    }
    func loadObjects(ofClass aClass: NSItemProviderReading.Type,
                     completion: @escaping ([NSItemProviderReading]) -> Void) -> Progress {
        completion([])
        return progress
    }
}
