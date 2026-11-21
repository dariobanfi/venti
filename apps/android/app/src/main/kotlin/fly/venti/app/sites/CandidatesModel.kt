package fly.venti.app.sites

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.wind.contains
import fly.venti.core.Bounds
import fly.venti.core.SiteCandidateView
import fly.venti.core.SiteCandidates
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The map's layer "Take-offs without a site": the places in view where the
 * public flights of several pilots began and no site stands.
 *
 * Which places, how large a part of the map may be asked about, and every
 * word are the core's. This class asks for the part in view while the
 * layer is on and keeps the answer; a part the core will not ask about is
 * no answer, and the core's words for it stand over the map.
 */
class CandidatesModel(
    /** Where requests wait for the map to come to rest. */
    private val scope: CoroutineScope,
    private val edit: SiteEditModel,
    /** How long the map must rest before the server is asked. */
    private val restMilliseconds: Long = SitesModel.REST_MILLISECONDS,
) {
    /** Whether the layer is on: the pilot's switch. Off, nothing is asked and nothing is drawn. */
    var isOn: Boolean by mutableStateOf(false)
        private set

    /** The answer for the part of the map in view; `null` while there is none. */
    var answer: SiteCandidates? by mutableStateOf(null)
        private set

    /** The places to draw. */
    val candidates: List<SiteCandidateView> get() = answer?.candidates.orEmpty()

    /**
     * What to say over the map, in the core's words: zoom in, more places
     * than one answer lists, or why the places could not be read.
     */
    var notice: String? by mutableStateOf(null)
        private set

    private var asked: Bounds? = null
    private var newest: Bounds? = null
    private var waiting: Job? = null

    /** Switches the layer; on, the part of the map last shown is asked for at once. */
    fun switch(on: Boolean) {
        if (on == isOn) return
        isOn = on
        waiting?.cancel()
        if (on) {
            newest?.let { region -> waiting = scope.launch { load(region) } }
        } else {
            answer = null
            asked = null
            notice = null
        }
    }

    /** The map says which part of it is in view; asked about unless the answer at hand covers it. */
    fun show(region: Bounds) {
        newest = region
        if (!isOn) return
        val asked = asked
        if (asked != null && answer != null && asked.contains(region)) return
        waiting?.cancel()
        waiting = scope.launch {
            // The map reports every pause of a drag; wait until it rests.
            delay(restMilliseconds)
            load(region)
        }
    }

    /** Asks for the places of a part of the map. */
    suspend fun load(area: Bounds) {
        try {
            val got = edit.candidates(area)
            if (!isOn) return
            answer = got
            asked = area
            notice = got.moreWords.takeIf { it.isNotEmpty() }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (!isOn) return
            answer = null
            asked = null
            notice = edit.words(error)
        }
    }
}
