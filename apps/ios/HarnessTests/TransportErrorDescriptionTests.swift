import Foundation
import XCTest
@testable import Harness

final class TransportErrorDescriptionTests: XCTestCase {
    func testTransportErrorDescriptionDoesNotExposeUserInfo() {
        let nested = NSError(domain: "Nested", code: 7,
                             userInfo: [NSLocalizedDescriptionKey: "SENTINEL"])
        let error = NSError(
            domain: NSURLErrorDomain,
            code: NSURLErrorTimedOut,
            userInfo: [
                NSURLErrorFailingURLStringErrorKey: "wss://x/ws?token=SENTINEL",
                NSUnderlyingErrorKey: nested,
            ]
        )
        let description = describeTransportError(error)
        XCTAssertFalse(description.contains("SENTINEL"))
        XCTAssertTrue(description.contains("-1001"))
        XCTAssertTrue(description.contains("timedOut"))
    }

    // #423: a refused checkout reached the ref picker as a bridged type name
    // ("Harness.RelayError#0"); each relay failure must keep its own reason.
    func testRelayFailuresKeepDistinctReadableMessages() {
        let cases: [(RelayError, String)] = [
            (.rpc("git: synthetic checkout refused"), "git: synthetic checkout refused"),
            (.timeout, "The device didn't respond"),
            (.hostOffline, "The device is offline"),
            (.notConnected, "Not connected to the device"),
        ]
        for (error, expected) in cases {
            let message = describeUserFacingError(error)
            XCTAssertEqual(message, expected)
            XCTAssertFalse(message.contains("RelayError"), message)
            XCTAssertFalse(message.contains("#"), message)
        }
        XCTAssertEqual(Set(cases.map { describeUserFacingError($0.0) }).count, cases.count)
    }

    func testGitRefusalKeepsMultilineStderrAndBlankRpcStillExplains() {
        let stderr = "error: Your local changes to the following files would be overwritten by checkout:\n\tREADME.md"
        XCTAssertEqual(describeUserFacingError(RelayError.rpc(stderr + "\n")), stderr)
        XCTAssertEqual(describeUserFacingError(RelayError.rpc("  ")), "The device refused the request")
    }

    func testUrlErrorsAreReadableWithDiagnosticCodeAndNoUserInfo() {
        let error = NSError(domain: NSURLErrorDomain, code: NSURLErrorTimedOut,
                            userInfo: [NSURLErrorFailingURLStringErrorKey: "wss://x/ws?token=SENTINEL",
                                       NSLocalizedDescriptionKey: "SENTINEL"])
        let message = describeUserFacingError(error)
        XCTAssertTrue(message.hasPrefix("The connection timed out"), message)
        XCTAssertTrue(message.contains("-1001"), message)
        XCTAssertFalse(message.contains("SENTINEL"), message)
        // A relay-level timeout and a socket-level one stay distinguishable.
        XCTAssertNotEqual(message, describeUserFacingError(RelayError.timeout))
        let offline = describeUserFacingError(URLError(.notConnectedToInternet))
        XCTAssertTrue(offline.hasPrefix("No internet connection"), offline)
    }

    func testUnknownErrorsFallBackToDiagnosticIdentity() {
        let error = NSError(domain: "Synthetic", code: 42, userInfo: [NSLocalizedDescriptionKey: "SENTINEL"])
        XCTAssertEqual(describeUserFacingError(error), "Synthetic#42")
        XCTAssertEqual(describeUserFacingError(CancellationError()), "The request was cancelled")
    }

    // #424: the picker's pinned failure names the attempted ref, the reason,
    // and the selection that stays.
    func testRefCheckoutFailureKeepsReasonAndOldSelection() {
        let reason = describeUserFacingError(RelayError.rpc("git: synthetic checkout refused"))
        let failure = RefCheckoutFailure(ref: "main", reason: reason, kept: "feature/x")
        XCTAssertEqual(failure.title, "Couldn't switch to main")
        XCTAssertEqual(failure.keptNote, "feature/x is still selected.")
        XCTAssertEqual(failure.summary, "Couldn't switch to main: git: synthetic checkout refused")
        XCTAssertEqual(failure.announcement,
                       "Couldn't switch to main. git: synthetic checkout refused. feature/x is still selected.")
        let timedOut = RefCheckoutFailure(ref: "main", reason: describeUserFacingError(RelayError.timeout), kept: nil)
        XCTAssertEqual(timedOut.keptNote, "The selection didn't change.")
        XCTAssertNotEqual(timedOut.summary, failure.summary)
    }
}
