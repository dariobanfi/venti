import VentiCore
import XCTest

@testable import Venti

/// What Venti answers about the public flights at a site and the places
/// where pilots take off and no site stands, in the server's own format
/// (as `crates/server/tests/golden/` has them, the terms shortened).
private enum ServerFlights {
    static let site = """
    {"id": "venti:0000000000000000000000s001", "within_m": 500.0, "takeoff": true,
     "flights": 3, "pilots": 2, "latest_day": "2026-07-11",
     "by_month": [0, 0, 0, 0, 1, 0, 2, 0, 0, 0, 0, 0],
     "best": [{"id": "ww5qkxhxaedefvypb3wy3v6m3m", "pilot": {"handle": "lena", "display_name": "Lena Probe"},
               "day": "2026-05-09",
               "score": {"kind": "free_flight", "closed": false, "distance_m": 12007.4, "points": 12.01}}],
     "terms": {"text": "These flights are their pilots'.", "decided": false}}
    """

    static let landing = """
    {"id": "venti:0000000000000000000000s002", "within_m": 500.0, "takeoff": false,
     "flights": 0, "pilots": 0, "by_month": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], "best": [],
     "terms": {"text": "These flights are their pilots'.", "decided": false}}
    """

    static let candidates = """
    {"candidates": [{"position": {"lat": 47.2612, "lon": 10.9312}, "altitude_m": 1620, "flights": 3,
                     "pilots": 3, "latest_day": "2026-06-07",
                     "by_month": [0, 0, 0, 1, 0, 2, 0, 0, 0, 0, 0, 0]}],
     "more": false, "min_pilots": 3, "cell_deg": 0.0025, "clear_of_sites_m": 500.0,
     "terms": {"text": "These flights are their pilots'.", "decided": false}}
    """
}

/// The addresses asked for.
private actor Asked {
    private(set) var addresses: [String] = []

    func note(_ url: URL) {
        addresses.append(url.absoluteString)
    }
}

@MainActor
final class FlightsAtSitesTests: XCTestCase {
    private let server = URL(string: "http://127.0.0.1:8080")!
    /// The box of the made-up candidate in the Lechtal.
    private let lechtal = Bounds(south: 47.25, west: 10.92, north: 47.27, east: 10.95)

    func testASitesFlightsAreAskedWithoutATokenAndOpenAsPublicFlights() async throws {
        let asked = Asked()
        let flights = try await SiteFlightsReading.read(
            "venti:0000000000000000000000s001", from: server,
            transport: { url in
                await asked.note(url)
                return (200, Data(ServerFlights.site.utf8))
            }
        )
        let addresses = await asked.addresses
        XCTAssertEqual(addresses, ["http://127.0.0.1:8080/v1/sites/venti:0000000000000000000000s001/flights"])
        XCTAssertTrue(flights.isTakeoff)
        XCTAssertFalse(flights.words.isEmpty)
        XCTAssertEqual(flights.months.count, 12)
        XCTAssertEqual(flights.months.map(\.share).max(), 1)
        XCTAssertEqual(flights.best.count, 1)

        // A best flight opens as a public flight opens: by its file.
        let line = try XCTUnwrap(flights.best.first)
        let opened = PublicFlight(best: line)
        XCTAssertEqual(opened.id, line.id)
        XCTAssertEqual(opened.filePath, line.filePath)
        XCTAssertEqual(opened.pagePath, line.pagePath)
        XCTAssertEqual(opened.pilot, "Lena Probe")
        XCTAssertEqual(opened.handle, "lena")
    }

    func testALandingSaysNothingOfFlights() async throws {
        let flights = try await SiteFlightsReading.read(
            "venti:0000000000000000000000s002", from: server,
            transport: { _ in (200, Data(ServerFlights.landing.utf8)) }
        )
        XCTAssertFalse(flights.isTakeoff)
        XCTAssertTrue(flights.words.isEmpty)
        XCTAssertTrue(flights.nobodyWords.isEmpty)
        XCTAssertTrue(flights.months.isEmpty)
    }

