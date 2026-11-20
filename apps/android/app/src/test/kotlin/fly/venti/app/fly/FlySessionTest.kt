package fly.venti.app.fly

import fly.venti.app.LaunchOptions
import fly.venti.app.TestSupport
import fly.venti.app.airspace.AirspaceWords
import fly.venti.app.flight.REPLAY_SPEEDS
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.settings.MemoryPreferences
import fly.venti.core.AirspaceCheck
import fly.venti.core.AirspaceLevel
import fly.venti.core.Coordinate
import fly.venti.core.FlightPhase
import fly.venti.core.FlightSession
import fly.venti.core.LocationSample
import fly.venti.core.Replay
import fly.venti.core.TargetPlace
import fly.venti.core.ToneView
import fly.venti.core.pendingRecordings
import fly.venti.core.windMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** The cockpit's session model against the real core, fed by a replay or by made-up sensors. */
class FlySessionTest {
    private val scope = CoroutineScope(Dispatchers.Default)

    @After
    fun stop() {
        scope.cancel()
    }

    /** Sensors that report whatever the test tells them to. */
    private class FakeSensors : FlightSensors {
        var session: FlightSession? = null
        var stopped = false

        override fun start(session: FlightSession, onProblem: (String) -> Unit) {
            this.session = session
        }

        override fun stop() {
            stopped = true
        }
    }

    /** A tone player that remembers what it was asked to play. */
    private class RecordedTone : TonePlayer {
        override var isMuted = false
        var isStarted = false
        val played = mutableListOf<ToneView?>()

        override fun start() {
            isStarted = true
        }

        override fun stop() {
            isStarted = false
        }

        @Synchronized
        override fun play(tone: ToneView?) {
            played += tone
        }
    }

    private fun makeSession(
        folder: File,
        sensors: FlightSensors = FakeSensors(),
        sound: TonePlayer = SilentTone(),
        preferences: MemoryPreferences = MemoryPreferences(),
    ) = FlySession(folder, preferences, sensors, sound, scope)

    @Test
    fun aReplayDrivesTheCockpitAndSavesNothing() {
        val folder = TestSupport.newFolder("fly")
        val sound = RecordedTone()
        val fly = makeSession(folder, sound = sound)
        assertNull(fly.mode)
        assertNull(fly.view)

        // Five hundred times faster: five minutes of flight in under a second.
        fly.startReplay(TestSupport.sampleFlight, speed = 500.0)
        assertEquals(FlySession.Mode.Replay(500.0), fly.mode)
        assertNotNull(fly.view)
        assertTrue(sound.isStarted)

        Thread.sleep(1200)

        val view = checkNotNull(fly.view)
        assertEquals(FlightPhase.FLYING, view.phase)
        assertFalse(view.recording)
        assertNotNull(view.altitudeM)
        assertNotNull(view.climbMs)
        assertTrue(view.trail.isNotEmpty())
        // The tone the core described was handed on to be played.
        assertTrue(synchronized(sound) { sound.played.any { it != null } })

        assertNull(fly.finish())
        assertNull(fly.mode)
        assertNull(fly.view)
        assertFalse(sound.isStarted)
        assertTrue(pendingRecordings(folder.path).isEmpty())
    }

    @Test
    fun aFlightOfTheLogbookIsReplayedFromItsOwnFileAndNothingIsKeptOfIt() = runBlocking {
        val folder = TestSupport.newFolder("fly")
        val logbook = LogbookModel.open(folder, TestSupport.files)
        val flight = logbook.addSample().added.single()

        // The file the logbook keeps holds every position that was brought in.
        val text = logbook.fileText(flight)
        fun positions(file: String) = file.lines().count { it.startsWith("B") }
        assertTrue(positions(text) > 1000)
        assertEquals(positions(TestSupport.sampleFlight), positions(text))

        // Played through the cockpit the pilot flies with, in the logbook's own folder.
        val fly = makeSession(folder)
        fly.startReplay(text, speed = 500.0)
        assertEquals(FlySession.Mode.Replay(500.0), fly.mode)
        Thread.sleep(1200)
        val view = checkNotNull(fly.view)
        assertEquals(FlightPhase.FLYING, view.phase)
        // Nothing is recorded and nothing is shared.
        assertFalse(view.recording)
        assertNull(fly.live)
        assertNull(fly.finish())
        assertTrue(pendingRecordings(folder.path).isEmpty())
        logbook.refresh()
        assertEquals(1, logbook.view.totals.flights.toInt())

        // The speeds a flight's menu offers, and what it calls them.
        assertEquals(
            listOf("In real time" to 1.0, "10 times faster" to 10.0, "20 times faster" to 20.0),
            REPLAY_SPEEDS,
        )
    }

