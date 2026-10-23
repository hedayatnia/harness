package harness.codegraff.android

import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.test.ext.junit.runners.AndroidJUnit4
import harness.codegraff.android.model.DeviceCatalog
import harness.codegraff.android.ui.components.rememberDeviceCatalog
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CatalogLoadingTest {
    @get:Rule val rule = createComposeRule()

    @Test fun changingHostOrAgentClearsPriorRowsBeforeTheNewRequestCompletes() {
        var context by mutableStateOf("host-a" to "agent-a")
        val oldReply = CompletableDeferred<DeviceCatalog<String>>()
        val nextReply = CompletableDeferred<DeviceCatalog<String>>()
        val lastReply = CompletableDeferred<DeviceCatalog<String>>()
        rule.setContent {
            val selected = context
            val load = rememberDeviceCatalog(selected, Unit,
                initial = { if (selected == "host-a" to "agent-a") DeviceCatalog(listOf("cached-a")) else DeviceCatalog() },
                fetch = {
                    // Exercise canceled requests whose underlying relay does not cooperate with cancellation.
                    withContext(NonCancellable) {
                        when (selected) {
                            "host-a" to "agent-a" -> oldReply.await()
                            "host-b" to "agent-a" -> nextReply.await()
                            else -> lastReply.await()
                        }
                    }
                },
            )
            Text(load.catalog.items.firstOrNull() ?: "No metadata")
        }
        rule.onNodeWithText("cached-a").assertExists()
        rule.runOnIdle { context = "host-b" to "agent-a" }
        rule.onNodeWithText("cached-a").assertDoesNotExist()
        rule.onNodeWithText("No metadata").assertExists()
        rule.runOnIdle { nextReply.complete(DeviceCatalog(listOf("host-b-model"), true)) }
        rule.onNodeWithText("host-b-model").assertExists()
        rule.runOnIdle { context = "host-b" to "agent-b" }
        rule.onNodeWithText("host-b-model").assertDoesNotExist()
        rule.onNodeWithText("No metadata").assertExists()
        rule.runOnIdle {
            lastReply.complete(DeviceCatalog(listOf("agent-b-model"), true))
            oldReply.complete(DeviceCatalog(listOf("stale-a"), true))
        }
        rule.onNodeWithText("agent-b-model").assertExists()
        rule.onNodeWithText("stale-a").assertDoesNotExist()
    }
}
