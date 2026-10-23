// Images on a queued message (#187): the Attachments editor changes only the
// row's attachment list, keeps its text and queue position, and commits to
// that row alone.

import XCTest
import UIKit
@testable import Harness

@MainActor
final class QueuedAttachmentDraftTests: XCTestCase {
    private func png(_ color: UIColor) -> StagedAttachment {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let data = UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8), format: format)
            .pngData { context in
                color.setFill()
                context.fill(CGRect(x: 0, y: 0, width: 8, height: 8))
            }
        return StagedAttachment.stage(data: data)!
    }

    private func lease(_ rowId: String, text: String) -> QueueEditLease {
        QueueEditLease(rowId: rowId, leaseId: "lease-1", text: text,
                       baseTextHash: "hash-1", expiresAtMs: 60_000)
    }

    func testAddingToATextOnlyRowKeepsItsTextAndAppendsInOrder() throws {
        var draft = QueuedAttachmentDraft(row: QueuedMessage(id: "row-2", text: "Review the layout."))
        XCTAssertFalse(draft.isChanged)
        XCTAssertFalse(draft.canConfirm, "nothing to save yet")

        let first = png(.red), second = png(.blue)
        draft.add([first, second])

        XCTAssertTrue(draft.canConfirm)
        XCTAssertEqual(draft.commitText, "Review the layout.")
        XCTAssertEqual(draft.pendingUploads.map(\.id), [first.id, second.id])
        XCTAssertNil(draft.committedPaths(uploaded: [first.id: "/u/a.png"]),
                     "every staged image needs its uploaded path before the commit")
        XCTAssertEqual(draft.committedPaths(uploaded: [first.id: "/u/a.png", second.id: "/u/b.png"]),
                       ["/u/a.png", "/u/b.png"])
    }

    func testReplaceKeepsTheSlotAndRemoveKeepsTheRestInOrder() {
        let row = QueuedMessage(id: "row-1", text: "Compare these.",
                                attachments: ["/u/one.png", "/u/two.png", "/u/three.png"])
        var draft = QueuedAttachmentDraft(row: row)
        XCTAssertEqual(draft.commitText, "Compare these.")

        let replacement = png(.green)
        XCTAssertTrue(draft.replace(id: draft.items[1].id, with: replacement))
        XCTAssertEqual(draft.committedPaths(uploaded: [replacement.id: "/u/new.png"]),
                       ["/u/one.png", "/u/new.png", "/u/three.png"])

        XCTAssertTrue(draft.remove(id: draft.items[0].id))
        XCTAssertEqual(draft.committedPaths(uploaded: [replacement.id: "/u/new.png"]),
                       ["/u/new.png", "/u/three.png"])
        XCTAssertEqual(draft.commitText, "Compare these.")
        XCTAssertFalse(draft.remove(id: "missing"))
    }

    func testRestoringTheOriginalListIsNotAChange() {
        var draft = QueuedAttachmentDraft(row: QueuedMessage(id: "row-1", text: "Hi",
                                                             attachments: ["/u/one.png"]))
        let extra = png(.red)
        draft.add([extra])
        XCTAssertTrue(draft.isChanged)
        draft.remove(id: extra.id)
        XCTAssertFalse(draft.isChanged)
    }

    func testAnImageOnlyRowCanSwapImagesButNotLoseTheLastOne() {
        let row = QueuedMessage(id: "row-1", text: attachmentOnlyText, attachments: ["/u/one.png"])
        var draft = QueuedAttachmentDraft(row: row)
        draft.remove(id: draft.items[0].id)
        XCTAssertTrue(draft.isChanged)
        XCTAssertNil(draft.commitText, "an empty message is not a queued message")
        XCTAssertFalse(draft.canConfirm)

        let photo = png(.red)
        draft.add([photo])
        XCTAssertEqual(draft.commitText, attachmentOnlyText)
        XCTAssertTrue(draft.canConfirm)
    }

    func testRemovingEveryImageFromATextRowLeavesItsText() {
        var draft = QueuedAttachmentDraft(row: QueuedMessage(id: "row-1", text: "Keep me",
                                                             attachments: ["/u/one.png"]))
        draft.remove(id: draft.items[0].id)
        XCTAssertEqual(draft.commitText, "Keep me")
        XCTAssertEqual(draft.committedPaths(uploaded: [:]), [])
    }

    func testALegacyTrailerIsStrippedSoTheHostRebuildsItFromTheNewList() {
        let paths = ["/u/one.png"]
        let row = QueuedMessage(id: "row-1", text: withAttachments(text: "Look", paths: paths),
                                attachments: paths)
        var draft = QueuedAttachmentDraft(row: row)
        draft.add([png(.red)])
        XCTAssertEqual(draft.commitText, "Look")
    }

    func testAppshotCapturesStayWithTheirContext() {
        let context = AppshotContext.marker
            + "\n<appshot app=\"Notes\" window-title=\"Plan\" image=\"/u/shot.png\">observed</appshot>\n"
        let row = QueuedMessage(id: "row-1", text: "Check this" + context,
                                attachments: ["/u/shot.png", "/u/plain.png"])
        var draft = QueuedAttachmentDraft(row: row)
        XCTAssertTrue(draft.items[0].locked)
        XCTAssertFalse(draft.items[1].locked)
        XCTAssertFalse(draft.remove(id: draft.items[0].id))
        XCTAssertFalse(draft.replace(id: draft.items[0].id, with: png(.red)))
        XCTAssertTrue(draft.remove(id: draft.items[1].id))
        XCTAssertEqual(draft.commitText, "Check this" + AppshotContext.suffix(row.text)!)
    }

    func testTheCommitIsBoundToTheLeasedRow() throws {
        var draft = QueuedAttachmentDraft(row: QueuedMessage(id: "row-2", text: "Second"),
                                          leasedText: "Second")
        let photo = png(.red)
        draft.add([photo])
        let uploaded = [photo.id: "/u/photo.png"]

        XCTAssertNil(draft.finishParams(chatId: "chat-1", lease: lease("row-1", text: "First"),
                                        uploaded: uploaded),
                     "a lease on another row must never carry these images")

        let params = try XCTUnwrap(draft.finishParams(chatId: "chat-1", lease: lease("row-2", text: "Second"),
                                                      uploaded: uploaded))
        XCTAssertEqual(params["chatId"] as? String, "chat-1")
        XCTAssertEqual(params["id"] as? String, "row-2")
        XCTAssertEqual(params["leaseId"] as? String, "lease-1")
        XCTAssertEqual(params["action"] as? String, "commit")
        XCTAssertEqual(params["expectedTextHash"] as? String, "hash-1")
        XCTAssertEqual(params["text"] as? String, "Second")
        XCTAssertEqual(params["attachments"] as? [String], ["/u/photo.png"])
    }

    func testTextOnlyFinishesLeaveTheAttachmentListAlone() {
        let params = SessionStore.finishQueuedEditParams(chatId: "chat-1", lease: lease("row-1", text: "x"),
                                                         action: "cancel")
        XCTAssertNil(params["attachments"], "absent means keep the row's images")
        XCTAssertNil(params["text"])
    }

    private func store() -> SessionStore {
        SessionStore(chatId: "chat-1", config: AppConfig(
            edgeURL: URL(string: "https://example.test")!,
            mode: .dev, userId: "u1", orgId: "o1",
            deviceId: "ios-test", deviceName: "Test phone",
            devBearer: "cmt_dev_test"))
    }

    /// The local write (demo mode) mirrors the host's: the row keeps its id,
    /// text and slot, and no other row changes or sends.
    func testApplyingAttachmentsKeepsQueuePositionAndOtherRows() throws {
        let store = store()
        let ids = ["First", "Second", "Third"].compactMap { store.enqueueMessage(text: $0) }
        XCTAssertEqual(ids.count, 3)
        var draft = QueuedAttachmentDraft(row: store.queue[1])
        let photo = png(.red)
        draft.add([photo])
        let paths = try XCTUnwrap(draft.committedPaths(uploaded: [photo.id: "/demo/queued/a.png"]))

        XCTAssertTrue(store.applyQueuedAttachmentsLocally(id: draft.rowId, text: try XCTUnwrap(draft.commitText),
                                                          attachments: paths))

        XCTAssertEqual(store.queue.map(\.id), ids)
        XCTAssertEqual(store.queue.map(\.text), ["First", "Second", "Third"])
        XCTAssertEqual(store.queue.map(\.attachments), [[], ["/demo/queued/a.png"], []])
        XCTAssertNotNil(store.queue[1].editedAt)
        XCTAssertFalse(store.applyQueuedAttachmentsLocally(id: "gone", text: "x", attachments: []))
    }
}
