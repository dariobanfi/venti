package fly.venti.app.task

import fly.venti.app.TestSupport
import fly.venti.app.logbook.LogbookModel
import fly.venti.core.Coordinate
import fly.venti.core.RouteKind
import fly.venti.core.planRoute
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** A route drawn on the map, against the real core. */
class RouteDrawingTest {
    private val start = Coordinate(lat = 47.6642, lon = 11.7967)
    private val east = Coordinate(lat = 47.6642, lon = 11.93)
    private val north = Coordinate(lat = 47.75, lon = 11.86)

    @Test
    fun aTapOnThePlaceBeforeCountsAsTheSamePlace() {
        val one = RouteDrawing.add(emptyList(), start)
        assertEquals(listOf(start), one)
        // 100 m north of it: the same tap.
        assertEquals(one, RouteDrawing.add(one, Coordinate(start.lat + 0.0009, start.lon)))
        assertEquals(listOf(start, east), RouteDrawing.add(one, east))
    }

    @Test
    fun aRouteBackToItsStartIsFoundToBeATriangle() {
        val drawn = listOf(start, east, north)
        assertTrue(RouteDrawing.canGoBackToStart(drawn))
        val closed = drawn + start
        assertFalse(RouteDrawing.canGoBackToStart(closed))
        assertFalse(RouteDrawing.canGoBackToStart(listOf(start, east)))

        val plan = planRoute(closed)
        assertEquals(3, plan.legs.size)
        assertTrue(plan.score!!.kind != RouteKind.FREE_FLIGHT)
        assertTrue(plan.words, plan.words.contains("km along the route."))
        assertTrue(RouteDrawing.legs(plan).startsWith("1  10.0 km E · 2  "))
    }

    @Test
    fun withOneLegTheMapShowsWhereAThirdPlaceMakesAnFaiTriangle() {
        val plan = planRoute(listOf(start, east))
        assertEquals(2, plan.faiAreas.size)
        assertEquals("Tap the map where the route begins.", planRoute(emptyList()).words)
        assertEquals("Tap the next place to fly to.", planRoute(listOf(start)).words)
    }

    @Test
    fun aDrawnRouteIsKeptAsTheTask() {
        val logbook = LogbookModel.open(TestSupport.newFolder("route"), TestSupport.files)
        val tasks = logbook.makeTasks()
        assertFalse(tasks.keepDrawnRoute(listOf(start)))
        assertEquals("A route needs at least two places.", tasks.problem)
        assertTrue(tasks.keepDrawnRoute(listOf(start, east, north, start)))
        assertNull(tasks.problem)
        val task = checkNotNull(tasks.task)
        assertTrue(task.isRoute)
        assertEquals(listOf("Start", "Turnpoint 1", "Turnpoint 2", "Finish"), task.turnpoints.map { it.name })
    }
}
