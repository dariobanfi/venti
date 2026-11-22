import VentiCore
import XCTest

@testable import Venti

/// Counts what a test's bookmarks were asked.
private final class Asked: @unchecked Sendable {
    var made = 0
    /// What `resolve` says of every bookmark from now on.
    var isStale = false
    var leadsNowhere = false
    var cannotBeMade = false
}

private struct Gone: Error {}

/// The folder new flights come in from by themselves, against the real
/// core and real folders in the simulator's temporary folder.
@MainActor
final class WatchedFolderTests: XCTestCase {
    /// Bookmarks that are the folder's path, and say what a test tells
    /// them to: stale, or leading nowhere.
    private func bookmarks(_ asked: Asked) -> WatchedFolder.Bookmarks {
        WatchedFolder.Bookmarks(
            make: { folder in
                if asked.cannotBeMade {
                    throw Gone()
                }
                asked.made += 1
                return Data("\(asked.made)|\(folder.path)".utf8)
            },
            resolve: { bookmark in
                if asked.leadsNowhere {
                    throw Gone()
                }
                let path = String(decoding: bookmark, as: UTF8.self).split(separator: "|", maxSplits: 1)[1]
                return (URL(fileURLWithPath: String(path), isDirectory: true), asked.isStale)
            }
        )
    }

    private func makeLogbook() throws -> LogbookModel {
        try LogbookModel.open(
            folder: FileManager.default.temporaryDirectory
                .appendingPathComponent("venti-test-\(UUID().uuidString)", isDirectory: true)
        )
    }

    private func makeStore() throws -> UserDefaults {
        try XCTUnwrap(UserDefaults(suiteName: "venti-watched-test-\(UUID().uuidString)"))
    }

