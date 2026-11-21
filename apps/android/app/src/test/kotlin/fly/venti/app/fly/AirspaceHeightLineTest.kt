package fly.venti.app.fly

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Where the height line beside the large airspace reading puts the glider and the airspace. */
class AirspaceHeightLineTest {
    @Test
    fun aFloorAboveIsDrawnAboveTheGliderWithRoomOverIt() {
        // Floor 400 m above, ceiling 1,500 m above.
        val scale = AirspaceHeightLine.of(altitudeM = 1600.0, floorM = 2000.0, ceilingM = 3100.0)
        // The glider in the middle, the scale reaching past the ceiling.
        assertEquals(0.5f, scale.glider, 0.001f)
        assertTrue(scale.floor > scale.glider)
        assertTrue(scale.ceiling > scale.floor && scale.ceiling < 1f)
        assertFalse(scale.floorRunsOff)
        assertFalse(scale.ceilingRunsOff)
        assertEquals(1600.0 - 1500 * 1.25, scale.lowM, 0.001)
    }

    @Test
    fun aCeilingBelowIsDrawnUnderTheGlider() {
        val scale = AirspaceHeightLine.of(altitudeM = 2000.0, floorM = 600.0, ceilingM = 1750.0)
        assertTrue(scale.ceiling < scale.glider)
        assertTrue(scale.floor < scale.ceiling)
        assertFalse(scale.floorRunsOff)
    }

    @Test
    fun theGliderInTheAirspaceLiesBetweenItsLimits() {
        val scale = AirspaceHeightLine.of(altitudeM = 1200.0, floorM = 900.0, ceilingM = 1500.0)
        assertTrue(scale.floor < scale.glider && scale.glider < scale.ceiling)
        // Limits 300 m away: the scale keeps its least reach, 500 m each way.
        assertEquals(700.0, scale.lowM, 0.001)
        assertEquals(1700.0, scale.highM, 0.001)
    }

    @Test
    fun anUnlimitedCeilingAndAFloorFarBelowRunOffTheScale() {
        // A floor at the ground without terrain: the core puts it at -500 m.
        val scale = AirspaceHeightLine.of(altitudeM = 1800.0, floorM = -500.0, ceilingM = null)
        assertTrue(scale.ceilingRunsOff)
        assertTrue(scale.floorRunsOff)
        assertEquals(1f, scale.ceiling)
        assertEquals(0f, scale.floor)
        // The far floor does not stretch the scale: it keeps its least reach.
        assertEquals(1300.0, scale.lowM, 0.001)
        assertEquals(0.5f, scale.glider, 0.001f)
    }

    @Test
    fun theScaleReachesNoFurtherThanItsMost() {
        val scale = AirspaceHeightLine.of(altitudeM = 1000.0, floorM = 3900.0, ceilingM = 5000.0)
        // The floor 2,900 m above is on the scale; the ceiling 4,000 m above is not.
        assertEquals(1000.0 + AirspaceHeightLine.MOST_M, scale.highM, 0.001)
        assertFalse(scale.floor >= 1f)
        assertTrue(scale.ceilingRunsOff)
        assertEquals(1f, scale.ceiling)
    }
}
