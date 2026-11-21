package fly.venti.app.radar

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.core.CoreException
import fly.venti.core.RadarFrameView
import fly.venti.core.RadarKey
import fly.venti.core.RadarLayerView
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
    /** The frames are being asked for, and none were fetched before. */
    data object Asking : RadarState

    /** The radar as the core read it, and the layer of it that is shown. */
    data class There(val view: RadarView, val layer: RadarLayerView) : RadarState

    /** No radar to show, and why, in words for the pilot. */
    data class Failed(val message: String) : RadarState
}

/**
 * A frame's picture in the app's colours, as the map takes it: a PNG, and
 * the four edges it is pinned to the map by.
 */
class RadarPicture(
    /** The frame the picture is of, as it was when the picture was fetched. */
    val frame: RadarFrameView,
    val png: ByteArray,
    val west: Double,
    val south: Double,
    val east: Double,
    val north: Double,
)

/** The colours the steps of rain and what the radar did not see are drawn in, each as `0xRRGGBBAA`. */
class RadarColours(
    /** The colour of a line of the core's key. */
    val rain: (RadarKey) -> UInt,
    val unseen: UInt,
)

/**
 * The rain radar for the map tab's "Radar" layer, from Venti.
 *
 * What to ask the server and how to read its answer are the core's
 * (`radarPath`, `radar`, `radarFrameImage`): the frames, their times and
 * ages in words, when the newest is too old to be the rain of now, the key
 * and every word on the card come from there. This class asks for the list
 * and for the pictures, keeps each picture for this run of the app while
 * its frame is in the list, and remembers which frame the pilot looks at:
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
    /** How long a frame shows while the frames are played, milliseconds: about four a second. */
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
     * The frame the pilot chose, by its time; `null` for the newest, which
     * then stays the one shown when a newer frame comes.
     */
    var chosenTime: Long? by mutableStateOf(null)
        private set

    /** The picture on the map: the chosen frame's, or the one before it while that is fetched or could not be. */
    var shown: RadarPicture? by mutableStateOf(null)
        private set

    /** Why the chosen frame's picture is not the one on the map, in words for the pilot; `null` when it is. */
    var pictureProblem: String? by mutableStateOf(null)
        private set

    /** Whether the frames are being played through. */
    var isPlaying: Boolean by mutableStateOf(false)
        private set

    /** The pictures in the app's colours, by their frames' paths. */
    private val kept = mutableMapOf<String, ByteArray>()

    /** One asking for the list at a time: the layer's own and the pilot's "Try again". */
    private val asking = Mutex()

    /** One picture is fetched at a time. */
    private val fetching = Mutex()

    /**
     * The layer shown: the first the server has. A server has one for each
     * radar source, never merged; Venti's has one today.
     */
    val layer: RadarLayerView? get() = view?.layers?.firstOrNull()

    /** The frame looked at: the one chosen while the list has it, else the newest. */
    val frame: RadarFrameView?
        get() = layer?.frames?.let { frames -> frames.firstOrNull { it.timeUnixS == chosenTime } ?: frames.lastOrNull() }

    /**
     * The frame whose picture is on the map, as the list has it now, so
     * that its age is today's: its label is what the card writes.
     */
    val shownFrame: RadarFrameView?
        get() = shown?.let { picture -> layer?.frames?.firstOrNull { it.path == picture.frame.path } ?: picture.frame }

    /** What the card shows. */
    val state: RadarState
        get() {
            val view = view
            val layer = layer
            return when {
                view != null && layer != null -> RadarState.There(view, layer)
                isAsking -> RadarState.Asking
                problem != null -> RadarState.Failed(checkNotNull(problem))
                // The core's own words for a layer without a picture.
                view != null -> RadarState.Failed(RadarWords.NO_LAYER)
                else -> RadarState.Asking
            }
        }

    /** Whose radar it is, for the map's credit line while the layer shows: the source asks for it at the data. */
    val creditLine: String? get() = layer?.credit?.text

    /** Looks at a frame of the list by its place; the newest is followed when a newer one comes. */
    fun choose(place: Int) {
        val frames = layer?.frames ?: return
        val picked = frames.getOrNull(place) ?: return
        chosenTime = picked.timeUnixS.takeIf { place != frames.lastIndex }
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
        kept.keys.retainAll(paths)
        // A frame the list has dropped is no longer one to choose.
        if (layer?.frames?.none { it.timeUnixS == chosenTime } == true) chosenTime = null
    }

    /**
     * Puts the picture of the frame looked at on the map: the one kept, or
     * fetched now and written in the app's colours by the core. Until it
     * is there, and when it cannot be fetched, the picture before stays
     * with its own time: never an empty map under a newer time.
     */
    suspend fun showFrame() {
        val server = server() ?: return
        val layer = layer ?: return
        val frame = frame
        if (frame == null) {
            shown = null
            pictureProblem = null
            return
        }
        try {
            val png = picture(server, frame)
            // Another frame may have been chosen while this one was fetched.
            if (this.frame?.path != frame.path) return
            if (shown?.frame?.path != frame.path || shown?.png !== png) {
                shown = RadarPicture(frame, png, west = layer.west, south = layer.south, east = layer.east, north = layer.north)
            }
            pictureProblem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            if (this.frame?.path != frame.path) return
            pictureProblem = when (error) {
                is CoreException -> error.pilotMessage
                else -> "${RadarWords.PICTURE_UNREACHABLE} ${error.pilotMessage}"
            }
        }
    }

    /** A frame's picture in the app's colours: kept, or fetched and kept. One at a time, so that none is fetched twice. */
    private suspend fun picture(server: String, frame: RadarFrameView): ByteArray = fetching.withLock {
        kept[frame.path]?.let { return@withLock it }
        val view = checkNotNull(view)
        val fetched = pictures(server + frame.path)
        if (fetched.status != HTTP_OK) {
            // The core has the words for a server's refusal: it reads the list's the same way.
            radar(fetched.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(), fetched.body, 0, 0)
            throw CoreException.Server(RadarWords.PICTURE_UNREACHABLE)
        }
        // The colours in the order of the steps, the first step first.
        val rain = view.key.sortedBy { it.step }.map(colours.rain)
        val png = withContext(background) { radarFrameImage(fetched.body, rain, colours.unseen) }
        // Only what the list still has is kept.
        if (this.layer?.frames?.any { it.path == frame.path } == true) kept[frame.path] = png
        png
    }

    /**
     * Plays the frames through once, from the oldest, about four a
     * second, and stops on the newest. Each frame shows once its picture
     * is there. Cancelled, it stops at the frame it had reached.
     */
    suspend fun play() {
        val frames = layer?.frames ?: return
        if (frames.size < 2) return
        isPlaying = true
        try {
            for (frame in frames) {
                // The newest is chosen as "the newest", which a newer frame then follows.
                chosenTime = frame.timeUnixS.takeIf { frame !== frames.last() }
                showFrame()
                if (frame !== frames.last()) delay(stepMilliseconds)
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

        /** How long a frame shows while the frames are played. */
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

    /** What TalkBack calls the line of frames. */
    const val FRAME = "Radar picture"

    /** The button that plays the frames through, and the one that stops it. */
    const val PLAY = "Play the radar pictures"
    const val STOP = "Stop"

    /** A frame as TalkBack says it on the strip: its time and its age, both the core's: "12:40, 4 minutes ago". */
    fun spoken(frame: RadarFrameView): String = "${frame.clock}, ${frame.age}"

    /** What a cell of the strip holds: the minutes of the frame's time on the clock, "40" of "12:40". */
    fun minutes(frame: RadarFrameView): String = frame.clock.substringAfter(':')
}
