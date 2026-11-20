package fly.venti.app.fly

import fly.venti.app.TestSupport
import fly.venti.app.map.MapStyle
import fly.venti.app.terrain.TerrainModel
import fly.venti.core.AirspaceAheadSpan
import fly.venti.core.AirspacePolicy
import fly.venti.core.Coordinate
import fly.venti.core.FlightSession
import fly.venti.core.Replay
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.util.Locale

/**
 * The line ahead, from the side: what the sheet says beside the core's
 * words, and where the glider's line ends in the picture.
 */
class AheadTest {
    private val locale = Locale.getDefault()

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    /** An airspace on the line, as the core hands one out. */
    private fun span(fromM: Double, inTheWay: Boolean, uncertain: Boolean = false) = AirspaceAheadSpan(
        id = "sample",
        name = "SAMPLE ZONE (NOT REAL)",
        kind = "Class D control zone",
        lower = "GND",
        upper = "2,000 m",
        fromM = fromM,
        toM = fromM + 2_000.0,
        floorM = 0.0,
        ceilingM = 2_000.0,
        uncertain = uncertain,
        inTheWay = inTheWay,
        policy = AirspacePolicy.AVOID,
    )

    @Test
    fun anAirspaceOnTheLineSaysHowFarItIsAndWhetherTheLineRunsThroughIt() {
        // To ten metres under a kilometre, in kilometres from there on.
        assertEquals("Here", AheadWords.place(0.0))
        assertEquals("In 620 m", AheadWords.place(623.0))
        assertEquals("In 990 m", AheadWords.place(994.0))
        assertEquals("In 1.0 km", AheadWords.place(995.0))
        assertEquals("In 4.2 km", AheadWords.place(4_210.0))

        assertNull(AheadWords.onLine(span(1_900.0, inTheWay = false)))
        assertEquals("On your line", AheadWords.onLine(span(1_900.0, inTheWay = true)))
        // An airspace whose height could not be placed is never said to
        // be on the line for certain.
        assertEquals("Possibly on your line", AheadWords.onLine(span(1_900.0, inTheWay = true, uncertain = true)))

        // A row, for someone who cannot see it.
        assertEquals(
            "Class D control zone, GND to 2,000 m. In 1.9 km. On your line.",
            AheadWords.spoken(span(1_900.0, inTheWay = true)),
        )
        assertEquals(
            "Class D control zone, GND to 2,000 m. Its height could not be placed exactly. Here. Possibly on your line.",
            AheadWords.spoken(span(0.0, inTheWay = true, uncertain = true)),
        )
        assertEquals("Class D control zone, GND to 2,000 m. In 620 m.", AheadWords.spoken(span(623.0, inTheWay = false)))
    }

    @Test
    fun theGlidersLineEndsWhereItMeetsTheGroundOrLeavesThePicture() {
        // The ground of the region that comes with the app, and the
        // sample flight nine minutes in, high over the valley.
        val archive = TestSupport.assets.resolve("MapDemo").resolve(MapStyle.BUNDLED_TERRAIN)
        val model = TerrainModel(source = { TerrainModel.Source.Archive(archive) }, folder = null)
        runBlocking { model.load(Coordinate(lat = 47.6642, lon = 11.7967)) }
        val session = FlightSession(TestSupport.newFolder("ahead").path)
        session.useTerrain(model.terrain)
        val replay = Replay(TestSupport.sampleFlight)
        replay.advance(session, 540.0)

        val ahead = checkNotNull(session.airspaceAhead(replay.now()))
        // The ground every hundred metres for ten kilometres, and no
        // airspace: nothing then sounds like "clear".
        assertEquals(101, ahead.ground.size)
        assertEquals("No airspace in the app", ahead.words)
        assertTrue(ahead.spans.isEmpty())
        assertTrue(checkNotNull(ahead.groundWords).startsWith("Your line meets the ground in "))

        // The line is drawn to where it meets the ground, on its own slope.
        val meets = checkNotNull(ahead.lineMeetsGroundM)
        val fall = ahead.altitudeM - ahead.lineEndM
        val (kilometres, height) = checkNotNull(AheadWords.lineEnd(ahead))
        assertEquals(meets / 1000, kilometres, 1e-9)
        assertEquals(ahead.altitudeM - fall * meets / ahead.distanceM, height, 1e-6)
        assertTrue(height < ahead.altitudeM)

        // Without ground on it, a flat line runs to the far end of the
        // picture, and a steep one to the picture's foot.
        val flat = ahead.copy(lineMeetsGroundM = null, lineEndM = ahead.altitudeM - 100, bottomM = ahead.altitudeM - 500)
        assertEquals(ahead.distanceM / 1000 to ahead.altitudeM - 100, AheadWords.lineEnd(flat))
        val steep = ahead.copy(lineMeetsGroundM = null, lineEndM = ahead.altitudeM - 1000, bottomM = ahead.altitudeM - 500)
        val (steepKilometres, steepHeight) = checkNotNull(AheadWords.lineEnd(steep))
        assertEquals(ahead.distanceM / 2000, steepKilometres, 1e-9)
        assertEquals(steep.bottomM, steepHeight, 1e-9)
        // A glider that makes no way on the line has no line to draw.
        assertNull(AheadWords.lineEnd(ahead.copy(lineEndM = ahead.altitudeM)))
    }
}
