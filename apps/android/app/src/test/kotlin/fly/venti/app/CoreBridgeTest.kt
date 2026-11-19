package fly.venti.app

import fly.venti.core.Core
import fly.venti.core.CoreException
import fly.venti.core.FlightSession
import fly.venti.core.Replay
import fly.venti.core.RouteKind
import fly.venti.core.pendingRecordings
import fly.venti.core.varioSerialServices
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.File
import java.util.concurrent.Callable
import java.util.concurrent.Executors

/**
 * Calls the Rust core through its generated Kotlin interface.
 *
 * These run on the build machine against a build of the core for that
 * machine, without a phone or an emulator, so they are the fast check that
 * the bridge works.
 */
class CoreBridgeTest {
    private lateinit var folder: File

    @Before
    fun makeFolder() {
        folder = TestSupport.newFolder("bridge")
    }

    private fun makeCore() = Core(folder.path)

    @Test
    fun theCoreReportsItsVersion() {
        assertTrue(makeCore().version().isNotEmpty())
    }

    @Test
    fun aFlightFileCrossesTheBridge() {
        val core = makeCore()
        val imported = core.importFlight(THREE_FIXES)
        assertFalse(imported.alreadyInLogbook)

        val flight = core.flight(imported.id)

        assertEquals(imported.id, flight.id)
        assertEquals("XVE 000", flight.recorder)
        assertEquals(20L, flight.durationS)
        assertEquals(1620.0, flight.maxAltitudeM, 0.0)
        assertEquals(1618.0, flight.minAltitudeM, 0.0)
        assertEquals(3, flight.track.size)
        assertEquals(47 + 39.852 / 60, flight.track[0].lat, 1e-9)
        assertEquals(11 + 47.802 / 60, flight.track[0].lon, 1e-9)
        assertEquals(20u, flight.track[2].seconds)
        assertEquals(-0.1, flight.track[1].climbMs, 1e-9)
        assertEquals(listOf(1620.0, 1619.0, 1618.0), flight.profile.map { it.altitudeM })
        assertNull(flight.pilot)
    }

    @Test
    fun theLogbookListsTotalsAndForgetsADeletedFlight() {
        val core = makeCore()
        assertTrue(core.logbook().years.isEmpty())

        val id = core.importFlight(THREE_FIXES).id
        assertTrue(core.importFlight(THREE_FIXES).alreadyInLogbook)

        val logbook = core.logbook()
        assertEquals(1u, logbook.totals.flights)
        assertEquals(20L, logbook.totals.airtimeS)
        assertEquals(listOf(2026), logbook.years.map { it.year })
        assertEquals(listOf(id), logbook.years[0].flights.map { it.id })
        // Three fixes in a straight line: the outline is its two ends.
        assertEquals(2, logbook.years[0].flights[0].outline.size)

        core.deleteFlight(id)
        assertTrue(core.logbook().years.isEmpty())
    }

    @Test
    fun theLogbookIsStillThereWhenTheCoreIsCreatedAgain() {
        val id = makeCore().importFlight(THREE_FIXES).id
        assertEquals(id, makeCore().logbook().years.first().flights.first().id)
    }

    @Test
    fun aFileThatIsNotAFlightThrowsAnErrorWithTheCoresOwnWords() {
        val core = makeCore()

        val notAFlight = assertThrows(CoreException.InvalidFlight::class.java) { core.importFlight("hello") }
        assertTrue(notAFlight.pilotMessage, notAFlight.pilotMessage.contains("not a flight file"))
        // The words are the core's, with nothing of the bridge's around them.
        assertEquals(notAFlight.reason, notAFlight.pilotMessage)

        val noPositions = assertThrows(CoreException.InvalidFlight::class.java) {
            core.importFlight("AXVE000\r\nHFDTE040726\r\n")
        }
        assertTrue(noPositions.pilotMessage.contains("no positions"))

        val unknown = assertThrows(CoreException.UnknownFlight::class.java) { core.flight("nothing") }
        assertTrue(unknown.pilotMessage.contains("no longer in the logbook"))
    }

