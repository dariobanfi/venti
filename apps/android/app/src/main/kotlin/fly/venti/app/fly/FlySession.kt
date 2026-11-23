package fly.venti.app.fly

import fly.venti.core.DaylightLeft
import fly.venti.core.SessionOver
import fly.venti.core.LiveGlider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.HttpAnswer
import fly.venti.app.pilotMessage
import fly.venti.app.settings.GliderWords
import fly.venti.app.settings.Preferences
import fly.venti.app.waypoints.place
import fly.venti.core.GroundWind
import fly.venti.core.GroundWindAlert
import fly.venti.core.GroundWindPlace
import fly.venti.core.WindMap
import fly.venti.core.Credit
import fly.venti.core.FlightPhase
import fly.venti.core.groundWind
import fly.venti.core.AirspaceAhead
import fly.venti.core.AirspaceCheck
import fly.venti.core.AirspaceSet
import fly.venti.core.AirspaceWarning
import fly.venti.core.CableSet
import fly.venti.core.CableWarning
import fly.venti.core.CockpitTakeoffs
import fly.venti.core.OthersNear
import fly.venti.core.ClimbsNear
import fly.venti.core.cockpitClimbsPath
import fly.venti.core.cockpitOthersPath
import fly.venti.core.liveClimbs
import fly.venti.core.liveOthers
import fly.venti.core.CockpitView
import fly.venti.core.Coordinate
import fly.venti.core.ForecastView
import fly.venti.core.FlightSession
import fly.venti.core.GlideRange
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
import fly.venti.core.Terrain
import fly.venti.core.ToneView
import fly.venti.core.WayToTarget
import fly.venti.core.WaypointView
import kotlinx.coroutines.CancellationException
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

    /**
     * Keeps the app alive and awake with the screen off, for as long as the
     * sensors run: from the moment the cockpit is on screen. A watch for
     * the take-off from another tab does without it until the take-off.
     */
    fun keepAlive() = Unit

    /**
     * Whether the sensors may be switched on without asking the pilot
     * anything: the position is allowed. Only then is the take-off watched
     * for from another tab.
     */
    val mayWatch: Boolean get() = false
}

/** Whatever says a warning aloud. */
interface Announcer {
    /** Says this at once, in place of whatever is being said. */
    fun say(text: String)

    /** Says this once what is being said is over: after a warning about airspace. */
    fun sayAfter(text: String) = say(text)

    fun stop()
}

/** Says nothing. Tests have nobody listening. */
class SilentAnnouncer : Announcer {
    override fun say(text: String) = Unit

    override fun stop() = Unit
}

/**
 * Whatever makes the vario's sound. It has no mute: the core says when the
 * phone sounds (`CockpitView.phoneTone`), silent on the ground, after the
 * landing and while the pilot's own vario beeps (the owner, 2026-11-22).
 */
interface TonePlayer {
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

    /**
     * Whether the session runs unseen: the app is in front on another tab,
     * and the sensors feed the core's session only so that a take-off is
     * noticed. Nothing is shown, sounded, sent or asked for while it is;
     * the core's take-off makes it the cockpit's session, recording.
     */
    var isWatching: Boolean by mutableStateOf(false)
        private set

    /** Whether the cockpit is open: a session runs and is not a watch. */
    val isOpen: Boolean get() = mode != null && !isWatching

    /**
     * Counts the take-offs noticed while the session was a watch: the app
     * then brings the cockpit to the front. Read by the screen.
     */
    var takeoffsSeenUnseen: Int by mutableIntStateOf(0)
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

    /** How much of the day is left where the glider is. */
    var daylight: DaylightLeft? by mutableStateOf(null)
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
     * The cables to warn about, asked for while a session runs. The app
     * sets this to the set fetched for the flight's area.
     */
    var cables: () -> CableSet? = { null }

    /** The cables last handed to the core's session. */
    private var handedCables: CableSet? = null

    /** The cable ahead, by the core's rule; absent with none ahead, which is no promise. */
    var cableWarning: CableWarning? by mutableStateOf(null)
        private set

    /**
     * Fetches a path of Venti's server and returns its answer, or `null`
     * without a server or an answer. The app sets this.
     */
    var fetch: suspend (String) -> HttpAnswer? = { null }

