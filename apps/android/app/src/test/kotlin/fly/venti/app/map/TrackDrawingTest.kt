package fly.venti.app.map

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import fly.venti.app.design.ClimbBand
import fly.venti.app.flight.TrackSearch
import fly.venti.app.logbook.TrackShape
import fly.venti.core.Coordinate
import fly.venti.core.FlightMoment
import fly.venti.core.FlightScore
import fly.venti.core.RouteKind
import fly.venti.core.TrackPoint
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The small pieces of logic the shell needs to draw a track. */
class TrackDrawingTest {
    /** One point every two seconds with the given climb rates. */
    private fun track(vararg climbs: Double): List<TrackPoint> =
        climbs.mapIndexed { index, climb ->
            TrackPoint(
                lat = 47.0 + index * 0.0001,
                lon = 11.0,
                altitudeM = 1000.0,
                seconds = (index * 2).toUInt(),
                climbMs = climb,
            )
        }

    @Test
    fun climbBandsSplitAtTheDocumentedRates() {
        // The design pack's breaks: a value on a break belongs to the step above it.
        assertEquals(ClimbBand.StrongSink, ClimbBand.of(-3.1))
        assertEquals(ClimbBand.Sink, ClimbBand.of(-3.0))
        assertEquals(ClimbBand.Sink, ClimbBand.of(-2.0))
        assertEquals(ClimbBand.LightSink, ClimbBand.of(-1.2))
        assertEquals(ClimbBand.Level, ClimbBand.of(-0.5))
        assertEquals(ClimbBand.Level, ClimbBand.of(0.4))
        assertEquals(ClimbBand.Climb, ClimbBand.of(0.5))
        assertEquals(ClimbBand.GoodClimb, ClimbBand.of(2.0))
        assertEquals(ClimbBand.StrongClimb, ClimbBand.of(3.0))
    }

    @Test
    fun theClimbColoursAreTheOnesOfTheDesign() {
        assertEquals(
            // The pack's vario scale, light: teal for sink, grey, coral and red for lift.
            listOf(0x0A5F6C, 0x1F93A1, 0x86C9CC, 0xCBC6C2, 0xF3A682, 0xE56954, 0xB02A45),
            ClimbBand.entries.map { it.rgb },
        )
    }

    @Test
    fun aTrackIsCutIntoRunsThatShareTheirEndPoints() {
        val runs = TrackBands.runs(track(0.0, -1.0, -1.0, 3.0, 3.0, 3.0, -1.0))

        assertEquals(
            listOf(
                TrackBands.Run(ClimbBand.LightSink, 0..2),
                TrackBands.Run(ClimbBand.StrongClimb, 2..5),
                TrackBands.Run(ClimbBand.LightSink, 5..6),
            ),
            runs,
        )
    }

    @Test
    fun shortTracksHaveNoRunsOrOne() {
        assertTrue(TrackBands.runs(emptyList()).isEmpty())
        assertTrue(TrackBands.runs(track(1.0)).isEmpty())
        assertEquals(listOf(TrackBands.Run(ClimbBand.Climb, 0..1)), TrackBands.runs(track(0.0, 1.0)))
    }

    @Test
    fun theCursorSnapsToTheNearestMoment() {
        // Points at 0, 2, 4, 6 and 8 seconds.
        val track = track(0.0, 0.0, 0.0, 0.0, 0.0)

        assertEquals(0, TrackSearch.nearestIndex(track, -5.0))
        assertEquals(0, TrackSearch.nearestIndex(track, 0.9))
        assertEquals(1, TrackSearch.nearestIndex(track, 1.1))
        assertEquals(3, TrackSearch.nearestIndex(track, 6.0))
        assertEquals(4, TrackSearch.nearestIndex(track, 100.0))
        assertEquals(0, TrackSearch.nearestIndex(emptyList(), 3.0))
    }

    @Test
    fun aThumbnailFitsItsAreaWithoutDistortion() {
        // Twice as far north-south as east-west, at the equator.
        val outline = listOf(Coordinate(lat = 0.0, lon = 0.0), Coordinate(lat = 0.02, lon = 0.01))
        val area = Rect(Offset(10f, 10f), Size(100f, 100f))

        val points = checkNotNull(TrackShape.points(outline, area))

        val height = points.maxOf { it.y } - points.minOf { it.y }
        val width = points.maxOf { it.x } - points.minOf { it.x }
        assertEquals(100f, height, 0.01f)
        assertEquals(50f, width, 0.01f)
        assertEquals(area.center.x, (points.maxOf { it.x } + points.minOf { it.x }) / 2, 0.01f)
        // North is up: the northern end is the upper one.
        assertTrue(points[1].y < points[0].y)
        assertNull(TrackShape.points(listOf(Coordinate(0.0, 0.0)), area))
    }

    @Test
    fun theOverviewFramesEveryFlight() {
        val box = checkNotNull(
            OverviewMap.box(
                listOf(
                    listOf(Coordinate(47.6, 11.7), Coordinate(47.7, 11.8)),
                    listOf(Coordinate(46.9, 12.1)),
                ),
            ),
        )

        assertEquals(46.9, box.south, 0.0)
        assertEquals(47.7, box.north, 0.0)
        assertEquals(11.7, box.west, 0.0)
        assertEquals(12.1, box.east, 0.0)
        assertNull(OverviewMap.box(emptyList()))
    }

    @Test
    fun aTrianglesRouteIsDrawnClosedWithTheWayToItAndFromIt() {
        fun moment(index: Int) = FlightMoment(
            trackIndex = index.toUInt(),
            seconds = index.toUInt(),
            position = Coordinate(47.0 + index, 11.0),
            altitudeM = 1000.0,
        )
        fun score(kind: RouteKind, points: Int) = FlightScore(
            kind = kind,
            closed = true,
            distanceM = 1000.0,
            points = 1.4,
            pointsPerKm = 1.4,
            route = List(points, ::moment),
            closingM = 10.0,
        )

        val triangle = RouteShape.pieces(score(RouteKind.FAI_TRIANGLE, 5))
        assertEquals(listOf(2, 4, 2), triangle.map { it.size })
        // The triangle ends on the turnpoint it began on.
        assertEquals(triangle[1].first(), triangle[1].last())

        assertEquals(listOf(5), RouteShape.pieces(score(RouteKind.FREE_FLIGHT, 5)).map { it.size })
    }
}