    @Test
    fun aDataFolderThatCannotBeCreatedIsAStorageError() {
        val file = folder.resolve("a-file").also { it.writeText("x") }

        val error = assertThrows(CoreException.Storage::class.java) { Core(file.resolve("inside").path) }

        assertTrue(error.pilotMessage.isNotEmpty())
    }

    @Test
    fun theCoreCanBeSharedAcrossThreads() {
        val core = makeCore()
        val id = core.importFlight(THREE_FIXES).id
        val threads = Executors.newFixedThreadPool(8)

        val counts = threads.invokeAll(List(8) { Callable { core.flight(id).track.size } }).map { it.get() }
        threads.shutdown()

        assertEquals(List(8) { 3 }, counts)
    }

    @Test
    fun theSampleFlightIsAnalysedScoredAndCounted() {
        val core = makeCore()
        val id = core.importFlight(TestSupport.sampleFlight).id

        val analysis = core.flightAnalysis(id)
        assertTrue(analysis.thermals.isNotEmpty())
        assertTrue(analysis.glides.isNotEmpty())
        assertEquals(1744L, analysis.airtimeS)
        val score = checkNotNull(analysis.score)
        assertTrue(score.distanceM > 0)
        assertTrue(score.route.size >= 2)

        // The logbook's line carries the same score in short.
        val entry = core.logbook().years.first().flights.first()
        assertEquals(score.kind, entry.score?.kind)
        assertEquals(score.distanceM, entry.score?.distanceM ?: 0.0, 1e-6)
        assertTrue(score.kind in RouteKind.entries)

        val statistics = core.statistics()
        assertEquals(12, statistics.months.size)
        assertEquals(1u, statistics.months.last().flights)
        assertEquals("Sample Wing", statistics.gliders.single().glider)
        assertEquals(id, statistics.longest?.flightId)
        assertEquals(1744.0, statistics.longest?.value ?: 0.0, 0.0)
    }

    @Test
    fun aFlightComesBackOutUnchangedUnderARecorderStyleName() {
        val core = makeCore()
        val id = core.importFlight(TestSupport.sampleFlight).id

        val file = core.flightFile(id)

        assertEquals("2026-07-04-XVE-000-01.igc", file.name)
        assertEquals(TestSupport.sampleFlight, file.igcText)
    }

    @Test
    fun aReplayedFlightDrivesTheCockpitAndIsRecorded() {
        val session = FlightSession(folder.path)
        session.startRecording(pilot = "Test Pilot", glider = null)
        val replay = Replay(TestSupport.sampleFlight)

        // Five minutes in, the sample is circling in its first thermal.
        assertTrue(replay.advance(session, 300.0))
        val view = session.view(replay.now())

        assertTrue(view.recording)
        assertEquals(301u, view.recordedFixes)
        assertTrue(view.circling)
        assertTrue(checkNotNull(view.climbMs) > 1.0)
        assertTrue(checkNotNull(view.altitudeM) > 1000)
        assertTrue(view.trail.isNotEmpty())
        // In the air the core describes a tone for the shell to make.
        val tone = checkNotNull(view.tone)
        assertTrue(tone.frequencyHz > 0 && tone.cycleS > 0 && tone.duty > 0)

        val recording = checkNotNull(session.stopRecording())
        assertTrue(recording.igcText.contains("Test Pilot"))
        assertEquals(listOf(recording.name), pendingRecordings(folder.path).map { it.name })
    }

    @Test
    fun theCoreListsTheBluetoothServicesVariosUse() {
        val services = varioSerialServices()

        assertEquals(3, services.size)
        assertTrue(services.any { it.service.startsWith("6E400001") })
    }

    private companion object {
        /** Three fixes, ten seconds apart, in the IGC flight-file format. */
        val THREE_FIXES = listOf(
            "AXVE000",
            "HFDTEDATE:040726,01",
            "B1030004739852N01147802EA0158501620",
            "B1030104739860N01147790EA0158401619",
            "B1030204739868N01147778EA0158301618",
            "",
        ).joinToString("\r\n")
    }
}
