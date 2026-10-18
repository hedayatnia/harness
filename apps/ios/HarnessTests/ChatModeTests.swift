import XCTest
@testable import Harness

/// Chat mode offers graff's three auto tiers only, under their picker names,
/// and warns about API costs on Auto (smart). Mirrors crates/ui/src/chat_mode.rs.
final class ChatModeTests: XCTestCase {
    private func model(_ id: String, _ label: String) -> ModelInfo {
        ModelInfo(id: id, label: label, description: nil, reasoningLevels: ["low", "high"])
    }

    func testOnlyTheAutoTiersAreOfferedInOrderUnderTheirNames() {
        let catalog = [
            model("codegraff/auto-smart", "auto-smart"),
            model("codegraff/claude-opus-5-5", "Claude Opus 5.5"),
            model("codegraff/auto-high", "Auto (high)"),
            model("chatgpt-new/gpt-6.1-sol", "GPT-6.1 Sol"),
            model("codegraff/auto-low", "Auto (low)"),
        ]
        let offered = ChatMode.models(from: catalog)
        XCTAssertEqual(offered.map(\.id), ["codegraff/auto-low", "codegraff/auto-high", "codegraff/auto-smart"])
        XCTAssertEqual(offered.map(\.label), ["Auto (low)", "Auto (high)", "Auto (smart)"])
        XCTAssertEqual(offered[2].description, ChatMode.smartCostNote)
        XCTAssertTrue(offered.allSatisfy { $0.reasoningLevels.isEmpty }, "the tier picks its own thinking")
    }

    func testAnOlderGatewayOffersAutoMediumAsAutoLow() {
        let offered = ChatMode.models(from: [model("codegraff/auto-medium", "Auto (medium)"),
                                             model("codegraff/auto-high", "Auto (high)")])
        XCTAssertEqual(offered.map(\.id), ["codegraff/auto-medium", "codegraff/auto-high"])
        XCTAssertEqual(offered.first?.label, "Auto (low)")
    }

    func testBeforeTheCatalogLoadsTheThreeTiersStillShow() {
        XCTAssertEqual(ChatMode.models(from: []).map(\.label), ["Auto (low)", "Auto (high)", "Auto (smart)"])
    }

    func testTheCostNoteBelongsToAutoSmartOnly() {
        XCTAssertEqual(ChatMode.note(for: "codegraff/auto-smart"), ChatMode.smartCostNote)
        XCTAssertNil(ChatMode.note(for: "codegraff/auto-high"))
        XCTAssertNil(ChatMode.note(for: "codegraff/auto-medium"))
        XCTAssertNil(ChatMode.note(for: nil))
    }

    func testAChatConfigRoundTripsItsMode() throws {
        let chat = ChatConfig(harness: "graff", model: "codegraff/auto-low", reasoning: nil,
                              sandbox: "workspace-write", chatMode: true)
        let data = try JSONEncoder().encode(chat)
        XCTAssertTrue(String(decoding: data, as: UTF8.self).contains("\"chatMode\":true"))
        XCTAssertTrue(try JSONDecoder().decode(ChatConfig.self, from: data).isChatMode)

        let code = ChatConfig(harness: "codex", model: "gpt-6-sol", reasoning: nil, sandbox: "workspace-write")
        let codeData = try JSONEncoder().encode(code)
        XCTAssertFalse(String(decoding: codeData, as: UTF8.self).contains("chatMode"), "omitted on coding sessions")
        XCTAssertFalse(try JSONDecoder().decode(ChatConfig.self, from: codeData).isChatMode)
    }
}
