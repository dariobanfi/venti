import VentiCore
import XCTest

@testable import Venti

/// A summer day's answer for the Tegernsee valley in the server's own
/// format: two models, the coarser one first, as the server may list them.
private enum CannedForecast {
    static let answer = """
        {"point": {"lat": 47.7, "lon": 11.8}, "fetched_at": "2026-07-04T09:40:00Z",
         "layers": [{"source": "open-meteo", "series": [
           {"source": "open-meteo", "model": "icon-eu", "run": "2026-07-04T06:00:00Z",
            "point": {"lat": 47.7, "lon": 11.8}, "cell": {"lat": 47.6875, "lon": 11.8125},
            "cell_elevation_m": 905.0,
            "hours": [{"time": "2026-07-04T10:00:00Z", "wind_speed_ms": 3.0}]},
           {"source": "open-meteo", "model": "icon-d2", "run": "2026-07-04T06:00:00Z",
            "point": {"lat": 47.7, "lon": 11.8}, "cell": {"lat": 47.7, "lon": 11.8},
            "cell_elevation_m": 812.0,
            "hours": [
              {"time": "2026-07-04T10:00:00Z", "wind_speed_ms": 2.5, "wind_gust_ms": 6.9,
               "wind_direction_deg": 250.0, "temperature_c": 24.3, "cloud_cover_pct": 30.0,
               "precipitation_mm": 0.0, "freezing_level_m": 3900.0, "convective_cloud_base_m": 2440.0,
               "winds_aloft": [
                 {"above_ground_m": 80.0, "wind_speed_ms": 3.6, "wind_direction_deg": 255.0},
                 {"pressure_hpa": 850.0, "altitude_m": 1532.0, "wind_speed_ms": 5.6, "wind_direction_deg": 270.0},
                 {"pressure_hpa": 700.0, "altitude_m": 3140.0, "wind_speed_ms": 9.4, "wind_direction_deg": 290.0}
               ]},
              {"time": "2026-07-04T11:00:00Z", "wind_speed_ms": 3.0, "wind_direction_deg": 260.0,
               "temperature_c": 25.4, "cloud_cover_pct": 80.0, "precipitation_mm": 1.24,
               "winds_aloft": [
                 {"pressure_hpa": 850.0, "altitude_m": 1548.0, "wind_speed_ms": 4.0}
               ]},
              {"time": "2026-07-04T23:00:00Z", "wind_speed_ms": 0.8, "wind_direction_deg": 180.0}
            ]}
         ]}],
         "attributions": [{"source": "open-meteo", "provider": "Open-Meteo", "licence": "CC BY 4.0",
                           "text": "Weather data by Open-Meteo.com", "url": "https://open-meteo.com/"}]}
        """
}

/// The forecast of a place: the wind model asks the server, the real core
/// reads the answer, and the table lays it out.
@MainActor
final class ForecastTests: XCTestCase {
    /// The addresses the model asked for.
    private actor Asked {
        var urls: [URL] = []
        func add(_ url: URL) { urls.append(url) }
    }

    private func fetched(noting asked: Asked = Asked()) async throws -> ForecastView {
        let wind = WindModel(address: "http://127.0.0.1:8080") { url in
            await asked.add(url)
            return (200, Data(CannedForecast.answer.utf8))
        }
        return try await wind.forecast(at: Coordinate(lat: 47.7, lon: 11.8))
    }

    func testTheForecastOfAPlaceIsAskedOfTheServerAndReadModelByModel() async throws {
        let asked = Asked()
        let forecast = try await fetched(noting: asked)

        let url = await asked.urls.last
        XCTAssertEqual(url?.path, "/v1/forecast")
        XCTAssertEqual(url?.query, "lat=47.7&lon=11.8")
        // The finest model first, whatever order the server gave.
        XCTAssertEqual(forecast.models.map(\.name), ["ICON-D2", "ICON-EU"])
        XCTAssertEqual(forecast.credits.map(\.text), ["Weather data by Open-Meteo.com"])
        XCTAssertTrue(ForecastScreen.caveat(for: forecast.models[0]).contains("812 m"))
        XCTAssertTrue(ForecastScreen.caveat(for: forecast.models[0]).contains("2 km cells"))
    }

