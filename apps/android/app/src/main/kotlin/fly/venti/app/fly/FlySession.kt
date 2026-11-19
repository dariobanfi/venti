package fly.venti.app.fly

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.pilotMessage
import fly.venti.app.settings.Preferences
import fly.venti.core.AirspaceCheck
import fly.venti.core.AirspaceSet
import fly.venti.core.AirspaceWarning
import fly.venti.core.CockpitView
import fly.venti.core.FlightSession
import fly.venti.core.LiveScore
import fly.venti.core.PendingRecording
import fly.venti.core.FaiAreas
import fly.venti.core.Replay
import fly.venti.core.TargetPlace
import fly.venti.core.ToneView
import fly.venti.core.WayToTarget
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.io.File
import java.util.TimeZone

/**
 * The phone's sensors, as a flying session needs them: the position, the
 * pressure sensor and the Bluetooth vario, each reporting straight into the
 * core's session with the time of its reading.
 */
interface FlightSensors {
    /** Switches the sensors on. `onProblem` is called with words for the pilot. */
    fun start(session: FlightSession, onProblem: (String) -> Unit)

    fun stop()
}

/** Whatever says a warning aloud. */
interface Announcer {
    fun say(text: String)

    fun stop()
}

/** Says nothing. Tests have nobody listening. */
class SilentAnnouncer : Announcer {
    override fun say(text: String) = Unit

    override fun stop() = Unit
}

/** Whatever makes the vario's sound. */
interface TonePlayer {
    var isMuted: Boolean

    fun start()

    fun stop()

    /** Sets what the vario sounds like from now on. `null` is silence. */
    fun play(tone: ToneView?)
}

/**
 * One flying session as the cockpit screen sees it.
 *
 * It passes what the phone's sensors report to the core's session; five
 * times a second it asks the core what to show and what the vario should
 * sound like. It contains no flying logic.
 *
 * A session is either live, fed by the sensors and recorded, or a replay of
 * a recorded flight, fed by the core and not recorded.
 */
