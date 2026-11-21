import VentiCore
import XCTest

@testable import Venti

/// Answers in the server's own format.
private enum Canned {
    static let health = """
        {"status": "ok", "version": "0.1.0", "time": "2026-11-18T19:05:00Z", "sources": [
          {"source": "dwd-10min", "provider": "Deutscher Wetterdienst", "cadence_seconds": 600,
           "stale": true, "stations": 2}
        ]}
        """

    static let stations = """
        {"stations": [
          {"id": "dwd-10min:05792", "source": "dwd-10min", "name": "Zugspitze",
           "position": {"lat": 47.421, "lon": 10.9848}, "elevation_m": 2956.0,
           "latest": {"time": "2026-11-18T18:20:00Z", "wind_speed_ms": 14.9, "wind_gust_ms": 25.7,
                      "wind_direction_deg": 260.0, "temperature_c": -7.1}},
          {"id": "dwd-10min:01550", "source": "dwd-10min", "name": "Garmisch-Partenkirchen",
           "position": {"lat": 47.483, "lon": 11.0621}, "elevation_m": 719.0,
           "latest": {"time": "2026-11-18T14:00:00Z", "wind_speed_ms": 1.3, "wind_direction_deg": 350.0}}
        ], "truncated": false, "attributions": [
          {"source": "dwd-10min", "provider": "Deutscher Wetterdienst", "licence": "CC BY 4.0",
           "text": "Quelle: Deutscher Wetterdienst", "url": "https://www.dwd.de/"}
        ]}
        """

    static let history = """
        {"station": {"id": "dwd-10min:05792", "source": "dwd-10min", "name": "Zugspitze",
                     "position": {"lat": 47.421, "lon": 10.9848}, "elevation_m": 2956.0},
         "observations": [
           {"time": "2026-11-18T18:10:00Z", "wind_speed_ms": 10.0, "wind_gust_ms": 20.0, "wind_direction_deg": 250.0},
           {"time": "2026-11-18T18:20:00Z", "wind_speed_ms": 14.9, "wind_gust_ms": 25.7, "wind_direction_deg": 260.0}
         ],
         "attributions": [
          {"source": "dwd-10min", "provider": "Deutscher Wetterdienst", "licence": "CC BY 4.0",
           "text": "Quelle: Deutscher Wetterdienst", "url": "https://www.dwd.de/"}
         ]}
        """
}

/// The wind model against the real core, with the server played by canned
/// answers in the server's own format.
@MainActor
final class WindModelTests: XCTestCase {
    /// The addresses the model asked for.
    private actor Asked {
        var urls: [URL] = []
        func add(_ url: URL) { urls.append(url) }
    }

    /// 18 November 2026, 19:05 UTC, as the server's clock in these answers.
    private static let serverNow: Int64 = 1_795_028_700

    /// A server that answers like the real one, and notes what it was asked.
    private func server(noting asked: Asked) -> WindModel.Transport {
        { url in
            await asked.add(url)
            let body =
                switch url.path {
                case "/v1/health": Canned.health
                case "/v1/stations": Canned.stations
                default: Canned.history
                }
            return (200, Data(body.utf8))
        }
    }

    private let alps = Bounds(south: 47.0, west: 10.5, north: 48.0, east: 12.0)

    func testAnAddressIsReadTheWayPeopleWriteThem() {
        XCTAssertEqual(WindModel.server(from: " wind.example.org/ ")?.absoluteString, "https://wind.example.org")
        XCTAssertEqual(
            WindModel.server(from: "http://192.168.1.20:8080")?.absoluteString, "http://192.168.1.20:8080"
        )
        XCTAssertNil(WindModel.server(from: ""))
        XCTAssertNil(WindModel.server(from: "ftp://example.org"))
        XCTAssertNil(WindModel.server(from: "not an address"))
    }

    func testStationsComeFromTheServerWithTheirAgeByTheServersClock() async throws {
        let asked = Asked()
        let wind = WindModel(address: "http://127.0.0.1:8080", transport: server(noting: asked))

        wind.show(alps)
        await wind.load()

        XCTAssertNil(wind.problem)
        XCTAssertEqual(wind.stations.map(\.name), ["Zugspitze", "Garmisch-Partenkirchen"])
        XCTAssertEqual(wind.credits.map(\.provider), ["Deutscher Wetterdienst"])
        XCTAssertEqual(wind.lateProviders, ["Deutscher Wetterdienst"])
        // The phone's clock says a later day; ages go by the server's.
        XCTAssertEqual(Double(wind.now), Double(Self.serverNow), accuracy: 5)
        XCTAssertEqual(wind.stations[0].ageMin, 45)
        XCTAssertFalse(wind.stations[0].isOld)
        XCTAssertTrue(wind.stations[1].isOld)

        let urls = await asked.urls
        XCTAssertEqual(urls.first?.path, "/v1/health")
        // More than is in view was asked for, so a small drag needs nothing new.
        let box = try XCTUnwrap(urls.first { $0.path == "/v1/stations" }?.query)
        XCTAssertEqual(box, "bbox=10.125,46.75,12.375,48.25")

        let history = try await wind.history(of: wind.stations[0])
        XCTAssertEqual(history.readings.count, 2)
        XCTAssertEqual(StationScreen.samples(from: history.readings).last?.gusts ?? 0, 25.7 * 3.6, accuracy: 0.01)
        let last = await asked.urls.last
        XCTAssertEqual(last?.path, "/v1/stations/dwd-10min:05792/observations")
    }

