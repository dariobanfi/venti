import VentiCore

/// Cuts a track into runs of one climb band each, for colouring.
enum TrackBands {
    struct Run: Equatable {
        let band: ClimbBand
        /// Positions in the track. A run ends on the point the next one
        /// starts on, so the coloured pieces join without a gap.
        let points: ClosedRange<Int>
    }

    /// The step from one point to the next takes the band of the point it
    /// arrives at. A track of fewer than two points has no runs.
    static func runs(in track: [TrackPoint]) -> [Run] {
        guard track.count >= 2 else { return [] }

        var runs: [Run] = []
        var start = 0
        var band = ClimbBand(metresPerSecond: track[1].climbMs)
        for index in track.indices.dropFirst(2) {
            let next = ClimbBand(metresPerSecond: track[index].climbMs)
            if next != band {
                runs.append(Run(band: band, points: start...(index - 1)))
                start = index - 1
                band = next
            }
        }
        runs.append(Run(band: band, points: start...(track.count - 1)))
        return runs
    }
}
