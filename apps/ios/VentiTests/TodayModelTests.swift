import VentiCore
import XCTest

@testable import Venti

/// What a Venti server answers when asked for the sites around the
/// Tegernsee: a take-off with a name, one without, and a landing with a
/// fee and a web page.
private enum Answers {
    static let sites = """
    {"layers": [{
        "source": "osm-free-flying", "fetched_at": "2026-11-19T18:10:00Z", "as_of": "2026-11-18T21:21:02Z",
        "covers": [{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}],
        "sites": [
            {"id": "osm-free-flying:node/1", "name": "Wallberg Süd", "position": {"lat": 47.6652, "lon": 11.7954},
             "elevation_m": 1620.0, "uses": ["takeoff"],
             "url": "https://www.openstreetmap.org/node/1"},
            {"id": "osm-free-flying:node/2", "position": {"lat": 47.6654, "lon": 11.7953}, "uses": ["takeoff"],
             "description": "Startrampe"},
            {"id": "osm-free-flying:node/3", "name": "Landeplatz", "position": {"lat": 47.6919, "lon": 11.7880},
             "uses": ["landing"], "website": "https://www.dhv.de/", "fee": "yes"}
        ]
    }],
    "attributions": [{
        "source": "osm-free-flying", "provider": "OpenStreetMap contributors", "licence": "ODbL 1.0",
        "text": "© OpenStreetMap contributors", "url": "https://www.openstreetmap.org/copyright",
        "notice": "Anyone can correct it."
    }]}
    """
}

/// The addresses a model asked for.
private actor Requests {
    private(set) var addresses: [URL] = []

    func note(_ url: URL) {
        addresses.append(url)
    }
}

/// Today's model and the model of the pilot's own sites against the real
/// core, with the server played by a canned answer in the server's own
/// format. Which take-offs Today lists and in which order is the core's
/// and is tested there; these test that the app asks it and shows what
/// it says.
@MainActor
final class TodayModelTests: XCTestCase {
    private let tegernsee = Coordinate(lat: 47.6, lon: 11.75)

    private func makeCore() throws -> Core {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-today-\(UUID().uuidString)", isDirectory: true)
        addTeardownBlock {
            try? FileManager.default.removeItem(at: folder)
        }
        return try Core(dataFolder: folder.path)
    }

    /// A server that has sites and no forecast for any place.
    private func models(asked: Requests = Requests()) throws -> (TodayModel, KeptSitesModel) {
        let own = KeptSitesModel(core: try makeCore())
        let today = TodayModel(kept: own, transport: { url in
            await asked.note(url)
            return url.path.hasSuffix("/sites") ? (200, Data(Answers.sites.utf8)) : (404, Data())
        })
        let server = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        today.server = { server }
        return (today, own)
    }

    func testTodayAsksForTheCoresAreaAndListsTheCoresTakeoffs() async throws {
        let asked = Requests()
        let (today, _) = try models(asked: asked)

        await today.load(around: tegernsee)

        // The area asked for is the one the core names.
        let area = try todayArea(lat: tegernsee.lat, lon: tegernsee.lon)
        let path = try sitesPath(south: area.south, west: area.west, north: area.north, east: area.east)
        let addresses = await asked.addresses.map(\.absoluteString)
        XCTAssertEqual(addresses.first, "http://127.0.0.1:8080" + path)
        // A forecast was asked for each take-off, and for nothing else.
        XCTAssertEqual(addresses.count, 3)

        XCTAssertEqual(today.state, .loaded)
        XCTAssertNil(today.problem)
        XCTAssertEqual(today.places.map(\.site.name), ["Wallberg Süd", "Take-off"])
        XCTAssertEqual(today.places.map(\.isKept), [false, false])
        XCTAssertTrue(today.places.allSatisfy(\.isAsked))
        XCTAssertEqual(today.listed?.credits.map(\.text), ["© OpenStreetMap contributors"])
        XCTAssertNotNil(today.sitesAsOfUnixS)
        XCTAssertTrue(today.isFresh(for: tegernsee))

        // With none kept the groups have no names, and the first is the card.
        let shown = TodayScreen.arrange(today.places) { _ in nil }
        XCTAssertEqual(shown.card?.site.name, "Wallberg Süd")
        XCTAssertFalse(shown.namesGroups)
        XCTAssertTrue(shown.kept.isEmpty)
        XCTAssertEqual(shown.near.map(\.site.name), ["Take-off"])
    }

