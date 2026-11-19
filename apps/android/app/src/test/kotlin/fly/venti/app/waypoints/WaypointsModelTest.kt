package fly.venti.app.waypoints

import fly.venti.app.LaunchOptions
import fly.venti.app.TestSupport
import fly.venti.app.logbook.FlightFileSource
import fly.venti.app.logbook.LogbookModel
import fly.venti.core.Coordinate
import fly.venti.core.WaypointKind
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.IOException
import java.util.Locale

/** The pilot's waypoints against the real core, which reads the files and keeps the places. */
class WaypointsModelTest {
    private val locale = Locale.getDefault()

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    private fun makeModel(): WaypointsModel =
        LogbookModel.open(TestSupport.newFolder("waypoints"), TestSupport.files).makeWaypoints()

    private fun file(name: String, text: String) = FlightFileSource(name) { text }

    /** The made-up waypoints the UI tests start with, as a SeeYou file. */
    private val sample = LaunchOptions.SAMPLE_WAYPOINT_FILE

    @Test
    fun aFilesWaypointsAreListedByNameWithWhatIsKnownOfThem() = runBlocking {
        val waypoints = makeModel()
        assertTrue(waypoints.all.isEmpty())

        val notice = waypoints.importFiles(listOf(file("alps.cup", sample)))

        assertEquals("3 waypoints added", notice.title)
        assertEquals("", notice.text)
        assertEquals(
            listOf("Sample landing field", "Sample summit far away", "Sample turnpoint"),
            waypoints.all.map { it.name },
        )
        val field = waypoints.all.first()
        assertEquals(WaypointKind.LANDING, field.kind)
        assertEquals("SLF", field.code)
        // Without a place to measure from there is no distance to say.
        assertNull(field.distanceM)
        assertEquals("735 m · SLF", field.detail)
        assertEquals("Sample landing field", field.place.name)
        assertEquals(735.0, checkNotNull(field.place.elevationM), 0.5)
    }

    @Test
    fun fromAPlaceTheNearestComeFirstWithDistanceAndDirection() {
        val waypoints = makeModel()
        waypoints.importText(sample)

        // The take-off of the sample flight, above the landing field.
        val near = waypoints.near(Coordinate(lat = 47.66, lon = 11.80))

        assertEquals("Sample landing field", near.first().name)
        assertEquals("Sample summit far away", near.last().name)
        val distances = near.map { checkNotNull(it.distanceM) }
        assertEquals(distances.sorted(), distances)
        // About 4.7 km to the north-west, as the detail says.
        assertTrue(near.first().detail, near.first().detail.matches(Regex("""\d\.\d km (N|NW|W) · 735 m · SLF""")))
    }

    @Test
    fun whatIsAlreadyThereIsNotAddedTwice() = runBlocking {
        val waypoints = makeModel()
        waypoints.importFiles(listOf(file("alps.cup", sample)))

        val again = waypoints.importFiles(listOf(file("alps.cup", sample)))

        assertEquals("Nothing new", again.title)
        assertEquals("3 were already there.", again.text)
        assertEquals(3, waypoints.all.size)
    }

    @Test
    fun aFileThatCannotBeReadIsNamedWithTheReason() = runBlocking {
        val waypoints = makeModel()

        val notice = waypoints.importFiles(
            listOf(
                file("holiday.jpg", "not a waypoint file"),
                FlightFileSource("gone.cup") { throw IOException("The file could not be opened.") },
                file("alps.cup", sample),
            ),
        )

        // The good file is still brought in.
        assertEquals("3 waypoints added", notice.title)
        val lines = notice.text.lines()
        assertEquals(2, lines.size)
        assertTrue(lines[0], lines[0].startsWith("holiday.jpg: "))
        assertEquals("gone.cup: The file could not be opened.", lines[1])
    }

    @Test
    fun waypointsAreRemovedOneByOneOrAllAtOnceAndLeaveAsAFile() = runBlocking {
        val waypoints = makeModel()
        waypoints.importText(sample)

        val file = waypoints.exportFile(TestSupport.newFolder("export"))
        assertTrue(file.name, file.name.endsWith(".cup"))
        val text = file.readText()
        assertTrue("Sample turnpoint" in text && "Sample landing field" in text)

        waypoints.delete(waypoints.all.first { it.name == "Sample turnpoint" }.id)
        assertEquals(listOf("Sample landing field", "Sample summit far away"), waypoints.all.map { it.name })

        waypoints.deleteAll()
        assertTrue(waypoints.all.isEmpty())

        // What left as a file comes back from it.
        val back = waypoints.importFiles(listOf(file(file.name, text)))
        assertEquals("3 waypoints added", back.title)
    }

    @Test
    fun anImportIsSummedUpInWords() {
        assertEquals("No waypoints added", WaypointsModel.notice(0u, 0u, emptyList()).title)
        assertEquals("Nothing new", WaypointsModel.notice(0u, 2u, emptyList()).title)
        assertEquals("1 waypoint added", WaypointsModel.notice(1u, 0u, emptyList()).title)
        assertEquals("1,204 waypoints added", WaypointsModel.notice(1204u, 0u, emptyList()).title)
        assertEquals("1 was already there.", WaypointsModel.notice(4u, 1u, emptyList()).text)
        assertEquals(
            "2 were already there.\nold.wpt: 3 lines could not be read.",
            WaypointsModel.notice(4u, 2u, listOf("old.wpt: 3 lines could not be read.")).text,
        )
    }

    @Test
    fun aWaypointIsFoundByItsNameOrItsShortName() {
        val waypoints = makeModel()
        waypoints.importText(sample)

        assertEquals(3, waypoints.all.matching("  ").size)
        assertEquals(listOf("Sample turnpoint"), waypoints.all.matching("turn").map { it.name })
        assertEquals(listOf("Sample landing field"), waypoints.all.matching("slf").map { it.name })
        assertTrue(waypoints.all.matching("nowhere").isEmpty())
    }
}
