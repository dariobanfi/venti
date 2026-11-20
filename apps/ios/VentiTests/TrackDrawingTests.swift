import SwiftUI
import VentiCore
import XCTest

@testable import Venti

/// The small pieces of logic the shell needs to draw a track.
@MainActor
final class TrackDrawingTests: XCTestCase {
    /// One point every two seconds with the given climb rates.
    private func track(climbs: [Double]) -> [TrackPoint] {
        climbs.enumerated().map { index, climb in
            TrackPoint(
                lat: 47.0 + Double(index) * 0.0001,
                lon: 11.0,
                altitudeM: 1000,
                seconds: UInt32(index * 2),
                climbMs: climb
            )
        }
    }

    func testClimbBandsSplitAtTheDocumentedRates() {
        // The design language's vario scale: -3, -1.5, -0.5, +0.5, +1.5, +3.
        XCTAssertEqual(ClimbBand(metresPerSecond: -3.1), .strongSink)
        XCTAssertEqual(ClimbBand(metresPerSecond: -3), .sink)
        XCTAssertEqual(ClimbBand(metresPerSecond: -2), .sink)
        XCTAssertEqual(ClimbBand(metresPerSecond: -1.5), .lightSink)
        XCTAssertEqual(ClimbBand(metresPerSecond: -1.2), .lightSink)
        XCTAssertEqual(ClimbBand(metresPerSecond: -0.5), .level)
        XCTAssertEqual(ClimbBand(metresPerSecond: 0.4), .level)
        XCTAssertEqual(ClimbBand(metresPerSecond: 0.5), .lightClimb)
        XCTAssertEqual(ClimbBand(metresPerSecond: 1.5), .climb)
        XCTAssertEqual(ClimbBand(metresPerSecond: 2), .climb)
        XCTAssertEqual(ClimbBand(metresPerSecond: 3), .strongClimb)
        XCTAssertEqual(ClimbBand.allCases.count, VentiTheme.light.vario.count)
    }

    func testATrackIsCutIntoRunsThatShareTheirEndPoints() {
        let runs = TrackBands.runs(in: track(climbs: [0, -1, -1, 3, 3, 3, -1]))

        XCTAssertEqual(
            runs,
            [
                TrackBands.Run(band: .lightSink, points: 0...2),
                TrackBands.Run(band: .strongClimb, points: 2...5),
                TrackBands.Run(band: .lightSink, points: 5...6),
            ]
        )
    }

    func testShortTracksHaveNoRunsOrOne() {
        XCTAssertTrue(TrackBands.runs(in: []).isEmpty)
        XCTAssertTrue(TrackBands.runs(in: track(climbs: [1])).isEmpty)
        XCTAssertEqual(
            TrackBands.runs(in: track(climbs: [0, 1])),
            [TrackBands.Run(band: .lightClimb, points: 0...1)]
        )
    }

    func testTheCursorSnapsToTheNearestMoment() {
        // Points at 0, 2, 4, 6 and 8 seconds.
        let track = track(climbs: [0, 0, 0, 0, 0])

        XCTAssertEqual(AltitudeChart.nearestIndex(in: track, toSeconds: -5), 0)
        XCTAssertEqual(AltitudeChart.nearestIndex(in: track, toSeconds: 0.9), 0)
        XCTAssertEqual(AltitudeChart.nearestIndex(in: track, toSeconds: 1.1), 1)
        XCTAssertEqual(AltitudeChart.nearestIndex(in: track, toSeconds: 6), 3)
        XCTAssertEqual(AltitudeChart.nearestIndex(in: track, toSeconds: 100), 4)
        XCTAssertEqual(AltitudeChart.nearestIndex(in: [], toSeconds: 3), 0)
    }

    func testAThumbnailFitsItsAreaWithoutDistortion() throws {
        // Twice as far north-south as east-west, at the equator.
        let outline = [
            Coordinate(lat: 0, lon: 0),
            Coordinate(lat: 0.02, lon: 0.01),
        ]
        let area = CGRect(x: 10, y: 10, width: 100, height: 100)

        let box = try XCTUnwrap(TrackShape.path(outline, in: area)).boundingRect

        XCTAssertEqual(box.height, 100, accuracy: 0.01)
        XCTAssertEqual(box.width, 50, accuracy: 0.01)
        XCTAssertEqual(box.midX, area.midX, accuracy: 0.01)
        XCTAssertNil(TrackShape.path([Coordinate(lat: 0, lon: 0)], in: area))
    }
}