    /** The aircraft other networks hear around the glider, as the core keeps them, with their tracks. */
    var othersNear: OthersNear? by mutableStateOf(null)
        private set

    /** When the other networks were last asked, by the phone's clock, and whether an answer is awaited. */
    private var askedForNetworks = Double.NEGATIVE_INFINITY
    private var isAskingForNetworks = false

    /**
     * Whether the pilot has "Climbs of others in the cockpit" switched on.
     * The app sets this. While it is off nothing is asked and nothing drawn.
     */
    var wantsClimbs: () -> Boolean = { false }

    /**
     * Where others circle and climb near the glider, as the core keeps and
     * ages them; `null` while the setting is off or nothing was heard. A
     * hint, not an instrument.
     */
    var climbsNear: ClimbsNear? by mutableStateOf(null)
        private set

    /** When the climbs were last asked for, by the phone's clock, and whether an answer is awaited. */
    private var askedForClimbs = Double.NEGATIVE_INFINITY
    private var isAskingForClimbs = false

    /**
     * The take-offs around the glider for the cockpit's map, as the core
     * gives them from what the app holds. The app sets this.
     */
    var takeoffsAround: suspend (Coordinate) -> CockpitTakeoffs? = { null }

    /** The take-offs around the glider, nearest first. */
    var takeoffs: CockpitTakeoffs? by mutableStateOf(null)
        private set

    /**
     * The landings for the core's standing reading of the nearest one
     * within glide, as the core itself picks them (`Core.cockpitLandings`)
     * from the sites the app holds around the glider, the pilot's own and
     * the pilot's landing waypoints. The app sets this.
     */
    var landingsAround: suspend (Coordinate) -> List<TargetPlace> = { emptyList() }

    /** The landings last handed to the core's session. */
    private var handedLandings: List<TargetPlace>? = null

    /** When the take-offs were last asked for, by the phone's clock, and whether an answer is awaited. */
    private var askedForTakeoffs = Double.NEGATIVE_INFINITY
    private var isAskingForTakeoffs = false

    /** "I have seen this cable": quiet for ten minutes, as an airspace warning's "Seen". */
    @Synchronized
    fun acknowledgeCable(id: String) {
        session?.acknowledgeCable(id, minutes = 10u)
        cableWarning = session?.cableWarning()
    }

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
     * How far the glider glides over the ground as it is, every way, for a
     * line on the map. Absent on the ground and where the phone has no
     * ground under the glider.
     */
    var range: GlideRange? by mutableStateOf(null)
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

    /**
     * The wind on the ground around the glider and where the pilot means
     * to land, as the wind stations last measured it. Absent without a
     * server, without a network, and on the ground.
     */
    var windBelow: GroundWind? by mutableStateOf(null)
        private set

    /** Whose data those stations are: the licences ask for the credit beside the data. */
    var windBelowCredits: List<Credit> by mutableStateOf(emptyList())
        private set

    /** Whether the pilot has put the warning of a strong wind at the landing away. */
    private var isWindWarningPutAway by mutableStateOf(false)

    /**
     * Asks Venti for the wind stations around a place and around a second
     * one. The app sets this; `null` when there is no answer.
     */
    var windOnTheGround: suspend (Coordinate, Coordinate?) -> WindMap? = { _, _ -> null }

    /**
     * Asks Venti for the forecast of a place, which the core's thermal view
     * takes its wind from before the flight has one and no station speaks
     * for the glider's air. The app sets this; `null` when there is none.
     */
    var forecastAt: suspend (Coordinate) -> ForecastView? = { null }

    /** When the wind on the ground was last asked for, by the phone's clock, and whether an answer is awaited. */
    private var askedForWindBelow = Double.NEGATIVE_INFINITY
    private var isAskingForWindBelow = false

    /** When and where the forecast was last asked for, and whether an answer is awaited. */
    private var askedForForecast = Double.NEGATIVE_INFINITY
    private var forecastAskedAt: Coordinate? = null
    private var isAskingForForecast = false

    /** The station whose strong wind was last said aloud. */
    private var warnedOfWindAt: String? = null

    /** The warning of a strong wind where the pilot means to land, unless the pilot has put it away. */
    val windWarning: GroundWindAlert?
        get() = if (isWindWarningPutAway) null else windBelow?.alert

