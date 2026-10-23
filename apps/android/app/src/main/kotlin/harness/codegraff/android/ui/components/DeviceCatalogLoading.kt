package harness.codegraff.android.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import harness.codegraff.android.model.DeviceCatalog
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive

class CatalogLoad<T>(initial: DeviceCatalog<T>) {
    var catalog by mutableStateOf(initial)
        internal set
    var loading by mutableStateOf(true)
        internal set
    internal var attempt by mutableStateOf(0)
    fun retry() { attempt++ }
}

/** remember is keyed as well as the effect: no frame can expose the previous host/agent's rows. */
@Composable
fun <T> rememberDeviceCatalog(
    context: Any?,
    refreshKey: Any?,
    initial: () -> DeviceCatalog<T>,
    fetch: suspend () -> DeviceCatalog<T>,
): CatalogLoad<T> {
    val state = remember(context) { CatalogLoad(initial()) }
    LaunchedEffect(context, refreshKey, state.attempt) {
        do {
            state.loading = true
            val result = fetch()
            currentCoroutineContext().ensureActive()
            state.catalog = result
            state.loading = false
            if (!result.isLive) delay(15_000) // Recover from an unavailable relay without reopening the screen.
        } while (!result.isLive)
    }
    return state
}
