package harness.codegraff.android.model

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

/** Only a successful reply proves absence. Cached rows remain useful while a host is offline. */
data class DeviceCatalog<T>(val items: List<T> = emptyList(), val isLive: Boolean = false)

/** Device/context-keyed metadata; neither failures nor canceled/older requests may overwrite a reply. */
class DeviceCatalogCache<K, T> {
    private val items = mutableMapOf<K, List<T>>()
    private val requests = mutableMapOf<K, Long>()
    private val latest = mutableMapOf<K, DeviceCatalog<T>>()

    fun cached(key: K): DeviceCatalog<T> = DeviceCatalog(items[key].orEmpty())

    suspend fun load(key: K, fetch: suspend () -> List<T>?): DeviceCatalog<T> {
        val request = (requests[key] ?: 0) + 1
        requests[key] = request
        val fetched = try {
            fetch()
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        }
        currentCoroutineContext().ensureActive()
        if (requests[key] != request) return latest[key] ?: cached(key)
        if (fetched != null) items[key] = fetched // A successful empty response clears the cache, too.
        return DeviceCatalog(items[key].orEmpty(), isLive = fetched != null).also { latest[key] = it }
    }
}

data class ModelSelection(val model: ModelInfo, val reasoning: String?, val unavailable: Boolean) {
    val canSend: Boolean get() = model.id.isNotEmpty() && !unavailable
}

/** A saved identity/effort is configuration, not a catalog index or a provisional default. */
fun modelSelection(
    harness: String,
    modelId: String?,
    reasoning: String?,
    models: DeviceCatalog<ModelInfo>,
    harnesses: DeviceCatalog<HarnessInfo> = DeviceCatalog(),
): ModelSelection {
    val selected = HarnessCatalog.resolve(modelId?.takeIf { it.isNotBlank() }, models.items, harness)
    val unavailable = (harnesses.isLive && harnesses.items.none { it.id == harness }) ||
        (models.isLive && selected.id.isNotEmpty() && HarnessCatalog.resolveExisting(selected.id, models.items) == null)
    return ModelSelection(selected, reasoning?.takeIf { it.isNotBlank() } ?: HarnessCatalog.defaultReasoning(selected), unavailable)
}
