import VentiCore
import XCTest

@testable import Venti

/// What a Venti server answers when asked for airspace: one made-up zone
/// over the valley of the sample flight, complete for the Alps.
private enum ServerAirspace {
    static let answer = """
    {"layers": [{
        "source": "openaip", "fetched_at": "2026-11-18T19:05:00Z", "file_date": "2026-11-18T01:12:00Z",
        "covers": [{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}], "countries": ["DE", "AT"],
        "airspaces": [{
            "id": "openaip:sample", "name": "SERVER ZONE (NOT REAL)", "class": "D", "airspace_type": "ctr",
            "country": "DE",
            "lower": {"reference": "ground", "metres": 0.0},
            "upper": {"reference": "mean-sea-level", "value": 9000.0, "unit": "feet", "metres": 2743.2},
            "geometry": {"exterior": [[11.70, 47.68], [11.76, 47.68], [11.76, 47.72], [11.70, 47.72], [11.70, 47.68]]},
            "activity": {"by_notam": false, "on_request": false, "on_demand": false, "special_agreement": false},
            "validity": {"file_date": "2026-11-18T01:12:00Z"}
        }]
    }],
    "attributions": [{
        "source": "openaip", "provider": "openAIP", "licence": "CC BY-NC 4.0",
        "text": "Data used comes from openAIP.", "url": "https://www.openaip.net",
        "notice": "OpenAIP data is not certified."
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

/// Airspace files and the cockpit's warnings against the real core.
@MainActor
final class AirspaceTests: XCTestCase {
    private func makeFolder() throws -> URL {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-airspace-test-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    func testAFileIsTakenInDrawnAndRemoved() async throws {
        let folder = try makeFolder()
        let airspace = AirspaceModel(core: try Core(dataFolder: folder.appendingPathComponent("data").path))
        XCTAssertTrue(airspace.isEmpty)
        XCTAssertNil(airspace.set)

        // The sample zone, and an airspace without a shape, which cannot
        // be read and must be reported.
        let text = LaunchOptions.sampleAirspace + "\n\nAC C\nAN NO SHAPE\nAL GND\nAH FL 100\n"
        let file = folder.appendingPathComponent("My Zones.txt")
        try text.write(to: file, atomically: true, encoding: .utf8)
        let notes = folder.appendingPathComponent("notes.txt")
        try "hello".write(to: notes, atomically: true, encoding: .utf8)

        let notice = await airspace.importFiles(at: [file, notes])
        XCTAssertTrue(notice.text.contains("My Zones.txt: 1 airspace read."), notice.text)
        XCTAssertTrue(notice.text.contains("1 airspace could not be read and is left out: you will not be warned about it."), notice.text)
        XCTAssertTrue(notice.text.contains("NO SHAPE"), notice.text)
        XCTAssertTrue(notice.text.contains("notes.txt: This is not an airspace file"), notice.text)

        XCTAssertEqual(airspace.files.map(\.name), ["my-zones"])
        XCTAssertEqual(airspace.files.first?.leftOut, 1)
        XCTAssertEqual(airspace.set?.count(), 1)

        let tegernsee = Bounds(south: 47.6, west: 11.6, north: 47.8, east: 11.9)
        let shapes = airspace.shapes(in: tegernsee)
        XCTAssertEqual(shapes.map(\.name), ["SAMPLE ZONE (NOT REAL)"])
        XCTAssertEqual(shapes.first?.policy, .avoid)
        XCTAssertEqual(
            AirspaceLayers.label(for: try XCTUnwrap(shapes.first)), "SAMPLE ZONE (NOT REAL)\nGND – 9000 ft AMSL"
        )
        XCTAssertTrue(airspace.shapes(in: Bounds(south: 40, west: 0, north: 41, east: 1)).isEmpty)

        airspace.delete(try XCTUnwrap(airspace.files.first))
        XCTAssertTrue(airspace.isEmpty)
        XCTAssertNil(airspace.set)
    }

    func testTheCockpitWarnsNearTheZoneAndTheWarningCanBeQuieted() async throws {
        let folder = try makeFolder()
        let core = try Core(dataFolder: folder.path)
        _ = try core.importAirspace(
            fileName: "zones.txt", bytes: Data(LaunchOptions.sampleAirspace.utf8), nowUnixS: 1_795_000_000
        )
        let set = try core.airspace()
        let fly = FlySession(dataFolder: folder)
        fly.airspace = { set }

        // Two thousand times faster: the whole flight in under a second.
        let sample = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        fly.startReplay(igcText: try String(contentsOf: sample, encoding: .utf8), speed: 2000)
        try await Task.sleep(for: .milliseconds(2500))

        // The flight ends beside the zone.
        let warning = try XCTUnwrap(fly.airspaceWarnings.first)
        XCTAssertEqual(warning.name, "SAMPLE ZONE (NOT REAL)")
        XCTAssertEqual(warning.detail, "Class D control zone · GND to 9000 ft AMSL")
        XCTAssertEqual(warning.source, "zones")
        XCTAssertFalse(warning.silenced)

        fly.acknowledgeAirspace(warning.id)
        XCTAssertEqual(fly.airspaceWarnings.first?.silenced, true)
        fly.unsilenceAirspace(warning.id)
        XCTAssertEqual(fly.airspaceWarnings.first?.silenced, false)
        fly.ignoreAirspaceToday(warning.id)
        XCTAssertEqual(fly.airspaceWarnings.first?.silenced, true)

        _ = fly.finish()
        XCTAssertTrue(fly.airspaceWarnings.isEmpty)
    }

    func testTheSideViewShowsTheZoneOnTheLineAhead() throws {
        let folder = try makeFolder()
        let core = try Core(dataFolder: folder.path)
        _ = try core.importAirspace(
            fileName: "zones.txt", bytes: Data(LaunchOptions.sampleAirspace.utf8), nowUnixS: 1_795_000_000
        )
        let session = FlightSession(dataFolder: folder.path)
        let sample = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        let replay = try Replay(igcText: try String(contentsOf: sample, encoding: .utf8))

        // Nine minutes in, on the glide towards the zone. Without airspace
        // there is nothing to show.
        _ = replay.advance(session: session, seconds: 540)
        XCTAssertNil(session.airspaceAhead(nowUnixS: replay.now()))

        session.useAirspace(airspace: try core.airspace())
        let ahead = try XCTUnwrap(session.airspaceAhead(nowUnixS: replay.now()))
        XCTAssertEqual(ahead.toward, "On course NW")
        XCTAssertEqual(ahead.distanceM, 10_000)
        XCTAssertTrue(ahead.lineWords.hasPrefix("Your glide of the last seconds, "), ahead.lineWords)
        let zone = try XCTUnwrap(ahead.spans.first)
        XCTAssertEqual(zone.name, "SAMPLE ZONE (NOT REAL)")
        XCTAssertEqual(zone.kind, "Class D control zone")
        XCTAssertTrue(zone.inTheWay)
        XCTAssertFalse(zone.uncertain)
        XCTAssertEqual(zone.policy, .avoid)
        XCTAssertTrue(ahead.words.hasPrefix("SAMPLE ZONE (NOT REAL) in 1."), ahead.words)
        XCTAssertTrue(ahead.words.hasSuffix("on your line"), ahead.words)
        // The glider, the end of its line and the zone's ceiling fit in
        // the picture.
        XCTAssertLessThan(ahead.lineEndM, ahead.altitudeM)
        XCTAssertLessThan(ahead.bottomM, ahead.lineEndM)
        XCTAssertGreaterThan(ahead.topM, try XCTUnwrap(zone.ceilingM))

        // A session that is not running has no side view.
        XCTAssertNil(FlySession(dataFolder: folder).airspaceAhead())
    }

    func testAirspaceIsFetchedFromTheServerOnceKeptAndUsed() async throws {
        let folder = try makeFolder()
        let asked = Requests()
        let airspace = AirspaceModel(
            core: try Core(dataFolder: folder.path),
            transport: { url in
                await asked.note(url)
                return (200, Data(ServerAirspace.answer.utf8))
            },
            remembersChoice: false
        )
        let tegernsee = Coordinate(lat: 47.7, lon: 11.75)

        // Without a server nothing is asked.
        await airspace.refresh(around: tegernsee)
        let none = await asked.addresses
        XCTAssertTrue(none.isEmpty)
        XCTAssertEqual(airspace.inALine, "None")

        airspace.server = { URL(string: "http://127.0.0.1:8080") }
        await airspace.refresh(around: tegernsee)
        let first = await asked.addresses
        XCTAssertEqual(first.map(\.absoluteString), ["http://127.0.0.1:8080/v1/airspace?bbox=8.75,45.7,14.75,49.7"])
        XCTAssertNil(airspace.serverProblem)
        XCTAssertEqual(airspace.inALine, "openAIP, 1 airspace")
        let layer = try XCTUnwrap(airspace.kept.first)
        XCTAssertTrue(layer.saysWhatItCovers)
        XCTAssertEqual(layer.countries, ["DE", "AT"])
        XCTAssertEqual(AirspaceScreen.names(of: ["DE"]), "Germany")
        XCTAssertEqual(layer.credit?.notice, "OpenAIP data is not certified.")
        XCTAssertEqual(airspace.credits.map(\.provider), ["openAIP"])
        XCTAssertEqual(
            airspace.shapes(in: Bounds(south: 47.6, west: 11.6, north: 47.8, east: 11.9)).map(\.name),
            ["SERVER ZONE (NOT REAL)"]
        )

        // What is kept is fresh: a second look around sends nothing.
        await airspace.refresh(around: Coordinate(lat: 47.5, lon: 11.9))
        let second = await asked.addresses
        XCTAssertEqual(second.count, 1)

        // The cockpit warns from it, and says where it is from.
        let fly = FlySession(dataFolder: folder)
        fly.airspace = { airspace.set }
        let sample = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        fly.startReplay(igcText: try String(contentsOf: sample, encoding: .utf8), speed: 2000)
        try await Task.sleep(for: .milliseconds(2500))
        let warning = try XCTUnwrap(fly.airspaceWarnings.first)
        XCTAssertEqual(warning.origin, .server)
        XCTAssertTrue(
            AirspaceWarningsSheet.origin(of: warning).hasPrefix("From openAIP, by way of Venti, as of "),
            AirspaceWarningsSheet.origin(of: warning)
        )
        _ = fly.finish()

        // Switched off, what was kept is gone and nothing is asked.
        airspace.stopUsingServer()
        XCTAssertTrue(airspace.isEmpty)
        await airspace.refresh(around: tegernsee)
        let third = await asked.addresses
        XCTAssertEqual(third.count, 1)
    }

    func testAServerThatCannotAnswerIsSaidAndLeftAloneForAWhile() async throws {
        let folder = try makeFolder()
        let asked = Requests()
        let airspace = AirspaceModel(
            core: try Core(dataFolder: folder.path),
            transport: { url in
                await asked.note(url)
                return (503, Data())
            },
            remembersChoice: false
        )
        airspace.server = { URL(string: "http://127.0.0.1:8080") }
        let tegernsee = Coordinate(lat: 47.7, lon: 11.75)

        await airspace.refresh(around: tegernsee)
        XCTAssertEqual(airspace.serverProblem, "The server is busy or being restarted. Try again in a moment.")
        XCTAssertTrue(airspace.isEmpty)

        await airspace.refresh(around: tegernsee)
        let addresses = await asked.addresses
        XCTAssertEqual(addresses.count, 1)
    }

    func testTheCockpitsMapZoomsOutAsFarAsItTakesToShowTheAirspace() {
        let size = CGSize(width: 400, height: 360)
        let zoom = { (circling: Bool, metres: Double?) in
            CockpitMap.Coordinator.zoom(circling: circling, showing: metres, latitude: 47.7, in: size)
        }
        // Without a warning the kind of flying decides.
        XCTAssertEqual(zoom(false, nil), 13)
        XCTAssertEqual(zoom(true, nil), 15.5)
        // Two kilometres do not fit at the cruising zoom; one does.
        XCTAssertEqual(zoom(false, 2000), 11.5)
        XCTAssertEqual(zoom(false, 900), 13)
        // In a thermal the map is close in, and steps back for a zone
        // 500 m away.
        XCTAssertEqual(zoom(true, 500), 13.5)
        XCTAssertEqual(zoom(true, 100), 15.5)
        // Never out to where the valley is a dot.
        XCTAssertEqual(zoom(false, 80_000), 10)
        // Before the map has a size nothing is worked out from it.
        XCTAssertEqual(
            CockpitMap.Coordinator.zoom(circling: false, showing: 2000, latitude: 47.7, in: .zero), 13
        )
    }

    func testAFlightIsCheckedAgainstTheAirspaceTheAppHas() async throws {
        let folder = try makeFolder()
        let core = try Core(dataFolder: folder.path)
        let logbook = LogbookModel(core: core, folder: folder)
        let added = await logbook.addSample()
        let flight = try XCTUnwrap(added.added.first)

        // Without airspace nothing is checked, and the screen says what to do.
        let unchecked = try await logbook.airspace(of: flight)
        XCTAssertEqual(unchecked.cover, .none)
        XCTAssertTrue(unchecked.crossings.isEmpty)
        XCTAssertTrue(FlightDetailsSheet.caveat(for: .none).hasPrefix("Add an airspace file"))

        // With the sample zone: the flight clips its edge for about a
        // minute, twelve minutes in.
        _ = try core.importAirspace(
            fileName: "zones.txt", bytes: Data(LaunchOptions.sampleAirspace.utf8), nowUnixS: 1_795_000_000
        )
        let checked = try await logbook.airspace(of: flight)
        XCTAssertEqual(checked.cover, .whole)
        XCTAssertEqual(checked.crossings.map(\.name), ["SAMPLE ZONE (NOT REAL)"])
        let crossing = try XCTUnwrap(checked.crossings.first)
        XCTAssertEqual(crossing.policy, .avoid)
        XCTAssertNil(crossing.uncertain)
        XCTAssertGreaterThan(crossing.entry.seconds, 600)
        XCTAssertGreaterThan(crossing.seconds, 30)
        XCTAssertTrue(FlightDetailsSheet.caveat(for: .whole).contains("not the airspace of the day of the flight"))
    }

    func testTheWholeFlightIsTheBoxAroundEverythingItHasToShow() {
        let box = CockpitMap.Coordinator.box(around: [
            Coordinate(lat: 47.66, lon: 11.80), Coordinate(lat: 47.71, lon: 11.52),
            Coordinate(lat: 47.58, lon: 11.66),
        ])
        XCTAssertEqual(box, Bounds(south: 47.58, west: 11.52, north: 47.71, east: 11.80))
        // One place is a box without extent; the map gives it a margin.
        let here = CockpitMap.Coordinator.box(around: [Coordinate(lat: 47.66, lon: 11.80)])
        XCTAssertEqual(here.south, here.north)
    }

    func testAnImportIsSummedUpForThePilot() {
        XCTAssertEqual(
            AirspaceModel.summary(
                of: AirspaceImport(airspaces: 312, leftOut: 0, assumed: 0, problems: []), file: "de.txt"
            ),
            "de.txt: 312 airspaces read."
        )
        let troubled = AirspaceModel.summary(
            of: AirspaceImport(airspaces: 10, leftOut: 2, assumed: 1, problems: ["Line 7 (X): no shape."]),
            file: "x.txt"
        )
        XCTAssertTrue(troubled.contains("2 airspaces could not be read and are left out: you will not be warned about them."))
        XCTAssertTrue(troubled.contains("In 1 place something could not be read and the cautious reading was taken."))
        XCTAssertTrue(troubled.hasSuffix("Line 7 (X): no shape."))
    }

    func testTheCockpitCreditsTheAirspaceItDrawsFromAServer() {
        XCTAssertNil(CockpitScreen.airspaceCredit([]))
        let openAip = Credit(
            source: "openaip", provider: "openAIP", notice: "Not certified.", text: "openAIP",
            licence: "CC BY-NC 4.0", url: "https://www.openaip.net/"
        )
        XCTAssertEqual(CockpitScreen.airspaceCredit([openAip]), "Airspace: openAIP, not certified")
        let plain = Credit(
            source: "other", provider: "Another", notice: nil, text: "Another",
            licence: "CC BY 4.0", url: "https://example.org/"
        )
        XCTAssertEqual(CockpitScreen.airspaceCredit([plain]), "Airspace: Another")
    }
}
