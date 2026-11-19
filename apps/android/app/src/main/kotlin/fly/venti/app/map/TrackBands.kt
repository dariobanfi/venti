package fly.venti.app.map

import fly.venti.app.design.ClimbBand
import fly.venti.core.Coordinate
import fly.venti.core.FlightScore
import fly.venti.core.RouteKind
import fly.venti.core.TrackPoint

/** Cuts a track into runs of one climb band each, for colouring. */
object TrackBands {
    data class Run(
        val band: ClimbBand,
        /**
         * Positions in the track. A run ends on the point the next one
         * starts on, so the coloured pieces join without a gap.
         */
        val points: IntRange,
    )

    /** The runs of a track. */
    fun runs(track: List<TrackPoint>): List<Run> = runsOfClimbs(track.map { it.climbMs })

    /**
     * The runs of a line whose points have these climb rates. The step from
     * one point to the next takes the band of the point it arrives at.
     * Fewer than two points have no runs.
     */
    fun runsOfClimbs(climbs: List<Double>): List<Run> {
        if (climbs.size < 2) return emptyList()

        val runs = mutableListOf<Run>()
        var start = 0
        var band = ClimbBand.of(climbs[1])
        for (index in 2 until climbs.size) {
            val next = ClimbBand.of(climbs[index])
            if (next != band) {
                runs += Run(band, start..index - 1)
                start = index - 1
                band = next
            }
        }
        runs += Run(band, start..climbs.lastIndex)
        return runs
    }
}

/** The scored route as the lines to draw. */
object RouteShape {
    /**
     * A triangle's route is start, three turnpoints, finish: the triangle
     * itself is drawn closed, with the way to it and from it. Any other
     * route is one line through its points.
     */
    fun pieces(score: FlightScore): List<List<Coordinate>> {
        val points = score.route.map { it.position }
        return if (score.kind != RouteKind.FREE_FLIGHT && points.size == 5) {
            listOf(
                listOf(points[0], points[1]),
                listOf(points[1], points[2], points[3], points[1]),
                listOf(points[3], points[4]),
            )
        } else {
            listOf(points)
        }
    }
}
