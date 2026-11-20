import VentiCore
import XCTest

@testable import Venti

/// A second flight laid beside the one on screen, against the real core:
/// where its glider is at a moment of the first, and what the chart makes
/// room for.
@MainActor
final class FlightComparingTests: XCTestCase {
    private func sample() throws -> (FlightView, FlightAnalysis) {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        let text = try String(contentsOf: url, encoding: .utf8)
        return (try flightFromFile(igcText: text), try flightAnalysisFromFile(igcText: text))
    }

    /// The same flight begun later, a little to the east and higher.
    private func moved(_ flight: FlightView, laterS: Int64, higherM: Double) -> FlightView {
        var other = flight
        other.startUnixS += laterS
        other.maxAltitudeM += higherM
        other.minAltitudeM += higherM
        other.track = flight.track.map { point in
            var moved = point
            moved.lon += 0.002
            moved.altitudeM += higherM
            return moved
        }
        return other
    }

    func testAFlightInTheAirAtTheSameTimeIsLaidOverThisOneByTheClock() throws {
        let (first, analysis) = try sample()
        let other = moved(first, laterS: 600, higherM: 80)

        let comparison = compareFlights(first: first, other: other, firstAnalysis: analysis, otherAnalysis: analysis)
        let compared = ComparedFlight(flight: other, name: "Mara Muster", comparison: comparison)

        XCTAssertTrue(comparison.byTheClock)
        XCTAssertEqual(comparison.offsetS, 600)
        XCTAssertTrue(comparison.words.hasPrefix("In the air together for"), comparison.words)
        XCTAssertEqual(comparison.rows.first?.label, "First fix")

        // Before the other has begun it is nowhere; ten minutes in it is
        // at its first fix; later it is where this flight was ten minutes
        // before, moved as it was.
        XCTAssertNil(compared.point(atSeconds: 300))
        let begun = try XCTUnwrap(compared.point(atSeconds: 600))
        XCTAssertEqual(begun.seconds, 0)
        XCTAssertEqual(begun.altitudeM, first.track[0].altitudeM + 80, accuracy: 0.01)
        let later = try XCTUnwrap(compared.point(atSeconds: 1500))
        XCTAssertEqual(Double(later.seconds), 900, accuracy: 5)
        // After this flight's end the other is still in the air, and
        // after its own end it is nowhere again.
        XCTAssertNotNil(compared.point(atSeconds: UInt32(first.durationS)))
        XCTAssertNil(compared.point(atSeconds: UInt32(first.durationS) + 700))

        // The line under the numbers says how high and how far away.
        let moment = first.track[AltitudeChart.nearestIndex(in: first.track, toSeconds: 1500)]
        let words = try XCTUnwrap(compared.words(at: moment))
        XCTAssertTrue(words.contains(" m · "), words)
        XCTAssertTrue(words.hasSuffix(" away"), words)
        XCTAssertNil(compared.words(at: first.track[0]))

        // The chart makes room for both flights, and the nearest moment
        // is one of this flight's.
        let alone = AltitudeChart.altitudeRange(of: first)
        let both = AltitudeChart.altitudeRange(of: first, beside: other)
        XCTAssertEqual(both.lowerBound, alone.lowerBound)
        XCTAssertGreaterThanOrEqual(both.upperBound, first.maxAltitudeM + 80)
        let nearest = try XCTUnwrap(compared.nearestIndex(in: first.track))
        XCTAssertTrue(first.track.indices.contains(nearest))
        XCTAssertGreaterThanOrEqual(first.track[nearest].seconds, 600)
    }

    func testAFlightOfAnotherDayIsLaidOutFromItsFirstMoment() throws {
        let (first, analysis) = try sample()
        let other = moved(first, laterS: 3 * 86_400, higherM: -200)

        let comparison = compareFlights(first: first, other: other, firstAnalysis: analysis, otherAnalysis: nil)
        let compared = ComparedFlight(flight: other, name: "Your flight of 18 Nov", comparison: comparison)

        XCTAssertFalse(comparison.byTheClock)
        XCTAssertEqual(comparison.offsetS, 0)
        XCTAssertEqual(comparison.words, "Flown 3 days apart. Each is laid out from its first moment.")
        XCTAssertNil(compared.nearestIndex(in: first.track))
        // Both begin at the same moment of the page.
        XCTAssertEqual(compared.point(atSeconds: 0)?.seconds, 0)
        XCTAssertEqual(try XCTUnwrap(compared.point(atSeconds: 900)).seconds, 900)
        // The lower flight makes the chart reach further down.
        let both = AltitudeChart.altitudeRange(of: first, beside: other)
        XCTAssertLessThanOrEqual(both.lowerBound, first.minAltitudeM - 200)
        // The lines that need both analyses are left out.
        XCTAssertFalse(comparison.rows.contains { $0.label == "Climb in thermals" })
        XCTAssertEqual(comparison.rows.first { $0.label == "Highest" }?.lead, .first)
    }
}
