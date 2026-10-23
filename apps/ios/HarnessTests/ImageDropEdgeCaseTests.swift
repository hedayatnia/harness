import ImageIO
import SwiftUI
import UIKit
import UniformTypeIdentifiers
import XCTest
@testable import Harness

@MainActor
final class ImageDropEdgeCaseTests: XCTestCase {
    private func image(_ color: UIColor = .red) -> UIImage {
        UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8)).image { context in
            color.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 8, height: 8))
        }
    }

    private func provider(_ data: Data, type: UTType = .png) -> NSItemProvider {
        NSItemProvider(item: data as NSData, typeIdentifier: type.identifier)
    }

    private func draft() -> AttachmentDraft {
        let draft = AttachmentDraft()
        draft.enabled = true
        return draft
    }

    private func finish(_ draft: AttachmentDraft) async {
        let deadline = Date().addingTimeInterval(5)
        while draft.loading, Date() < deadline {
            try? await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertFalse(draft.loading, "Image import should finish")
    }

    private func encoded(_ type: UTType, colors: [UIColor]) throws -> Data {
        let data = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(data, type.identifier as CFString,
                                                                        colors.count, nil))
        for color in colors {
            CGImageDestinationAddImage(destination, try XCTUnwrap(image(color).cgImage), nil)
        }
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        return data as Data
    }

    func testEmptyAndNonImageOnlyDropsPreserveExistingErrorAndAttachments() throws {
        let draft = draft()
        let existing = try XCTUnwrap(StagedAttachment.stage(data: try XCTUnwrap(image().pngData())))
        draft.items = [existing]
        draft.error = "Previous import failed"
        XCTAssertFalse(draft.receive([]))
        XCTAssertFalse(draft.receive([NSItemProvider(object: "text" as NSString)]))
        XCTAssertEqual(draft.items.map(\.id), [existing.id])
        XCTAssertEqual(draft.error, "Previous import failed")
        XCTAssertFalse(draft.loading)
    }

    func testMixedTextAndImageDropImportsOnlyImagesInOrder() async throws {
        let draft = draft()
        let first = try XCTUnwrap(image().pngData())
        let second = try XCTUnwrap(image(.blue).pngData())
        XCTAssertTrue(draft.receive([NSItemProvider(object: "ignore me" as NSString),
                                    provider(first), provider(second),
                                    NSItemProvider(object: URL(string: "https://example.com")! as NSURL)]))
        await finish(draft)
        XCTAssertEqual(draft.items.map(\.data), [first, second])
        XCTAssertNil(draft.error)
    }

    func testProviderReadFailureDoesNotPreventLaterImageImport() async throws {
        let unavailable = DataOnlyImageProvider()
        unavailable.registerDataRepresentation(forTypeIdentifier: UTType.png.identifier, visibility: .all) { completion in
            completion(nil, NSError(domain: NSItemProvider.errorDomain, code: -1))
            return nil
        }
        let draft = draft()
        let data = try XCTUnwrap(image().pngData())
        XCTAssertTrue(draft.receive([unavailable, provider(data)]))
        await finish(draft)
        XCTAssertEqual(draft.items.map(\.data), [data])
        XCTAssertEqual(draft.error, "One image couldn't be attached (unsupported or over 24 MB).")
    }

    func testFailedFirstRepresentationTriesNextAvailableImageRepresentation() async throws {
        let source = DataOnlyImageProvider()
        source.registerDataRepresentation(forTypeIdentifier: UTType.png.identifier, visibility: .all) { completion in
            completion(nil, NSError(domain: NSItemProvider.errorDomain, code: -1))
            return nil
        }
        let jpeg = try XCTUnwrap(image().jpegData(compressionQuality: 0.8))
        source.registerDataRepresentation(forTypeIdentifier: UTType.jpeg.identifier, visibility: .all) { completion in
            completion(jpeg, nil)
            return nil
        }
        let draft = draft()
        XCTAssertTrue(draft.receive([source]))
        await finish(draft)
        XCTAssertEqual(draft.items.map(\.data), [jpeg])
        XCTAssertNil(draft.error)
    }

    func testJPEGBytesAndMatchingExtensionArePreserved() async throws {
        let jpeg = try XCTUnwrap(image().jpegData(compressionQuality: 0.8))
        let draft = draft()
        XCTAssertTrue(draft.receive([provider(jpeg, type: .jpeg)]))
        await finish(draft)
        XCTAssertEqual(draft.items.first?.data, jpeg)
        XCTAssertTrue(draft.items.first?.name.hasSuffix(".jpg") == true)
    }

    func testAnimatedGIFKeepsAllFramesAndOriginalBytes() async throws {
        let gif = try encoded(.gif, colors: [.red, .blue])
        let draft = draft()
        XCTAssertTrue(draft.receive([provider(gif, type: .gif)]))
        await finish(draft)
        let staged = try XCTUnwrap(draft.items.first)
        XCTAssertEqual(staged.data, gif)
        XCTAssertTrue(staged.name.hasSuffix(".gif"))
        let source = try XCTUnwrap(CGImageSourceCreateWithData(staged.data as CFData, nil))
        XCTAssertEqual(CGImageSourceGetCount(source), 2)
    }

    func testTIFFTranscodesToReadableJPEG() async throws {
        let tiff = try encoded(.tiff, colors: [.green])
        let draft = draft()
        XCTAssertTrue(draft.receive([provider(tiff, type: .tiff)]))
        await finish(draft)
        let staged = try XCTUnwrap(draft.items.first)
        XCTAssertTrue(staged.name.hasSuffix(".jpg"))
        XCTAssertNotEqual(staged.data, tiff)
        XCTAssertNotNil(UIImage(data: staged.data))
        XCTAssertLessThanOrEqual(staged.data.count, maxAttachmentBytes)
    }

    func testExactly24MBIsAcceptedButOneByteOverIsRejected() async throws {
        var data = try XCTUnwrap(image().pngData())
        // PNG permits trailing data; keep a small decoded image at the byte boundary.
        data.append(Data(count: maxAttachmentBytes - data.count))
        let draft = draft()
        XCTAssertTrue(draft.receive([provider(data)]))
        await finish(draft)
        XCTAssertEqual(draft.items.first?.data.count, maxAttachmentBytes)
        XCTAssertNil(draft.error)
        data.append(0)
        XCTAssertTrue(draft.receive([provider(data)]))
        await finish(draft)
        XCTAssertEqual(draft.items.count, 1)
        XCTAssertNotNil(draft.error)
    }

    func testHEICTranscodesToJPEGWithoutChangingDimensions() async throws {
        let heic = try encoded(.heic, colors: [.blue])
        let draft = draft()
        XCTAssertTrue(draft.receive([provider(heic, type: .heic)]))
        await finish(draft)
        let staged = try XCTUnwrap(draft.items.first)
        XCTAssertTrue(staged.name.hasSuffix(".jpg"))
        XCTAssertEqual(staged.image.size, UIImage(data: heic)?.size)
        XCTAssertNotEqual(staged.data, heic)
        XCTAssertNil(draft.error)
    }

    func testSVGEmptyAndTruncatedRasterDoNotEraseExistingAttachment() async throws {
        let draft = draft()
        let original = try XCTUnwrap(StagedAttachment.stage(data: try XCTUnwrap(image().pngData())))
        draft.items = [original]
        let svg = Data("<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"8\" height=\"8\"/></svg>".utf8)
        let truncated = Data([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13])
        XCTAssertTrue(draft.receive([provider(svg, type: .svg), provider(Data()), provider(truncated)]))
        await finish(draft)
        XCTAssertEqual(draft.items.map(\.id), [original.id])
        XCTAssertEqual(draft.error, "3 images couldn't be attached (unsupported or over 24 MB).")
    }

    func testOversizedRepresentationCannotBypassLimitWithSmallerAlternative() async throws {
        let source = DataOnlyImageProvider()
        let oversized = Data(count: maxAttachmentBytes + 1)
        source.registerDataRepresentation(forTypeIdentifier: UTType.png.identifier, visibility: .all) { completion in
            completion(oversized, nil)
            return nil
        }
        let jpeg = try XCTUnwrap(image().jpegData(compressionQuality: 0.8))
        source.registerDataRepresentation(forTypeIdentifier: UTType.jpeg.identifier, visibility: .all) { completion in
            completion(jpeg, nil)
            return nil
        }
        let draft = draft()
        XCTAssertTrue(draft.receive([source]))
        await finish(draft)
        XCTAssertTrue(draft.items.isEmpty)
        XCTAssertNotNil(draft.error)
    }

    func testMislabelledProviderUsesActualImageBytesForFilename() async throws {
        let jpeg = try XCTUnwrap(image().jpegData(compressionQuality: 0.8))
        let draft = draft()
        XCTAssertTrue(draft.receive([provider(jpeg, type: .png)]))
        await finish(draft)
        XCTAssertEqual(draft.items.first?.data, jpeg)
        XCTAssertTrue(draft.items.first?.name.hasSuffix(".jpg") == true)
        XCTAssertNil(draft.error)
    }

    func testDroppingSameImageTwiceCreatesIndependentlyRemovableAttachments() async throws {
        let draft = draft()
        let data = try XCTUnwrap(image().pngData())
        XCTAssertTrue(draft.receive([provider(data), provider(data)]))
        await finish(draft)
        XCTAssertEqual(draft.items.count, 2)
        XCTAssertEqual(Set(draft.items.map(\.id)).count, 2)
        XCTAssertEqual(Set(draft.items.map(\.name)).count, 2)
        let removed = try XCTUnwrap(draft.items.first).id
        draft.items.removeAll { $0.id == removed }
        XCTAssertEqual(draft.items.count, 1)
        XCTAssertNotEqual(draft.items.first?.id, removed)
        XCTAssertEqual(draft.items.first?.data, data)
    }
}

/// Models a source that vends raw data but no UIImage object representation.
private final class DataOnlyImageProvider: NSItemProvider, @unchecked Sendable {
    override func canLoadObject(ofClass aClass: NSItemProviderReading.Type) -> Bool { false }
}
