package fly.venti.app.fly

import fly.venti.app.TestSupport
import fly.venti.app.design.ClimbBand
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.FlightSession
import fly.venti.core.LiftPointView
import fly.venti.core.LiftView
import fly.venti.core.Replay
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.maplibre.geojson.Point

/**
 * What the cockpit's map shows by itself: how far in it is, the thermals the
 * glider has left, and the picture of the one it is circling in.
 */
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

    @Test
    fun theWholeFlightIsFramedByTheSmallestBoxAroundItsPlaces() {
        val places = listOf(Coordinate(47.66, 11.80), Coordinate(47.69, 11.74), Coordinate(47.67, 11.77))

        assertEquals(Bounds(south = 47.66, west = 11.74, north = 47.69, east = 11.80), CockpitMap.box(places))
    }

    @Test
    fun theCoreSaysWhereToFlyForAnFaiTriangle() {
        val session = FlightSession(TestSupport.newFolder("fly").path)
        val replay = Replay(TestSupport.sampleFlight)
        // Before the flight has gone anywhere there is no side to stand a triangle on.
        replay.advance(session, 60.0)
        session.liveScore()
        assertNull(session.faiAreas())

        // Twenty minutes in, the flight has covered some kilometres in a straight line.
        replay.advance(session, 1140.0)
        session.liveScore()
        val areas = checkNotNull(session.faiAreas())

        assertEquals(2, areas.side.size)
        assertEquals(2, areas.outlines.size)
        for (outline in areas.outlines) {
            // Closed outlines, for a line the map draws round each.
            assertTrue(outline.size >= 4)
            assertEquals(outline.first(), outline.last())
        }
        assertTrue(areas.smallestM < areas.largestM)
    }

    @Test
    fun aThermalTheGliderHasLeftIsMarkedWithItsClimb() {
        val session = FlightSession(TestSupport.newFolder("fly").path)
        val replay = Replay(TestSupport.sampleFlight)

        // Five minutes in, the glider is circling: a thermal is marked once it is left, not before.
        replay.advance(session, 300.0)
        assertTrue(session.view(replay.now()).circling)
        assertTrue(session.view(replay.now()).thermals.isEmpty())

        // Ten minutes in, on the glide after it, the core has the mark for the map.
        replay.advance(session, 300.0)
        val view = session.view(replay.now())
        assertFalse(view.circling)
        val mark = view.thermals.single()
        assertTrue(mark.label, mark.label.startsWith("+2."))

        val feature = CockpitMap.thermalFeature(mark)
        assertEquals(mark.label, feature.getStringProperty(CockpitMap.LABEL))
        val place = feature.geometry() as Point
        assertEquals(mark.position.lat, place.latitude(), 1e-9)
        assertEquals(mark.position.lon, place.longitude(), 1e-9)
    }

    @Test
    fun theThermalsPictureIsADotForEveryPlaceOfTheLastCircles() {
        val session = FlightSession(TestSupport.newFolder("fly").path)
        val replay = Replay(TestSupport.sampleFlight)

        // Five minutes in, circling: the core has the last circles and where the lift is among them.
        replay.advance(session, 300.0)
        val lift = checkNotNull(session.view(replay.now()).lift)
        assertEquals(51, lift.points.size)
        assertEquals("Centred", lift.centre?.words)

        val dots = CockpitMap.liftFeatures(lift)
        assertEquals(lift.points.size, dots.size)
        lift.points.zip(dots).forEach { (point, dot) ->
            val place = dot.geometry() as Point
            assertEquals(point.lat, place.latitude(), 1e-9)
            assertEquals(point.lon, place.longitude(), 1e-9)
            // The colour is the trail's: the band of the climb there.
            assertEquals(ClimbBand.of(point.climbMs).ordinal, dot.getNumberProperty(CockpitMap.BAND).toInt())
            // The size is between the smallest and the largest dot, by the climb beside the picture's mean.
            val radius = dot.getNumberProperty(CockpitMap.RADIUS).toDouble()
            assertTrue("$radius", radius in 2.0..5.0)
            assertEquals(2 + 3 * point.strength, radius, 1e-9)
        }
        // The sample's thermal lifts at 2.4 m/s all around: every dot of it is in a climbing band.
        assertTrue(dots.all { it.getNumberProperty(CockpitMap.BAND).toInt() >= ClimbBand.Climb.ordinal })

        // On the glide after it there is no picture, and nothing of it to draw.
        replay.advance(session, 300.0)
        assertNull(session.view(replay.now()).lift)
        assertTrue(CockpitMap.liftFeatures(null).isEmpty())
    }

    @Test
    fun aDotOfTheThermalsPictureHasTheColourOfItsClimbAndGrowsWithItsStrength() {
        val climbs = listOf(-3.0, -1.0, 0.0, 1.0, 3.0)
        val lift = LiftView(
            points = climbs.mapIndexed { index, climb ->
                LiftPointView(lat = 47.7, lon = 11.8 + index * 0.001, climbMs = climb, strength = index / 4.0)
            },
            centre = null,
            drifted = false,
        )
        val dots = CockpitMap.liftFeatures(lift)
        // One band each, from sinking fast to climbing fast, as the trail has them.
        assertEquals(ClimbBand.entries.map { it.ordinal }, dots.map { it.getNumberProperty(CockpitMap.BAND).toInt() })
        assertEquals(
            listOf(2.0, 2.75, 3.5, 4.25, 5.0),
            dots.map { it.getNumberProperty(CockpitMap.RADIUS).toDouble() },
        )

        // A strength the core would not give draws no dot smaller or larger than the others.
        assertEquals(2.0, CockpitMap.liftDotRadius(-1.0), 0.0)
        assertEquals(5.0, CockpitMap.liftDotRadius(7.0), 0.0)
    }
}
