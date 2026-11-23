import VentiCore
import XCTest

@testable import Venti

/// What a Venti server answers when asked for flying sites: a take-off
/// with a name and its wind, one without anything, and a landing.
private enum ServerSites {
    static let answer = """
    {"layers": [{
        "source": "osm-free-flying", "fetched_at": "2026-11-19T18:10:00Z", "as_of": "2026-11-18T21:21:02Z",
        "covers": [{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}],
        "sites": [
            {"id": "osm-free-flying:node/1", "name": "Wallberg Süd", "position": {"lat": 47.6652, "lon": 11.7954},
             "elevation_m": 1620.0, "uses": ["takeoff"],
             "wind_directions": [{"from_deg": 180.0, "to_deg": 225.0}], "official": true,
             "url": "https://www.openstreetmap.org/node/1"},
            {"id": "osm-free-flying:node/2", "position": {"lat": 47.6654, "lon": 11.7953}, "uses": ["takeoff"]},
            {"id": "osm-free-flying:node/3", "position": {"lat": 47.6919, "lon": 11.7880}, "uses": ["landing"]}
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
private actor SiteRequests {
    private(set) var addresses: [URL] = []

    func note(_ url: URL) {
        addresses.append(url)
    }
}

/// The sites model against the real core, with the server played by a
/// canned answer in the server's own format.
@MainActor
final class SitesModelTests: XCTestCase {
    private let tegernsee = Bounds(south: 47.60, west: 11.65, north: 47.75, east: 11.90)

    /// A take-off that gives its directions has a fan over them, pointing
    /// where the wind comes from; one without directions and a landing
    /// have none. The fan's state is the core's `windFits` for the wind
    /// of the hour the map shows (plan item 19).
    func testATakeoffsFanIsOverItsDirectionsAndColouredByTheCoresFit() async throws {
        let sites = SitesModel(transport: { _ in (200, Data(ServerSites.answer.utf8)) })
        let server = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        sites.server = { server }
        await sites.load(tegernsee, from: server)
        let (named, bare, landing) = (sites.sites[0], sites.sites[1], sites.sites[2])
        XCTAssertTrue(SiteFans.hasFan(named))
        XCTAssertFalse(SiteFans.hasFan(bare))
        XCTAssertFalse(SiteFans.hasFan(landing))

        // S to SW, the two points whole: from half a point before S to
        // half a point after SW.
        let wedges = SiteFans.wedges(of: named.wind)
        XCTAssertEqual(wedges.count, 1)
        XCTAssertEqual(wedges[0].from, 168.75, accuracy: 0.01)
        XCTAssertEqual(wedges[0].to, 236.25, accuracy: 0.01)
        // A span across north goes on past 360; a single direction is a
        // point wide.
        let across = SiteFans.wedges(of: [WindSpan(fromDeg: 315, toDeg: 45), WindSpan(fromDeg: 90, toDeg: 90)])
        XCTAssertEqual(across[0].from, 303.75, accuracy: 0.01)
        XCTAssertEqual(across[0].to, 416.25, accuracy: 0.01)
        XCTAssertEqual(across[1].to - across[1].from, 22.5, accuracy: 0.01)

        XCTAssertEqual(SiteFans.fit(of: named, windFrom: 200), .fits)
        XCTAssertEqual(SiteFans.fit(of: named, windFrom: 0), .doesNotFit)
        XCTAssertEqual(SiteFans.fit(of: named, windFrom: nil), .unknown)
        XCTAssertEqual(SiteFans.name(of: named.wind, fit: .fits), "site-fan-fits-180-225")
        XCTAssertNotEqual(SiteFans.name(of: named.wind, fit: .fits), SiteFans.name(of: named.wind, fit: .unknown))

        // The core's arrows point where the wind blows to: an arrow to the
        // NNE is a wind from the SSW; a calm square has no direction.
        let arrows = """
        {"type":"FeatureCollection","features":[
        {"type":"Feature","geometry":{"type":"Point","coordinates":[11.8,47.7]},
         "properties":{"cell":3,"symbol":"arrow","band":"light","heading":20,"label":"9"}},
        {"type":"Feature","geometry":{"type":"Point","coordinates":[11.9,47.7]},
         "properties":{"cell":4,"symbol":"calm","band":"light","heading":0,"label":"1"}}]}
        """
        let from = SiteFans.windFrom(arrows: arrows)
        XCTAssertEqual(from, [3: 200])
        XCTAssertEqual(SiteFans.windFrom(arrows: ""), [:])

        // Turned with the map, the fan's middle stays on the pin's head.
        XCTAssertEqual(SiteFans.offset(for: 0).dx, 0, accuracy: 0.001)
        XCTAssertEqual(SiteFans.offset(for: 0).dy, -SiteFans.headAboveTip, accuracy: 0.001)
        XCTAssertEqual(SiteFans.offset(for: 90).dx, SiteFans.headAboveTip, accuracy: 0.001)
        XCTAssertEqual(SiteFans.offset(for: 90).dy, 0, accuracy: 0.001)

        // The map's feature names the fan; none is the empty name.
        let drawn = OverviewMap.Coordinator.siteFeature(for: named, fan: "site-fan-plain-180-225")
        XCTAssertEqual(drawn.attribute(forKey: "fan") as? String, "site-fan-plain-180-225")
        XCTAssertEqual(OverviewMap.Coordinator.siteFeature(for: bare).attribute(forKey: "fan") as? String, "")
    }

    func testSitesAreAskedForOnceAndReadAsTheSourceGivesThem() async throws {
        let asked = SiteRequests()
        let sites = SitesModel(transport: { url in
            await asked.note(url)
            return (200, Data(ServerSites.answer.utf8))
        })
        let server = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        sites.server = { server }

        await sites.load(tegernsee, from: server)

        let first = await asked.addresses
        XCTAssertEqual(
            first.map(\.absoluteString), ["http://127.0.0.1:8080/v1/sites?bbox=11.65,47.6,11.9,47.75"]
        )
        XCTAssertNil(sites.problem)
        XCTAssertTrue(sites.isWhole)
        XCTAssertEqual(sites.sites.map(\.name), ["Wallberg Süd", "Take-off", "Landing"])
        XCTAssertEqual(sites.credits.map(\.text), ["© OpenStreetMap contributors"])

        let named = try XCTUnwrap(sites.sites.first)
        XCTAssertTrue(named.isNamed)
        XCTAssertEqual(named.kind, .takeoff)
        XCTAssertEqual(named.elevationM, 1620)
        XCTAssertEqual(named.windInWords, "S to SW")
        XCTAssertEqual(named.official, true)
        // The map writes a name only where the source gives one.
        let drawn = OverviewMap.Coordinator.siteFeature(for: named)
        XCTAssertEqual(drawn.attribute(forKey: "label") as? String, "Wallberg Süd")
        XCTAssertEqual(drawn.attribute(forKey: "picture") as? String, "site-takeoff")
        let unnamed = OverviewMap.Coordinator.siteFeature(for: sites.sites[1])
        XCTAssertEqual(unnamed.attribute(forKey: "label") as? String, "")
        XCTAssertEqual(
            OverviewMap.Coordinator.siteFeature(for: sites.sites[2]).attribute(forKey: "picture") as? String,
            "site-landing"
        )

        // A part of the map the answer covers needs no second request,
        // and a map of half a country asks for none at all.
        sites.show(Bounds(south: 47.65, west: 11.70, north: 47.70, east: 11.85))
        sites.show(Bounds(south: 46.0, west: 9.0, north: 49.0, east: 14.0))
        try await Task.sleep(for: .milliseconds(700))
        let second = await asked.addresses
        XCTAssertEqual(second.count, 1)
    }

    func testASiteCanBeKeptAsAWaypoint() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-sites-test-\(UUID().uuidString)", isDirectory: true)
        let waypoints = WaypointsModel(core: try Core(dataFolder: folder.path))
        XCTAssertTrue(waypoints.all.isEmpty)

        try waypoints.add(name: "Wallberg Süd", position: Coordinate(lat: 47.6652, lon: 11.7954), elevationM: 1620)

        XCTAssertEqual(waypoints.all.map(\.name), ["Wallberg Süd"])
    }

    func testAServerThatCannotAnswerIsSaidAndWithoutAServerNothingIsAsked() async throws {
        let asked = SiteRequests()
        let sites = SitesModel(transport: { url in
            await asked.note(url)
            return (503, Data())
        })
        // No server set: the map says where it looks, and nothing is sent.
        sites.show(tegernsee)
        try await Task.sleep(for: .milliseconds(600))
        let none = await asked.addresses
        XCTAssertTrue(none.isEmpty)

        let server = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        sites.server = { server }
        await sites.load(tegernsee, from: server)
        XCTAssertEqual(sites.problem, "The server is busy or being restarted. Try again in a moment.")
        XCTAssertTrue(sites.sites.isEmpty)
    }

    func testAStationNearASiteIsDescribedFromTheSite() {
        let reading = WindReading(
            timeUnixS: 1_000, speedMs: 4, gustMs: nil, directionDeg: 280, temperatureC: nil, humidityPct: nil
        )
        let summit = WindStation(
            id: "s", name: "Summit", source: "test", position: Coordinate(lat: 47.67, lon: 11.80),
            elevationM: 1_700, latest: reading, ageMin: 10, isOld: false,
            mark: WindMark(symbol: .calm, headingDeg: 0, label: "14"), network: "DWD"
        )
        let far = WindStation(
            id: "f", name: "Far", source: "test", position: Coordinate(lat: 47.42, lon: 10.98),
            elevationM: 2_956, latest: reading, ageMin: 10, isOld: false,
            mark: WindMark(symbol: .calm, headingDeg: 0, label: "14")
        )
        let site = Coordinate(lat: 47.66, lon: 11.80)
        let westToNorthWest = [WindSpan(fromDeg: 270, toDeg: 315)]

        // Only the station within reach, with whether its west wind fits.
        let near = stationsNear(
            stations: [far, summit], place: site, elevationM: 1_600, wind: westToNorthWest,
            withinM: SiteScreen.stationsWithinM, atMost: 3
        )
        XCTAssertEqual(near.map(\.station.name), ["Summit"])
        XCTAssertEqual(StationNearbyRow.whereabouts(near[0]), "1.1 km N · 100 m higher · DWD")
        XCTAssertEqual(StationNearbyRow.fitWords(near[0]), "From W: a direction the site works in")

        // A site that works in the east: the same wind does not fit. A
        // site without directions is not judged.
        let east = stationsNear(
            stations: [summit], place: site, elevationM: nil, wind: [WindSpan(fromDeg: 90, toDeg: 90)],
            withinM: SiteScreen.stationsWithinM, atMost: 3
        )
        XCTAssertEqual(StationNearbyRow.fitWords(east[0]), "From W: not a direction the site works in")
        XCTAssertEqual(StationNearbyRow.whereabouts(east[0]), "1.1 km N · DWD")
        let unjudged = stationsNear(
            stations: [summit], place: site, elevationM: nil, wind: [],
            withinM: SiteScreen.stationsWithinM, atMost: 3
        )
        XCTAssertNil(StationNearbyRow.fitWords(unjudged[0]))
        XCTAssertEqual(StationNearbyRow.ageWords(summit), "10 minutes ago")
        // Against an unknown clock the core gives no age: the reading's
        // own time stands in its place.
        var unaged = summit
        unaged.ageMin = nil
        XCTAssertEqual(StationNearbyRow.ageWords(unaged), Format.time(unixSeconds: reading.timeUnixS))
    }

    /// The places from pilots are asked for only once the pilot switched
    /// them on, the choice is kept, and a place from pilots says its tier
    /// under its name on the map.
    func testThePlacesFromPilotsAreAskedForOnlyWhenSwitchedOn() async throws {
        let asked = SiteRequests()
        let pilots = """
        {"layers": [{
            "source": "venti-pilots", "fetched_at": "2026-11-18T19:05:00Z", "as_of": "2026-11-18T19:05:00Z",
            "covers": [],
            "sites": [{"id": "venti-pilots:0000000000000000000000s002", "name": "Musterkogel Ost",
                       "position": {"lat": 47.66, "lon": 11.79}, "elevation_m": 1700.0, "uses": ["takeoff"]}],
            "standing": [{"id": "venti-pilots:0000000000000000000000s002", "revision": 1,
                          "created_at": "2026-11-06T19:05:00Z", "changed_at": "2026-11-06T19:05:00Z"}]
        }], "attributions": []}
        """
        let name = "venti-sites-test-\(UUID().uuidString)"
        let store = try XCTUnwrap(UserDefaults(suiteName: name))
        defer { store.removePersistentDomain(forName: name) }
        let sites = SitesModel(
            transport: { url in
                await asked.note(url)
                let with = url.absoluteString.contains("venti-pilots")
                return (200, Data((with ? pilots : ServerSites.answer).utf8))
            },
            store: store
        )
        let server = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        sites.server = { server }

        // Off by default: the path names no layer, and nothing is marked.
        XCTAssertFalse(sites.choice.withPilots)
        await sites.load(tegernsee, from: server)
        var addresses = await asked.addresses
        XCTAssertFalse(try XCTUnwrap(addresses.last).absoluteString.contains("layers"))
        XCTAssertFalse(sites.filtered.isOn)
        XCTAssertEqual(sites.filtered.fromPilotsWords, "")

        // Switched on: asked anew with the layer, counted as a filter
        // being on, and the core's line says how many are from pilots.
        var choice = sites.choice
        choice.withPilots = true
        sites.choose(choice)
        for _ in 0..<200 where sites.filtered.fromPilots == 0 {
            try await Task.sleep(for: .milliseconds(10))
        }
        addresses = await asked.addresses
        XCTAssertTrue(try XCTUnwrap(addresses.last).absoluteString.contains("venti-pilots"))
        XCTAssertTrue(sites.filtered.isOn)
        XCTAssertTrue(sites.filtered.filterWords.hasSuffix("with places from pilots"), sites.filtered.filterWords)
        XCTAssertEqual(sites.filtered.fromPilots, 1)
        XCTAssertEqual(
            sites.filtered.fromPilotsWords,
            "It is a place from pilots: not official and not checked by anyone."
        )
        XCTAssertTrue(SitesFilterLine.leaves(sites.filtered).hasSuffix(sites.filtered.fromPilotsWords))
        let place = try XCTUnwrap(sites.sites.first)
        XCTAssertEqual(place.byPilots?.label, "From pilots")
        let drawn = OverviewMap.Coordinator.siteFeature(for: place)
        XCTAssertEqual(drawn.attribute(forKey: "label") as? String, "Musterkogel Ost\nFrom pilots")
        XCTAssertEqual(drawn.attribute(forKey: "picture") as? String, "site-takeoff")

        // Kept as the pilot left it.
        XCTAssertTrue(SitesChoice(keptIn: store).withPilots)

        // Switched off: the places from pilots go at once, before any
        // answer, and the next answer is asked for without the layer.
        choice.withPilots = false
        sites.choose(choice)
        XCTAssertTrue(sites.sites.isEmpty)
        for _ in 0..<200 where sites.sites.isEmpty {
            try await Task.sleep(for: .milliseconds(10))
        }
        addresses = await asked.addresses
        XCTAssertFalse(try XCTUnwrap(addresses.last).absoluteString.contains("layers"))
        XCTAssertFalse(SitesChoice(keptIn: store).withPilots)
        XCTAssertFalse(sites.filtered.isOn)
    }
}
