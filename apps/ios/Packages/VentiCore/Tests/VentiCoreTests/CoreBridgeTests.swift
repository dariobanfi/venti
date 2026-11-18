import VentiCore
import XCTest

/// Calls the Rust core through its generated Swift interface.
///
/// These run on the Mac with `swift test`, without a simulator, so they are
/// the fast check that the bridge works under Swift 6's concurrency rules.
final class CoreBridgeTests: XCTestCase {
    /// Three fixes, ten seconds apart, in the IGC flight-file format.
    private static let threeFixes = [
        "AXVE000",
        "HFDTEDATE:040726,01",
        "B1030004739852N01147802EA0158501620",
        "B1030104739860N01147790EA0158401619",
        "B1030204739868N01147778EA0158301618",
        "",
    ].joined(separator: "\r\n")

    func testTheCoreReportsItsVersion() {
        XCTAssertFalse(Core().version().isEmpty)
    }

    func testAFlightFileCrossesTheBridge() throws {
        let flight = try Core().openFlight(igcText: Self.threeFixes)

        XCTAssertEqual(flight.recorder, "XVE 000")
        XCTAssertEqual(flight.durationS, 20)
        XCTAssertEqual(flight.maxAltitudeM, 1620)
        XCTAssertEqual(flight.minAltitudeM, 1618)
        XCTAssertEqual(flight.track.count, 3)
        XCTAssertEqual(flight.track[0].lat, 47 + 39.852 / 60, accuracy: 1e-9)
        XCTAssertEqual(flight.track[0].lon, 11 + 47.802 / 60, accuracy: 1e-9)
        XCTAssertEqual(flight.track[2].seconds, 20)
        XCTAssertNil(flight.pilot)
    }

    func testAFileThatIsNotAFlightThrowsAReadableError() {
        XCTAssertThrowsError(try Core().openFlight(igcText: "hello")) { error in
            XCTAssertTrue(error is CoreError)
            XCTAssertTrue(error.localizedDescription.contains("not a flight file"))
        }
    }

    func testTheCoreCanBeSharedAcrossTasks() async throws {
        let core = Core()
        let text = Self.threeFixes

        let counts = try await withThrowingTaskGroup(of: Int.self) { group in
            for _ in 0..<8 {
                group.addTask {
                    try core.openFlight(igcText: text).track.count
                }
            }
            var counts: [Int] = []
            for try await count in group {
                counts.append(count)
            }
            return counts
        }

        XCTAssertEqual(counts, Array(repeating: 3, count: 8))
    }
}