    /** Puts the warning away. It comes back for another station, and for this one after its wind has dropped and risen again. */
    fun putWindWarningAway() {
        isWindWarningPutAway = true
    }

    /**
     * Other pilots in the air around the glider who show their flight to
     * everyone, as Venti last listed them.
     */
    var gliders: List<LiveGlider> by mutableStateOf(emptyList())
        private set

    /** Asks Venti who is in the air around a place. The app sets this; `null` when there is no answer. */
    var others: suspend (Coordinate) -> List<LiveGlider>? = { null }

    /** When the other gliders were last asked for, by the phone's clock, and whether an answer is awaited. */
    private var askedForOthers = Double.NEGATIVE_INFINITY
    private var isAskingForOthers = false

    /** Something the pilot must be told, such as location being denied. */
    var problem: String? by mutableStateOf(null)

    /**
     * Takes the recording of a flight that ended by itself, to put it into
     * the logbook as one the pilot ended. The app sets this. Without it the
     * file waits in the data folder and is found at the next launch. It is
     * called from the refresh, which runs with the screen off too.
     */
    var savedByItself: (PendingRecording) -> Unit = {}

    /**
     * The logbook's identifier of a flight that ended by itself and was
     * saved, for the screen to show once it is there. The screen clears it.
     */
    var savedFlight: String? by mutableStateOf(null)

    /**
     * Words to show quietly for a while, then put away by the screen: a
     * flight taken back. Kept after the cockpit closed, so that the tab it
     * returns to says them.
     */
    var quietNotice: String? by mutableStateOf(null)

    /** Puts the quiet notice away. */
    fun putQuietNoticeAway() {
        quietNotice = null
    }

    /**
     * Counts the cockpits closed because the take-off that opened them was
     * taken back: the app then returns to the tab the pilot was on.
     */
    var closedByFalseStart: Int by mutableIntStateOf(0)
        private set

    /** Whether the cockpit on screen was opened by the take-off watch, not by the pilot. */
    private var openedByTakeoff = false

    private var session: FlightSession? = null
    private var replay: Replay? = null

    /** Where the replay began, seconds since 1970: the moment of its first fix. */
    private var replayBegan = 0.0

    /** Seconds of the replay to let pass at the next refresh besides its own step: [replayTo]. */
    private var replayLeap = 0.0
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

    /**
     * Switches the sensors on and makes the session ready: it waits on the
     * ground, writes nothing, and records from the take-off. When the flight
     * is over the core says so, and the session ends by itself.
     */
    @Synchronized
    fun startFlight() {
        // A watch that runs already is the cockpit's session from now on:
        // what it has kept of the last half minute stays.
        if (isWatching) {
            comeIntoView()
            refresh(elapsed = 0.0)
            return
        }
        // The pilot opened it, whatever opened it before.
        openedByTakeoff = false
        if (mode != null) return
        val session = readySession() ?: return
        sensors.start(session) { problem = it }
        sensors.keepAlive()
        begin(session, Mode.Live)
        // Shown to others from take-off, if the pilot chose so.
        share(LiveChoice.stored(preferences).audience)
    }

    /**
     * Watches for a take-off while the app is in front on another tab: the
     * same session as the cockpit's, made ready, unseen. Nothing happens
     * without the position allowed (nothing is asked from here), while a
     * session runs, or while the core finds no take-off; once it finds
     * one, the session records and [takeoffsSeenUnseen] counts up.
     */
    @Synchronized
    fun watchForTakeoff() {
        if (mode != null || !sensors.mayWatch) return
        val session = readySession(quietly = true) ?: return
        isWatching = true
        // A problem with the position is the cockpit's to say, not a watch's.
        sensors.start(session) { if (!isWatching) problem = it }
        begin(session, Mode.Live)
    }

    /** Ends a watch that found no take-off: the app went to the back. Nothing was written. */
    @Synchronized
    fun stopWatching() {
        if (!isWatching) return
        finish()
    }