    @Test
    fun aReplayIsHeldAtAMomentAndRunsOnAtAnotherSpeed() {
        val fly = makeSession(TestSupport.newFolder("fly"))
        fly.startReplay(TestSupport.sampleFlight, speed = 500.0)
        Thread.sleep(1200)

        // Held: the flight stays where it is, however long the pilot looks.
        fly.setReplaySpeed(0.0)
        assertEquals(FlySession.Mode.Replay(0.0), fly.mode)
        Thread.sleep(400)
        val held = checkNotNull(fly.view).flightTimeS
        assertNotNull(held)
        Thread.sleep(800)
        assertEquals(held, checkNotNull(fly.view).flightTimeS)

        // Run on, it carries on from that moment.
        fly.setReplaySpeed(500.0)
        Thread.sleep(800)
        assertTrue(checkNotNull(checkNotNull(fly.view).flightTimeS) > checkNotNull(held))

        // A speed that is no speed changes nothing.
        fly.setReplaySpeed(-1.0)
        fly.setReplaySpeed(Double.NaN)
        assertEquals(FlySession.Mode.Replay(500.0), fly.mode)
        fly.finish()

        // What the cockpit's menu offers, and what its line says.
        assertEquals(
            listOf("Paused", "Real time", "10 times faster", "20 times faster"),
            ReplayWords.SPEEDS.map(ReplayWords::choice),
        )
        assertEquals(
            listOf("Replay paused", "Replay", "Replay ×10", "Replay ×20"),
            ReplayWords.SPEEDS.map(ReplayWords::words),
        )
    }

    @Test
    fun aRecordedFlightHasNoSpeedToChange() {
        val sensors = FakeSensors()
        val fly = makeSession(TestSupport.newFolder("fly"), sensors = sensors)
        fly.startFlight()
        assertEquals(FlySession.Mode.Live, fly.mode)
        fly.setReplaySpeed(0.0)
        assertEquals(FlySession.Mode.Live, fly.mode)
        fly.finish()
    }

    @Test
    fun theCockpitNavigatesBackToTheTakeoffUntilAPlaceIsChosen() {
        val fly = makeSession(TestSupport.newFolder("fly"))
        assertNull(fly.way)

        fly.startReplay(TestSupport.sampleFlight, speed = 500.0)
        Thread.sleep(1200)

        // In the air with nothing chosen: the way back to where it took off.
        val home = checkNotNull(fly.way)
        assertTrue(home.isTakeoff)
        assertTrue(home.distanceM > 0)
        assertTrue(home.bearingDeg in 0.0..360.0)

        // A place the pilot chooses takes its place at once.
        val field = TargetPlace(
            name = "Sample landing field",
            position = Coordinate(lat = 47.6833, lon = 11.755),
            elevationM = 735.0,
        )
        fly.goTo(field)
        val way = checkNotNull(fly.way)
        assertFalse(way.isTakeoff)
        assertEquals("Sample landing field", way.name)
        assertEquals(field.position, way.position)
        // The glider is above the field, so there is a glide that would reach it.
        assertNotNull(way.heightAboveM)
        assertTrue(TargetWords.spoken(way).startsWith("Sample landing field, "))
        // No ground was handed to this session, so nothing is in the way.
        assertNull(way.groundInTheWayM)
        // With a ridge on the straight line, that is what the line says,
        // in place of the height the glide would arrive with.
        val behind = TargetWords.spoken(way.copy(groundInTheWayM = 4_020.0))
        assertTrue(behind, behind.endsWith(", the ground is in the way in 4.0 kilometres"))
        assertFalse(behind.contains("expected to"))

        // Choosing nothing goes back to the take-off.
        fly.goTo(null)
        assertTrue(checkNotNull(fly.way).isTakeoff)

        fly.finish()
        assertNull(fly.way)
        assertNull(fly.score)
        assertEquals("Nothing chosen", TargetWords.spoken(null))
    }

    /** An announcer that remembers what it was asked to say. */
    private class Spoken : Announcer {
        val said: MutableList<String> = java.util.Collections.synchronizedList(mutableListOf<String>())

        override fun say(text: String) {
            said += text
        }

        override fun stop() = Unit
    }

