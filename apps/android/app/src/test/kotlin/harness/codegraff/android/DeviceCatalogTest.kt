package harness.codegraff.android

import harness.codegraff.android.model.ChatConfig
import harness.codegraff.android.model.DeviceCatalog
import harness.codegraff.android.model.DeviceCatalogCache
import harness.codegraff.android.model.HarnessCatalog
import harness.codegraff.android.model.HarnessInfo
import harness.codegraff.android.model.ModelInfo
import harness.codegraff.android.model.modelSelection
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class DeviceCatalogTest {
    private val model = ModelInfo("device-model", "Device model", null, listOf("low", "high"))
    private val key = "host" to "agent"

    @Test fun savedIdentityAndEffortSurviveHydrationAndSerializeUnchanged() {
        val selection = modelSelection("agent", "saved-model", "ultra", DeviceCatalog())
        val config = ChatConfig("agent", selection.model.id, selection.reasoning)
        assertEquals("saved-model", config.model)
        assertEquals("ultra", config.reasoning)
        assertTrue(selection.canSend) // Explicit, unverified configuration may queue offline.
    }

    @Test fun successfulAbsenceBlocksSavedSelectionWithoutReplacingIt() {
        val selection = modelSelection("agent", "saved-model", "ultra", DeviceCatalog(listOf(model), isLive = true))
        assertEquals("saved-model", selection.model.id)
        assertEquals("ultra", selection.reasoning)
        assertTrue(selection.unavailable)
        assertFalse(selection.canSend)
    }

    @Test fun successfulEmptyCatalogIsAuthoritative() {
        val selection = modelSelection("agent", "saved-model", "high", DeviceCatalog(emptyList(), isLive = true))
        assertFalse(selection.canSend)
        assertEquals("saved-model", selection.model.id)
    }

    @Test fun cachedAbsenceDoesNotDisableAnExplicitOfflineSelection() {
        val selection = modelSelection("agent", "saved-model", "max", DeviceCatalog(listOf(model)))
        assertTrue(selection.canSend)
        assertEquals("max", selection.reasoning)
    }

    @Test fun agentMustBeOfferedWhenTheDeviceHasConfirmedItsAgents() {
        val selection = modelSelection("agent", model.id, null, DeviceCatalog(listOf(model)),
            DeviceCatalog(listOf(HarnessInfo("other-agent", "Other agent")), isLive = true))
        assertFalse(selection.canSend)
        assertEquals(model.id, selection.model.id)
    }

    @Test fun unconfiguredComposerCannotSendBeforeActualDeviceMetadata() {
        val selection = modelSelection("claude-code", "", "", DeviceCatalog())
        assertEquals("", selection.model.id)
        assertEquals("Select model", selection.model.label)
        assertFalse(selection.canSend)
        assertFalse(modelSelection("claude-code", null, null, DeviceCatalog(emptyList(), true)).canSend)
    }

    @Test fun actualCachedOrLiveRowsCanSupplyAnUnconfiguredDefault() {
        for (live in listOf(false, true)) {
            val selection = modelSelection("agent", null, null, DeviceCatalog(listOf(model), live))
            assertTrue(selection.canSend)
            assertEquals(model.id, selection.model.id)
            assertEquals("high", selection.reasoning)
        }
    }

    @Test fun metadataNeverRewritesExplicitEffortEvenWhenItDoesNotAdvertiseIt() {
        val selection = modelSelection("agent", model.id, "ultra", DeviceCatalog(listOf(model), true))
        assertEquals("ultra", selection.reasoning)
        assertTrue(selection.canSend)
    }

    @Test fun aliasMetadataEnrichesTheLabelButPreservesWireIdentity() {
        val selection = modelSelection("claude-code", "claude-opus-5[1m]", "max",
            DeviceCatalog(HarnessCatalog.models("claude-code"), true))
        assertEquals("claude-opus-5[1m]", selection.model.id)
        assertEquals("Opus 5", selection.model.label)
        assertTrue(selection.canSend)
    }

    @Test fun unknownAgentsNeverInheritClaudeModels() {
        assertTrue(HarnessCatalog.models("new-agent").isEmpty())
        assertEquals("", HarnessCatalog.defaultModel("new-agent").id)
        assertEquals("Select model", HarnessCatalog.resolve(null, emptyList(), "new-agent").label)
    }

    @Test fun successfulEmptyResponseClearsTheDeviceCache() = runTest {
        val cache = DeviceCatalogCache<Pair<String, String>, ModelInfo>()
        cache.load(key) { listOf(model) }
        val result = cache.load(key) { emptyList() }
        assertTrue(result.isLive)
        assertTrue(result.items.isEmpty())
        val offline = cache.load(key) { null }
        assertFalse(offline.isLive)
        assertTrue(offline.items.isEmpty())
    }

    @Test fun failuresUseOnlySameDeviceAndAgentCacheAndAreNotAuthoritative() = runTest {
        val cache = DeviceCatalogCache<Pair<String, String>, ModelInfo>()
        cache.load(key) { listOf(model) }
        val result = cache.load(key) { error("unavailable") }
        assertEquals(listOf(model), result.items)
        assertFalse(result.isLive)
        assertTrue(cache.cached("other-host" to "agent").items.isEmpty())
        assertTrue(cache.cached("host" to "other-agent").items.isEmpty())
        assertTrue(cache.load("other-host" to "agent") { null }.items.isEmpty())
    }

    @Test fun olderCompletionCannotOverwriteANewerSuccessfulReply() = runTest {
        val cache = DeviceCatalogCache<Pair<String, String>, ModelInfo>()
        val slowReply = CompletableDeferred<List<ModelInfo>?>()
        val old = async { cache.load(key) { slowReply.await() } }
        runCurrent()
        val latest = model.copy(id = "latest")
        cache.load(key) { listOf(latest) }
        slowReply.complete(listOf(model))
        val result = old.await()
        assertEquals(listOf(latest), result.items)
        assertTrue(result.isLive)
        assertEquals(listOf(latest), cache.cached(key).items)
    }

    @Test fun olderReplyCannotUndoANewerConfirmedAbsence() = runTest {
        val cache = DeviceCatalogCache<Pair<String, String>, ModelInfo>()
        val slowReply = CompletableDeferred<List<ModelInfo>?>()
        val old = async { cache.load(key) { slowReply.await() } }
        runCurrent()
        cache.load(key) { emptyList() }
        slowReply.complete(listOf(model))
        val result = old.await()
        assertTrue(result.items.isEmpty())
        assertTrue(result.isLive)
        assertFalse(modelSelection("agent", "saved-model", "max", result).canSend)
    }

    @Test fun canceledNonCooperativeFetchCannotPoisonTheCache() = runTest {
        val cache = DeviceCatalogCache<Pair<String, String>, ModelInfo>()
        val slowReply = CompletableDeferred<List<ModelInfo>?>()
        val job = launch {
            cache.load(key) { withContext(NonCancellable) { slowReply.await() } }
        }
        runCurrent()
        job.cancel()
        slowReply.complete(listOf(model))
        job.join()
        assertTrue(cache.cached(key).items.isEmpty())
    }

    @Test fun refMetadataAlsoDistinguishesNoBranchesFromAnUnavailableHost() = runTest {
        val cache = DeviceCatalogCache<Pair<String, String>, String>()
        val project = "host" to "/project"
        cache.load(project) { listOf("branch") }
        assertEquals(listOf("branch"), cache.load(project) { null }.items)
        assertFalse(cache.load(project) { null }.isLive)
        assertTrue(cache.load(project) { emptyList() }.isLive)
        assertTrue(cache.cached(project).items.isEmpty())
        assertTrue(cache.cached("host" to "/other-project").items.isEmpty())
    }
}
