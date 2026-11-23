import MapLibre
import SwiftUI
import VentiCore
import XCTest

@testable import Venti

/// The forecast map by the hour: asking for a layer of a day, keeping the
/// answers of this run of the app, the strip of a square's days, and how
/// a key's line is coloured. Against the real core, with the server's own
/// golden answers (`crates/server/tests/golden`), read from the
/// repository: a simulator reads the Mac's files.
@MainActor
final class ThermalHoursTests: XCTestCase {
    /// The server's golden answer of this name.
    private static func golden(_ name: String) throws -> Data {
        let here = URL(fileURLWithPath: #filePath)
        let file = here.deletingLastPathComponent()
            .appendingPathComponent("../../../crates/server/tests/golden/\(name).json")
        return try Data(contentsOf: file.standardizedFileURL)
    }

    /// Noon on 21 November 2026 by UTC: the golden map's first day.
    private static let noon: Int64 = 1_795_262_400

    /// What the server was asked, and how often.
    private actor Asked {
        var paths: [String] = []
        func add(_ url: URL) { paths.append(url.path + (url.query.map { "?" + $0 } ?? "")) }
    }

    /// A model with a map of the days and a server that answers the hours
    /// of every layer with the golden answers, or with `failing` refuses.
    private func model(asked: Asked, failing: Bool = false) async throws -> ThermalMapModel {
        let days = try Self.golden("thermals")
        let thermals = try Self.golden("thermal_hours")
        let wind = try Self.golden("thermal_hours_wind")
        let model = ThermalMapModel(transport: { url in
            await asked.add(url)
            if url.path == "/v1/thermals" {
                return (200, days)
            }
            if failing {
                return (503, Data(#"{"error": {"code": "unavailable", "message": "x"}}"#.utf8))
            }
            return (200, (url.query ?? "").contains("wind") || (url.query ?? "").contains("gusts") ? wind : thermals)
        })
        let address = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        model.server = { address }
        model.clock = { Self.noon }
        await model.load()
        return model
    }

    func testTheMapOffersItsLayersAndLevels() async throws {
        let thermals = try await model(asked: Asked())
        let map = try XCTUnwrap(thermals.map)
        XCTAssertEqual(map.layers.map(\.layer), [.thermals, .wind, .gusts, .cloud, .rain])
        XCTAssertEqual(map.levels.first?.heightM, nil)
        XCTAssertEqual(map.levels.compactMap(\.heightM), [1000, 1500, 2000, 2500, 3000, 4000])
        XCTAssertTrue(map.layers.first { $0.layer == .wind }?.hasLevels == true)
    }

    func testALayerOfADayIsAskedForOnceAndKept() async throws {
        let asked = Asked()
        let thermals = try await model(asked: asked)
        let key = ThermalMapModel.HoursKey(date: "2026-11-22", layer: .thermals, heightM: nil)
        await thermals.showHours(key)
        XCTAssertEqual(thermals.hoursState, .loaded)
        let view = try XCTUnwrap(thermals.hours?.view())
        XCTAssertEqual(view.date, "2026-11-22")
        XCTAssertEqual(view.hours.count, 7)
        XCTAssertTrue(view.hours.indices.contains(Int(view.firstHour)))
        // Shapes for the map: the squares and the arrows, as GeoJSON.
        let shapes = try XCTUnwrap(thermals.hours?.shapes(hour: view.firstHour))
        XCTAssertNotNil(ThermalMapLayers.shape(from: shapes.squares))
        XCTAssertNotNil(ThermalMapLayers.shape(from: shapes.arrows))

        // The day again, then the layer again: nothing more is asked.
        await thermals.showHours(nil)
        XCTAssertEqual(thermals.hoursState, .idle)
        XCTAssertNil(thermals.hours)
        await thermals.showHours(key)
        XCTAssertEqual(thermals.hoursState, .loaded)
        let paths = await asked.paths
        XCTAssertEqual(paths.filter { $0.hasPrefix("/v1/thermals/") }.count, 1, "\(paths)")
    }

    /// The wind at the ground and the gusts are one answer, read twice.
    func testTheWindAtTheGroundAndTheGustsAreOneAnswer() async throws {
        let asked = Asked()
        let thermals = try await model(asked: asked)
        await thermals.showHours(.init(date: "2026-11-22", layer: .wind, heightM: nil))
        XCTAssertEqual(thermals.hoursState, .loaded)
        // The golden answer is of the wind at 1000 m, which has no gusts
        // to read; what counts here is that nothing more is asked.
        await thermals.showHours(.init(date: "2026-11-22", layer: .gusts, heightM: nil))
        let paths = await asked.paths.filter { $0.hasPrefix("/v1/thermals/") }
        XCTAssertEqual(paths.count, 1, "\(paths)")
        // A height is another answer.
        await thermals.showHours(.init(date: "2026-11-22", layer: .wind, heightM: 1000))
        let more = await asked.paths.filter { $0.hasPrefix("/v1/thermals/") }
        XCTAssertEqual(more.count, 2, "\(more)")
    }

    /// Refused: said in words, nothing to draw, and the map shows the day.
    func testARefusalIsSaidAndTheDayIsShown() async throws {
        let thermals = try await model(asked: Asked(), failing: true)
        await thermals.showHours(.init(date: "2026-11-22", layer: .rain, heightM: nil))
        guard case .failed(let words) = thermals.hoursState else {
            return XCTFail("\(thermals.hoursState)")
        }
        XCTAssertFalse(words.isEmpty)
        XCTAssertNil(thermals.hours)
    }

    /// A tapped square's days side by side, from the map's own answer.
    func testASquaresDaysAreReadFromTheMap() async throws {
        let thermals = try await model(asked: Asked())
        let strip = await thermals.strip(at: Coordinate(lat: 47.65, lon: 11.85))
        XCTAssertEqual(strip.map(\.date), try XCTUnwrap(thermals.map).days.map(\.date))
        XCTAssertTrue(strip.contains { $0.square != nil })
        // Nowhere near the squares: no square on any day.
        let away = await thermals.strip(at: Coordinate(lat: 40, lon: 0))
        XCTAssertTrue(away.allSatisfy { $0.square == nil })
    }

    /// A tap finds the square at a place, drawn or not.
    func testATapFindsTheSquareAtThePlace() async throws {
        let thermals = try await model(asked: Asked())
        await thermals.showHours(.init(date: "2026-11-22", layer: .wind, heightM: 1000))
        let hours = try XCTUnwrap(thermals.hours)
        let cell = try XCTUnwrap(hours.cellAt(lat: 47.65, lon: 11.85))
        let square = try XCTUnwrap(hours.square(cell: cell, hour: hours.view().firstHour))
        XCTAssertFalse(square.title.isEmpty)
        XCTAssertNil(hours.cellAt(lat: 40, lon: 0))
    }

    /// A line of a key is coloured by the scale and the step the core
    /// names, from 1, and the veil by the muted ink.
    func testAKeysLineHasThePacksColour() {
        let theme = VentiTheme.light
        XCTAssertEqual(ThermalKeyColour.onMap(.init(step: 1, words: "", scale: .wind, scaleStep: 2)), theme.wind[1])
        XCTAssertEqual(ThermalKeyColour.onMap(.init(step: 1, words: "", scale: .vario, scaleStep: 4)), theme.vario[3])
        XCTAssertEqual(ThermalKeyColour.onMap(.init(step: 1, words: "", scale: .thermal, scaleStep: 6)), theme.thermal[5])
        XCTAssertEqual(
            UIColor(ThermalKeyColour.onMap(.init(step: 1, words: "", scale: .veil, scaleStep: 0))),
            Palette.notFlyableOnMap
        )
    }

    /// The layer as the preference keeps it, both ways.
    func testTheLayerIsKeptByName() {
        for layer in [ThermalHoursLayer.thermals, .wind, .gusts, .cloud, .rain] {
            XCTAssertEqual(ThermalHoursLayer(stored: layer.stored), layer)
        }
        XCTAssertEqual(ThermalHoursLayer(stored: "something"), .thermals)
    }

    /// The map's one line of time for the wind: the stations of now
    /// first, then each day's hours as they came, a day's name over its
    /// first hour; without stations "Now" on today's first hour.
    func testTheWindsLineRunsFromTheStationsThroughEveryDaysHours() async throws {
        let asked = Asked()
        let thermals = try await model(asked: asked)
        let map = try XCTUnwrap(thermals.map)
        await thermals.loadLine(layer: .wind, heightM: nil)
        XCTAssertEqual(Set(thermals.line.keys), Set(map.days.map(\.date)))
        // Each day asked for once; asked again, nothing is asked.
        let first = await asked.paths.count
        await thermals.loadLine(layer: .wind, heightM: nil)
        let again = await asked.paths.count
        XCTAssertEqual(first, again)

        let words = mapLayersWords()
        let cells = MapTimeLine.cells(
            what: .wind, radar: nil, stations: true, days: map.days, hours: thermals.line, words: words
        )
        XCTAssertEqual(cells.first?.stop, .stations)
        XCTAssertEqual(cells.first?.label, "Now")
        XCTAssertEqual(cells.first?.isNow, true)
        XCTAssertEqual(cells.filter(\.isNow).count, 1)
        for day in map.days {
            let ofDay = cells.filter {
                if case .hour(let date, _) = $0.stop { return date == day.date }
                return false
            }
            XCTAssertFalse(ofDay.isEmpty, day.date)
            XCTAssertEqual(ofDay.first?.over, day.name)
            XCTAssertTrue(ofDay.allSatisfy { $0.label.hasPrefix("\(day.name), ") })
        }

        // Without the stations: "Now" on today's first hour, from the
        // hour the core names.
        let forecast = MapTimeLine.cells(
            what: .wind, radar: nil, stations: false, days: map.days, hours: thermals.line, words: words
        )
        XCTAssertFalse(forecast.contains { $0.stop.isMeasured })
        if map.days.first?.today == true, let today = thermals.line[map.days[0].date] {
            XCTAssertEqual(forecast.first?.isNow, true)
            XCTAssertEqual(forecast.first?.stop, .hour(map.days[0].date, Int(today.firstHour)))
        }
    }

    /// The thermals' line: the day as a whole before each day's hours; a
    /// day whose hours are not there has that cell alone.
    func testTheThermalsLineHasEachDayBeforeItsHours() async throws {
        let thermals = try await model(asked: Asked())
        let map = try XCTUnwrap(thermals.map)
        let words = mapLayersWords()
        let bare = MapTimeLine.cells(what: .thermals, radar: nil, stations: true, days: map.days, hours: [:], words: words)
        XCTAssertEqual(bare.map(\.stop), map.days.map { .day($0.date) })
        XCTAssertEqual(bare.first?.short, "Day")
        await thermals.loadLine(layer: .thermals, heightM: nil)
        let cells = MapTimeLine.cells(
            what: .thermals, radar: nil, stations: true, days: map.days, hours: thermals.line, words: words
        )
        XCTAssertEqual(cells.first?.stop, .day(map.days[0].date))
        XCTAssertGreaterThan(cells.count, map.days.count)
        // A key's words, short: up to the first comma.
        XCTAssertEqual(MapTimeLine.short("Light rain, 0.2 to 1 mm an hour"), "Light rain")
        XCTAssertEqual(MapTimeLine.short("Cloudy"), "Cloudy")
    }

    /// What the map shows, as the preference keeps it: the wind until the
    /// pilot chooses, nothing for "none".
    func testWhatTheMapShowsIsKeptByName() {
        XCTAssertEqual(MapWhat(stored: "rain"), .rain)
        XCTAssertNil(MapWhat(stored: "none"))
        XCTAssertEqual(MapWhat(stored: ""), .wind)
        XCTAssertEqual(MapWhat.wind.layer(gusts: true), .gusts)
        XCTAssertEqual(MapWhat.cloud.layer(gusts: true), .cloud)
    }

    /// The days' segmented control: equal shares where every name fits
    /// one, and else a name that needs more gets what it needs and the
    /// others share the rest, so that "Tomorrow" is not cut.
    func testFourDaysShareTheWidthOrTakeWhatTheirNamesNeed() {
        XCTAssertEqual(VentiSegmentWidths.widths(needed: [50, 60, 30, 30], available: 280), [70, 70, 70, 70])
        XCTAssertEqual(VentiSegmentWidths.widths(needed: [50, 80, 30, 30], available: 260), [60, 80, 60, 60])
        XCTAssertEqual(VentiSegmentWidths.widths(needed: [70, 80, 30, 30], available: 260), [70, 80, 55, 55])
        // One taken out, and the next needs more than its share of the rest.
        XCTAssertEqual(VentiSegmentWidths.widths(needed: [62, 90, 30, 30], available: 260), [62, 90, 54, 54])
        // Wider together than the pill, which scrolls before it comes to
        // this: each by what it needs.
        XCTAssertEqual(VentiSegmentWidths.widths(needed: [100, 100], available: 100), [50, 50])
        XCTAssertEqual(VentiSegmentWidths.widths(needed: [], available: 100), [])
    }

    /// How long a step of an hour takes for a whole region: the core's two
    /// texts for the hour, and MapLibre reading them into shapes. Against
    /// the stand-in with a made-up day (`tools/made-up-day/serve.py`),
    /// named in VENTI_MADE_UP_DAY_SERVER; skipped without it. The times
    /// are printed, and an hour must take less than half a second.
    func testAnHourOfAWholeRegionIsQuickToDraw() async throws {
        guard let address = ProcessInfo.processInfo.environment["VENTI_MADE_UP_DAY_SERVER"],
              let server = URL(string: address)
        else {
            throw XCTSkip("No stand-in server with a made-up day was named in VENTI_MADE_UP_DAY_SERVER.")
        }
        let model = ThermalMapModel()
        model.server = { server }
        model.clock = {
            let wind = WindModel(address: address)
            return await wind.serverNow()
        }
        await model.load()
        let map = try XCTUnwrap(model.map)
        let day = try XCTUnwrap(map.days.first)
        for layer in [ThermalHoursLayer.thermals, .wind, .cloud, .rain] {
            let asked = Date()
            await model.showHours(.init(date: day.date, layer: layer, heightM: nil))
            let read = Date().timeIntervalSince(asked)
            let hours = try XCTUnwrap(model.hours)
            let view = hours.view()
            var slowest = 0.0
            var size = 0
            for hour in view.hours.indices {
                let started = Date()
                let shapes = hours.shapes(hour: UInt32(hour))
                _ = ThermalMapLayers.shape(from: shapes.squares)
                _ = ThermalMapLayers.shape(from: shapes.arrows)
                slowest = max(slowest, Date().timeIntervalSince(started))
                size = max(size, shapes.squares.utf8.count + shapes.arrows.utf8.count)
            }
            print("hours of \(layer.stored): asked and read in \(Int(read * 1000)) ms; \(view.hours.count) hours, the slowest step \(Self.ms(slowest)) ms, up to \(size / 1000) kB")
            XCTAssertLessThan(slowest, 0.5)
        }

        // The stand-in's map is small, nine squares by five. The same for
        // a region of the first one's size, 78 squares by 22 (46.3 to
        // 48.5 N, 9.4 to 17.2 E): the stand-in's wind at the ground, its
        // squares repeated, which has an arrow and a label in every one.
        let path = try thermalHoursPath(date: day.date, layer: .wind, heightM: nil)
        let (status, body) = try await WindModel.fetch(XCTUnwrap(URL(string: server.absoluteString + path)))
        let large = try Self.repeated(body, rows: 22, columns: 78)
        let now = await model.clock()
        let offset = TimeZone.current.secondsFromGMT(for: Date(timeIntervalSince1970: TimeInterval(now)))
        let begun = Date()
        let hours = try ThermalMapHours.read(
            status: UInt16(clamping: status), body: large, layer: .wind, utcOffsetS: Int32(offset), nowUnixS: now
        )
        let read = Date().timeIntervalSince(begun)
        var slowest = 0.0
        var size = 0
        for hour in hours.view().hours.indices {
            let started = Date()
            let shapes = hours.shapes(hour: UInt32(hour))
            _ = ThermalMapLayers.shape(from: shapes.squares)
            _ = ThermalMapLayers.shape(from: shapes.arrows)
            slowest = max(slowest, Date().timeIntervalSince(started))
            size = max(size, shapes.squares.utf8.count + shapes.arrows.utf8.count)
        }
        print("hours of wind, 78 by 22 squares: read in \(Self.ms(read)) ms; \(hours.view().hours.count) hours, the slowest step \(Self.ms(slowest)) ms, up to \(size / 1000) kB")
        XCTAssertLessThan(slowest, 0.5)
    }

    /// Seconds as milliseconds with a tenth, for a test's printed line.
    private static func ms(_ seconds: TimeInterval) -> String {
        String(format: "%.1f", seconds * 1000)
    }

    /// A layer's answer on a larger grid: every list that has a value for
    /// each square is repeated until it has one for each of the new.
    private static func repeated(_ body: Data, rows: Int, columns: Int) throws -> Data {
        var answer = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        var grid = try XCTUnwrap(answer["grid"] as? [String: Any])
        let before = (grid["rows"] as? Int ?? 0) * (grid["columns"] as? Int ?? 0)
        try XCTSkipIf(before == 0, "The stand-in's answer names no grid.")
        grid["rows"] = rows
        grid["columns"] = columns
        grid["south"] = 46.3
        grid["west"] = 9.4
        answer["grid"] = grid
        func larger(_ value: Any) -> Any {
            guard let list = value as? [Any], list.count == before else { return value }
            return (0..<rows * columns).map { list[$0 % before] }
        }
        answer = answer.mapValues(larger)
        answer["hours"] = (answer["hours"] as? [[String: Any]] ?? []).map { $0.mapValues(larger) }
        return try JSONSerialization.data(withJSONObject: answer)
    }
}
