package fly.venti.app.fly

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.HttpAnswer
import fly.venti.app.pilotMessage
import fly.venti.app.settings.Preferences
import fly.venti.app.waypoints.place
import fly.venti.core.AirspaceCheck
import fly.venti.core.AirspaceSet
import fly.venti.core.AirspaceWarning
import fly.venti.core.CockpitView
import fly.venti.core.FlightSession
import fly.venti.core.LandingOption
import fly.venti.core.LiveAudience
import fly.venti.core.LiveScore
import fly.venti.core.LiveShareState
import fly.venti.core.LiveShareView
import fly.venti.core.PendingRecording
import fly.venti.core.ReachView
import fly.venti.core.FaiAreas
import fly.venti.core.Replay
import fly.venti.core.ServerRequest
import fly.venti.core.TargetPlace
import fly.venti.core.TaskFile
import fly.venti.core.TaskStatusView
import fly.venti.core.ToneView
import fly.venti.core.WayToTarget
import fly.venti.core.WaypointView
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.io.File
import java.security.SecureRandom
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
 *
 * A live flight the pilot shares is sent from here as well: this object
 * lives as long as the app's process, which the recording's service keeps
 * alive with the screen off, so the positions go out wherever the pilot
 * looks. What is due, and when, is the core's to say.
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
    /** The phone's clock, in seconds since 1970. */
    private val clock: () -> Double = { System.currentTimeMillis() / 1000.0 },
    /**
     * So many bytes from the system's source of random numbers for secrets.
     * The link of a shared flight is made of them, and whoever has the link
     * sees the flight.
     */
    private val random: (Int) -> ByteArray = ::randomBytes,
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

    /**
     * The task to fly, asked for when a session starts. The app sets this
     * to the task the pilot keeps.
     */
    var taskFile: () -> TaskFile? = { null }

    /** Where the flight stands in its task. Absent without a task. */
    var task: TaskStatusView? by mutableStateOf(null)
        private set

    /**
     * The place the pilot chose to fly to, by its name. Absent while the
     * cockpit leads along the task, or back to the take-off.
     */
    var chosenPlace: String? by mutableStateOf(null)
        private set

    /**
     * The landing to think of first, among the pilot's own: the one the
     * glider would arrive highest above. Absent on the ground, and for a
     * pilot who keeps no landings with a height.
     */
    var landing: LandingOption? by mutableStateOf(null)
        private set

    /**
     * The pilot's landings, asked for every second or two in the air. The
     * app sets this to the waypoints that are places to land.
     */
    var landings: () -> List<WaypointView> = { emptyList() }

    /**
     * How the flight is shared with others while it is under way. Absent
     * while it is not.
     */
    var live: LiveShareView? by mutableStateOf(null)
        private set

    /**
     * The address of the pilot's Venti server, if this phone is signed in
     * to one. The app sets this.
     */
    var server: () -> String? = { null }

    /**
     * Carries a request to that server and brings back its answer, or
     * `null` if none came. The app sets this.
     */
    var carry: suspend (ServerRequest) -> HttpAnswer? = { null }

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

    /** When the core was last asked whether a request for the shared flight is due. */
    private var askedForLiveRequest = 0.0

    /** The request for the shared flight that is on its way to the server, if one is. */
    private var sending: Job? = null

    /** Whether the flight in the air is going out to others. */
    val isSharingLive: Boolean get() = live?.let { it.state != LiveShareState.STOPPED } ?: false

    /**
     * Shows the flight in the air to these people, or to others than
     * before. Does nothing without a flight, in a replay, without an
     * account, and for nobody: taking a flight away from everyone is
     * [deleteLiveTrack].
     */
    @Synchronized
    fun share(audience: LiveAudience?) {
        val session = session ?: return
        if (mode != Mode.Live || audience == null) return
        if (isSharingLive) {
            session.setLiveAudience(audience)
        } else {
            val server = server() ?: return
            try {
                session.shareLive(server, random(LINK_BYTES), audience)
            } catch (error: Exception) {
                problem = error.pilotMessage
            }
        }
        live = session.liveShare(clock())
    }

    /**
     * Takes the flight away from everyone who could watch it: the track is
     * removed from the server and nothing more is sent.
     */
    @Synchronized
    fun deleteLiveTrack() {
        val session = session ?: return
        val request = session.deleteLive() ?: return
        live = session.liveShare(clock())
        scope.launch {
            // Twice, in case the network was away the first time. The
            // server removes the track by itself after two days.
            if (carry(request) == null) {
                delay(DELETE_AGAIN_MILLISECONDS)
                carry(request)
            }
        }
    }

    /**
     * Sends what the core says is due for the shared flight, and hands the
     * answer back. The core hands out nothing further until it has an
     * answer, or word that none came.
     */
    private fun sendLive(session: FlightSession, now: Double) {
        // Once a second is as often as the core asks to be asked.
        if (now - askedForLiveRequest < 1.0 && now >= askedForLiveRequest) return
        askedForLiveRequest = now
        val request = session.liveRequest(now) ?: return
        sending = scope.launch { hand(session, request) }
    }

    private suspend fun hand(session: FlightSession, request: ServerRequest) {
        val answer = carry(request)
        if (answer != null) {
            val status = answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()
            session.liveAnswer(status, answer.body, answer.retryAfterSeconds, clock())
        } else {
            session.liveNoAnswer(clock())
        }
    }

    /**
     * Tells the server that the flight is over, with what is still waiting,
     * after the cockpit has closed: for a minute and a half at most, then
     * the track simply goes quiet.
     */
    private fun sayLanded(session: FlightSession) {
        if (session.liveShare(clock()) == null) return
        session.endLive()
        val onItsWay = sending
        scope.launch {
            // A request that is on its way is answered first: until then
            // the core hands out nothing, the landing included.
            onItsWay?.join()
            val until = clock() + LANDED_WITHIN_SECONDS
            while (clock() < until) {
                val now = clock()
                val request = session.liveRequest(now)
                if (request != null) {
                    hand(session, request)
                    continue
                }
                // Asked to wait, or positions from a gap in the network are
                // still to go out, a batch every quarter of a minute.
                val share = session.liveShare(now) ?: break
                val isWaiting = share.state == LiveShareState.WAITING ||
                    (share.state == LiveShareState.SHARING && share.waiting > 0u)
                if (!isWaiting) break
                delay(LANDED_AGAIN_MILLISECONDS)
            }
        }
    }

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

    /**
     * Chooses the place to navigate to; `null` goes back to the task, or
     * without one to the take-off.
     */
    @Synchronized
    fun goTo(place: TargetPlace?) {
        val session = session ?: return
        session.goTo(place)
        chosenPlace = place?.name
        refresh(elapsed = 0.0)
    }

    /**
     * How each of some places lies within the glider's reach now, in their
     * order: for the list of places to fly to. Empty without a position.
     */
    @Synchronized
    fun reach(places: List<WaypointView>): List<ReachView> {
        val session = session ?: return emptyList()
        return session.reach(places.map { it.place }, replay?.now() ?: clock())
    }

    /** Moves the task on to its next turnpoint, or back to the one before. */
    @Synchronized
    fun stepTask(forward: Boolean) {
        val session = session ?: return
        session.stepTask(forward)
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
        // Shown to others from take-off, if the pilot chose so.
        share(LiveChoice.stored(preferences).audience)
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
        if (mode == Mode.Live) {
            sayLanded(session)
        }
        this.session = null
        replay = null
        mode = null
        live = null
        landing = null
        view = null
        way = null
        score = null
        faiAreas = null
        airspaceWarnings = emptyList()
        airspaceCheck = AirspaceCheck.NO_AIRSPACE
        handedAirspace = null
        task = null
        chosenPlace = null
        return recording
    }

    private fun begin(session: FlightSession, mode: Mode) {
        this.session = session
        this.mode = mode
        // How the pilot's glider glides, for predicting arrival heights.
        session.setGlider(trimSpeedMs = preferences.trimSpeed / 3.6, glideRatio = preferences.glideRatio)
        handedAirspace = airspace()
        session.useAirspace(handedAirspace)
        // The task of the day, on the day of the flight: a replay's day is
        // the day it was flown.
        try {
            session.flyTask(taskFile()?.text, replay?.now() ?: clock())
        } catch (error: Exception) {
            problem = error.pilotMessage
        }
        refreshesSinceScore = 0
        askedForLiveRequest = 0.0
        sending = null
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
            replay.advance(session, replayStep(elapsed, mode.speed))
            replay.now()
        } else {
            clock()
        }
        val view = session.view(now)
        this.view = view
        way = session.navigation(now)
        val status = session.taskStatus(now)
        if (status != task) {
            task = status
        }
        // A flight shared with others: what is due goes out, and the
        // cockpit shows how it stands.
        if (mode == Mode.Live) {
            sendLive(session, now)
            val shared = session.liveShare(now)
            if (shared != live) {
                live = shared
            }
        }
        // What happens in the task is said once: the start taken, a
        // turnpoint reached, the goal.
        while (true) {
            val event = session.takeTaskEvent() ?: break
            voice.say(event.words)
        }
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
            // Which of the pilot's landings the glider could go to now.
            val option = session.landingOption(landings().map { it.place }, now)
            if (option != landing) {
                landing = option
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

        /**
         * The longest stretch of the clock a replay moves on by at once,
         * in seconds. When the phone was busy with something else for
         * longer, the replay carries on from where it was instead of
         * leaping over minutes of the flight that nobody saw.
         */
        const val REPLAY_STEP_AT_MOST_SECONDS = 2.0

        /**
         * How many seconds of the flight a replay at this many times its
         * speed moves on by, when the last refresh was `elapsed` seconds
         * ago.
         */
        fun replayStep(elapsed: Double, speed: Double): Double = minOf(elapsed, REPLAY_STEP_AT_MOST_SECONDS) * speed

        /** How long passes between two looks at the score. */
        private const val SCORE_MILLISECONDS = 1_000L

        /** The core makes a shared flight's link from this many random bytes. */
        private const val LINK_BYTES = 26

        /** So many bytes from the system's source of random numbers for secrets. */
        fun randomBytes(size: Int): ByteArray = ByteArray(size).also(SecureRandom()::nextBytes)

        /** How long after a deletion that got no answer it is sent once more. */
        private const val DELETE_AGAIN_MILLISECONDS = 10_000L

        /** How long the server is still told of the landing after the pilot ended the flight, seconds. */
        const val LANDED_WITHIN_SECONDS = 90.0

        /** How long to wait before asking the core again while the landing waits to be sent. */
        private const val LANDED_AGAIN_MILLISECONDS = 2_000L
    }
}
