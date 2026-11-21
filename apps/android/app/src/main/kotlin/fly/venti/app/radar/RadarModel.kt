package fly.venti.app.radar

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.core.CoreException
import fly.venti.core.RadarKey
import fly.venti.core.RadarLayerView
import fly.venti.core.RadarMoment
import fly.venti.core.RadarView
import fly.venti.core.radar
import fly.venti.core.radarFrameImage
import fly.venti.core.radarPath
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.util.TimeZone

/** What the rain radar's card has to show. */
sealed interface RadarState {
    /** The list is being asked for, and none was fetched before. */
    data object Asking : RadarState

    /** The radar as the core read it: its layers, and the moments of the card's strip. */
    data class There(val view: RadarView) : RadarState

    /** No radar to show, and why, in words for the pilot. */
    data class Failed(val message: String) : RadarState
}

/**
 * One layer's picture in the app's colours, as the map takes it: a PNG, and
 * the four edges it is pinned to the map by.
 */
class RadarPicture(
    /** Which layer the picture is of: its place in the view's layers, the lowest first. */
    val layer: Int,
    /** The path of the frame the picture is of. */
    val path: String,
    /** Whether what the source did not see is drawn as the veil; clear in a picture that lies over another. */
    val drawsUnseen: Boolean,
    val png: ByteArray,
    val west: Double,
    val south: Double,
    val east: Double,
    val north: Double,
)

/**
 * What lies on the map at a moment: a picture for each layer that has one
 * then, the lowest first, each a layer of its own on the map.
 */
class RadarShown(
    /** The moment the pictures are of, as it was when they were fetched. */
    val moment: RadarMoment,
    val pictures: List<RadarPicture>,
)

/** The colours the steps of rain and what the radar did not see are drawn in, each as `0xRRGGBBAA`. */
class RadarColours(
    /** The colour of a line of the core's key. */
    val rain: (RadarKey) -> UInt,
    val unseen: UInt,
) {
    companion object {
        /** No colour at all: what a picture did not see where it lies over another picture. */
        const val CLEAR: UInt = 0u
    }
}

/**
 * The rain radar for the map tab's "Radar" layer, from Venti.
 *
 * What to ask the server and how to read its answer are the core's
 * (`radarPath`, `radar`, `radarFrameImage`): the layers and their frames,
 * the moments of the card's strip and which frame of which layer lies on
 * the map at each, their times and ages in words, when the newest is too
 * old to be the rain of now, the key and every word on the card come from
 * there. This class asks for the list and for the pictures, keeps each
 * picture for this run of the app while its frame is in the list, and
 * remembers which moment the pilot looks at: the one the core opens on,
 * the newest, until another is chosen.
 */
