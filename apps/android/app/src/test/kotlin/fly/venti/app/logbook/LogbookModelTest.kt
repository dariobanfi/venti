package fly.venti.app.logbook

import fly.venti.app.TestSupport
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException
import java.util.concurrent.CountDownLatch

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
    fun deletingSaysWhereTheFlightGoesFrom() {
        // Nobody is signed in on this phone: the flight is here and nowhere else.
        assertEquals("Its file is removed from this phone. This cannot be undone.", makeLogbook().deletionWarning)
        assertEquals(
            "Its file is removed from this phone, from Venti and from your other devices. " +
                "This cannot be undone.",
            LogbookModel.deletionWarning(signedIn = true),
        )
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
    }

    @Test
    fun aBatchSaysWhatItDidAndOneNewFlightSimplyOpens() {
        // One new flight opens; there is nothing to say.
        assertNull(ImportReport(added = listOf("a")).notice)

        val several = checkNotNull(ImportReport(added = listOf("a", "b", "c")).notice)
        assertEquals("3 flights added", several.title)
        assertEquals("They are in your logbook, on this phone.", several.text)

        val mixed = checkNotNull(ImportReport(added = listOf("a"), alreadyThere = listOf("b", "c", "d")).notice)
        assertEquals("1 flight added", mixed.title)
        assertEquals("It is in your logbook, on this phone. 3 were in your logbook already.", mixed.text)
        assertEquals(
            "They are in your logbook, on this phone. 1 was in your logbook already.",
            ImportReport(added = listOf("a", "b"), alreadyThere = listOf("c")).notice?.text,
        )

        val none = checkNotNull(ImportReport().notice)
        assertEquals("No flights found", none.title)
        assertEquals("There was no flight file among what you chose.", none.text)
    }

    /** The sample flight as flown on another day: another flight. */
    private fun flown(day: String): String = TestSupport.sampleFlight.replace("HFDTEDATE:040726", "HFDTEDATE:$day")

    @Test
    fun inAFolderOnlyFlightFilesCountOldestNameFirst() {
        val found = listOf(
            "b.IGC" to 1,
            "notes.txt" to 2,
            "a.igc" to 3,
            "2025/c.igc" to 4,
            "photos/launch.jpg" to 5,
            "igc" to 6,
        )

        assertEquals(listOf("2025/c.igc", "a.igc", "b.IGC"), FlightFiles.flightFilesAmong(found).map { it.first })
        assertEquals(listOf(4, 3, 1), FlightFiles.flightFilesAmong(found).map { it.second })
        assertTrue(FlightFiles.flightFilesAmong(emptyList<Pair<String, Int>>()).isEmpty())
    }

    @Test
    fun aFolderOfFlightsIsImportedWholeAndSaysWhatItDid() = runBlocking {
        val logbook = makeLogbook()
        val history = mutableListOf(
            FlightFileSource("c.igc") { flown("060725") },
            FlightFileSource("a.igc") { flown("040726") },
            FlightFileSource("b.IGC") { flown("050726") },
        )

        val report = logbook.importFolder("History") { history }

        assertEquals(3, report.added.size)
        assertTrue(report.failures.isEmpty())
        assertEquals("3 flights added", report.notice?.title)
        assertEquals("They are in your logbook, on this phone.", report.notice?.text)
        assertNull(logbook.importProgress)
        assertEquals(3u, logbook.view.totals.flights)

        // The same folder a month later, with one more flight in it.
        history += FlightFileSource("d.igc") { flown("070726") }
        val again = logbook.importFolder("History") { history }
        assertEquals(1, again.added.size)
        assertEquals("1 flight added", again.notice?.title)
        assertEquals("It is in your logbook, on this phone. 3 were in your logbook already.", again.notice?.text)
        assertEquals(4u, logbook.view.totals.flights)

        // A folder without flights says so.
        assertEquals("No flights found", logbook.importFolder("Photos") { emptyList() }.notice?.title)
    }

    @Test
    fun anImportCountsItsFilesAndFillsTheListWhileItRuns() = runBlocking {
        val logbook = makeLogbook()
        val seen = mutableListOf<ImportProgress?>()
        val listed = mutableListOf<UInt>()
        // Twenty-two flights, one for each day; each notes how far the import had come when it was read.
        val sources = (1..22).map { day ->
            FlightFileSource("flight-$day.igc") {
                seen += logbook.importProgress
                listed += logbook.view.totals.flights
                flown("%02d0626".format(day))
            }
        }

        val report = logbook.importFolder("June") { sources }

        assertEquals(22, report.added.size)
        // Counted once the folder has been listed, then file by file.
        assertEquals((0..21).map { ImportProgress(done = it, total = 22) }, seen)
        // The list is read again after every twenty files, not after each.
        assertEquals(0u, listed[19])
        assertEquals(20u, listed[20])
        assertEquals(20u, listed[21])
        assertNull(logbook.importProgress)
        assertEquals(22u, logbook.view.totals.flights)
    }

    @Test
    fun aFolderThatCannotBeReadIsNamedWithTheReason() = runBlocking {
        val logbook = makeLogbook()

        val report = logbook.importFolder("History") {
            // While the folder is looked through, how many files it holds is not known.
            assertEquals(ImportProgress(done = 0, total = 0), logbook.importProgress)
            throw IOException("Venti is not allowed to read this folder.")
        }

        assertEquals(
            listOf(ImportReport.Failure("History", "Venti is not allowed to read this folder.")),
            report.failures,
        )
        assertEquals("1 file could not be imported", report.notice?.title)
        assertNull(logbook.importProgress)
    }

    @Test
    fun aSecondImportWaitsItsTurnByBeingTurnedAway() = runBlocking {
        val logbook = makeLogbook()
        val reading = CountDownLatch(1)
        val release = CountDownLatch(1)
        val first = async(Dispatchers.Default) {
            logbook.importFiles(
                listOf(
                    FlightFileSource("slow.igc") {
                        reading.countDown()
                        release.await()
                        TestSupport.sampleFlight
                    },
                ),
            )
        }
        reading.await()
        assertEquals(ImportProgress(done = 0, total = 1), logbook.importProgress)

        val second = logbook.importFiles(listOf(FlightFileSource("other.igc") { TestSupport.sampleFlight }))

        assertEquals(
            listOf(
                ImportReport.Failure("other.igc", "Another import is still running. Try again when it has finished."),
            ),
            second.failures,
        )
        release.countDown()
        assertEquals(1, first.await().added.size)
        assertNull(logbook.importProgress)
        assertEquals(1u, logbook.view.totals.flights)
    }
}
