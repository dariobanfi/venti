package fly.venti.app.logbook

import fly.venti.app.TestSupport
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

/** Checks the logbook's model against the real core. */
class LogbookModelTest {
    private fun makeLogbook() = LogbookModel.open(TestSupport.newFolder("logbook"), TestSupport.files)

    @Test
    fun theShippedSampleFlightIsImportedAndOpens() = runBlocking {
        val logbook = makeLogbook()
        assertTrue(logbook.view.years.isEmpty())

        val report = logbook.addSample()

        assertEquals(1, report.added.size)
        assertNull(report.notice)
        assertEquals(1u, logbook.view.totals.flights)
        val entry = logbook.view.years.first().flights.first()
        assertEquals("Sample Wing", entry.glider)
        assertTrue(entry.outline.size >= 2)

        val flight = logbook.flight(entry.id)
        assertEquals("Sample Pilot", flight.pilot)
        assertEquals(1745, flight.track.size)
        assertEquals(1744L, flight.durationS)
        assertTrue(flight.profile.size <= 400)

        val analysis = logbook.analysis(entry.id)
        assertTrue(analysis.thermals.isNotEmpty())
        assertEquals(entry.id, logbook.statistics().longest?.flightId)
    }

    @Test
    fun importingTheSameFlightAgainSaysSoAndAddsNothing() = runBlocking {
        val logbook = makeLogbook()
        logbook.addSample()

        val report = logbook.addSample()

        assertTrue(report.added.isEmpty())
        assertEquals("Already in your logbook", report.notice?.title)
        assertEquals(1u, logbook.view.totals.flights)
    }

    @Test
    fun aFileThatIsNotAFlightIsReportedByNameWithTheCoresReason() = runBlocking {
        val logbook = makeLogbook()

        val report = logbook.importFiles(listOf(FlightFileSource("notes.igc") { "hello" }))

        assertEquals(1, report.failures.size)
        val notice = checkNotNull(report.notice)
        assertEquals("1 file could not be imported", notice.title)
        assertTrue(notice.text, notice.text.startsWith("notes.igc: "))
        assertTrue(notice.text, notice.text.contains("not a flight file"))
        // The reason is the core's sentence, not a description of the error.
        assertTrue(notice.text, !notice.text.contains("reason="))
        assertTrue(logbook.view.years.isEmpty())
    }

    @Test
    fun aFileThatCannotBeReadIsReportedAndTheOthersAreImported() = runBlocking {
        val logbook = makeLogbook()

        val report = logbook.importFiles(
            listOf(
                FlightFileSource("gone.igc") { throw IOException("The file could not be opened.") },
                FlightFileSource("sample.igc") { TestSupport.sampleFlight },
            ),
        )

        assertEquals(1, report.added.size)
        assertEquals(listOf(ImportReport.Failure("gone.igc", "The file could not be opened.")), report.failures)
        assertEquals(1u, logbook.view.totals.flights)
    }

    @Test
    fun aDeletedFlightLeavesTheLogbook() = runBlocking {
        val logbook = makeLogbook()
        val id = logbook.addSample().added.first()

        logbook.delete(id)

        assertTrue(logbook.view.years.isEmpty())
    }

    @Test
    fun aFlightIsWrittenOutForSharingUnderItsRecorderStyleName() = runBlocking {
        val logbook = makeLogbook()
        val id = logbook.addSample().added.first()

        val file = logbook.exportFile(id, into = TestSupport.newFolder("export").resolve("venti-export"))

        assertEquals("2026-07-04-XVE-000-01.igc", file.name)
        assertEquals(TestSupport.sampleFlight, file.readText())
    }

    @Test
    fun flightFilesInAnOldEncodingKeepEveryLine() {
        val text = "HFPLTPILOT:José\r\nB1030004739852N01147802EA0158501620\r\n"

        assertEquals(text, LogbookModel.decode(text.toByteArray(Charsets.UTF_8)))
        assertEquals(text, LogbookModel.decode(text.toByteArray(Charsets.ISO_8859_1)))
    }

    @Test
    fun theNoticeNamesAFewFailuresAndCountsTheRest() {
        val failures = (1..6).map { ImportReport.Failure("file$it.igc", "No positions.") }

        val notice = checkNotNull(ImportReport(failures = failures).notice)

        assertEquals("6 files could not be imported", notice.title)
        assertEquals(5, notice.text.lines().size)
        assertEquals("and 2 more.", notice.text.lines().last())
        assertEquals(
            "These flights were imported before. Nothing was added.",
            ImportReport(alreadyThere = listOf("a", "b")).notice?.text,
        )
        assertNull(ImportReport(added = listOf("a"), alreadyThere = listOf("b")).notice)
    }
}
