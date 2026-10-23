import XCTest
@testable import Harness

@MainActor
final class ModelCatalogLoadingTests: XCTestCase {
    func testUnreachableDeviceDoesNotInventEnabledAgents() async {
        let model = AppModel()
        let harnesses = await model.listHarnesses(deviceId: "unreachable-catalog-test")
        XCTAssertTrue(harnesses.items.isEmpty)
        XCTAssertFalse(harnesses.isLive)
    }

    func testUnreachableDeviceWithoutCacheDoesNotInventModels() async {
        let model = AppModel()
        let models = await model.listModels(deviceId: UUID().uuidString, harness: "claude-code")
        XCTAssertTrue(models.items.isEmpty)
        XCTAssertFalse(models.isLive)
    }

    func testUnreachableDeviceRetainsItsOwnCachedMetadata() async {
        let deviceId = UUID().uuidString
        let harness = "dynamic-agent"
        let cached = ModelInfo(id: "saved-live-only", label: "Saved live model", description: nil,
                               reasoningLevels: ["high"])
        XCTAssertTrue(DocDisk.saveModels([cached], deviceId: deviceId, harness: harness))
        defer { try? FileManager.default.removeItem(at: DocDisk.modelsURL(deviceId: deviceId, harness: harness)) }
        let model = AppModel()
        let models = await model.listModels(deviceId: deviceId, harness: harness)
        XCTAssertEqual(models.items, [cached])
        XCTAssertFalse(models.isLive)
        let otherHost = await model.listModels(deviceId: UUID().uuidString, harness: harness)
        XCTAssertTrue(otherHost.items.isEmpty, "a different execution device must not inherit this catalog")
    }
}
