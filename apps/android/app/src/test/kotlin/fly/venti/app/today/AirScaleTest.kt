package fly.venti.app.today

import org.junit.Assert.assertEquals
import org.junit.Test

/** How the picture of a day's air lays its heights out, as the iPhone's AirScale does. */
class AirScaleTest {
    @Test
    fun theScaleRunsFromARoundFloorToARoundCeilingOverTheThermals() {
        val scale = AirScale.of(groundM = 1266.0, takeoffM = 1620.0, tops = listOf(3400.0, 3500.0, 3450.0))
        assertEquals(1000.0, scale.floorM, 0.0)
        assertEquals(4000.0, scale.ceilingM, 0.0)
        assertEquals(listOf(1000.0, 1500.0, 2000.0, 2500.0, 3000.0, 3500.0, 4000.0), scale.lines)
        // The floor is at the foot of the picture and the ceiling at its head.
        assertEquals(300f, scale.y(1000.0, 300f), 0.001f)
        assertEquals(0f, scale.y(4000.0, 300f), 0.001f)
    }

    @Test
    fun withoutThermalsTheScaleReachesAKilometreOverTheTakeoff() {
        val scale = AirScale.of(groundM = 812.0, takeoffM = 1550.0, tops = emptyList())
        assertEquals(500.0, scale.floorM, 0.0)
        assertEquals(3500.0, scale.ceilingM, 0.0)
        // A take-off under the model's ground sets the floor.
        assertEquals(500.0, AirScale.of(groundM = 1266.0, takeoffM = 780.0, tops = emptyList()).floorM, 0.0)
        // Without a take-off's height the model's ground stands for it.
        val unknown = AirScale.of(groundM = 1266.0, takeoffM = null, tops = emptyList())
        assertEquals(1000.0, unknown.floorM, 0.0)
        assertEquals(3000.0, unknown.ceilingM, 0.0)
        // The model's next level above that kilometre is in the picture.
        val high = AirScale.of(groundM = 1490.0, takeoffM = null, tops = emptyList(), levels = listOf(1500.0, 1570.0, 1670.0, 3010.0, 4180.0))
        assertEquals(1000.0, high.floorM, 0.0)
        assertEquals(4000.0, high.ceilingM, 0.0)
        // A picture that spans more than 3000 m has a line every 1000 m.
        assertEquals(listOf(0.0, 1000.0, 2000.0, 3000.0, 4000.0, 5000.0), AirScale.of(groundM = 400.0, takeoffM = null, tops = listOf(4200.0)).lines)
    }
}