    /// An empty folder called "Tracklogs", as a vario's might be.
    private func makeFolder() throws -> URL {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-watched-\(UUID().uuidString)", isDirectory: true)
            .appendingPathComponent("Tracklogs", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    /// The sample flight, and the same flight on other days of July 2026,
    /// which the logbook takes for other flights.
    private func flights() throws -> [String] {
        let sample = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        return [try String(contentsOf: sample, encoding: .utf8)] + LaunchOptions.sampleLogbook()
    }

    private func write(_ text: String, as name: String, in folder: URL) throws {
        let file = folder.appendingPathComponent(name)
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try text.write(to: file, atomically: true, encoding: .utf8)
    }

    func testANewFileIsImportedOnceAndAFileSeenBeforeIsLeftAlone() async throws {
        let (logbook, store, folder, flights) = (try makeLogbook(), try makeStore(), try makeFolder(), try flights())
        let watched = WatchedFolder(logbook: logbook, store: store, bookmarks: bookmarks(Asked()))
        XCTAssertNil(watched.name)
        try write(flights[0], as: "first.igc", in: folder)
        // Not a flight file by its name: a folder of a vario holds more.
        try write("not a flight", as: "notes.txt", in: folder)

        // Naming the folder brings in what is in it.
        let first = try await watched.watch(folder)
        XCTAssertEqual(watched.name, "Tracklogs")
        XCTAssertEqual(first?.added.count, 1)
        XCTAssertEqual(first?.failures.count, 0)
        XCTAssertEqual(logbook.view.totals.flights, 1)

        // Coming to the front again: nothing is new, nothing is read.
        let again = await watched.bringInNew()
        XCTAssertNil(again)
        XCTAssertEqual(logbook.view.totals.flights, 1)

        // A new file, in a folder inside: it alone is read. Had the first
        // been read again, the report would name it as already there.
        try write(flights[1], as: "2026/second.igc", in: folder)
        let second = await watched.bringInNew()
        XCTAssertEqual(second?.added.count, 1)
        XCTAssertEqual(second?.alreadyThere.count, 0)
        XCTAssertEqual(logbook.view.totals.flights, 2)
        let third = await watched.bringInNew()
        XCTAssertNil(third)
    }

    func testAFlightTheLogbookHasIsRecognisedAndAFileThatIsNoFlightIsTriedOnce() async throws {
        let (logbook, store, folder, flights) = (try makeLogbook(), try makeStore(), try makeFolder(), try flights())
        let watched = WatchedFolder(logbook: logbook, store: store, bookmarks: bookmarks(Asked()))
        _ = await logbook.addSample()
        // The same flight under a vario's name, and a file that is none.
        try write(flights[0], as: "copy.igc", in: folder)
        try write("HFDTE not a flight", as: "broken.igc", in: folder)

        let report = try await watched.watch(folder)
        XCTAssertEqual(report?.added.count, 0)
        XCTAssertEqual(report?.alreadyThere.count, 1)
        XCTAssertEqual(report?.failures.map(\.file), ["broken.igc"])
        XCTAssertEqual(logbook.view.totals.flights, 1)

        // Neither is tried again while it has not changed.
        let again = await watched.bringInNew()
        XCTAssertNil(again)
    }

    func testTheFolderIsStillWatchedAfterARestart() async throws {
        let (logbook, store, folder, flights) = (try makeLogbook(), try makeStore(), try makeFolder(), try flights())
        try write(flights[0], as: "first.igc", in: folder)
        // The system's own bookmarks, which a folder in the app's own
        // container has too.
        try await WatchedFolder(logbook: logbook, store: store).watch(folder)
        XCTAssertEqual(logbook.view.totals.flights, 1)

        // The app, started again: another model over what was kept.
        let restarted = WatchedFolder(logbook: logbook, store: store)
        XCTAssertEqual(restarted.name, "Tracklogs")
        let nothing = await restarted.bringInNew()
        XCTAssertNil(nothing)
        try write(flights[1], as: "second.igc", in: folder)
        let report = await restarted.bringInNew()
        XCTAssertEqual(report?.added.count, 1)
        XCTAssertFalse(restarted.isLost)

        // Stopped: nothing is kept, and nothing comes in.
        restarted.stop()
        XCTAssertNil(restarted.name)
        try write(flights[2], as: "third.igc", in: folder)
        let stopped = await restarted.bringInNew()
        XCTAssertNil(stopped)
        XCTAssertNil(WatchedFolder(logbook: logbook, store: store).name)
        XCTAssertEqual(logbook.view.totals.flights, 2)
    }

    func testAStaleBookmarkIsMadeAnewWithoutAWord() async throws {
        let (logbook, store, folder, flights) = (try makeLogbook(), try makeStore(), try makeFolder(), try flights())
        let asked = Asked()
        let watched = WatchedFolder(logbook: logbook, store: store, bookmarks: bookmarks(asked))
        try await watched.watch(folder)
        let first = store.data(forKey: Preferences.watchedFolder)
        XCTAssertEqual(asked.made, 1)

        // The folder was moved: the bookmark still leads to it, and says
        // that it is stale.
        asked.isStale = true
        try write(flights[0], as: "first.igc", in: folder)
        let report = await watched.bringInNew()
        XCTAssertEqual(report?.added.count, 1)
        XCTAssertFalse(watched.isLost)
        XCTAssertEqual(asked.made, 2)
        XCTAssertNotEqual(store.data(forKey: Preferences.watchedFolder), first)
    }

    func testABookmarkThatLeadsNowhereIsSaidAndTheFolderCanBeNamedAgain() async throws {
        let (logbook, store, folder, flights) = (try makeLogbook(), try makeStore(), try makeFolder(), try flights())
        let asked = Asked()
        let watched = WatchedFolder(logbook: logbook, store: store, bookmarks: bookmarks(asked))
        try await watched.watch(folder)
        XCTAssertFalse(watched.isLost)

        // The folder is gone, or the app that held it.
        asked.leadsNowhere = true
        try write(flights[0], as: "first.igc", in: folder)
        let report = await watched.bringInNew()
        XCTAssertNil(report)
        XCTAssertTrue(watched.isLost)
        XCTAssertEqual(logbook.view.totals.flights, 0)
        // The logbook still knows which folder it was, to say so.
        XCTAssertEqual(watched.name, "Tracklogs")
        XCTAssertTrue(WatchedFolder.Words.lost("Tracklogs").contains("Tracklogs"))
        // And after a restart it says so again.
        let restarted = WatchedFolder(logbook: logbook, store: store, bookmarks: bookmarks(asked))
        XCTAssertEqual(restarted.name, "Tracklogs")
        await restarted.bringInNew()
        XCTAssertTrue(restarted.isLost)

        // Named again, it is found and what is in it comes in.
        asked.leadsNowhere = false
        let named = try await watched.watch(folder)
        XCTAssertFalse(watched.isLost)
        XCTAssertEqual(named?.added.count, 1)
    }

    func testAStaleBookmarkThatCannotBeMadeAnewIsSaid() async throws {
        let (logbook, store, folder) = (try makeLogbook(), try makeStore(), try makeFolder())
        let asked = Asked()
        let watched = WatchedFolder(logbook: logbook, store: store, bookmarks: bookmarks(asked))
        try await watched.watch(folder)

        asked.isStale = true
        asked.cannotBeMade = true
        await watched.bringInNew()
        XCTAssertTrue(watched.isLost)

        // A folder the system gives no bookmark for is not kept, and the
        // one watched before stays.
        do {
            try await watched.watch(try makeFolder())
            XCTFail("A folder without a bookmark was kept.")
        } catch {
            XCTAssertEqual(watched.name, "Tracklogs")
        }
    }

    func testTheKeysKeptAsSeenAreThoseStillInTheFolder() {
        XCTAssertEqual(
            WatchedFolder.known(seen: ["a|1|1", "gone|1|1"], new: ["b|2|2"], keep: ["a|1|1", "b|2|2"]),
            ["a|1|1", "b|2|2"]
        )
    }
}
