package fly.venti.app.fly

import fly.venti.app.TestSupport
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.settings.MemoryPreferences
import fly.venti.core.FlightPhase
import fly.venti.core.FlightSession
import fly.venti.core.LocationSample
import fly.venti.core.Replay
import fly.venti.core.ToneView
import fly.venti.core.pendingRecordings
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