    /** A new session of the core, ready to record from the take-off, or `null` with the pilot told why. */
    private fun readySession(quietly: Boolean = false): FlightSession? {
        val session = FlightSession(dataFolder.path)
        return try {
            session.recordFromTakeoff(
                pilot = preferences.pilot.ifBlank { null },
                glider = preferences.glider.ifBlank { null },
                nowUnixS = clock(),
            )
            session
        } catch (error: Exception) {
            if (!quietly) problem = error.pilotMessage
            null
        }
    }

    /**
     * The watch becomes the cockpit's session: the app is kept alive with
     * the screen off, the vario sounds, and the flight is shown to others
     * from the take-off if the pilot chose so.
     */
    private fun comeIntoView() {
        isWatching = false
        sensors.keepAlive()
        sound.start()
        share(LiveChoice.stored(preferences).audience)
    }

    /**
     * Records from now, for a pilot who would rather not wait for the
     * take-off to be noticed. The flight file begins with the half minute
     * the session has kept.
     */
    @Synchronized
    fun recordNow() {
        val session = session ?: return
        if (mode != Mode.Live) return
        try {
            session.startRecording(
                pilot = preferences.pilot.ifBlank { null },
                glider = preferences.glider.ifBlank { null },
            )
        } catch (error: Exception) {
            problem = error.pilotMessage
        }
        refresh(elapsed = 0.0)
    }

    /**
     * Ends a flight that is over by itself: landed and saved as if the pilot
     * had ended it, or given up on because nobody took off.
     */
    private fun endByItself(over: SessionOver) {
        val recording = finish()
        when (over) {
            SessionOver.LANDED -> recording?.let(savedByItself)
            SessionOver.NO_TAKEOFF -> problem = NO_TAKEOFF
        }
    }

    /**
     * Opens the cockpit again after Android ended the app while a cockpit
     * was open and then started the recording's service again by itself.
     * The session of before is gone with the process; what it wrote was
     * listed at this launch as an interrupted recording and goes into the
     * logbook as a flight of its own. This one waits for the take-off,
     * which in the air the core notices within seconds, and records from
     * there into a new file. The core joins no two files: a flight cut by
     * the system is two flights in the logbook (to do in the core).
     */
    @Synchronized
    fun reopenAfterRestart() {
        if (isOpen) return
        startFlight()
    }

    /** Plays a recorded flight through the cockpit, to try it on the ground. */
    @Synchronized
    fun startReplay(igcText: String, speed: Double) {
        // A watch, or a cockpit that waits on the ground with nothing
        // written, gives way to the replay.
        if (isWatching || (mode == Mode.Live && view?.recording == false)) finish()
        if (mode != null) return
        replay = try {
            Replay(igcText)
        } catch (error: Exception) {
            problem = error.pilotMessage
            return
        }
        replayBegan = replay?.now() ?: 0.0
        replayLeap = 0.0
        begin(FlightSession(dataFolder.path), Mode.Replay(speed))
    }

    /**
     * For a UI test: the replay leaps on to so many seconds after its first
     * fix at the next refresh, every fix on the way handed to the core as
     * a replay hands them, so that the cockpit then shows what it shows at
     * that moment of the flight without the test waiting for it. Nothing
     * happens where the replay is there already. The app's screens never
     * call it.
     */
    @Synchronized
    fun replayTo(secondsIntoFlight: Double) {
        val replay = replay ?: return
        replayLeap = maxOf(0.0, replayBegan + secondsIntoFlight - replay.now())
    }

    /**
     * Changes how fast a replay runs: 1 is the flight's own speed, and 0
     * holds it where it is. Does nothing while a flight is recorded.
     */
    @Synchronized
    fun setReplaySpeed(speed: Double) {
        if (mode !is Mode.Replay || !speed.isFinite() || speed < 0) return
        mode = Mode.Replay(speed)
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
        isWatching = false
        openedByTakeoff = false
        live = null
        landing = null
        range = null
        view = null
        way = null
        score = null
        faiAreas = null
        daylight = null
        airspaceWarnings = emptyList()
        airspaceCheck = AirspaceCheck.NO_AIRSPACE
        handedAirspace = null
        handedCables = null
        cableWarning = null
        othersNear = null
        isAskingForNetworks = false
        climbsNear = null
        isAskingForClimbs = false
        askedForClimbs = Double.NEGATIVE_INFINITY
        takeoffs = null
        isAskingForTakeoffs = false
        task = null
        chosenPlace = null
        gliders = emptyList()
        isAskingForOthers = false
        windBelow = null
        windBelowCredits = emptyList()
        isWindWarningPutAway = false
        warnedOfWindAt = null
        isAskingForWindBelow = false
        return recording
    }

