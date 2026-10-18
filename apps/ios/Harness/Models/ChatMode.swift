// Chat mode: a plain conversation instead of a coding session. No project,
// graff on the hosted gateway's auto tiers only, offered as Auto (low),
// Auto (high) and Auto (smart). Mirrors crates/ui/src/chat_mode.rs.

import Foundation

enum ChatMode {
    /// Chat mode always runs on graff, which reaches the hosted gateway.
    static let harness = "graff"

    /// Switching to Auto (smart) bills Gemini 4 Argon at API rates.
    static let smartCostNote = "Uses Gemini 4 Argon and may incur API costs."

    struct Tier: Equatable {
        /// The id graff lists (`<provider>/<model>`).
        let id: String
        /// Older gateways list the same tier under this id.
        let fallbackId: String?
        let label: String
        let note: String?
    }

    static let tiers: [Tier] = [
        Tier(id: "codegraff/auto-low", fallbackId: "codegraff/auto-medium", label: "Auto (low)", note: nil),
        Tier(id: "codegraff/auto-high", fallbackId: nil, label: "Auto (high)", note: nil),
        Tier(id: "codegraff/auto-smart", fallbackId: nil, label: "Auto (smart)", note: smartCostNote),
    ]

    /// The chat-mode models graff's catalog offers, in tier order, under the
    /// tier names. Before graff's catalog loads, the three tiers themselves.
    static func models(from catalog: [ModelInfo]) -> [ModelInfo] {
        let offered: [ModelInfo] = tiers.compactMap { tier in
            guard let row = catalog.first(where: { $0.id == tier.id })
                ?? tier.fallbackId.flatMap({ id in catalog.first(where: { $0.id == id }) }) else { return nil }
            return ModelInfo(id: row.id, label: tier.label, description: tier.note,
                             reasoningLevels: [], options: [])
        }
        if !offered.isEmpty { return offered }
        return tiers.map { ModelInfo(id: $0.id, label: $0.label, description: $0.note, reasoningLevels: []) }
    }

    /// The note to show while `modelId` is selected (the API cost warning for Auto (smart)).
    static func note(for modelId: String?) -> String? {
        guard let modelId else { return nil }
        return tiers.first { $0.id == modelId || $0.fallbackId == modelId }?.note
    }
}
