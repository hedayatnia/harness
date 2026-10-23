import Loro
import XCTest
@testable import Harness

@MainActor
final class SessionDeleteTests: XCTestCase {
    private var config: AppConfig {
        AppConfig(edgeURL: URL(string: "http://localhost:1")!, mode: .dev,
                  userId: "delete-tests", orgId: "tests", deviceId: "ios-test",
                  deviceName: "Test phone")
    }

    private func row(_ id: String, title: String) -> RegistryRow {
        RegistryRow(kind: "chats", id: id, seq: 1, deleted: false, delHlc: nil,
                    fields: ["deviceId": .string("host"), "title": .string(title),
                             "archived": .bool(false), "createdAt": .int(1), "roomGen": .int(2)],
                    clocks: [:])
    }

    private func workspace() -> (AppModel, RegistryDoc) {
        let doc = RegistryDoc(deviceId: config.deviceId)
        doc.applyState(seq: 1, full: true, gcFloor: 0, rows: [
            row("chat-a", title: "Release plan"),
            row("chat-b", title: "Other session"),
        ])
        let model = AppModel()
        model.workspace = WorkspaceStore(config: config, doc: doc)
        return (model, doc)
    }

    func testDeleteRemovesTheSessionAndQueuesItsTombstones() throws {
        let (model, doc) = workspace()
        model.delete(chatId: "chat-a")
        XCTAssertNil(model.chat(id: "chat-a"))
        XCTAssertEqual(model.chat(id: "chat-b")?.displayTitle, "Other session")
        let ops = try XCTUnwrap(doc.pending.last?.ops)
        XCTAssertEqual(Set(ops.map { "\($0.kind)/\($0.id)" }), ["chats/chat-a", "sessions/chat-a"])
        XCTAssertTrue(ops.allSatisfy { $0.op == .delete })

        // Offline: the tombstones survive a reload and still hide the session.
        let restoredDoc = try RegistryDoc.from(data: doc.toData(), deviceId: config.deviceId)
        let restored = WorkspaceStore(config: config, doc: restoredDoc)
        XCTAssertEqual(restored.chats.map(\.id), ["chat-b"])
    }

    func testDemoDeleteOnlyRemovesThatSession() {
        let model = AppModel()
        model.demo = DemoDataset(devices: [], spaces: [], chats: [
            Chat(id: "chat-a", deviceId: "host", title: "Release plan", archived: false, createdAt: 1),
            Chat(id: "chat-b", deviceId: "host", title: "Other session", archived: false, createdAt: 2),
        ], sessions: [:])
        model.delete(chatId: "chat-a")
        XCTAssertNil(model.chat(id: "chat-a"))
        XCTAssertNotNil(model.chat(id: "chat-b"))
    }

    func testTheConfirmationUsesTheDesktopWording() {
        XCTAssertEqual(SessionDeletion.title, "Delete session?")
        XCTAssertEqual(SessionDeletion.message(title: "Release plan"),
                       "\u{201C}Release plan\u{201D} will be permanently deleted. This can\u{2019}t be undone.")
    }
}