    @Test
    fun inFlightTheCockpitAsksForTheWindOnTheGroundAndWarnsOfAStrongWindAtTheLanding() {
        val voice = Spoken()
        val fly = FlySession(TestSupport.newFolder("fly"), MemoryPreferences(), FakeSensors(), SilentTone(), scope, voice)
        // The stations Venti's own tests keep, read ten minutes after the newest of them.
        val now = java.time.Instant.parse("2026-11-18T19:10:00Z").epochSecond.toLong()
        val stations = windMap(200u, TestSupport.recordedAnswers.resolve("stations.json").readBytes(), now)
        val asked = java.util.Collections.synchronizedList(mutableListOf<Pair<Coordinate, Coordinate?>>())
        fly.windOnTheGround = { place, landing ->
            asked += place to landing
            stations
        }

        // On the ground nothing is asked. The pilot chooses to fly to a
        // field beside the station on the Hohenpeissenberg, where it blows
        // at 44 km/h, more than the glider can go against.
        fly.startReplay(TestSupport.sampleFlight, speed = 500.0)
        val field = TargetPlace(
            name = "Sample field by a station",
            position = Coordinate(lat = 47.8005, lon = 11.0100),
            elevationM = 977.0,
        )
        fly.goTo(field)
        Thread.sleep(1500)

        // Asked once in flight, around the glider and around the field.
        assertEquals(1, asked.size)
        assertEquals(field.position, asked.single().second)
        val below = checkNotNull(fly.windBelow)
        val first = below.stations.first()
        assertEquals("Hohenpeißenberg", first.name)
        assertTrue(first.isAtLanding)
        assertTrue(first.isStrong)
        assertTrue(fly.windBelowCredits.isNotEmpty())
        // The warning stands over the map and is said once, after anything about airspace.
        val warning = checkNotNull(fly.windWarning)
        assertEquals("Strong wind at Sample field by a station", warning.title)
        assertEquals(listOf(warning.spoken), voice.said.toList())
        // Put away, it stays away.
        fly.putWindWarningAway()
        assertNull(fly.windWarning)

        fly.finish()
        assertNull(fly.windBelow)
        assertTrue(fly.windBelowCredits.isEmpty())
        assertNull(fly.windWarning)
    }

    @Test
    fun withoutAnAnswerTheCockpitSaysNothingAboutTheGroundsWind() {
        val fly = makeSession(TestSupport.newFolder("fly"))
        fly.startReplay(TestSupport.sampleFlight, speed = 500.0)
        Thread.sleep(1200)
        assertNull(fly.windBelow)
        assertNull(fly.windWarning)
        fly.finish()
    }

    @Test
    fun theCockpitWarnsOfThePilotsAirspaceAndCanBeToldToBeQuiet() {
        val folder = TestSupport.newFolder("fly")
        // A made-up control zone beside the valley the sample flight crosses.
        val airspace = LogbookModel.open(folder, TestSupport.files).makeAirspace()
        airspace.importText("zones.txt", LaunchOptions.SAMPLE_AIRSPACE_FILE)
        val voice = Spoken()
        val fly = FlySession(folder, MemoryPreferences(), FakeSensors(), SilentTone(), scope, voice)
        fly.airspace = { airspace.set }
        assertTrue(fly.airspaceWarnings.isEmpty())
        // Nothing runs, so nothing is checked, and nothing needs saying.
        assertEquals(AirspaceCheck.NO_AIRSPACE, fly.airspaceCheck)
        assertNull(AirspaceWords.notChecking(fly.airspaceCheck))

        fly.startReplay(TestSupport.sampleFlight, speed = 500.0)
        var waited = 0
        // Seconds on a machine at rest; a minute's patience for one that is building beside the test.
        while (fly.airspaceWarnings.isEmpty() && waited < 60_000) {
            Thread.sleep(50)
            waited += 50
        }

        val warning = fly.airspaceWarnings.first()
        assertEquals(AirspaceCheck.CHECKING, fly.airspaceCheck)
        assertEquals("SAMPLE ZONE (NOT REAL)", warning.name)
        assertFalse(warning.silenced)
        assertTrue(warning.headline, "SAMPLE ZONE (NOT REAL)" in warning.headline)
        assertEquals(warning, AirspaceWords.shown(fly.airspaceWarnings))
        assertTrue(AirspaceWords.origin(warning).startsWith("From your file \u201czones\u201d"))
        // It was said aloud once, in the words on the banner.
        assertTrue(voice.said.toString(), voice.said.first().startsWith("Airspace. "))

        // "Does not apply today" keeps it on the list, quieted.
        fly.ignoreAirspaceToday(warning.id)
        assertTrue(fly.airspaceWarnings.first { it.id == warning.id }.silenced)
        fly.unsilenceAirspace(warning.id)
        assertFalse(fly.airspaceWarnings.first { it.id == warning.id }.silenced)
        fly.acknowledgeAirspace(warning.id)
        assertTrue(fly.airspaceWarnings.first { it.id == warning.id }.silenced)

        fly.finish()
        assertTrue(fly.airspaceWarnings.isEmpty())

        // Silence that is not "no airspace near" is said in words.
        assertTrue(checkNotNull(AirspaceWords.notChecking(AirspaceCheck.WAITING_FOR_HEIGHT)).startsWith("Airspace is not being checked yet"))
        assertTrue(checkNotNull(AirspaceWords.notChecking(AirspaceCheck.STALE)).startsWith("Airspace is not being checked: no position."))
        assertTrue(checkNotNull(AirspaceWords.notChecking(AirspaceCheck.OUTSIDE_FILES)).startsWith("Your airspace does not cover this place."))
        assertNull(AirspaceWords.notChecking(AirspaceCheck.CHECKING))
    }

