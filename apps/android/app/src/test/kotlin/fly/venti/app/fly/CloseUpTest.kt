package fly.venti.app.fly

import fly.venti.core.Coordinate
import org.junit.Assert.assertEquals
import org.junit.Test

/** The thermal view's geometry: places as metres from the glider, the span shown and the scale bar. */
class CloseUpTest {
    private val glider = Coordinate(lat = 47.7, lon = 11.8)

    @Test
    fun aPlaceLiesInMetresEastAndNorthOfTheGlider() {
        val north = CloseUp.offset(glider, lat = 47.7 + 0.001, lon = 11.8)
        assertEquals(0.0, north.eastM, 1e-9)
        assertEquals(111.2, north.northM, 0.1)
        // A degree of longitude is shorter by the cosine of the latitude.
        val east = CloseUp.offset(glider, lat = 47.7, lon = 11.8 + 0.001)
        assertEquals(74.9, east.eastM, 0.1)
        assertEquals(0.0, east.northM, 1e-9)
        val southWest = CloseUp.offset(glider, Coordinate(lat = 47.699, lon = 11.799))
        assertEquals(-74.9, southWest.eastM, 0.1)
        assertEquals(-111.2, southWest.northM, 0.1)
    }

    @Test
    fun whileCirclingTheViewIsAsWideAsTheCirclesNeedAndNeverCloserThan150Metres() {
        assertEquals(150.0, CloseUp.span(circling = true, places = emptyList()), 0.0)
        assertEquals(150.0, CloseUp.span(true, listOf(CloseUp.Offset(30.0, -20.0))), 0.0)
        // The farthest place 100 m out: 200 m and some room round it.
        assertEquals(230.0, CloseUp.span(true, listOf(CloseUp.Offset(-100.0, 40.0), CloseUp.Offset(20.0, 60.0))), 1e-9)
    }

    @Test
    fun onTheGlideTheViewSpans600Metres() {
        assertEquals(600.0, CloseUp.span(circling = false, places = listOf(CloseUp.Offset(2000.0, 0.0))), 0.0)
    }

    @Test
    fun theScaleBarIsARoundLengthUpToAQuarterOfTheView() {
        assertEquals(20.0, CloseUp.scaleBar(150.0), 0.0)
        assertEquals(100.0, CloseUp.scaleBar(600.0), 0.0)
        assertEquals(50.0, CloseUp.scaleBar(230.0), 0.0)
        assertEquals(10.0, CloseUp.scaleBar(20.0), 0.0)
        assertEquals("100 m", CloseUp.scaleWords(100.0))
        assertEquals("1 km", CloseUp.scaleWords(1000.0))
    }
}
