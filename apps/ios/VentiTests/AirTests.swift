import MapLibre
import VentiCore
import XCTest

@testable import Venti

/// What others found in the air, as the app asks for it and draws it: the
/// climbs of the last minutes, where thermals have been, who flies at a
/// site, the alarm for pilots in the air, and the cockpit's climbs behind
/// their setting. The words and the rules are the core's; this is the
/// plumbing.
@MainActor
final class AirTests: XCTestCase {
    private actor Asked {
        var addresses: [URL] = []
        func add(_ url: URL) { addresses.append(url) }
    }

    private nonisolated static let ogn = """
        {"source": "ogn", "provider": "Open Glider Network", "licence": "ODbL 1.0",
         "text": "Positions: Open Glider Network, glidernet.org (ODbL)",
         "url": "https://www.glidernet.org/ogn-data-usage/"}
        """

    /// Two climbs by the Tegernsee, in the server's answer's form.
    nonisolated static let climbs = Data(
        """
        {"time": "2026-11-21T19:09:10Z", "listed_minutes": 10, "cell_deg": 0.005,
         "layers": [
           {"source": "ogn", "truncated": false, "climbs": [
             {"position": {"lat": 47.6275, "lon": 11.7425}, "climb_ms": 1.3, "from_m": 1350, "to_m": 1550,
              "pilots": 2, "age_s": 2},
             {"position": {"lat": 47.6675, "lon": 11.8025}, "climb_ms": 2.4, "from_m": 1600, "to_m": 2100,
              "pilots": 3, "age_s": 240}]},
           {"source": "venti", "climbs": [], "truncated": false}],
         "attributions": [\(ogn)]}
        """.utf8
    )

    nonisolated static let sites = Data(
        """
        {"time": "2026-11-21T19:09:10Z", "within_m": 5000.0, "took_off_minutes": 60,
         "layers": [{"source": "ogn", "truncated": false, "sites": [
           {"site": "osm-free-flying:node/5964676196", "airborne": 8, "took_off": 2}]}],
         "attributions": [\(ogn)]}
        """.utf8
    )

    nonisolated static let flown = Data(
        """
        {"cells": [
           {"position": {"lat": 47.6725, "lon": 11.7925}, "climbs": 5, "pilots": 3, "climb_ms": 2.4,
            "best_climb_ms": 2.4, "from_m": 1500, "to_m": 2100},
           {"position": {"lat": 47.6875, "lon": 11.7525}, "climbs": 2, "pilots": 1, "climb_ms": 1.5,
            "best_climb_ms": 1.5, "from_m": 1700, "to_m": 1950}],
         "more": false, "cell_deg": 0.005, "min_pilots": 3, "flights": 11,
         "terms": {"text": "These flights are their pilots'.", "decided": false}}
        """.utf8
    )

    private let valley = Bounds(south: 47.55, west: 11.6, north: 47.85, east: 12.0)
    private let alps = Bounds(south: 40, west: 0, north: 50, east: 20)

    private func server() throws -> URL {
        try XCTUnwrap(URL(string: "http://127.0.0.1:8090"))
    }

    func testTheClimbsAreAskedForOnlyWhileTheirLayerIsOn() async throws {
        let asked = Asked()
        let air = AirModel { url in
            await asked.add(url)
            return (200, Self.climbs)
        }
        let server = try server()
        air.server = { server }

        // Off: nothing is asked.
        await air.loadClimbs(valley, isOn: false)
        var addresses = await asked.addresses
        XCTAssertTrue(addresses.isEmpty)
        XCTAssertNil(air.climbs)

        await air.loadClimbs(valley, isOn: true)
        addresses = await asked.addresses
        XCTAssertEqual(
            addresses.map(\.absoluteString), ["http://127.0.0.1:8090/v1/live/climbs?bbox=11.6,47.55,12,47.85"]
        )
        let climbs = try XCTUnwrap(air.climbs)
        XCTAssertEqual(climbs.climbs.map(\.label), ["+1.3", "+2.4"])
        XCTAssertEqual(climbs.countWords, "2 climbs in the last 10 minutes")
        XCTAssertFalse(air.climbsTooWide)

        // Too much of the map: nothing is asked, none is shown, and the
        // model says so.
        await air.loadClimbs(alps, isOn: true)
        addresses = await asked.addresses
        XCTAssertEqual(addresses.count, 1)
        XCTAssertNil(air.climbs)
        XCTAssertTrue(air.climbsTooWide)

        // Off again: forgotten, and no word about the map's width.
        await air.loadClimbs(valley, isOn: true)
        await air.loadClimbs(valley, isOn: false)
        XCTAssertNil(air.climbs)
        XCTAssertFalse(air.climbsTooWide)

        // A server that does not answer leaves none: an old climb is not
        // shown as one of this moment.
        await air.loadClimbs(valley, isOn: true)
        XCTAssertNotNil(air.climbs)
        let silent = AirModel { _ in throw URLError(.timedOut) }
        silent.server = { server }
        await silent.loadClimbs(valley, isOn: true)
        XCTAssertNil(silent.climbs)
    }