class RadarModel(
    private val transport: WindTransport = { url -> Http.get(url) },
    /** Fetches a frame's picture. */
    private val pictures: WindTransport = { url -> Http.get(url, accept = "image/png") },
    private val colours: RadarColours = RadarLayer.colours,
    /** Where the core reads an answer and writes a picture's colours. */
    private val background: CoroutineDispatcher = Dispatchers.Default,
    /** How far the clock here is ahead of UTC at a moment, seconds: the frames' times are this clock's. */
    private val utcOffsetSeconds: (Long) -> Int = { unixS -> TimeZone.getDefault().getOffset(unixS * 1000) / 1000 },
    /** How long a moment shows while the moments are played, milliseconds: about four a second. */
    private val stepMilliseconds: Long = STEP_MILLISECONDS,
) {
    /** The server to ask, without a slash at its end: the app's (`AppServer`), or a test's. */
    var server: () -> String? = { null }

    /** The moment the frames' ages are counted against, seconds since 1970. The app sets this to Venti's clock. */
    var clock: suspend () -> Long = { System.currentTimeMillis() / 1000 }

    /** The radar as the core last read it, from the server set now. */
    var view: RadarView? by mutableStateOf(null)
        private set

    /** The server's answer the view was read from, and whose it is: read again as the clock moves on. */
    private var answer: Pair<String, HttpAnswer>? = null

    /** Whether the server is being asked for the list. */
    var isAsking: Boolean by mutableStateOf(false)
        private set

    /** Why the last asking brought no list, in words for the pilot; `null` after one that did. */
    var problem: String? by mutableStateOf(null)
        private set

    /**
     * The moment the pilot chose, by its time; `null` for the one the core
     * opens on, the newest, which then stays the one shown when a newer
     * frame comes.
     */
    var chosenTime: Long? by mutableStateOf(null)
        private set

    /** The pictures on the map: the chosen moment's, or those before them while these are fetched or could not be. */
    var shown: RadarShown? by mutableStateOf(null)
        private set

    /** Why the chosen moment's pictures are not the ones on the map, in words for the pilot; `null` when they are. */
    var pictureProblem: String? by mutableStateOf(null)
        private set

    /** Whether the moments are being played through. */
    var isPlaying: Boolean by mutableStateOf(false)
        private set

    /**
     * The pictures in the app's colours, each layer's frames by their
     * paths, and by whether what was not seen is drawn in them.
     */
    private val kept = mutableMapOf<Pair<String, Boolean>, ByteArray>()

    /** One asking for the list at a time: the layer's own and the pilot's "Try again". */
    private val asking = Mutex()

    /** One picture is fetched at a time. */
    private val fetching = Mutex()

    /** The stops of the card's strip, the oldest first: one line of time for all layers. */
    val moments: List<RadarMoment> get() = view?.moments.orEmpty()

    /** The place of the moment looked at: the one chosen while the list has it, else the one the core opens on. */
    val place: Int
        get() {
            val view = view ?: return 0
            return view.moments.indexOfFirst { it.timeUnixS == chosenTime }.takeIf { it >= 0 } ?: view.firstMoment.toInt()
        }

    /** The moment looked at. */
    val moment: RadarMoment? get() = moments.getOrNull(place)

    /**
     * The moment whose pictures are on the map, as the list has it now, so
     * that its age is today's: its label is what the card writes. Where
     * the list has other pictures for that moment than the map, the moment
     * is as it was when the map's were fetched.
     */
    val shownMoment: RadarMoment?
        get() = shown?.let { shown ->
            val view = view ?: return@let shown.moment
            view.moments.firstOrNull { it.timeUnixS == shown.moment.timeUnixS && wanted(view, it) == shown.pictures.map { p -> p.path to p.drawsUnseen } }
                ?: shown.moment
        }

    /**
     * The layer whose caution the card says, if it has one: the topmost
     * that has frames, or the topmost while none has.
     */
    val cautioned: RadarLayerView?
        get() = view?.layers?.let { layers -> layers.lastOrNull { it.frames.isNotEmpty() } ?: layers.lastOrNull() }

    /** The core's sentence when the rain shown is too old to be the rain of now; `null` while it is not. */
    val caution: String? get() = cautioned?.takeIf { it.stale }?.staleWords

    /**
     * The layers that have a picture at the moment on the map, or at the
     * one looked at until its pictures are there, the lowest first.
     */
    val drawn: List<RadarLayerView>
        get() {
            val view = view ?: return emptyList()
            return (shownMoment ?: moment)?.frames.orEmpty().mapNotNull { view.layers.getOrNull(it.layer.toInt()) }
        }

    /** What the card shows. */
    val state: RadarState
        get() {
            val view = view
            return when {
                view != null && view.layers.isNotEmpty() -> RadarState.There(view)
                isAsking -> RadarState.Asking
                problem != null -> RadarState.Failed(checkNotNull(problem))
                // The core's own words for a layer without a picture.
                view != null -> RadarState.Failed(RadarWords.NO_LAYER)
                else -> RadarState.Asking
            }
        }

    /**
     * Whose rain it is, for the map's credit lines while the layer shows:
     * a line for each layer drawn, since each source asks for its credit
     * at the data.
     */
    val creditLines: List<String> get() = drawn.mapNotNull { it.credit?.text }

    /** Looks at a moment of the strip by its place; the newest is followed when a newer one comes. */
    fun choose(place: Int) {
        val view = view ?: return
        val picked = view.moments.getOrNull(place) ?: return
        chosenTime = picked.timeUnixS.takeIf { place != view.firstMoment.toInt() }
    }

    /**
     * What the moment looked at draws, the lowest first: its pictures are
     * fetched and put on the map ([showFrame]) whenever this changes.
     */
    val picturesWanted: List<Pair<String, Boolean>>
        get() = view?.let { view -> moment?.let { wanted(view, it) } }.orEmpty()

    /** The pictures a moment draws, the lowest first: each frame's path, and whether it draws what was not seen. */
    private fun wanted(view: RadarView, moment: RadarMoment): List<Pair<String, Boolean>> =
        moment.frames.mapNotNull { drawn ->
            view.layers.getOrNull(drawn.layer.toInt())?.frames?.getOrNull(drawn.frame.toInt())?.let { it.path to drawn.drawsUnseen }
        }

    /**
     * Asks for the list when the layer is switched on or comes back to
     * the front, and again every five minutes while it is there. Between
     * two askings the answer at hand is read again each minute, so that
     * the ages in words go with the clock.
     */
    suspend fun keepCurrent() {
        load()
        var minutes = 0
        while (true) {
            delay(MINUTE_MILLISECONDS)
            minutes += 1
            if (minutes % ASK_EVERY_MINUTES == 0) load() else read()
        }
    }

    /** Asks the server for the list of frames now. */
    suspend fun load() = asking.withLock {
        val server = server()
        if (server == null) {
            forget()
            return@withLock
        }
        // Another server's rain is not this one's.
        if (answer?.let { it.first != server } == true) {
            forget()
        }
        isAsking = true
        try {
            val fetched = transport(server + radarPath())
            val read = read(fetched)
            // The pilot may have changed the server while this was under way.
            if (server != this.server()) return@withLock
            answer = server to fetched
            settle(read)
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (server != this.server()) return@withLock
            problem = when (error) {
                is CoreException -> error.pilotMessage
                else -> "${RadarWords.UNREACHABLE} ${error.pilotMessage}"
            }
            // What was fetched before stays, with its ages as they are
            // now: the core says when it is no longer the rain of now.
            read()
        } finally {
            isAsking = false
        }
    }

    /** Reads the answer at hand again, by the clock as it is now. */
    private suspend fun read() {
        val (_, fetched) = answer ?: return
        try {
            settle(read(fetched))
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // It was read before: the view stays as it is.
        }
    }

    private suspend fun read(fetched: HttpAnswer): RadarView {
        val now = clock()
        return withContext(background) {
            radar(fetched.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(), fetched.body, utcOffsetSeconds(now), now)
        }
    }

    /** Takes a reading of the list, and lets go of the pictures of frames it no longer has. */
    private fun settle(read: RadarView) {
        view = read
        val paths = read.layers.flatMap { layer -> layer.frames.map { it.path } }.toSet()
        kept.keys.retainAll { (path, _) -> path in paths }
        // A moment the list has dropped is no longer one to choose.
        if (read.moments.none { it.timeUnixS == chosenTime }) chosenTime = null
    }

    /**
     * Puts the pictures of the moment looked at on the map: for each of
     * its frames, the lowest first, the one kept, or fetched now and
     * written in the app's colours by the core. They change together, once
     * all are there. Until then, and when one cannot be fetched, the
     * pictures before stay with their own time: never an empty map under a
     * newer time, and never two moments mixed.
     */
    suspend fun showFrame() {
        val server = server() ?: return
        val view = view ?: return
        val moment = moment
        if (moment == null) {
            shown = null
            pictureProblem = null
            return
        }
        val wanted = wanted(view, moment)
        try {
            val pictures = moment.frames.mapNotNull { drawn ->
                val layer = view.layers.getOrNull(drawn.layer.toInt()) ?: return@mapNotNull null
                val frame = layer.frames.getOrNull(drawn.frame.toInt()) ?: return@mapNotNull null
                val png = picture(server, frame.path, drawn.drawsUnseen, view)
                RadarPicture(
                    layer = drawn.layer.toInt(),
                    path = frame.path,
                    drawsUnseen = drawn.drawsUnseen,
                    png = png,
                    west = layer.west,
                    south = layer.south,
                    east = layer.east,
                    north = layer.north,
                )
            }
            // Another moment may have been chosen, or the list read anew, while these were fetched.
            if (!isLookedAt(wanted)) return
            val before = shown?.pictures.orEmpty()
            val same = before.size == pictures.size &&
                before.zip(pictures).all { (old, new) -> old.layer == new.layer && old.png === new.png }
            if (!same || shown?.moment?.timeUnixS != moment.timeUnixS) shown = RadarShown(moment, pictures)
            pictureProblem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (!isLookedAt(wanted)) return
            pictureProblem = when (error) {
                is CoreException -> error.pilotMessage
                else -> "${RadarWords.PICTURE_UNREACHABLE} ${error.pilotMessage}"
            }
        }
    }

    /** Whether these are still the pictures of the moment looked at. */
    private fun isLookedAt(wanted: List<Pair<String, Boolean>>): Boolean = moment != null && picturesWanted == wanted

    /**
     * A frame's picture in the app's colours: kept, or fetched and kept.
     * What the source did not see is the veil in the lowest picture of a
     * moment and clear in each above it, so that one source's veil does
     * not hide another's rain. One at a time, so that none is fetched twice.
     */
    private suspend fun picture(server: String, path: String, drawsUnseen: Boolean, view: RadarView): ByteArray = fetching.withLock {
        kept[path to drawsUnseen]?.let { return@withLock it }
        val fetched = pictures(server + path)
        if (fetched.status != HTTP_OK) {
            // The core has the words for a server's refusal: it reads the list's the same way.
            radar(fetched.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(), fetched.body, 0, 0)
            throw CoreException.Server(RadarWords.PICTURE_UNREACHABLE)
        }
        // The colours in the order of the steps, the first step first.
        val rain = view.key.sortedBy { it.step }.map(colours.rain)
        val unseen = if (drawsUnseen) colours.unseen else RadarColours.CLEAR
        val png = withContext(background) { radarFrameImage(fetched.body, rain, unseen) }
        // Only what the list still has is kept.
        if (this.view?.layers?.any { layer -> layer.frames.any { it.path == path } } == true) kept[path to drawsUnseen] = png
        png
    }

    /**
     * Plays the moments through once, from the oldest, about four a
     * second, and stops on the newest. Each moment shows once its pictures
     * are there. Cancelled, it stops at the moment it had reached.
     */
    suspend fun play() {
        val view = view ?: return
        val moments = view.moments
        if (moments.size < 2) return
        isPlaying = true
        try {
            for ((place, moment) in moments.withIndex()) {
                // The newest is chosen as "the newest", which a newer frame then follows.
                chosenTime = moment.timeUnixS.takeIf { place != view.firstMoment.toInt() }
                showFrame()
                if (place != moments.lastIndex) delay(stepMilliseconds)
            }
        } finally {
            isPlaying = false
        }
    }

    /** Lets everything go: another server, or none. */
    private fun forget() {
        answer = null
        view = null
        problem = null
        chosenTime = null
        shown = null
        pictureProblem = null
        kept.clear()
    }

    /** How many pictures are kept: for the tests. */
    val keptPictures: Int get() = kept.size

    companion object {
        /** How often the answer at hand is read again, for the ages in words. */
        const val MINUTE_MILLISECONDS = 60_000L

        /** After how many minutes the server is asked for the list again: a frame comes every five. */
        const val ASK_EVERY_MINUTES = 5

        /** How long a moment shows while the moments are played. */
        const val STEP_MILLISECONDS = 250L

        private const val HTTP_OK = 200
    }
}

/** Words of the rain radar that are not the core's: the shell's own, for when there is nothing to read. */
object RadarWords {
    /** The chip of the layer. */
    const val LAYER = "Radar"

    const val ASKING = "Asking Venti for the rain radar"
    const val UNREACHABLE = "Venti could not be reached for the rain radar."
    const val PICTURE_UNREACHABLE = "Venti could not be reached for this radar picture."

    /** A server that answers with a radar of no layers at all. */
    const val NO_LAYER = "Venti has no radar picture."

    /** What TalkBack calls the line of moments. */
    const val FRAME = "Radar picture"

    /** The button that plays the moments through, and the one that stops it. */
    const val PLAY = "Play the radar pictures"
    const val STOP = "Stop"

    /** A moment as TalkBack says it on the strip: the line the core writes for it, "Rain radar, 14:15, 4 minutes ago." */
    fun spoken(moment: RadarMoment): String = moment.label

    /** What a cell of the strip holds: the minutes of the moment's time on the clock, "15" of "14:15". */
    fun minutes(moment: RadarMoment): String = moment.clock.substringAfter(':')
}
