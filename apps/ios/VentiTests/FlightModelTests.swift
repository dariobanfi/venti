import VentiCore
import XCTest

@testable import Venti

/// Checks the app shell against the real core, inside the app on a simulator.
@MainActor
final class FlightModelTests: XCTestCase {
    func testTheBundledSampleFlightLoads() async {
        let model = FlightModel()

        await model.loadSample()

        guard case .loaded(let flight) = model.state else {
            return XCTFail("the sample flight did not load: \(model.state)")
        }
        XCTAssertEqual(flight.pilot, "Sample Pilot")
        XCTAssertEqual(flight.track.count, 1745)
        XCTAssertEqual(flight.durationS, 1744)
    }

    func testAFileThatIsNotAFlightShowsAReadableMessage() async {
        let model = FlightModel()

        await model.open(igcText: "hello")

        guard case .failed(let message) = model.state else {
            return XCTFail("expected a failure, got \(model.state)")
        }
        XCTAssertTrue(message.contains("not a flight file"), message)
    }

    func testFormattingUsesMetricUnits() {
        // Digit grouping and the decimal mark follow the device's region, so
        // only the parts that do not depend on it are checked.
        XCTAssertEqual(Format.altitude(metres: 950.4), "950 m")
        XCTAssertTrue(Format.distance(metres: 15_240).hasSuffix(" km"))
        XCTAssertTrue(Format.verticalSpeed(metresPerSecond: 2.4).hasPrefix("+"))
        XCTAssertTrue(Format.verticalSpeed(metresPerSecond: -1.5).hasSuffix(" m/s"))
    }
}