    func testWhereThermalsHaveBeenIsAskedForInEveryMonthAndAtAnyHour() async throws {
        let asked = Asked()
        let air = AirModel { url in
            await asked.add(url)
            return (200, Self.flown)
        }
        let server = try server()
        air.server = { server }

        await air.loadFlown(valley, isOn: false)
        var addresses = await asked.addresses
        XCTAssertTrue(addresses.isEmpty)

        await air.loadFlown(valley, isOn: true)
        addresses = await asked.addresses
        // No month and no hours: the layer is not empty in winter.
        XCTAssertEqual(
            addresses.map(\.absoluteString), ["http://127.0.0.1:8090/v1/thermals/flown?bbox=11.6,47.55,12,47.85"]
        )
        let flown = try XCTUnwrap(air.flown)
        XCTAssertEqual(flown.cells.count, 2)
        XCTAssertFalse(air.flownTooWide)

        // Wider than the server answers for: the core's words stand over
        // the map, and nothing is drawn.
        await air.loadFlown(alps, isOn: true)
        XCTAssertTrue(air.flownTooWide)
        XCTAssertNil(air.flown)
        XCTAssertFalse(flownTooWideWords().isEmpty)

        // On the map: a rectangle of the answer's side around each
        // middle, the more solid the more it weighs.
        let cells = AirLayers.features(for: flown)
        XCTAssertEqual(cells.count, 2)
        XCTAssertEqual(cells.compactMap { $0.attribute(forKey: "id") as? String }, ["0", "1"])
        let opacities = cells.compactMap { $0.attribute(forKey: "opacity") as? Double }
        XCTAssertEqual(opacities[0], AirLayers.flownMost, accuracy: 0.001)
        XCTAssertGreaterThan(opacities[0], opacities[1])
        XCTAssertGreaterThanOrEqual(opacities[1], AirLayers.flownLeast)
        XCTAssertEqual(cells[0].pointCount, 5)
        XCTAssertEqual(cells[0].overlayBounds.ne.latitude - cells[0].overlayBounds.sw.latitude, 0.005, accuracy: 1e-9)
        XCTAssertTrue(AirLayers.features(for: nil as FlownThermals?).isEmpty)

        // Tapped: the cell's line, with what the layer is and the terms.
        let chosen = ChosenAirLine(cell: flown.cells[0], of: flown)
        XCTAssertEqual(chosen.line, flown.cells[0].line)
        XCTAssertEqual(chosen.notes, [flown.note, "These flights are their pilots'."])
        XCTAssertEqual(chosen.title, mapLayersWords().flownThermals)
    }

    func testAClimbIsAMarkWithTheCoresLabelAndALineWithItsCredit() throws {
        let climbs = try liveClimbs(status: 200, body: Self.climbs)
        let marks = AirLayers.features(for: climbs.climbs)
        XCTAssertEqual(marks.compactMap { $0.attribute(forKey: "label") as? String }, ["+1.3", "+2.4"])
        XCTAssertEqual(marks.compactMap { $0.attribute(forKey: "id") as? String }, ["0", "1"])
        // The colour is the step of the pack's vario scale the rate is in.
        XCTAssertEqual(
            marks.compactMap { $0.attribute(forKey: "band") as? Int },
            [ClimbBand.lightClimb.rawValue, ClimbBand.climb.rawValue]
        )
        XCTAssertEqual(marks[1].coordinate.latitude, 47.6675, accuracy: 1e-6)

        let chosen = ChosenAirLine(climb: climbs.climbs[1], among: climbs)
        XCTAssertEqual(chosen.line, "Climbing now: +2.4 m/s between 1,600 and 2,100 m, 3 pilots, 4 min ago")
        XCTAssertEqual(chosen.notes, ["Positions: Open Glider Network, glidernet.org (ODbL)"])
        XCTAssertEqual(chosen.identifier, "climb-line")
        XCTAssertEqual(chosen.title, mapLayersWords().climbsNow)
    }