    private fun begin(session: FlightSession, mode: Mode) {
        this.session = session
        this.mode = mode
        // How the pilot's glider glides, for predicting arrival heights.
        session.setGlider(trimSpeedMs = preferences.trimSpeed / 3.6, glideRatio = preferences.glideRatio)
        // Only for a pilot who set a class: the air's own rise and fall
        // and a speed to fly. And hike and fly, which never lands.
        session.setGliderClass(GliderWords.kind(preferences.gliderClass))
        session.setNeverLand(preferences.neverLand)
        handedLandings = null
        handedAirspace = airspace()
        session.useAirspace(handedAirspace)
        handedCables = cables()
        session.useCables(handedCables)
        // The task of the day, on the day of the flight: a replay's day is
        // the day it was flown.
        try {
            session.flyTask(taskFile()?.text, replay?.now() ?: clock())
        } catch (error: Exception) {
            problem = error.pilotMessage
        }
        // The ground, as far as the phone has it; more of it arrives
        // while the session runs. The first refresh asks for it.
        session.useTerrain(terrain())
        refreshesSinceTerrain = (TERRAIN_MILLISECONDS / refreshMilliseconds).toInt()
        refreshesSinceScore = 0
        // The wind on the ground is asked for as soon as the glider flies.
        askedForWindBelow = Double.NEGATIVE_INFINITY
        isAskingForWindBelow = false
        askedForForecast = Double.NEGATIVE_INFINITY
        forecastAskedAt = null
        isAskingForForecast = false
        askedForOthers = Double.NEGATIVE_INFINITY
        isAskingForOthers = false
        askedForNetworks = Double.NEGATIVE_INFINITY
        isAskingForNetworks = false
        askedForTakeoffs = Double.NEGATIVE_INFINITY
        isAskingForTakeoffs = false
        askedForLiveRequest = 0.0
        sending = null
        // A watch makes no sound: it starts once the cockpit has the session.
        if (!isWatching) {
            sound.start()
        }
        refresh(elapsed = 0.0)

        ticker = scope.launch {
            var last = System.nanoTime()
            while (isActive) {
                // A watch looks once a second: the core keeps every fix in
                // any case, and the screen shows nothing of it.
                delay(if (isWatching) maxOf(refreshMilliseconds, WATCH_MILLISECONDS) else refreshMilliseconds)
                val now = System.nanoTime()
                refresh(elapsed = (now - last) / 1e9)
                last = now
            }
        }
    }

    /**
     * What lies on the line ahead, seen from the side: on the glider's
     * course on a glide, towards the place the cockpit navigates to while
     * circling. `null` without a position and a height, and with neither
     * airspace nor ground to show.
     */
    @Synchronized
    fun airspaceAhead(): AirspaceAhead? = session?.airspaceAhead(replay?.now() ?: clock())

    /**
     * The ground, as far as the phone has it. Without it the cockpit knows
     * no height above ground.
     */
    var terrain: () -> Terrain? = { null }

    /** Asks for the ground around a place to be brought, where it is not there yet. */
    var wantTerrain: (Coordinate) -> Unit = {}

    /** Refreshes of the screen since the ground was last asked for. */
    private var refreshesSinceTerrain = 0

