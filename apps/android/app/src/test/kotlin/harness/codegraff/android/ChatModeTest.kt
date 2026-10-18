package harness.codegraff.android

import harness.codegraff.android.model.ChatMode
import harness.codegraff.android.model.ModelInfo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Chat mode offers graff's three auto tiers only and warns about API costs on Auto (smart) (ChatModeTests.swift). */
class ChatModeTest {
    private fun model(id: String, label: String) = ModelInfo(id, label, null, listOf("low", "high"))

    @Test
    fun onlyTheAutoTiersAreOfferedInOrderUnderTheirNames() {
        val offered = ChatMode.models(
            listOf(
                model("codegraff/auto-smart", "auto-smart"),
                model("codegraff/claude-opus-5-5", "Claude Opus 5.5"),
                model("codegraff/auto-high", "Auto (high)"),
                model("chatgpt-new/gpt-6.1-sol", "GPT-6.1 Sol"),
                model("codegraff/auto-low", "Auto (low)"),
            ),
        )
        assertEquals(listOf("codegraff/auto-low", "codegraff/auto-high", "codegraff/auto-smart"), offered.map { it.id })
        assertEquals(listOf("Auto (low)", "Auto (high)", "Auto (smart)"), offered.map { it.label })
        assertEquals(ChatMode.SMART_COST_NOTE, offered[2].description)
        assertTrue(offered.all { it.reasoningLevels.isEmpty() })
    }

    @Test
    fun anOlderGatewayOffersAutoMediumAsAutoLowAndEmptyShowsAllThree() {
        val offered = ChatMode.models(listOf(model("codegraff/auto-medium", "Auto (medium)"), model("codegraff/auto-high", "Auto (high)")))
        assertEquals(listOf("codegraff/auto-medium", "codegraff/auto-high"), offered.map { it.id })
        assertEquals("Auto (low)", offered.first().label)
        assertEquals(listOf("Auto (low)", "Auto (high)", "Auto (smart)"), ChatMode.models(emptyList()).map { it.label })
    }

    @Test
    fun theCostNoteBelongsToAutoSmartOnly() {
        assertEquals(ChatMode.SMART_COST_NOTE, ChatMode.note("codegraff/auto-smart"))
        assertNull(ChatMode.note("codegraff/auto-high"))
        assertNull(ChatMode.note("codegraff/auto-medium"))
        assertNull(ChatMode.note(null))
    }
}