    func testTheLayerIsOffUntilSwitchedOnAndKeptForTheNextLaunch() async throws {
        let store = try XCTUnwrap(UserDefaults(suiteName: "venti-candidates-\(UUID().uuidString)"))
        let asked = Asked()
        let model = CandidatesModel(
            transport: { url in
                await asked.note(url)
                return (200, Data(ServerFlights.candidates.utf8))
            },
            store: store
        )
        let server = server
        model.server = { server }
        XCTAssertFalse(model.isOn)

        model.show(lechtal)
        XCTAssertTrue(model.candidates.isEmpty)
        var addresses = await asked.addresses
        XCTAssertTrue(addresses.isEmpty, "Nothing is asked while the layer is off")

        model.switchOn(true)
        XCTAssertTrue(store.bool(forKey: Preferences.mapShowsCandidates))
        XCTAssertTrue(CandidatesModel(store: store).isOn, "The layer is on at the next launch")
        await model.load(lechtal, from: server)
        addresses = await asked.addresses
        XCTAssertTrue(addresses.last?.hasPrefix("http://127.0.0.1:8080/v1/sites/candidates?bbox=") == true)
        XCTAssertEqual(model.candidates.count, 1)
        XCTAssertFalse(try XCTUnwrap(model.candidates.first).words.isEmpty)
        XCTAssertNil(model.words)

        model.switchOn(false)
        XCTAssertTrue(model.candidates.isEmpty)
        XCTAssertFalse(store.bool(forKey: Preferences.mapShowsCandidates))
    }

    func testAPartOfTheMapTooLargeIsNotAskedAndTheCoreSaysToZoomIn() async throws {
        let asked = Asked()
        let model = CandidatesModel(transport: { url in
            await asked.note(url)
            return (200, Data(ServerFlights.candidates.utf8))
        })
        let server = server
        model.server = { server }
        model.switchOn(true)

        model.show(Bounds(south: 46.0, west: 10.0, north: 48.5, east: 12.5))

        let words = try XCTUnwrap(model.words)
        XCTAssertThrowsError(try siteCandidatesPath(south: 46.0, west: 10.0, north: 48.5, east: 12.5)) { error in
            XCTAssertEqual(error.words, words)
        }
        XCTAssertTrue(model.candidates.isEmpty)
        let addresses = await asked.addresses
        XCTAssertTrue(addresses.isEmpty)
    }

    func testNothingIsOfferedToAPilotWhoIsNotSignedIn() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-candidates-test-\(UUID().uuidString)", isDirectory: true)
        let account = AccountModel(
            core: try Core(dataFolder: folder.path),
            transport: { _ in (500, Data(), nil) }, readToken: { nil }, saveToken: { _ in }
        )
        let answer = try siteCandidates(status: 200, body: Data(ServerFlights.candidates.utf8))
        let offers = await account.candidateOffers(answer, takesSitesFromPilots: true)
        XCTAssertTrue(offers.isEmpty)
    }

    func testAnAnswerNarrowedToOnePlace() throws {
        let answer = try siteCandidates(status: 200, body: Data(ServerFlights.candidates.utf8))
        let place = try XCTUnwrap(answer.candidates.first)
        let one = CandidatesModel.only(place, of: answer)
        XCTAssertEqual(one.candidates, [place])
        XCTAssertEqual(one.clearOfSitesM, answer.clearOfSitesM)
        XCTAssertEqual(one.terms, answer.terms)
    }

    func testTodaysBoxHoldsTheTakeoffsWithRoomAround() throws {
        let box = try XCTUnwrap(
            TodayScreen.box(around: [Coordinate(lat: 47.26, lon: 10.93), Coordinate(lat: 47.30, lon: 10.90)])
        )
        XCTAssertLessThan(box.south, 47.26)
        XCTAssertGreaterThan(box.north, 47.30)
        XCTAssertLessThan(box.west, 10.90)
        XCTAssertGreaterThan(box.east, 10.93)
        XCTAssertNoThrow(try CandidatesModel.path(for: box))
        XCTAssertNil(TodayScreen.box(around: []))
    }
}
