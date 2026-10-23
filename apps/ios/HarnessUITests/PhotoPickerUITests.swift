import UIKit
import XCTest

/// Real system Photos UI, including rendered confirmation/selection colors.
/// Seed the simulator library with an image via simctl addmedia before running.
@MainActor
final class PhotoPickerUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
        XCUIDevice.shared.orientation = .portrait
    }

    private func capture(_ name: String) -> UIImage {
        let screenshot = XCUIScreen.main.screenshot()
        let attachment = XCTAttachment(screenshot: screenshot)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
        return screenshot.image
    }

    /// Extract the two main native paints (fill and tick) from the inside of
    /// a selection circle. Compare the actual native selection and confirmation
    /// paints rather than assuming a fixed RGB value across appearances.
    private func selectionPaints(_ image: UIImage, in rect: CGRect, screenWidth: CGFloat) -> [[Double]]? {
        guard let cgImage = image.cgImage else { return nil }
        let scale = CGFloat(cgImage.width) / screenWidth
        let crop = CGRect(x: rect.minX * scale, y: rect.minY * scale,
                          width: rect.width * scale, height: rect.height * scale)
        guard let region = cgImage.cropping(to: crop) else { return nil }
        let width = region.width, height = region.height
        guard let context = CGContext(data: nil, width: width, height: height,
                                      bitsPerComponent: 8, bytesPerRow: width * 4,
                                      space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        context.draw(region, in: CGRect(x: 0, y: 0, width: width, height: height))
        guard let bytes = context.data?.assumingMemoryBound(to: UInt8.self) else { return nil }
        var counts: [Int: Int] = [:]
        for index in stride(from: 0, to: width * height * 4, by: 4) {
            // Quantize small glass/antialiasing differences into 16-level bins.
            let key = Int(bytes[index] / 16) << 8 | Int(bytes[index + 1] / 16) << 4 | Int(bytes[index + 2] / 16)
            counts[key, default: 0] += 1
        }
        let main = counts.sorted { $0.value > $1.value }.prefix(2)
        guard main.count == 2, main.allSatisfy({ $0.value > width * height / 50 }) else { return nil }
        return main.map { entry in
            [Double((entry.key >> 8) * 16 + 8) / 255,
             Double(((entry.key >> 4) & 15) * 16 + 8) / 255,
             Double((entry.key & 15) * 16 + 8) / 255]
        }
    }

    private func exercise(mode: String, theme: String, newSession: Bool = false) throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-sethomefilter", "", "-theme.mode", mode,
                               "-theme.\(mode)", theme]
        app.launch()
        if newSession {
            XCTAssertTrue(app.buttons["new-session"].waitForExistence(timeout: 10))
            app.buttons["new-session"].tap()
            app.buttons["new-session-space-space-harness"].tap()
        } else {
            let row = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Tool group header colors")).firstMatch
            XCTAssertTrue(row.waitForExistence(timeout: 10))
            row.tap()
        }
        let composer = app.descendants(matching: .any)["composer-input"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        composer.tap()
        let attach = app.buttons["Attach photos"]
        XCTAssertTrue(attach.waitForExistence(timeout: 10))
        attach.tap()
        let photos = app.navigationBars["Photos"]
        XCTAssertTrue(photos.waitForExistence(timeout: 10))
        let done = photos.buttons["Done"]
        XCTAssertFalse(done.isEnabled)
        _ = capture("photos-\(theme)-unselected")
        let image = app.images.matching(identifier: "PXGGridLayout-Info").firstMatch
        XCTAssertTrue(image.waitForExistence(timeout: 10), "Seed the simulator photo library before running")
        // Photos' remote grid exposes image bounds but reports isHittable=false
        // on iOS 26. Tap the center of the actual discovered image rectangle.
        image.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        XCTAssertTrue(done.isEnabled)
        let selected = capture("photos-\(theme)-selected")
        let donePaints = try XCTUnwrap(selectionPaints(selected, in: done.frame.insetBy(dx: 6, dy: 6), screenWidth: app.frame.width))
        let badge = CGRect(x: image.frame.maxX - 25, y: image.frame.maxY - 25, width: 16, height: 16)
        let badgePaints = try XCTUnwrap(selectionPaints(selected, in: badge, screenWidth: app.frame.width))
        for channel in 0..<3 {
            XCTAssertEqual(donePaints[0][channel], badgePaints[0][channel], accuracy: 0.15,
                           "Confirmation fill must match the selected photo's native palette")
        }
        // Native Liquid Glass renders the prominent tick differently from the
        // solid photo badge. Let Photos own that foreground, but require a
        // visible tick instead of the previous pale-on-pale appearance.
        let contrast = zip(donePaints[0], donePaints[1]).map { abs($0 - $1) }.reduce(0, +) / 3
        XCTAssertGreaterThan(contrast, 0.2, "Enabled confirmation must not retain the old pale disabled-looking tick")
        image.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        XCTAssertFalse(done.isEnabled, "Returning to zero selections disables confirmation")
        image.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        photos.buttons["Cancel"].tap()
        XCTAssertTrue(attach.waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["composer-send"].isEnabled, "Cancel must not stage the selected image")
        attach.tap()
        XCTAssertTrue(photos.waitForExistence(timeout: 10))
        XCTAssertFalse(done.isEnabled, "Canceled picks must not persist on reopening")
        image.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        done.tap()
        XCTAssertTrue(attach.waitForExistence(timeout: 10))
        let enabled = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"),
                                                object: app.buttons["composer-send"])
        XCTAssertEqual(XCTWaiter.wait(for: [enabled], timeout: 10), .completed,
                       "Confirming a photo must preserve attachment-only sending")
        _ = capture("photos-\(theme)-attachment-staged")
        app.terminate()
    }

    func testGitHubLight() throws { try exercise(mode: "light", theme: "github-light") }
    func testHarnessLight() throws { try exercise(mode: "light", theme: "harnesser-light", newSession: true) }
    func testNord() throws { try exercise(mode: "dark", theme: "nord") }
    func testDracula() throws { try exercise(mode: "dark", theme: "dracula") }
    func testGruvboxDark() throws { try exercise(mode: "dark", theme: "gruvbox-dark", newSession: true) }
}
