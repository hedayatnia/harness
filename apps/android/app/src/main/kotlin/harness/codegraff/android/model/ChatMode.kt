package harness.codegraff.android.model

/**
 * Chat mode: a plain conversation instead of a coding session. No project, graff on the hosted
 * gateway's auto tiers only, offered as Auto (low), Auto (high) and Auto (smart).
 * Mirrors crates/ui/src/chat_mode.rs and ChatMode.swift.
 */
object ChatMode {
    /** Chat mode always runs on graff, which reaches the hosted gateway. */
    const val HARNESS = "graff"

    /** Switching to Auto (smart) bills Gemini 4 Argon at API rates. */
    const val SMART_COST_NOTE = "Uses Gemini 4 Argon and may incur API costs."

    data class Tier(val id: String, val fallbackId: String?, val label: String, val note: String?)

    val tiers = listOf(
        Tier("codegraff/auto-low", "codegraff/auto-medium", "Auto (low)", null),
        Tier("codegraff/auto-high", null, "Auto (high)", null),
        Tier("codegraff/auto-smart", null, "Auto (smart)", SMART_COST_NOTE),
    )

    /** The chat-mode models graff's catalog offers, in tier order, under the tier names. Before the catalog loads, the three tiers themselves. */
    fun models(catalog: List<ModelInfo>): List<ModelInfo> {
        val offered = tiers.mapNotNull { tier ->
            val row = catalog.firstOrNull { it.id == tier.id }
                ?: tier.fallbackId?.let { id -> catalog.firstOrNull { it.id == id } }
                ?: return@mapNotNull null
            ModelInfo(row.id, tier.label, tier.note, emptyList())
        }
        return offered.ifEmpty { tiers.map { ModelInfo(it.id, it.label, it.note, emptyList()) } }
    }

    /** The note to show while [modelId] is selected (the API cost warning for Auto (smart)). */
    fun note(modelId: String?): String? =
        modelId?.let { id -> tiers.firstOrNull { it.id == id || it.fallbackId == id }?.note }
}