    func testTheCockpitGetsTheWindOnTheGroundFromTheStationsAround() async throws {
        let asked = Asked()
        let wind = WindModel(address: "http://127.0.0.1:8080", transport: server(noting: asked))
        // The map tab has read the server's clock before the flight.
        wind.show(alps)
        await wind.load()

        // A glider over Garmisch, with a landing in the valley below.
        let glider = Coordinate(lat: 47.48, lon: 11.06)
        let landing = GroundWindPlace(
            name: "Valley landing", position: Coordinate(lat: 47.49, lon: 11.08), elevationM: 710
        )
        let answer = await wind.stations(around: glider, andAround: landing.position)
        let found = try XCTUnwrap(answer)
        // Some eighty kilometres every way, asked for beside the map's own.
        let last = await asked.urls.last
        XCTAssertEqual(last?.path, "/v1/stations")
        XCTAssertTrue(last?.query?.hasPrefix("bbox=9.9") == true, last?.query ?? "")
        XCTAssertEqual(wind.stations.count, 2)

        let below = groundWind(
            stations: found.stations, position: glider, landing: landing, trimSpeedMs: 38 / 3.6
        )

        // Garmisch's reading is five hours old and left out. The
        // Zugspitze's is three quarters of an hour old, which is how late
        // the weather service's readings come, and says so. It is strong,
        // but nine kilometres off and two thousand metres up: it does not
        // speak for the landing, and nothing is warned of.
        XCTAssertEqual(below.stations.map(\.name), ["Zugspitze"])
        let summit = try XCTUnwrap(below.stations.first)
        XCTAssertEqual(summit.wind, "W 54/93 km/h")
        XCTAssertEqual(summit.age, "45 min ago")
        XCTAssertTrue(summit.isStrong)
        XCTAssertFalse(summit.isAtLanding)
        XCTAssertNil(below.alert)
        XCTAssertEqual(found.credits.map(\.text), ["Quelle: Deutscher Wetterdienst"])

        // A landing at the foot of the summit station, about as high as
        // it: there its wind is the landing's, and is warned of.
        let up = GroundWindPlace(
            name: "Summit plateau", position: Coordinate(lat: 47.42, lon: 10.99), elevationM: 2900
        )
        let warned = groundWind(stations: found.stations, position: glider, landing: up, trimSpeedMs: 38 / 3.6)
        XCTAssertEqual(warned.alert?.title, "Strong wind at Summit plateau")
        XCTAssertEqual(
            warned.alert?.spoken,
            "Strong wind at the landing. 54 kilometres per hour, gusts 93, from the west."
        )

        // A pilot who went straight to the cockpit: the map tab has not
        // read the server's clock, so the cockpit's asking reads it, and
        // ages are still the server's.
        let straight = WindModel(address: "http://127.0.0.1:8080", transport: server(noting: Asked()))
        let first = await straight.stations(around: glider, andAround: nil)
        XCTAssertEqual(first?.stations.first { $0.name == "Zugspitze" }?.ageMin, 45)

        // Without a server, and from a server that does not answer, there
        // is nothing, and nothing to say about it in the air.
        let none = await WindModel(address: "").stations(around: glider, andAround: nil)
        XCTAssertNil(none)
        let silent = WindModel(address: "http://127.0.0.1:8080") { _ in throw URLError(.timedOut) }
        let nothing = await silent.stations(around: glider, andAround: nil)
        XCTAssertNil(nothing)
    }

    func testAServerThatCannotBeReachedIsSaidAndLeavesNoStations() async {
        let wind = WindModel(address: "wind.example.org") { _ in
            throw URLError(.cannotConnectToHost)
        }

        wind.show(alps)
        await wind.load()

        XCTAssertTrue(wind.stations.isEmpty)
        XCTAssertTrue(wind.problem?.contains("could not be reached") ?? false)
    }

