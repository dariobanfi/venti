package fly.venti.app.fly

import org.junit.Assert.assertEquals
import org.junit.Test

/** How far in the cockpit's map is, by itself. */
class CockpitMapTest {
    /** A map of 400 by 360 density-independent pixels at the latitude of the Tegernsee. */
    private fun zoom(circling: Boolean, metres: Double?): Double =
        CockpitMap.zoom(circling, showing = metres, latitude = 47.7, width = 400f, height = 360f)

    @Test
    fun theMapZoomsOutAsFarAsItTakesToShowTheAirspaceOfAWarning() {
        // Without a warning the kind of flying decides.
        assertEquals(13.0, zoom(false, null), 0.0)
        assertEquals(15.5, zoom(true, null), 0.0)
        // Two kilometres do not fit at the cruising zoom; one does.
        assertEquals(11.5, zoom(false, 2000.0), 0.0)
        assertEquals(13.0, zoom(false, 900.0), 0.0)
        // In a thermal the map is close in, and steps back for a zone 500 m away.
        assertEquals(13.5, zoom(true, 500.0), 0.0)
        assertEquals(15.5, zoom(true, 100.0), 0.0)
        // Never out to where the valley is a dot.
        assertEquals(10.0, zoom(false, 80_000.0), 0.0)
        // Inside an airspace the way out is no distance at all.
        assertEquals(13.0, zoom(false, 0.0), 0.0)
    }

    @Test
    fun beforeTheMapHasASizeNothingIsWorkedOutFromIt() {
        assertEquals(13.0, CockpitMap.zoom(false, showing = 2000.0, latitude = 47.7, width = 0f, height = 0f), 0.0)
    }
}
