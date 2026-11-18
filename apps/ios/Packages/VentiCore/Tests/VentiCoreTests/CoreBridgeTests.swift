import Foundation
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

    private var folder: URL!

    override func setUpWithError() throws {
        folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-bridge-test-\(UUID().uuidString)", isDirectory: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: folder)
    }

    private func makeCore() throws -> Core {
        try Core(dataFolder: folder.path)
    }

    func testTheCoreReportsItsVersion() throws {
        XCTAssertFalse(try makeCore().version().isEmpty)
    }

    func testAFlightFileCrossesTheBridge() throws {
        let core = try makeCore()
        let imported = try core.importFlight(igcText: Self.threeFixes)
        XCTAssertFalse(imported.alreadyInLogbook)

        let flight = try core.flight(id: imported.id)

        XCTAssertEqual(flight.id, imported.id)
        XCTAssertEqual(flight.recorder, "XVE 000")
        XCTAssertEqual(flight.durationS, 20)
        XCTAssertEqual(flight.maxAltitudeM, 1620)
        XCTAssertEqual(flight.minAltitudeM, 1618)
        XCTAssertEqual(flight.track.count, 3)
        XCTAssertEqual(flight.track[0].lat, 47 + 39.852 / 60, accuracy: 1e-9)
        XCTAssertEqual(flight.track[0].lon, 11 + 47.802 / 60, accuracy: 1e-9)
        XCTAssertEqual(flight.track[2].seconds, 20)
        XCTAssertEqual(flight.track[1].climbMs, -0.1, accuracy: 1e-9)
        XCTAssertEqual(flight.profile.map(\.altitudeM), [1620, 1619, 1618])
        XCTAssertNil(flight.pilot)
    }

    func testTheLogbookListsTotalsAndForgetsADeletedFlight() throws {
        let core = try makeCore()
        XCTAssertTrue(core.logbook().years.isEmpty)

        let id = try core.importFlight(igcText: Self.threeFixes).id
        XCTAssertTrue(try core.importFlight(igcText: Self.threeFixes).alreadyInLogbook)

        let logbook = core.logbook()
        XCTAssertEqual(logbook.totals.flights, 1)
        XCTAssertEqual(logbook.totals.airtimeS, 20)
        XCTAssertEqual(logbook.years.map(\.year), [2026])
        XCTAssertEqual(logbook.years[0].flights.map(\.id), [id])
        XCTAssertEqual(logbook.years[0].flights[0].outline.count, 3)

        try core.deleteFlight(id: id)
        XCTAssertTrue(core.logbook().years.isEmpty)
    }

    func testTheLogbookIsStillThereWhenTheCoreIsCreatedAgain() throws {
        let id = try makeCore().importFlight(igcText: Self.threeFixes).id
        XCTAssertEqual(try makeCore().logbook().years.first?.flights.first?.id, id)
    }

    func testAFileThatIsNotAFlightThrowsAReadableError() throws {
        let core = try makeCore()
        XCTAssertThrowsError(try core.importFlight(igcText: "hello")) { error in
            XCTAssertTrue(error is CoreError)
            XCTAssertTrue(error.localizedDescription.contains("not a flight file"))
        }
        XCTAssertThrowsError(try core.flight(id: "nothing")) { error in
            XCTAssertTrue(error.localizedDescription.contains("no longer in the logbook"))
        }
    }

    func testTheCoreCanBeSharedAcrossTasks() async throws {
        let core = try makeCore()
        let id = try core.importFlight(igcText: Self.threeFixes).id

        let counts = try await withThrowingTaskGroup(of: Int.self) { group in
            for _ in 0..<8 {
                group.addTask {
                    try core.flight(id: id).track.count
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