    func testAServerWithoutForecastsIsSaidInWords() async {
        let wind = WindModel(address: "http://127.0.0.1:8080") { _ in
            (503, Data(#"{"error": {"code": "forecast_unavailable", "message": "x"}}"#.utf8))
        }
        do {
            _ = try await wind.forecast(at: Coordinate(lat: 47.7, lon: 11.8))
            XCTFail("A refusal was read as a forecast")
        } catch {
            XCTAssertTrue(error is CoreError)
            XCTAssertFalse(error.words.isEmpty)
        }
    }

    func testTheTableHasTheWindFromTheHighestLevelDownAndThenTheSky() async throws {
        let model = try await fetched().models[0]
        // Every hour of the answer, so that the test does not depend on
        // where the sun stands by this machine's clock.
        let hours = model.days.flatMap(\.hours)
        XCTAssertEqual(hours.count, 3)

        let rows = ForecastTable.rows(for: model, hours: hours)
        XCTAssertEqual(
            rows.map(\.id),
            ["700 hPa", "850 hPa", "80 m above ground", "ground", "base", "cloud", "rain", "temperature", "freezing"]
        )
        // A pressure surface is named by about how high it lies.
        XCTAssertEqual(rows[0].name, "3,100 m")
        XCTAssertEqual(rows[0].detail, "700 hPa")
        XCTAssertEqual(rows[2].name, "80 m")
        XCTAssertEqual(rows[2].detail, "above ground")

        // An hour without a level is a gap in its row, not a shift.
        guard case .wind(let high) = rows[0].cells[0] else {
            return XCTFail("No wind at 700 hPa in the first hour")
        }
        XCTAssertEqual(high.label, "34")
        XCTAssertEqual(high.band, .strong)
        XCTAssertEqual(rows[0].cells[1], .empty)
        XCTAssertEqual(rows.map(\.cells.count), Array(repeating: 3, count: rows.count))

        // The ground wind carries its gusts.
        guard case .wind(let ground) = rows[3].cells[0] else {
            return XCTFail("No wind at the ground in the first hour")
        }
        XCTAssertEqual(ground.label, "9/25")

        XCTAssertEqual(rows[4].cells, [.number("2,400"), .empty, .empty])
        XCTAssertEqual(rows[5].cells, [.cloud(percent: 30), .cloud(percent: 80), .empty])
        // A dry hour is a dot, so that the wet ones stand out.
        XCTAssertEqual(rows[6].cells, [.number(ForecastTable.dry), .number("1.2"), .empty])
        XCTAssertEqual(rows[7].cells, [.number("24"), .number("25"), .empty])
        XCTAssertEqual(rows[8].cells, [.number("3,900"), .empty, .empty])
    }

    func testAModelWithOnlyAGroundWindHasOnlyThatRow() async throws {
        let coarse = try await fetched().models[1]
        let rows = ForecastTable.rows(for: coarse, hours: coarse.days.flatMap(\.hours))
        XCTAssertEqual(rows.map(\.id), ["ground"])
    }

    func testOnlyTheHoursOfDaylightGetAColumnUnlessThereAreNone() async throws {
        let model = try await fetched().models[0]
        // The day with light in it, whatever this machine's time zone is.
        let day = try XCTUnwrap(model.days.first { $0.hours.contains(where: \.isDaylight) })
        let light = ForecastTable.hours(of: day)
        XCTAssertFalse(light.isEmpty)
        XCTAssertTrue(light.allSatisfy(\.isDaylight))

        let night = ForecastDay(
            startUnixS: day.startUnixS, sunriseUnixS: nil, sunsetUnixS: nil,
            hours: day.hours.map { hour in
                var dark = hour
                dark.isDaylight = false
                return dark
            }
        )
        XCTAssertEqual(ForecastTable.hours(of: night).count, day.hours.count)
    }
}
