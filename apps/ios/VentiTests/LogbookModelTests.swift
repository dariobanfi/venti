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

    func testAFolderOfFlightsIsImportedWholeAndSaysWhatItDid() async throws {
        let logbook = try makeLogbook()
        let sample = try String(
            contentsOf: try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc")), encoding: .utf8
        )
        // The sample flight as flown on another day: another flight.
        func flown(on day: String) -> String {
            sample.replacingOccurrences(of: "HFDTEDATE:040726", with: "HFDTEDATE:\(day)")
        }
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-history-\(UUID().uuidString)", isDirectory: true)
        let earlier = folder.appendingPathComponent("2025", isDirectory: true)
        try FileManager.default.createDirectory(at: earlier, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        try flown(on: "040726").write(to: folder.appendingPathComponent("a.igc"), atomically: true, encoding: .utf8)
        try flown(on: "050726").write(to: folder.appendingPathComponent("b.IGC"), atomically: true, encoding: .utf8)
        try flown(on: "060725").write(to: earlier.appendingPathComponent("c.igc"), atomically: true, encoding: .utf8)
        // What else lies in such a folder is not a list of failures.
        try "hello".write(to: folder.appendingPathComponent("notes.txt"), atomically: true, encoding: .utf8)

        XCTAssertEqual(
            LogbookModel.flightFiles(in: [folder]).map(\.lastPathComponent), ["c.igc", "a.igc", "b.IGC"]
        )

        let report = await logbook.importFiles(at: [folder])
        XCTAssertEqual(report.added.count, 3)
        XCTAssertTrue(report.failures.isEmpty)
        XCTAssertEqual(report.notice?.title, "3 flights added")
        XCTAssertEqual(report.notice?.text, "They are in your logbook, on this iPhone.")
        XCTAssertNil(logbook.importProgress)
        XCTAssertEqual(logbook.view.totals.flights, 3)

        // The same folder a month later, with one more flight in it.
        try flown(on: "070726").write(to: folder.appendingPathComponent("d.igc"), atomically: true, encoding: .utf8)
        let again = await logbook.importFiles(at: [folder])
        XCTAssertEqual(again.added.count, 1)
        XCTAssertEqual(again.notice?.title, "1 flight added")
        XCTAssertEqual(again.notice?.text, "It is in your logbook, on this iPhone. 3 were in your logbook already.")
        XCTAssertEqual(logbook.view.totals.flights, 4)

        // A folder without flights says so.
        let photos = folder.appendingPathComponent("photos", isDirectory: true)
        try FileManager.default.createDirectory(at: photos, withIntermediateDirectories: true)
        let none = await logbook.importFiles(at: [photos])
        XCTAssertEqual(none.notice?.title, "No flights found")
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