    func testAKeptTakeoffComesFirstAndForgottenItDoesNot() async throws {
        let (today, own) = try models()
        await today.load(around: tegernsee)
        let ramp = try XCTUnwrap(today.places.last?.site)
        let wasKept = await own.isKept(ramp.id)
        XCTAssertFalse(wasKept)

        try await own.keep(ramp, credits: today.listed?.credits ?? [], asOfUnixS: today.sitesAsOfUnixS)

        // What is shown is no longer fresh: Today reads its take-offs again.
        XCTAssertFalse(today.isFresh(for: tegernsee))
        let isKept = await own.isKept(ramp.id)
        XCTAssertTrue(isKept)
        let asOf = await own.asOf(ramp.id)
        XCTAssertEqual(asOf, today.sitesAsOfUnixS)
        await today.load(around: tegernsee)
        XCTAssertEqual(today.places.map(\.site.name), ["Take-off", "Wallberg Süd"])
        XCTAssertEqual(today.places.map(\.isKept), [true, false])
        XCTAssertEqual(today.listed?.keptTitle, "Your take-offs")
        XCTAssertEqual(today.listed?.nearTitle, "Within 30 km")
        // The groups are named, and without a forecast the card is the
        // first of the core's order: the pilot's own.
        let shown = TodayScreen.arrange(today.places) { _ in nil }
        XCTAssertTrue(shown.namesGroups)
        XCTAssertEqual(shown.card?.site.id, ramp.id)
        XCTAssertTrue(shown.kept.isEmpty)
        XCTAssertEqual(shown.near.map(\.site.name), ["Wallberg Süd"])

        // The list of the pilot's own, with the core's note and the credit.
        await own.load(from: tegernsee)
        let list = try XCTUnwrap(own.list)
        XCTAssertEqual(list.sites.map(\.site.id), [ramp.id])
        XCTAssertNotNil(list.sites.first?.distanceM)
        XCTAssertTrue(list.note.hasPrefix("Kept on this device only"))
        XCTAssertEqual(
            KeptSitesScreen.footnote(list), "\(list.note)\n© OpenStreetMap contributors"
        )
        XCTAssertTrue(KeptSitesScreen.detail(of: list.sites[0]).contains(" km "))
        XCTAssertTrue(KeptSitesScreen.detail(of: list.sites[0]).contains("as of "))

        try await own.forget(ramp.id)
        XCTAssertFalse(today.isFresh(for: tegernsee))
        await today.load(around: tegernsee)
        XCTAssertEqual(today.places.map(\.isKept), [false, false])
        await own.load(from: nil)
        XCTAssertEqual(own.list?.sites.count, 0)
    }

    func testWithoutVentiThePilotsOwnTakeoffsStillCome() async throws {
        let (online, own) = try models()
        await online.load(around: tegernsee)
        let named = try XCTUnwrap(online.places.first?.site)

        // Nothing kept and no Venti: the screen says what is missing.
        let offline = TodayModel(kept: own, transport: { _ in throw URLError(.cannotConnectToHost) })
        await offline.load(around: tegernsee)
        XCTAssertEqual(offline.state, .failed("Venti could not be reached."))
        XCTAssertTrue(offline.places.isEmpty)

        // One kept: it is listed, from the iPhone, with why the others
        // are not and with its source's credit.
        try await own.keep(named, credits: online.listed?.credits ?? [], asOfUnixS: online.sitesAsOfUnixS)
        await offline.load(around: tegernsee)
        XCTAssertEqual(offline.state, .loaded)
        XCTAssertEqual(offline.problem, "Venti could not be reached.")
        XCTAssertEqual(offline.places.map(\.site.name), ["Wallberg Süd"])
        XCTAssertEqual(offline.places.map(\.isKept), [true])
        XCTAssertTrue(offline.places.allSatisfy(\.isAsked))
        XCTAssertEqual(offline.listed?.credits.map(\.text), ["© OpenStreetMap contributors"])

        // The same with a Venti that does not answer.
        let server = try XCTUnwrap(URL(string: "http://127.0.0.1:9"))
        offline.server = { server }
        await offline.load(around: tegernsee)
        XCTAssertEqual(offline.state, .loaded)
        XCTAssertEqual(offline.places.map(\.site.name), ["Wallberg Süd"])
        XCTAssertNotNil(offline.problem)
    }

    func testASitesWebPageIsALinkOnlyWhereABrowserOpensIt() {
        XCTAssertEqual(SiteScreen.webPage("https://www.dhv.de/")?.host(), "www.dhv.de")
        XCTAssertNotNil(SiteScreen.webPage("http://example.org/site"))
        XCTAssertNil(SiteScreen.webPage(nil))
        XCTAssertNil(SiteScreen.webPage(""))
        XCTAssertNil(SiteScreen.webPage("javascript:alert(1)"))
        XCTAssertNil(SiteScreen.webPage("file:///etc/hosts"))
        XCTAssertNil(SiteScreen.webPage("www.dhv.de"))
    }
}
