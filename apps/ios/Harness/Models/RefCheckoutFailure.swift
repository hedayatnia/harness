import Foundation

/// A refused checkout from the ref picker (#424). The picker pins it above
/// the ref list so a long list can't push it out of view, and the composer
/// keeps it after the sheet closes. It describes the selection, not the
/// host: a timeout doesn't prove git never ran.
struct RefCheckoutFailure: Equatable {
    /// The ref the user tried to check out.
    let ref: String
    /// Git's refusal or the relay's reason (`describeUserFacingError`).
    let reason: String
    /// The selection that stays in place, if any.
    let kept: String?

    var title: String { "Couldn't switch to \(ref)" }

    var keptNote: String {
        kept.map { "\($0) is still selected." } ?? "The selection didn't change."
    }

    /// One line for the composer once the sheet is gone.
    var summary: String { "\(title): \(reason)" }

    /// Spoken when the failure appears.
    var announcement: String {
        "\(title). \(reason)\(reason.hasSuffix(".") ? "" : ".") \(keptNote)"
    }
}
