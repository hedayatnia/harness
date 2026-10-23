import UIKit
import XCTest

@MainActor
final class ReasoningConfirmationUITests: XCTestCase {
    func testConfirmationMatchesSelectionAcrossThemes() throws {
        continueAfterFailure = false
        let themes = [
            ("harnesser-light", "light"),
            ("codegraff-dark", "dark"),
            ("tokyo-night", "dark"),
            ("gruvbox-light", "light"),
            ("synthwave-84", "dark"),
        ]
        for (theme, appearance) in themes {
            let app = XCUIApplication()
            app.launchArguments = ["-demo", "-route", "chat:chat-tabs",
                                   "-theme.mode", appearance, "-theme.\(appearance)", theme,
                                   "-theme.followDesktop", "NO"]
            app.launch()
            let composer = app.descendants(matching: .any)["composer-input"].firstMatch
            XCTAssertTrue(composer.waitForExistence(timeout: 10))
            composer.tap()
            let high = app.buttons["High"]
            XCTAssertTrue(high.waitForExistence(timeout: 5))
            high.tap()

            let confirm = app.buttons["reasoning-picker-confirm"]
            XCTAssertTrue(confirm.waitForExistence(timeout: 5))
            XCTAssertEqual(confirm.label, "Close", theme)
            let initial = app.scrollViews.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "High,")).firstMatch
            let alternate = app.scrollViews.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Low,")).firstMatch
            initial.tap()
            XCTAssertEqual(confirm.label, "Close", "Reselecting the opening value: \(theme)")
            alternate.tap()
            XCTAssertEqual(confirm.label, "Done", theme)
            XCTAssertTrue(confirm.isHittable)

            let screenshot = XCUIScreen.main.screenshot()
            let attachment = XCTAttachment(screenshot: screenshot)
            attachment.name = "reasoning-confirmation-\(theme)"
            attachment.lifetime = .keepAlways
            add(attachment)
            // Sample clear fill away from text, tick strokes, and rounded edges.
            // Native toolbar compositing can slightly alter the same semantic color.
            let circle = try pixel(screenshot.image, at: CGPoint(
                x: confirm.frame.minX + confirm.frame.width * 0.15, y: confirm.frame.midY))
            let row = try pixel(screenshot.image, at: CGPoint(
                x: alternate.frame.maxX - 8, y: alternate.frame.midY))
            for channel in 0..<3 {
                XCTAssertLessThanOrEqual(abs(circle[channel] - row[channel]), 16,
                                         "Confirmation fill must match selected row: \(theme)")
            }

            initial.tap()
            XCTAssertEqual(confirm.label, "Close", "A → B → A restores X: \(theme)")
            alternate.tap()
            XCTAssertEqual(confirm.label, "Done", theme)
            confirm.tap()
            let low = app.buttons["Low"]
            XCTAssertTrue(low.waitForExistence(timeout: 5))
            low.tap()
            XCTAssertTrue(confirm.waitForExistence(timeout: 5))
            XCTAssertEqual(confirm.label, "Close", "Reopening resets the baseline: \(theme)")
            confirm.tap()
            app.terminate()
        }
    }

    private func pixel(_ image: UIImage, at point: CGPoint) throws -> [Int] {
        let source = try XCTUnwrap(image.cgImage)
        let x = Int(point.x * image.scale)
        let y = Int(point.y * image.scale)
        let sample = try XCTUnwrap(source.cropping(to: CGRect(x: x, y: y, width: 1, height: 1)))
        var bytes = [UInt8](repeating: 0, count: 4)
        let context = try XCTUnwrap(CGContext(data: &bytes, width: 1, height: 1,
                                             bitsPerComponent: 8, bytesPerRow: 4,
                                             space: CGColorSpaceCreateDeviceRGB(),
                                             bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.draw(sample, in: CGRect(x: 0, y: 0, width: 1, height: 1))
        return bytes.prefix(3).map(Int.init)
    }
}