    /**
     * What the UI tests rely on: the sample flight comes near the sample
     * zone some minutes after take-off and then stays near until it lands.
     * A zone it left again for a while made those tests fail whenever a
     * slow emulator looked at the warnings in just that while.
     */
    @Test
    fun onceNearTheSampleZoneTheSampleFlightStaysNear() {
        val folder = TestSupport.newFolder("fly")
        val airspace = LogbookModel.open(folder, TestSupport.files).makeAirspace()
        airspace.importText("zones.txt", LaunchOptions.SAMPLE_AIRSPACE_FILE)
        val session = FlightSession(folder.path)
        session.useAirspace(airspace.set)
        val replay = Replay(TestSupport.sampleFlight)

        var seconds = 0
        var firstWarning: Int? = null
        val levels = mutableSetOf<AirspaceLevel>()
        while (replay.advance(session, 1.0)) {
            seconds += 1
            val warnings = session.airspaceWarnings()
            if (firstWarning == null && warnings.isNotEmpty()) firstWarning = seconds
            if (firstWarning != null) {
                assertTrue("No warning $seconds s into the flight.", warnings.isNotEmpty())
                levels += warnings.first().level
            }
        }

        // The cockpit starts without a warning, and the flight is long
        // enough afterwards for a test to look at one.
        assertTrue("First warning after $firstWarning s.", checkNotNull(firstWarning) in 300..700)
        assertTrue("The flight lasted $seconds s.", seconds > 1500)
        // The glider flies into the zone and out again: every kind of warning is seen.
        assertEquals(AirspaceLevel.entries.toSet(), levels)
        // And it is still there when the replay has run out.
        assertTrue(session.airspaceWarnings().isNotEmpty())
        assertEquals(AirspaceCheck.CHECKING, session.airspaceCheck(replay.now()))
        assertEquals(AirspaceCheck.STALE, session.airspaceCheck(replay.now() + 600.0))
        assertTrue(session.airspaceWarnings().isNotEmpty())
    }

    @Test
    fun aReplayDoesNotLeapOverWhatNobodySaw() {
        // Refreshed five times a second, a replay moves on by its speed.
        assertEquals(0.2, FlySession.replayStep(elapsed = 0.2, speed = 1.0), 1e-9)
        assertEquals(4.0, FlySession.replayStep(elapsed = 0.2, speed = 20.0), 1e-9)
        // Two seconds of the clock are the longest step there is.
        assertEquals(40.0, FlySession.replayStep(elapsed = 2.0, speed = 20.0), 1e-9)
        // Held for sixteen seconds, the replay carries on from where it
        // was: forty seconds of the flight at twenty times its speed, not
        // five minutes of it.
        assertEquals(40.0, FlySession.replayStep(elapsed = 16.0, speed = 20.0), 1e-9)
        assertEquals(2.0, FlySession.replayStep(elapsed = 16.0, speed = 1.0), 1e-9)
    }

    @Test
    fun theFlightIsScoredWhileItIsFlown() {
        val fly = makeSession(TestSupport.newFolder("fly"))
        fly.startReplay(TestSupport.sampleFlight, speed = 500.0)
        assertNull(fly.score)

        // Ten minutes of the flight, and the look at the score that follows.
        Thread.sleep(2600)

        val score = checkNotNull(fly.score)
        assertTrue(score.distanceM > 1000)
        assertTrue(score.route.size >= 2)
        fly.finish()
    }