    @Synchronized
    private fun refresh(elapsed: Double) {
        val session = session ?: return
        val replay = replay
        val mode = mode
        val now = if (replay != null && mode is Mode.Replay) {
            replay.advance(session, replayStep(elapsed, mode.speed) + replayLeap)
            replayLeap = 0.0
            replay.now()
        } else {
            clock()
        }
        val view = session.view(now)
        // A take-off taken back: what the rule took for one was a lift or a
        // drive. The core has removed what it wrote. Said quietly; a
        // cockpit the take-off opened closes again, and the app returns to
        // where the pilot was. A watch has shown nothing and says nothing.
        val falseStart = session.takeFalseStart()
        if (falseStart != null && !isWatching) {
            quietNotice = falseStart.words
            if (openedByTakeoff && mode == Mode.Live) {
                finish()
                closedByFalseStart += 1
                return
            }
        }
        if (isWatching) {
            when {
                // Two hours on another tab without a take-off: the watch
                // begins anew, with nothing said.
                view.over != null -> {
                    finish()
                    watchForTakeoff()
                    return
                }
                // The core has seen the take-off and records: the cockpit
                // takes the session, and the app brings it to the front.
                view.recording -> {
                    comeIntoView()
                    openedByTakeoff = true
                    takeoffsSeenUnseen += 1
                }
                // Unseen, nothing else is worked out or asked for.
                else -> return
            }
        }
        // A flight that records itself also ends by itself: some minutes
        // after the landing, or when nobody took off.
        val over = view.over
        if (over != null && mode == Mode.Live) {
            endByItself(over)
            return
        }
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
            val light = session.daylight(now)
            if (light != daylight) {
                daylight = light
            }
            // Which of the pilot's landings the glider could go to now.
            val option = session.landingOption(landings().map { it.place }, now)
            if (option != landing) {
                landing = option
            }
            // And how far it glides, every way, over the ground as it is.
            val reach = session.glideRange(now)
            if (reach != range) {
                range = reach
            }
        }
        // The ground around the glider: what is not there yet is brought.
        refreshesSinceTerrain += 1
        val position = view.position
        if (position != null && refreshesSinceTerrain * refreshMilliseconds >= TERRAIN_MILLISECONDS) {
            refreshesSinceTerrain = 0
            wantTerrain(position)
        }
        // The wind on the ground: around the glider, and where the pilot
        // means to land. In flight, and in a cockpit that waits at the
        // take-off, where the thermal view shows it until the first circle
        // gives a wind of the flight's own. Every two minutes by the
        // phone's own clock, also in a replay: stations report every five
        // to ten.
        val wallClock = System.nanoTime() / 1e9
        val asksWindBelow = view.phase == FlightPhase.FLYING || mode == Mode.Live
        if (position != null && asksWindBelow && !isAskingForWindBelow &&
            wallClock - askedForWindBelow >= WIND_BELOW_SECONDS
        ) {
            askedForWindBelow = wallClock
            isAskingForWindBelow = true
            val place = placeToLand(view)
            val trimSpeedMs = preferences.trimSpeed / 3.6
            scope.launch {
                val found = windOnTheGround(position, place?.position)
                isAskingForWindBelow = false
                if (found == null || this@FlySession.session !== session) return@launch
                val below = groundWind(found.stations, position, place, trimSpeedMs)
                // The thermal view's wind is the core's: it takes a station's until the flight has one.
                session.useGroundWind(below.stations)
                take(below, found.credits)
            }
        }
        // The forecast for the glider's place, for the thermal view's wind
        // before the flight has one: when the cockpit opens, every half hour,
        // and when the glider has gone far from where it was asked for.
        val movedFar = forecastAskedAt?.let { asked -> position != null && distanceKm(asked, position) >= FORECAST_AGAIN_KM } ?: true
        if (position != null && !isAskingForForecast && (movedFar || wallClock - askedForForecast >= FORECAST_SECONDS)) {
            askedForForecast = wallClock
            forecastAskedAt = position
            isAskingForForecast = true
            scope.launch {
                val forecast = try {
                    forecastAt(position)
                } catch (error: CancellationException) {
                    throw error
                } catch (error: Exception) {
                    null
                }
                isAskingForForecast = false
                if (forecast == null || this@FlySession.session !== session) return@launch
                session.useForecast(forecast)
            }
        }
        // Who else is in the air around the glider, in flight only: on the
        // ground the map tab shows them. Every quarter of a minute by the
        // phone's own clock, which is what Venti asks for.
        if (position != null && view.phase == FlightPhase.FLYING && !isAskingForOthers &&
            wallClock - askedForOthers >= OTHERS_SECONDS
        ) {
            askedForOthers = wallClock
            isAskingForOthers = true
            // The pilot's own glider, if the flight is shown to everyone, is
            // on Venti's list too: it is not another pilot.
            val own = live?.link?.substringAfterLast('/')
            scope.launch {
                val found = others(position)
                isAskingForOthers = false
                if (found == null || this@FlySession.session !== session) return@launch
                val around = found.filter { it.id != own }
                if (around != gliders) gliders = around
            }
        }
        // The aircraft other networks hear, in flight only, every quarter
        // of a minute: the core keeps their tracks from the answers.
        if (position != null && view.phase == FlightPhase.FLYING && !isAskingForNetworks &&
            wallClock - askedForNetworks >= OTHERS_SECONDS
        ) {
            val path = cockpitOthersPath(position.lat, position.lon)
            if (path != null) {
                askedForNetworks = wallClock
                isAskingForNetworks = true
                scope.launch {
                    val answer = fetch(path)
                    isAskingForNetworks = false
                    if (answer == null || this@FlySession.session !== session) return@launch
                    val status = answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()
                    val read = runCatching { liveOthers(status, answer.body) }.getOrNull() ?: return@launch
                    takeOthers(session, read)
                }
            }
        }
        // The climbs of others around the glider, only while the pilot has
        // them switched on, in flight, once a minute.
        val climbsWanted = wantsClimbs()
        if (!climbsWanted) {
            if (climbsNear != null) climbsNear = null
        } else if (position != null && view.phase == FlightPhase.FLYING && !isAskingForClimbs &&
            wallClock - askedForClimbs >= CLIMBS_SECONDS
        ) {
            val path = cockpitClimbsPath(position.lat, position.lon)
            if (path != null) {
                askedForClimbs = wallClock
                isAskingForClimbs = true
                scope.launch {
                    val answer = fetch(path)
                    isAskingForClimbs = false
                    if (answer == null || this@FlySession.session !== session) return@launch
                    val status = answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()
                    val read = runCatching { liveClimbs(status, answer.body) }.getOrNull() ?: return@launch
                    takeClimbs(session, read)
                }
            }
        }
        // The take-offs around the glider, every half minute: the core
        // takes them from the pilot's own and from what the app holds.
        if (position != null && !isAskingForTakeoffs && wallClock - askedForTakeoffs >= TAKEOFFS_SECONDS) {
            askedForTakeoffs = wallClock
            isAskingForTakeoffs = true
            scope.launch {
                val found = takeoffsAround(position)
                val landings = landingsAround(position)
                isAskingForTakeoffs = false
                if (this@FlySession.session !== session) return@launch
                if (found != null && found != takeoffs) takeoffs = found
                handLandings(session, landings)
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
        val saidAirspace = session.takeAirspaceAlert() != null
        if (saidAirspace) {
            warnings.firstOrNull { !it.silenced }?.let { loudest -> voice.say("Airspace. ${loudest.headline}.") }
        }
        // The cables of the flight's area, as fetched; a new set reaches
        // the core at once. Their warning is said once, as an airspace
        // warning of the level "Entering" is, after one said now.
        val latestCables = cables()
        if (latestCables !== handedCables) {
            handedCables = latestCables
            session.useCables(latestCables)
        }
        val cable = session.cableWarning()
        if (cable != cableWarning) cableWarning = cable
        if (session.takeCableAlert() && cable != null) {
            if (saidAirspace) voice.sayAfter(cable.words) else voice.say(cable.words)
        }
        // The pilot's own vario lost in the air: said once, loud; the top
        // line shows it, and the phone's tone has taken over.
        if (mode == Mode.Live && session.takeVarioLost(now)) {
            if (saidAirspace) voice.sayAfter(VARIO_LOST) else voice.say(VARIO_LOST)
        }
        if (refreshesSinceScore == 0) {
            val near = session.othersNear(now)
            if (near != othersNear) othersNear = near
            // The climbs heard, aged by the core between two answers.
            if (climbsWanted && climbsNear != null) {
                val climbs = session.climbsNear(now)
                if (climbs != climbsNear) climbsNear = climbs
            }
        }
        sound.play(view.tone)
    }

    /**
     * Hands the core's session the landings it keeps a standing reading
     * of, as the core picked them. Again only when they have changed,
     * since the session begins its reading anew with each.
     */
    @Synchronized
    private fun handLandings(session: FlightSession, places: List<TargetPlace>) {
        if (this.session !== session) return
        if (places != handedLandings) {
            handedLandings = places
            session.useLandings(places)
        }
    }

    /** Hands the core the climbs around the glider, and takes those near it as the core gives them. */
    @Synchronized
    private fun takeClimbs(session: FlightSession, climbs: fly.venti.core.LiveClimbs) {
        if (this.session !== session || !wantsClimbs()) return
        val now = replay?.now() ?: clock()
        session.onClimbs(climbs, now)
        climbsNear = session.climbsNear(now)
    }

    /** Hands the core an answer of the other networks, and takes what it keeps of them. */
    @Synchronized
    private fun takeOthers(session: FlightSession, others: fly.venti.core.LiveOthers) {
        if (this.session !== session) return
        val now = replay?.now() ?: clock()
        session.onOthers(others, now)
        val near = session.othersNear(now)
        if (near != othersNear) othersNear = near
    }

    /**
     * Where the pilot means to land, as far as the cockpit knows: the place
     * it navigates to, unless that is the take-off, else the landing it
     * names as the one to think of first.
     */
    private fun placeToLand(view: CockpitView): GroundWindPlace? {
        val way = way
        if (way != null && !way.isTakeoff) {
            val elevation = view.altitudeM?.let { altitude -> way.heightAboveM?.let { altitude - it } }
            return GroundWindPlace(way.name, way.position, elevation)
        }
        val option = landing ?: return null
        val place = landings().getOrNull(option.index.toInt()) ?: return null
        return GroundWindPlace(place.name, place.position, place.elevationM)
    }

    /**
     * Takes the wind on the ground as it was just worked out. A warning is
     * said aloud when it comes up, after anything about airspace, and again
     * only for another station or after it has been gone.
     */
    @Synchronized
    private fun take(wind: GroundWind, credits: List<Credit>) {
        if (wind != windBelow) windBelow = wind
        if (credits != windBelowCredits) windBelowCredits = credits
        val alert = wind.alert
        if (alert == null) {
            warnedOfWindAt = null
            return
        }
        if (alert.stationId != warnedOfWindAt) {
            warnedOfWindAt = alert.stationId
            isWindWarningPutAway = false
            voice.sayAfter(alert.spoken)
        }
    }

    companion object {
        /** Said aloud once when the pilot's own vario is lost in the air, and shown in the top line. */
        const val VARIO_LOST = "Vario lost"

        /** What the pilot is told when the cockpit closed without a take-off. */
        const val NO_TAKEOFF =
            "Nobody took off in two hours, so the cockpit closed to save the battery. Nothing was recorded."

        /** How often the wind on the ground is asked for in flight, seconds. */
        const val WIND_BELOW_SECONDS = 120.0

        /** How often the forecast for the glider's place is asked for again, seconds by the phone's clock. */
        const val FORECAST_SECONDS = 30 * 60.0

        /** How far the glider goes from where the forecast was asked for before it is asked for again, km. */
        const val FORECAST_AGAIN_KM = 20.0

        /** How far apart two places are, km, near enough for a few tens of kilometres. */
        fun distanceKm(one: Coordinate, other: Coordinate): Double {
            val north = (other.lat - one.lat) * KM_PER_DEGREE
            val east = (other.lon - one.lon) * KM_PER_DEGREE * kotlin.math.cos(Math.toRadians((one.lat + other.lat) / 2))
            return kotlin.math.hypot(north, east)
        }

        private const val KM_PER_DEGREE = 111.2

        /** How often the other gliders around are asked for in flight, seconds. */
        const val OTHERS_SECONDS = 15.0

        /** How often the climbs of others around the glider are asked for, seconds, while the pilot has them on. */
        const val CLIMBS_SECONDS = 60.0

        /** How often the take-offs around the glider are asked for, seconds. */
        const val TAKEOFFS_SECONDS = 30.0

        /** How often the screen and the vario tone are refreshed. */
        const val REFRESH_MILLISECONDS = 200L

        /** How often a watch for the take-off from another tab looks at the core's session. */
        const val WATCH_MILLISECONDS = 1_000L

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

        /**
         * How long passes before the ground around the glider is asked for
         * again: in ten seconds a glider goes two hundred metres, and the
         * tiles reach thirteen kilometres.
         */
        private const val TERRAIN_MILLISECONDS = 10_000L

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