    func testARefusalIsPassedOnInTheCoresWords() async {
        let wind = WindModel(address: "wind.example.org") { url in
            url.path == "/v1/health"
                ? (200, Data(Canned.health.utf8))
                : (503, Data(#"{"error": {"code": "unavailable", "message": "no database"}}"#.utf8))
        }

        wind.show(alps)
        await wind.load()

        XCTAssertEqual(wind.problem, "The server is busy or being restarted. Try again in a moment.")
    }

    func testChangingTheServerForgetsTheOldOnesStations() async {
        let wind = WindModel(address: "http://127.0.0.1:8080", transport: server(noting: Asked()))
        wind.show(alps)
        await wind.load()
        XCTAssertFalse(wind.stations.isEmpty)

        wind.use(address: "")

        XCTAssertNil(wind.server)
        XCTAssertTrue(wind.stations.isEmpty)
        XCTAssertTrue(wind.credits.isEmpty)
    }

    func testAStationIsDrawnPointingWhereTheWindBlows() async {
        let wind = WindModel(address: "http://127.0.0.1:8080", transport: server(noting: Asked()))
        wind.show(alps)
        await wind.load()

        // From 260 degrees, a little south of west: the arrow points east.
        let summit = wind.stations[0].mark
        XCTAssertEqual(summit.symbol, .arrow(band: .strong))
        XCTAssertEqual(summit.headingDeg, 80, accuracy: 0.001)
        XCTAssertEqual(summit.label, "54/93")

        // A reading from hours ago is not drawn as wind.
        let valley = wind.stations[1].mark
        XCTAssertEqual(valley.symbol, .old)
        XCTAssertEqual(valley.label, "")

        // The picture's name says which band it is.
        XCTAssertEqual(WindSymbols.name(of: summit.symbol), "wind-arrow-strong")
        XCTAssertEqual(WindSymbols.name(of: valley.symbol), "wind-old")
        XCTAssertEqual(Format.spokenKilometresPerHour(metresPerSecond: 0.3), "1 kilometre per hour")
        XCTAssertEqual(Format.spokenKilometresPerHour(metresPerSecond: 3.0), "11 kilometres per hour")
    }

    func testAgesAreWordedForAPilot() {
        XCTAssertEqual(Format.age(minutes: 0), "just now")
        XCTAssertEqual(Format.age(minutes: 1), "1 minute ago")
        XCTAssertEqual(Format.age(minutes: 45), "45 minutes ago")
        XCTAssertEqual(Format.age(minutes: 185), "3 hours ago")
        XCTAssertEqual(Format.age(minutes: 3 * 1440), "3 days ago")
        XCTAssertEqual(Format.minutes(from: 120, to: 60), 0)
        XCTAssertEqual(Format.temperature(celsius: -0.2), "0")
        XCTAssertEqual(Format.place(lat: 47.66421, lon: 11.79668), "47.6642° N, 11.7967° E")
        XCTAssertEqual(Format.place(lat: -33.5, lon: -70.25), "33.5000° S, 70.2500° W")
        XCTAssertEqual(Format.dayAndTime(unixSeconds: 0).isEmpty, false)
    }

    /// A past day of a station is asked for by its date, and comes back as
    /// that day, from its first moment to its last.
    func testAPastDayOfAStationIsAskedForByItsDate() async throws {
        let asked = Asked()
        let wind = WindModel(address: "http://127.0.0.1:8080", transport: server(noting: asked))
        wind.show(alps)
        await wind.load()
        let station = try XCTUnwrap(wind.stations.first)
        let day = try await wind.day(of: station, date: "2026-11-17")
        XCTAssertEqual(day.date, "2026-11-17")
        XCTAssertLessThan(day.startUnixS, day.endUnixS)
        let urls = await asked.urls
        let url = try XCTUnwrap(urls.last)
        XCTAssertTrue(url.absoluteString.contains("2026-11-17"), url.absoluteString)
        XCTAssertTrue(url.absoluteString.contains(station.id), url.absoluteString)
    }

    /// The step in temperature to another station, in one line: the other
    /// station, how far and how much higher, and the figure per 100 m.
    func testTheTemperatureStepIsWrittenInOneLine() {
        let step = TemperatureStep(
            otherId: "x", otherName: "Wallberg", distanceM: 4200, higherByM: 920, otherTemperatureC: 2.6,
            per100mC: -0.62, kind: .coolerAbove, words: "Cooler above, as in air at rest", rule: "", note: ""
        )
        XCTAssertEqual(
            StationScreen.stepLine(step), "Wallberg, 4.2 km away, 920 m higher, 3 °C: \u{2212}0.6 °C per 100 m"
        )
        var lower = step
        lower.higherByM = -650
        lower.per100mC = 0.3
        XCTAssertTrue(StationScreen.stepLine(lower).contains("650 m lower"))
        XCTAssertTrue(StationScreen.stepLine(lower).hasSuffix("+0.3 °C per 100 m"))
    }
}
