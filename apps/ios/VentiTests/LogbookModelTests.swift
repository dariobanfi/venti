import VentiCore
import XCTest

@testable import Venti

/// Checks the app shell against the real core, inside the app on a simulator.
@MainActor
final class LogbookModelTests: XCTestCase {
    private func makeLogbook() throws -> LogbookModel {
        try LogbookModel.open(
            folder: FileManager.default.temporaryDirectory
                .appendingPathComponent("venti-test-\(UUID().uuidString)", isDirectory: true)
        )
    }

    func testTheBundledSampleFlightIsImportedAndOpens() async throws {
        let logbook = try makeLogbook()
        XCTAssertTrue(logbook.view.years.isEmpty)

        let report = await logbook.addSample()

        XCTAssertEqual(report.added.count, 1)
        XCTAssertNil(report.notice)
        XCTAssertEqual(logbook.view.totals.flights, 1)
        let entry = try XCTUnwrap(logbook.view.years.first?.flights.first)
        XCTAssertEqual(entry.glider, "Sample Wing")
        XCTAssertGreaterThanOrEqual(entry.outline.count, 2)

        let flight = try await logbook.flight(id: entry.id)
        XCTAssertEqual(flight.pilot, "Sample Pilot")
        XCTAssertEqual(flight.track.count, 1745)
        XCTAssertEqual(flight.durationS, 1744)
        XCTAssertLessThanOrEqual(flight.profile.count, 400)
    }

    func testImportingTheSameFlightAgainSaysSoAndAddsNothing() async throws {
        let logbook = try makeLogbook()
        _ = await logbook.addSample()

        let report = await logbook.addSample()

        XCTAssertTrue(report.added.isEmpty)
        XCTAssertEqual(report.notice?.title, "Already in your logbook")
        XCTAssertEqual(logbook.view.totals.flights, 1)
    }

    func testAFileThatIsNotAFlightIsReportedByName() async throws {
        let logbook = try makeLogbook()
        let file = FileManager.default.temporaryDirectory
            .appendingPathComponent("notes-\(UUID().uuidString).igc")
        try "hello".write(to: file, atomically: true, encoding: .utf8)
        defer { try? FileManager.default.removeItem(at: file) }

        let report = await logbook.importFiles(at: [file])

        XCTAssertEqual(report.failures.count, 1)
        let notice = try XCTUnwrap(report.notice)
        XCTAssertEqual(notice.title, "1 file could not be imported")
        XCTAssertTrue(notice.text.contains(file.lastPathComponent), notice.text)
        XCTAssertTrue(notice.text.contains("not a flight file"), notice.text)
        XCTAssertTrue(logbook.view.years.isEmpty)
    }

    func testADeletedFlightLeavesTheLogbook() async throws {
        let logbook = try makeLogbook()
        let report = await logbook.addSample()
        let id = try XCTUnwrap(report.added.first)

        try logbook.delete(id: id)

        XCTAssertTrue(logbook.view.years.isEmpty)
    }

    func testFormattingUsesMetricUnits() {
        // Digit grouping and the decimal mark follow the device's region, so
        // only the parts that do not depend on it are checked.
        XCTAssertEqual(Format.metres(950.4), "950")
        XCTAssertEqual(Format.hoursMinutes(seconds: 1744), "0:29")
        XCTAssertEqual(Format.hoursMinutes(seconds: 5 * 3600 + 42 * 60), "5:42")
        XCTAssertTrue(Format.climb(metresPerSecond: 2.4).hasPrefix("+"))
        XCTAssertFalse(Format.climb(metresPerSecond: -1.5).hasPrefix("+"))
        XCTAssertEqual(Format.flights(1), "1 flight")
        XCTAssertEqual(Format.stopwatch(seconds: 754), "0:12:34")
        XCTAssertEqual(Format.kilometresPerHour(metresPerSecond: 10), "36")
        XCTAssertTrue(Format.signedMetres(244).hasPrefix("+"))
        XCTAssertEqual(
            [0, 44, 90, 200, 270, 359].map(Format.compassPoint(degrees:)),
            ["N", "NE", "E", "S", "W", "N"]
        )
        XCTAssertEqual(Format.flights(3), "3 flights")
    }
}
