package fly.venti.app.forecast

import fly.venti.app.groups.ChatOnScreen
import fly.venti.app.nearby.PushNotifications
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Which squares of the grid lie under a region, read from GeoJSON as the core hands the outlines out. */
class RegionShapesTest {
    private val outlines = """
        {"type":"FeatureCollection","features":[
          {"type":"Feature","properties":{"id":"A","name":"A"},
           "geometry":{"type":"MultiPolygon","coordinates":[[[[0,0],[2,0],[2,2],[0,2],[0,0]],[[0.5,0.5],[1.5,0.5],[1.5,1.5],[0.5,1.5],[0.5,0.5]]]]}},
          {"type":"Feature","properties":{"id":"B","name":"B"},
           "geometry":{"type":"Polygon","coordinates":[[[3,0],[4,0],[4,1],[3,1],[3,0]]]}}
        ]}
    """.trimIndent()

    private fun box(lon: Double, lat: Double) = RegionShapes.Box(south = lat - 0.05, west = lon - 0.05, north = lat + 0.05, east = lon + 0.05)

    @Test
    fun aSquareIsUnderARegionWhereItsMiddleIs() {
        val polygons = RegionShapes.polygons(outlines)
        assertEquals(setOf("A", "B"), polygons.keys)
        val cells = listOf(box(0.25, 0.25), box(1.0, 1.0), box(3.5, 0.5), box(5.0, 5.0))
        // In A's ring but not its hole; in B; outside both.
        assertEquals(setOf(0), RegionShapes.cellsUnder(cells, polygons.getValue("A")))
        assertEquals(setOf(0, 2), RegionShapes.cellsUnder(cells, polygons.values.flatten()))
        assertEquals(emptySet<Int>(), RegionShapes.cellsUnder(cells, emptyList()))
    }

    @Test
    fun outlinesThatCannotBeReadHaveNoRegions() {
        assertTrue(RegionShapes.polygons("not json").isEmpty())
    }

    @Test
    fun aGroupsMessageShowsNoBannerWhileItsChatIsOnScreen() {
        try {
            ChatOnScreen.id = null
            assertFalse(PushNotifications.isOnScreen("group_message", "g1"))
            ChatOnScreen.id = "g1"
            assertTrue(PushNotifications.isOnScreen("group_message", "g1"))
            assertFalse(PushNotifications.isOnScreen("group_message", "g2"))
            assertFalse(PushNotifications.isOnScreen("open_plan", "g1"))
        } finally {
            ChatOnScreen.id = null
        }
    }
}