    @Test
    fun aLiveFlightIsRecordedFromTheSensorsWithThePilotsNames() = runBlocking {
        val folder = TestSupport.newFolder("fly")
        val sensors = FakeSensors()
        val preferences = MemoryPreferences().apply {
            pilot = "Test Pilot"
            glider = "Test Wing"
        }
        val fly = makeSession(folder, sensors, preferences = preferences)

        fly.startFlight()
        assertEquals(FlySession.Mode.Live, fly.mode)
        val session = checkNotNull(sensors.session)
        // The phone reports a position a second, climbing.
        for (second in 0 until 30) {
            session.onLocation(
                LocationSample(
                    timeUnixS = 1_783_161_000.0 + second,
                    lat = 47.66 + second * 0.0001,
                    lon = 11.78,
                    altitudeM = 1500.0 + second * 2,
                    groundSpeedMs = 9.0,
                    courseDeg = 0.0,
                ),
            )
            session.onPressure(1_783_161_000.0 + second, 850.0 - second * 0.2)
        }

        val recording = checkNotNull(fly.finish())

        assertTrue(sensors.stopped)
        assertNull(fly.mode)
        assertTrue(recording.igcText.contains("Test Pilot"))
        assertTrue(recording.igcText.contains("Test Wing"))
        assertEquals(30, recording.igcText.lines().count { it.startsWith("B") })

        // The finished recording goes into the logbook and is cleared.
        val logbook = LogbookModel.open(folder, TestSupport.files)
        val id = logbook.adopt(recording)
        assertEquals(id, logbook.view.years.first().flights.first().id)
        assertTrue(pendingRecordings(folder.path).isEmpty())
        // Half a minute in the air is short, but it opens like any flight.
        val flight = logbook.flight(id)
        assertEquals("Test Wing", flight.glider)
        assertTrue(flight.track.size >= 2)
        assertTrue(flight.maxAltitudeM > 1500)
    }

    @Test
    fun aFinishedRecordingGoesIntoTheLogbookAndIsCleared() = runBlocking {
        val folder = TestSupport.newFolder("fly")
        val logbook = LogbookModel.open(folder, TestSupport.files)

        // Record the first two minutes of the sample through the core, as
        // a flight would be.
        val session = FlightSession(folder.path)
        session.startRecording(pilot = "Test Pilot", glider = null)
        Replay(TestSupport.sampleFlight).advance(session, 120.0)
        val recording = checkNotNull(session.stopRecording())
        assertEquals(1, pendingRecordings(folder.path).size)

        val id = logbook.adopt(recording)

        assertEquals(1u, logbook.view.totals.flights)
        assertEquals(id, logbook.view.years.first().flights.first().id)
        assertEquals(120L, logbook.view.totals.airtimeS)
        assertEquals("Test Pilot", logbook.flight(id).pilot)

        // The flight can be handed back out as a file with a recorder-style name.
        val exported = logbook.exportFile(id, into = folder.resolve("export"))
        assertEquals("2026-07-04-XVE-001-01.igc", exported.name)
        assertEquals(recording.igcText, exported.readText())
        assertTrue(pendingRecordings(folder.path).isEmpty())
    }

    @Test
    fun aRecordingLeftBehindIsAdoptedAtTheNextLaunch() = runBlocking {
        val folder = TestSupport.newFolder("fly")
        val session = FlightSession(folder.path)
        session.startRecording(pilot = null, glider = null)
        Replay(TestSupport.sampleFlight).advance(session, 60.0)
        // The app stops here without finishing the flight.

        val logbook = LogbookModel.open(folder, TestSupport.files)
        assertEquals(0u, logbook.view.totals.flights)
        logbook.adopt(logbook.interruptedRecordings())

        assertEquals(1u, logbook.view.totals.flights)
        assertTrue(pendingRecordings(folder.path).isEmpty())
    }

    @Test
    fun mutingIsRememberedAndReachesTheSound() {
        val preferences = MemoryPreferences()
        val sound = RecordedTone()
        val fly = makeSession(TestSupport.newFolder("fly"), sound = sound, preferences = preferences)

        fly.isMuted = true

        assertTrue(preferences.varioMuted)
        assertTrue(sound.isMuted)
        assertTrue(makeSession(TestSupport.newFolder("fly"), preferences = preferences).isMuted)
    }

    @Test
    fun aFileThatIsNotAFlightCannotBeReplayed() {
        val fly = makeSession(TestSupport.newFolder("fly"))

        fly.startReplay("hello", speed = 1.0)

        assertNull(fly.mode)
        assertTrue(checkNotNull(fly.problem).contains("not a flight file"))
    }
}