class FlySession(
    private val dataFolder: File,
    private val preferences: Preferences,
    private val sensors: FlightSensors,
    private val sound: TonePlayer,
    /** Where the refresh runs: the main thread in the app. */
    private val scope: CoroutineScope,
    /** Says a new airspace warning aloud. */
    private val voice: Announcer = SilentAnnouncer(),
    /**
     * How often the screen and the vario tone are refreshed, in
     * milliseconds. Five times a second in the app. A UI test on an
     * emulator asks for less: see [SLOW_REFRESH_MILLISECONDS].
     */
    private val refreshMilliseconds: Long = REFRESH_MILLISECONDS,
) {
    sealed interface Mode {
        data object Live : Mode

        /** Replaying a flight at this many times real speed. */
        data class Replay(val speed: Double) : Mode
    }

    /** What is running, if anything. */
    var mode: Mode? by mutableStateOf(null)
        private set

    /** What the cockpit shows. Present while a session runs. */
    var view: CockpitView? by mutableStateOf(null)
        private set

    /**
     * The way to the place the cockpit navigates to: the one the pilot
     * chose, or the take-off. Absent until there is one to go to.
     */
    var way: WayToTarget? by mutableStateOf(null)
        private set

    /** How the flight scores as a cross-country flight so far. */
    var score: LiveScore? by mutableStateOf(null)
        private set

    /**
     * Where to fly for an FAI triangle, once the flight has covered a few
     * kilometres in a straight line.
     */
    var faiAreas: FaiAreas? by mutableStateOf(null)
        private set

    /** The airspace warnings of the moment, the most urgent first. */
    var airspaceWarnings: List<AirspaceWarning> by mutableStateOf(emptyList())
        private set

    /**
     * Whether the glider is being checked against airspace right now.
     * Anything but `CHECKING` means that silence is not "no airspace near",
     * and the cockpit says which it is.
     */
    var airspaceCheck: AirspaceCheck by mutableStateOf(AirspaceCheck.NO_AIRSPACE)
        private set

    /**
     * The airspace to warn about, asked for while a session runs. The app
     * sets this to the pilot's airspace files.
     */
    var airspace: () -> AirspaceSet? = { null }

    /** The airspace last handed to the core's session. */
    private var handedAirspace: AirspaceSet? = null

    /** Something the pilot must be told, such as location being denied. */
    var problem: String? by mutableStateOf(null)

    private val muted = mutableStateOf(preferences.varioMuted)

    /** Whether the vario is silent. Remembered between flights. */
    var isMuted: Boolean
        get() = muted.value
        set(value) {
            muted.value = value
            sound.isMuted = value
            preferences.varioMuted = value
        }

    private var session: FlightSession? = null
    private var replay: Replay? = null
    private var ticker: Job? = null

    /** Refreshes of the screen since the score was last asked for. */
    private var refreshesSinceScore = 0

    /** "I have seen this warning": quiet for ten minutes, unless it becomes more urgent. */
    @Synchronized
    fun acknowledgeAirspace(id: String) {
        session?.acknowledgeAirspace(id, minutes = 10u)
        airspaceWarnings = session?.airspaceWarnings().orEmpty()
    }

    /** "This airspace does not apply to me today": quiet until midnight. */
    @Synchronized
    fun ignoreAirspaceToday(id: String) {
        val utcOffsetSeconds = TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 1000
        session?.ignoreAirspaceToday(id, utcOffsetSeconds)
        airspaceWarnings = session?.airspaceWarnings().orEmpty()
    }

    /** Takes either of them back. */
    @Synchronized
    fun unsilenceAirspace(id: String) {
        session?.unsilenceAirspace(id)
        airspaceWarnings = session?.airspaceWarnings().orEmpty()
    }

    /** Chooses the place to navigate to; `null` goes back to the take-off. */
    @Synchronized
    fun goTo(place: TargetPlace?) {
        val session = session ?: return
        session.goTo(place)
        refresh(elapsed = 0.0)
    }

    /** Switches the sensors on and starts recording. */
    @Synchronized
    fun startFlight() {
        if (mode != null) return
        val session = FlightSession(dataFolder.path)
        try {
            session.startRecording(
                pilot = preferences.pilot.ifBlank { null },
                glider = preferences.glider.ifBlank { null },
            )
        } catch (error: Exception) {
            problem = error.pilotMessage
            return
        }
        sensors.start(session) { problem = it }
        begin(session, Mode.Live)
    }

    /** Plays a recorded flight through the cockpit, to try it on the ground. */
    @Synchronized
    fun startReplay(igcText: String, speed: Double) {
        if (mode != null) return
        replay = try {
            Replay(igcText)
        } catch (error: Exception) {
            problem = error.pilotMessage
            return
        }
        begin(FlightSession(dataFolder.path), Mode.Replay(speed))
    }

    /**
     * Ends the session. For a live flight, returns the recording to put
     * into the logbook, if any position was recorded.
     */
    @Synchronized
    fun finish(): PendingRecording? {
        val session = session ?: return null
        ticker?.cancel()
        ticker = null
        if (mode == Mode.Live) {
            sensors.stop()
        }
        sound.stop()
        voice.stop()

        val recording = try {
            session.stopRecording()
        } catch (error: Exception) {
            // The file is still in the data folder and is picked up at the
            // next launch.
            problem = error.pilotMessage
            null
        }
        this.session = null
        replay = null
        mode = null
        view = null
        way = null
        score = null
        faiAreas = null
        airspaceWarnings = emptyList()
        airspaceCheck = AirspaceCheck.NO_AIRSPACE
        handedAirspace = null
        return recording
    }

    private fun begin(session: FlightSession, mode: Mode) {
        this.session = session
        this.mode = mode
        // How the pilot's glider glides, for predicting arrival heights.
        session.setGlider(trimSpeedMs = preferences.trimSpeed / 3.6, glideRatio = preferences.glideRatio)
        handedAirspace = airspace()
        session.useAirspace(handedAirspace)
        refreshesSinceScore = 0
        sound.isMuted = isMuted
        sound.start()
        refresh(elapsed = 0.0)

        ticker = scope.launch {
            var last = System.nanoTime()
            while (isActive) {
                delay(refreshMilliseconds)
                val now = System.nanoTime()
                refresh(elapsed = (now - last) / 1e9)
                last = now
            }
        }
    }

    @Synchronized
    private fun refresh(elapsed: Double) {
        val session = session ?: return
        val replay = replay
        val mode = mode
        val now = if (replay != null && mode is Mode.Replay) {
            replay.advance(session, elapsed * mode.speed)
            replay.now()
        } else {
            System.currentTimeMillis() / 1000.0
        }
        val view = session.view(now)
        this.view = view
        way = session.navigation(now)
        // The core works the score out anew every half minute of flight;
        // asking once a second is often enough to show it promptly.
        refreshesSinceScore += 1
        if (refreshesSinceScore * refreshMilliseconds >= SCORE_MILLISECONDS) {
            refreshesSinceScore = 0
            val latest = session.liveScore()
            if (latest != score) {
                score = latest
            }
            // Cheap, and it changes with the score: asked with it.
            val areas = session.faiAreas()
            if (areas != faiAreas) {
                faiAreas = areas
            }
        }
        // Files brought in or removed in the middle of a session reach the
        // core at once; what the pilot silenced stays silenced.
        val latest = airspace()
        if (latest !== handedAirspace) {
            handedAirspace = latest
            session.useAirspace(latest)
        }
        val check = session.airspaceCheck(now)
        if (check != airspaceCheck) {
            airspaceCheck = check
        }
        val warnings = session.airspaceWarnings()
        if (warnings != airspaceWarnings) {
            airspaceWarnings = warnings
        }
        // A warning that is new, or more urgent than it was, is said aloud once.
        if (session.takeAirspaceAlert() != null) {
            warnings.firstOrNull { !it.silenced }?.let { loudest -> voice.say("Airspace. ${loudest.headline}.") }
        }
        sound.play(view.tone)
    }

    companion object {
        /** How often the screen and the vario tone are refreshed. */
        const val REFRESH_MILLISECONDS = 200L

        /**
         * The refresh of a UI test on an emulator. Its software renderer
         * takes most of a second over a frame of the cockpit, more with a
         * sheet open over the map. Refreshed five times a second, the
         * screen then never comes to rest, and the test's look at it,
         * which waits for that, gives up after a minute. Every two seconds
         * leaves the rest it needs.
         */
        const val SLOW_REFRESH_MILLISECONDS = 2_000L

        /** How long passes between two looks at the score. */
        private const val SCORE_MILLISECONDS = 1_000L
    }
}
