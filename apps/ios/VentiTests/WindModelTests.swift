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
        XCTAssertEqual(StationScreen.samples(from: history).last?.gusts ?? 0, 25.7 * 3.6, accuracy: 0.01)
        let last = await asked.urls.last
        XCTAssertEqual(last?.path, "/v1/stations/dwd-10min:05792/observations")
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
    }
}
