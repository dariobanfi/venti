package fly.venti.app.fly

import fly.venti.app.TestSupport
import fly.venti.core.FlightSession
import fly.venti.core.Replay
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.maplibre.geojson.Point

/** What the cockpit's map shows by itself: how far in it is, and the thermals the glider has left. */
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
}
