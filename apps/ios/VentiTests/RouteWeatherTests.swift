import VentiCore
import XCTest

@testable import Venti

/// The weather along a drawn route: asking the server once for a route
/// and a day, the core again for every setting, the ground once for a
/// route, and the settings' days and times. Against the real core, with
/// the server's golden answer (`crates/server/tests/golden`), read from
/// the repository: a simulator reads the Mac's files.
@MainActor
final class RouteWeatherTests: XCTestCase {
    /// The development mode's answer for the Wallberg, a place south-east
    /// of it and the Brauneck, on 22 November 2026.
    private static func recorded() throws -> Data {
        let here = URL(fileURLWithPath: #filePath)
        let file = here.deletingLastPathComponent()
            .appendingPathComponent("../../../crates/server/tests/golden/route_weather.json")
        return try Data(contentsOf: file.standardizedFileURL)
    }

    private static let places = [
        Coordinate(lat: 47.6642, lon: 11.7967),
        Coordinate(lat: 47.62, lon: 11.91),
        Coordinate(lat: 47.6603, lon: 11.5197),
    ]
    /// 10:00 in Berlin on 22 November 2026.
    private static let start: Int64 = 1_795_338_000
    private static let berlin = TimeZone(identifier: "Europe/Berlin")!

    /// What the server was asked.
    private actor Asked {
        var paths: [String] = []
        func add(_ url: URL) { paths.append(url.path + (url.query.map { "?" + $0 } ?? "")) }
    }

    private func model(answering status: Int, with body: Data, noting asked: Asked) -> RouteWeatherModel {
        let weather = RouteWeatherModel { url in
            await asked.add(url)
            return (status, body)
        }
        let address = URL(string: "http://127.0.0.1:8080")!
        weather.server = { address }
        return weather
    }

    func testARouteAndADayAreAskedForOnceAndTheCoreReadsEverySetting() async throws {
        let asked = Asked()
        let weather = model(answering: 200, with: try Self.recorded(), noting: asked)

        await weather.show(Self.places, on: "2026-11-22")
        XCTAssertNil(weather.problem)
        XCTAssertFalse(weather.isAsking)
        let paths = await asked.paths
        XCTAssertEqual(
            paths, ["/v1/route/weather?date=2026-11-22&points=47.6642,11.7967;47.62,11.91;47.6603,11.5197"]
        )

        let along = weather.along(Self.places, startUnixS: Self.start, speedKmh: 20, glideRatio: 8, zone: Self.berlin)
        XCTAssertEqual(along.points.count, 3)
        XCTAssertEqual(along.legs.count, 2)
        XCTAssertEqual(along.points[0].clock, "10:00")
        XCTAssertTrue(along.points[0].words.contains("10:00"), along.points[0].words)
        XCTAssertNotNil(along.model)
        XCTAssertTrue(along.words.contains("20 km/h"), along.words)
        // Every place but the last says the height to leave it at.
        XCTAssertFalse(along.points[0].departureWords.isEmpty)
        XCTAssertTrue(along.points[2].departureWords.isEmpty)

        // Faster, or the same route and day again: the answer held, read
        // again by the core, and no new asking.
        let faster = weather.along(Self.places, startUnixS: Self.start, speedKmh: 30, glideRatio: 8, zone: Self.berlin)
        XCTAssertLessThan(faster.finishUnixS, along.finishUnixS)
        await weather.show(Self.places, on: "2026-11-22")
        let again = await asked.paths
        XCTAssertEqual(again.count, 1)

        // Another day is another asking.
        await weather.show(Self.places, on: "2026-11-23")
        let other = await asked.paths
        XCTAssertEqual(other.count, 2)
        XCTAssertTrue(other[1].contains("date=2026-11-23"), other[1])
    }

    func testWithoutAForecastTheTimesStandAndTheProblemIsSaid() async throws {
        // Refused: the core's words, and "Try again" asks anew.
        let asked = Asked()
        let refused = model(answering: 503, with: Data(), noting: asked)
        await refused.show(Self.places, on: "2026-11-22")
        let problem = try XCTUnwrap(refused.problem)
        XCTAssertFalse(problem.isEmpty)
        XCTAssertNil(refused.forecast)
        let along = refused.along(Self.places, startUnixS: Self.start, speedKmh: 20, glideRatio: 8, zone: Self.berlin)
        XCTAssertEqual(along.legs.count, 2)
        XCTAssertNil(along.model)
        XCTAssertEqual(along.points[1].clock.count, 5)
        refused.forgetForecast()
        await refused.show(Self.places, on: "2026-11-22")
        let paths = await asked.paths
        XCTAssertEqual(paths.count, 2)

        // Without a server: said so, and nothing asked.
        let nowhere = RouteWeatherModel { _ in
            XCTFail("Nothing is asked without a server.")
            return (500, Data())
        }
        await nowhere.show(Self.places, on: "2026-11-22")
        XCTAssertEqual(nowhere.problem, RouteWeatherWords.noServer)

        // One place is no route: nothing is asked, and nothing said.
        await refused.show([Self.places[0]], on: "2026-11-22")
        XCTAssertNil(refused.problem)
        let still = await asked.paths
        XCTAssertEqual(still.count, 2)
    }

    func testTheGroundIsBroughtOnceForARouteAndUnderAllOfIt() async throws {
        let asked = Asked()
        let weather = model(answering: 200, with: try Self.recorded(), noting: asked)
        var areas: [Bounds] = []
        weather.ground = { area in
            areas.append(area)
            return Terrain()
        }
        await weather.show(Self.places, on: "2026-11-22")
        await weather.show(Self.places, on: "2026-11-23")
        XCTAssertEqual(areas.count, 1)
        XCTAssertNotNil(weather.heights)
        let area = try XCTUnwrap(areas.first)
        XCTAssertEqual(area.south, 47.6, accuracy: 1e-9)
        XCTAssertEqual(area.north, 47.6842, accuracy: 1e-9)
        XCTAssertEqual(area.west, 11.4997, accuracy: 1e-9)
        XCTAssertEqual(area.east, 11.93, accuracy: 1e-9)

        // A terrain without tiles: the heights say the ground is not
        // loaded, and the times still stand.
        let along = weather.along(Self.places, startUnixS: Self.start, speedKmh: 20, glideRatio: 8, zone: Self.berlin)
        XCTAssertFalse(along.points[0].departureWords.isEmpty)

        // Another route: the ground under it.
        await weather.show(Array(Self.places.prefix(2)), on: "2026-11-22")
        XCTAssertEqual(areas.count, 2)
    }

    func testTheStartIsTheNextFullHourOnThePilotsClock() throws {
        // 18 November 2026, 17:40 UTC: 18:40 in Berlin.
        let evening: Int64 = 1_794_960_000 + 17 * 3600 + 40 * 60
        XCTAssertEqual(RouteWeatherWords.nextFullHour(after: evening, zone: Self.berlin), 19 * 60)
        XCTAssertEqual(RouteWeatherWords.date(of: evening, zone: Self.berlin), "2026-11-18")
        // Late in the evening it is the day's last hour.
        XCTAssertEqual(RouteWeatherWords.nextFullHour(after: evening + 5 * 3600, zone: Self.berlin), 23 * 60)
        XCTAssertEqual(
            RouteWeatherWords.startUnixS(date: "2026-11-22", minuteOfDay: 10 * 60, zone: Self.berlin), Self.start
        )
        XCTAssertNil(RouteWeatherWords.startUnixS(date: "not a day", minuteOfDay: 0))
        XCTAssertEqual(Format.clock(minuteOfDay: 9 * 60 + 30), "09:30")
    }
}
