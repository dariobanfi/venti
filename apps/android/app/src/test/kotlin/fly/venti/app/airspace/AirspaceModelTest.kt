package fly.venti.app.airspace

import fly.venti.app.LaunchOptions
import fly.venti.app.TestSupport
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.logbook.PickedFile
import fly.venti.core.AirspaceFile
import fly.venti.core.AirspaceImport
import fly.venti.core.AirspacePolicy
import fly.venti.core.Bounds
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.maplibre.geojson.Polygon
import java.io.IOException
import java.util.Locale

/** The pilot's airspace files against the real core, which reads them and answers for them. */
class AirspaceModelTest {
    private val locale = Locale.getDefault()

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    /** 4 July 2026, 10:30 UTC: the day the files are brought in. */
    private val today = 1_783_161_000L

    private fun makeModel(): AirspaceModel =
        LogbookModel.open(TestSupport.newFolder("airspace"), TestSupport.files).makeAirspace(now = { today })

    /** The made-up control zone the UI tests start with, as an OpenAir file. */
    private val zone = PickedFile("zones.txt") { LaunchOptions.SAMPLE_AIRSPACE_FILE.toByteArray() }

    /** The valley the zone lies in. */
    private val valley = Bounds(south = 47.6, west = 11.6, north = 47.8, east = 11.9)

    @Test
    fun aFilesAirspaceIsKeptAndFoundOnTheMap() = runBlocking {
        val airspace = makeModel()
        assertTrue(airspace.files.isEmpty())
        assertNull(airspace.set)
        assertTrue(airspace.shapes(valley).isEmpty())

        val notice = airspace.importFiles(listOf(zone))

        assertEquals("Airspace", notice.title)
        assertEquals("zones.txt: 1 airspace read.", notice.text)
        val file = airspace.files.single()
        // The file is known by its name without the ending.
        assertEquals("zones", file.name)
        assertEquals("1 airspace · brought in Sat, 4 Jul 2026", AirspaceModel.detail(file))
        assertNull(AirspaceModel.leftOutNote(file))
        assertNotNull(airspace.set)

        val shape = airspace.shapes(valley).single()
        assertEquals("SAMPLE ZONE (NOT REAL)", shape.name)
        // A control zone is closed to a paraglider.
        assertEquals(AirspacePolicy.AVOID, shape.policy)
        assertTrue(shape.outline.size >= 8)
        // Elsewhere there is nothing.
        assertTrue(airspace.shapes(Bounds(south = 45.0, west = 6.0, north = 45.5, east = 6.5)).isEmpty())

        airspace.delete(file)
        assertTrue(airspace.files.isEmpty())
        assertNull(airspace.set)
    }

    @Test
    fun aFileThatHoldsNoAirspaceIsNamedWithTheReason() = runBlocking {
        val airspace = makeModel()

        val notice = airspace.importFiles(
            listOf(
                PickedFile("holiday.jpg") { byteArrayOf(-1, -40, -1, -32, 0, 16) },
                PickedFile("gone.txt") { throw IOException("The file could not be opened.") },
                zone,
            ),
        )

        val lines = notice.text.split("\n\n")
        assertEquals(3, lines.size)
        assertTrue(lines[0], lines[0].startsWith("holiday.jpg: "))
        assertEquals("gone.txt: The file could not be opened.", lines[1])
        assertEquals("zones.txt: 1 airspace read.", lines[2])
        // Only the file that was read is kept.
        assertEquals(listOf("zones"), airspace.files.map { it.name })
    }

    @Test
    fun whatAFileLeftOutIsSaidPlainly() {
        fun result(airspaces: UInt, leftOut: UInt = 0u, assumed: UInt = 0u, problems: List<String> = emptyList()) =
            AirspaceImport(airspaces = airspaces, leftOut = leftOut, assumed = assumed, problems = problems)

        assertEquals("de.txt: 1,204 airspaces read.", AirspaceModel.summary(result(1204u), "de.txt"))
        assertEquals(
            "de.txt: 12 airspaces read. 1 airspace could not be read and is left out: " +
                "you will not be warned about it.",
            AirspaceModel.summary(result(12u, leftOut = 1u), "de.txt"),
        )
        assertEquals(
            "de.txt: 12 airspaces read. 3 airspaces could not be read and are left out: " +
                "you will not be warned about them. In 2 places something could not be read and the " +
                "cautious reading was taken.\nLine 40: no upper limit",
            AirspaceModel.summary(result(12u, leftOut = 3u, assumed = 2u, problems = listOf("Line 40: no upper limit")), "de.txt"),
        )
        assertEquals(
            "at.txt: 1 airspace read. In 1 place something could not be read and the cautious reading was taken.",
            AirspaceModel.summary(result(1u, assumed = 1u), "at.txt"),
        )

        fun file(leftOut: UInt) =
            AirspaceFile(storedName = "x", name = "x.txt", importedUnixS = null, airspaces = 5u, leftOut = leftOut)
        assertEquals("5 airspaces", AirspaceModel.detail(file(0u)))
        assertNull(AirspaceModel.leftOutNote(file(0u)))
        assertEquals(
            "1 airspace in this file could not be read. You will not be warned about it.",
            AirspaceModel.leftOutNote(file(1u)),
        )
        assertEquals(
            "4 airspaces in this file could not be read. You will not be warned about them.",
            AirspaceModel.leftOutNote(file(4u)),
        )
    }

    @Test
    fun theMapIsToldWhatToDrawForAnAirspace() = runBlocking {
        val airspace = makeModel()
        airspace.importFiles(listOf(zone))
        val shape = airspace.shapes(valley).single()

        val feature = AirspaceLayers.feature(shape)

        val rings = (feature.geometry() as Polygon).coordinates()
        assertEquals(1 + shape.holes.size, rings.size)
        // A closed ring: it ends where it began.
        assertEquals(rings[0].first(), rings[0].last())
        assertEquals(2, feature.getNumberProperty(AirspaceLayers.POLICY).toInt())
        val label = feature.getStringProperty(AirspaceLayers.LABEL)
        assertEquals("SAMPLE ZONE (NOT REAL)\n${shape.lower} – ${shape.upper}", label)
        assertEquals("${shape.lower} – ${shape.upper}", AirspaceLayers.label(shape.copy(name = "")))

        // What matters most is drawn last, on top.
        assertTrue(AirspaceLayers.rank(AirspacePolicy.FREE) < AirspaceLayers.rank(AirspacePolicy.CAUTION))
        assertTrue(AirspaceLayers.rank(AirspacePolicy.CAUTION) < AirspaceLayers.rank(AirspacePolicy.AVOID))
        assertEquals(3, AirspacePolicy.entries.map(AirspaceLayers::rgb).toSet().size)
    }
}
