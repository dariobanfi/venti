import VentiCore

/// Cuts a track into runs of one climb band each, for colouring.
enum TrackBands {
    struct Run: Equatable {
        let band: ClimbBand
        /// Positions in the track. A run ends on the point the next one
        /// starts on, so the coloured pieces join without a gap.
        let points: ClosedRange<Int>
    }

    /// The runs of a track.
    static func runs(in track: [TrackPoint]) -> [Run] {
        runs(climbs: track.map(\.climbMs))
    }

    /// The runs of a line whose points have these climb rates. The step
    /// from one point to the next takes the band of the point it arrives
    /// at. Fewer than two points have no runs.
    static func runs(climbs: [Double]) -> [Run] {
        guard climbs.count >= 2 else { return [] }

        var runs: [Run] = []
        var start = 0
        var band = ClimbBand(metresPerSecond: climbs[1])
        for index in climbs.indices.dropFirst(2) {
            let next = ClimbBand(metresPerSecond: climbs[index])
            if next != band {
                runs.append(Run(band: band, points: start...(index - 1)))
                start = index - 1
                band = next
            }
        }
        runs.append(Run(band: band, points: start...(climbs.count - 1)))
        return runs
    }
}