    func testASitesLineNamesTheSiteOrSaysHere() async throws {
        let asked = Asked()
        let air = AirModel { url in
            await asked.add(url)
            return (200, Self.sites)
        }
        let place = Coordinate(lat: 47.6642, lon: 11.7967)
        let id = "osm-free-flying:node/5964676196"
        // Without a server nothing is asked and nothing said.
        let nothing = await air.siteAir(id: id, name: "Wallberg", position: place)
        XCTAssertEqual(nothing, AirModel.SiteLine())

        let server = try server()
        air.server = { server }
        let named = await air.siteAir(id: id, name: "Wallberg", position: place)
        XCTAssertEqual(named.words, "8 pilots in the air at Wallberg, 2 took off in the last hour")
        XCTAssertEqual(named.credits.map(\.provider), ["Open Glider Network"])
        let first = await asked.addresses.first
        let address = try XCTUnwrap(first)
        XCTAssertTrue(address.absoluteString.hasPrefix("http://127.0.0.1:8090/v1/live/sites?bbox=11.74"), address.absoluteString)

        // A site without a name of its own: "here", never the app's word.
        let unnamed = await air.siteAir(id: id, name: "", position: place)
        XCTAssertEqual(unnamed.words, "8 pilots in the air here, 2 took off in the last hour")

        // A site nobody flies at says nothing, and credits nobody.
        let quiet = await air.siteAir(id: "osm-free-flying:node/1", name: "Nowhere", position: place)
        XCTAssertEqual(quiet, AirModel.SiteLine())
    }

    func testTheAlarmForPilotsStartsAtTheCoresNumbers() throws {
        let numbers = pilotsAlarmNumbers()
        let map = try sitesMap(
            south: 46, west: 9, north: 49, east: 18, status: 200, body: Data(AlertsModelTests.Answers.sites.utf8)
        )
        let site = AlertsModel.draft(for: try XCTUnwrap(map.sites.first { $0.name == "Wallberg Süd" }))
        XCTAssertEqual(site.kind, .day)
        XCTAssertNil(site.minPilots)

        let pilots = AlertsModel.draft(site, ofKind: .pilots)
        XCTAssertEqual(pilots.kind, .pilots)
        XCTAssertEqual(pilots.minPilots, numbers.minPilotsDefault)
        XCTAssertEqual(pilots.restHours, numbers.restHoursDefault)
        XCTAssertEqual(pilots.placeName, "Wallberg Süd")
        XCTAssertEqual(pilots.siteId, site.siteId)
        XCTAssertTrue(alertCheck(draft: pilots).isEmpty, "\(alertCheck(draft: pilots))")
        XCTAssertTrue(alertLine(draft: pilots).contains("Wallberg"), alertLine(draft: pilots))

        // Numbers the pilot chose stay when the kind goes there and back.
        var chosen = pilots
        chosen.minPilots = numbers.minPilotsMost
        let back = AlertsModel.draft(AlertsModel.draft(chosen, ofKind: .day), ofKind: .pilots)
        XCTAssertEqual(back.minPilots, numbers.minPilotsMost)
        XCTAssertEqual(AlertsModel.draft(chosen, ofKind: .day).kind, .day)

        // The two kinds' names are the core's.
        XCTAssertFalse(mapLayersWords().alarmDayKind.isEmpty)
        XCTAssertFalse(alertFormWords().pilotsKind.isEmpty)
    }

    func testTheCockpitAsksForClimbsOnlyWhileTheSettingIsOn() async throws {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        let igc = try String(contentsOf: url, encoding: .utf8)
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let climbs = try liveClimbs(status: 200, body: Self.climbs)
        let asked = Asked()

        // Off, which is how it starts: nothing is asked, nothing drawn.
        let off = FlySession(dataFolder: folder)
        off.showsClimbs = { false }
        off.climbsAround = { place in
            await asked.add(URL(fileURLWithPath: "/\(place.lat)"))
            return climbs
        }
        off.startReplay(igcText: igc, speed: 100)
        try await Task.sleep(for: .milliseconds(1500))
        XCTAssertEqual(off.view?.phase, .flying)
        let none = await asked.addresses
        XCTAssertTrue(none.isEmpty)
        XCTAssertNil(off.climbsNear)
        _ = off.finish()

        // On: asked once the glider flies, handed to the core, and the
        // core's nearest climbs come back for the map, each with a line.
        let on = FlySession(dataFolder: folder)
        on.showsClimbs = { true }
        on.climbsAround = { place in
            await asked.add(URL(fileURLWithPath: "/\(place.lat)"))
            return climbs
        }
        on.startReplay(igcText: igc, speed: 20)
        for _ in 0..<40 where on.climbsNear == nil {
            try await Task.sleep(for: .milliseconds(250))
        }
        let once = await asked.addresses
        XCTAssertEqual(once.count, 1, "asked once a minute, not with every refresh")
        let near = try XCTUnwrap(on.climbsNear)
        XCTAssertFalse(near.climbs.isEmpty)
        XCTAssertTrue(near.climbs[0].line.hasPrefix("Climb "), near.climbs[0].line)
        XCTAssertEqual(near.climbs[0].climb.label.first, "+")
        XCTAssertEqual(near.credits.map(\.provider), ["Open Glider Network"])
        // On the cockpit's map: the map tab's mark.
        let marks = AirLayers.features(for: near.climbs.map(\.climb))
        XCTAssertEqual(marks.count, near.climbs.count)
        XCTAssertEqual(FlySession.ticksBetweenClimbs, 300, "every 60 s at five refreshes a second")
        _ = on.finish()
        XCTAssertNil(on.climbsNear)
    }
}
