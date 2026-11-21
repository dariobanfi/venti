import SwiftUI
import VentiCore
import XCTest
import zlib

@testable import Venti

/// What a Venti server answers for the rain radar: one source with two
/// frames and a clock four minutes after the second, and the development
/// mode's recording of two sources.
private enum ServerRadar {
    static func answer(frames: [String] = ["1135", "1140"], time: String = "11:44:00") -> String {
        let list = frames.map { clock in
            """
            {"time": "2026-11-21T\(clock.prefix(2)):\(clock.suffix(2)):00Z", "interval_s": 300,
             "path": "/v1/radar/dwd-radar-ry/20261121T\(clock)Z.png", "bytes": 4500}
            """
        }.joined(separator: ",")
        return """
        {
            "time": "2026-11-21T\(time)Z",
            "steps_mm_h": [0.1, 0.2, 1.0, 4.0, 10.0, 30.0],
            "unseen": 7,
            "layers": [{
                "source": "dwd-radar-ry",
                "west": 9.4, "south": 46.291984, "east": 17.2, "north": 48.5,
                "columns": 624, "rows": 261, "cadence_seconds": 300,
                "frames": [\(list)]
            }],
            "attributions": [{
                "source": "dwd-radar-ry", "provider": "Deutscher Wetterdienst",
                "licence": "CC BY 4.0",
                "text": "Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben",
                "url": "https://www.dwd.de/DE/service/rechtliche_hinweise/rechtliche_hinweise_node.html"
            }]
        }
        """
    }

    /// 21 November 2026, 11:44 UTC.
    static let now: Int64 = 1_795_261_440
    /// 11:40 and 11:35 UTC of that day.
    static let newest: Int64 = now - 240
    static let before: Int64 = now - 540

    /// Both sources as the development mode has them recorded, by a clock
    /// at 13:19 UTC: DWD's radars with frames of 13:10 and 13:15, and
    /// GeoSphere Austria's analysis with frames of 12:45 and 13:00. The
    /// server sorts the layers by source; the order to draw them in is
    /// the core's.
    static func recorded(radars: [String] = ["1310", "1315"], analysis: [String] = ["1245", "1300"]) -> String {
        func layer(_ source: String, every seconds: Int, _ frames: [String]) -> String {
            let list = frames.map { clock in
                """
                {"time": "2026-11-21T\(clock.prefix(2)):\(clock.suffix(2)):00Z", "interval_s": \(seconds),
                 "path": "/v1/radar/\(source)/20261121T\(clock)Z.png", "bytes": 5000}
                """
            }.joined(separator: ",")
            return """
            {"source": "\(source)", "west": 9.4, "south": 46.291984, "east": 17.2, "north": 48.5,
             "columns": 624, "rows": 261, "cadence_seconds": \(seconds), "frames": [\(list)]}
            """
        }
        return """
        {
            "time": "2026-11-21T13:19:00Z",
            "steps_mm_h": [0.1, 0.2, 1.0, 4.0, 10.0, 30.0],
            "unseen": 7,
            "layers": [\(layer(radarsSource, every: 300, radars)), \(layer(analysisSource, every: 900, analysis))],
            "attributions": [{
                "source": "dwd-radar-ry", "provider": "Deutscher Wetterdienst",
                "licence": "CC BY 4.0",
                "text": "Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben",
                "url": "https://www.dwd.de/DE/service/rechtliche_hinweise/rechtliche_hinweise_node.html"
            }, {
                "source": "geosphere-inca-rr", "provider": "GeoSphere Austria",
                "licence": "CC BY 4.0",
                "text": "Data source: GeoSphere Austria - https://data.hub.geosphere.at",
                "url": "https://data.hub.geosphere.at"
            }]
        }
        """
    }

    static let radarsSource = "dwd-radar-ry"
    static let analysisSource = "geosphere-inca-rr"
    /// 21 November 2026, 13:19 UTC: where the recording's clock stands.
    static let recordedAt: Int64 = now + 95 * 60

    static let noRadar = """
    {"error": {"code": "no_radar", "message": "This server collects no rain radar."}}
    """

    /// A picture as a server writes one: two by two pixels with the steps
    /// 0, 3, 7 and 6, a palette of eight greys, a byte a pixel.
    static let picture: Data = {
        func chunk(_ name: String, _ data: [UInt8]) -> [UInt8] {
            let named = Array(name.utf8) + data
            let sum = UInt32(crc32(0, named, UInt32(named.count)))
            return bytes(UInt32(data.count)) + named + bytes(sum)
        }
        func bytes(_ number: UInt32) -> [UInt8] {
            [UInt8(number >> 24), UInt8(number >> 16 & 0xFF), UInt8(number >> 8 & 0xFF), UInt8(number & 0xFF)]
        }
        // Two rows, each a filter byte and two pixels, in a zlib stream
        // of one stored block.
        let rows: [UInt8] = [0, 0, 3, 0, 7, 6]
        let sum = UInt32(adler32(1, rows, UInt32(rows.count)))
        let packed: [UInt8] = [0x78, 0x01, 0x01, 6, 0, 0xF9, 0xFF] + rows + bytes(sum)
        var png: [UInt8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        png += chunk("IHDR", [0, 0, 0, 2, 0, 0, 0, 2, 8, 3, 0, 0, 0])
        png += chunk("PLTE", [UInt8](repeating: 128, count: 24))
        png += chunk("tRNS", [0, 255, 255, 255, 255, 255, 255, 48])
        png += chunk("IDAT", packed)
        png += chunk("IEND", [])
        return Data(png)
    }()
}

/// What the server does next, one answer at a time, and what it was asked.
private actor RadarServer {
    enum Answer {
        case body(Int, String)
        case picture
        case unreachable
    }

    private var answers: [Answer]
    private(set) var asked: [String] = []

    init(_ answers: [Answer]) {
        self.answers = answers
    }

    func answer(_ url: URL) throws -> (Int, Data) {
        asked.append(url.path)
        let next: Answer = answers.isEmpty ? .unreachable : answers.removeFirst()
        switch next {
        case .body(let status, let text):
            return (status, Data(text.utf8))
        case .picture:
            return (200, ServerRadar.picture)
        case .unreachable:
            throw URLError(.notConnectedToInternet)
        }
    }
}

/// The time by which a model counts the frames' ages, which a test moves on.
private final class TestClock: @unchecked Sendable {
    var unixS = ServerRadar.now
}

/// The rain radar's model against the real core, with the server played by
/// canned answers in the server's own format.
@MainActor
final class RadarTests: XCTestCase {
    private let list = "/v1/radar"
    private let newest = "/v1/radar/dwd-radar-ry/20261121T1140Z.png"
    private let before = "/v1/radar/dwd-radar-ry/20261121T1135Z.png"

    private func model(_ server: RadarServer, clock: TestClock = TestClock()) throws -> RadarModel {
        let model = RadarModel(transport: { url in try await server.answer(url) })
        let address = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        model.server = { address }
        model.clock = { clock.unixS }
        return model
    }

    func testTheListIsAskedForAndThenTheNewestFramesPicture() async throws {
        let server = RadarServer([.body(200, ServerRadar.answer()), .picture])
        let radar = try model(server)
        XCTAssertEqual(radar.state, .idle)

        await radar.load()

        let asked = await server.asked
        XCTAssertEqual(asked, [list, newest])
        XCTAssertEqual(radar.state, .loaded)
        // One layer: a moment for each of its frames.
        let view = try XCTUnwrap(radar.view)
        XCTAssertEqual(view.layers.count, 1)
        XCTAssertEqual(view.moments.count, 2)
        XCTAssertNil(radar.caution)
        // It opens on the newest moment, which is no choice of the pilot's.
        XCTAssertEqual(radar.chosenIndex, 1)
        XCTAssertNil(radar.chosenTime)
        XCTAssertEqual(radar.moment?.timeUnixS, ServerRadar.newest)
        XCTAssertTrue(radar.label.hasPrefix("Rain radar, "), radar.label)
        XCTAssertTrue(radar.label.hasSuffix(", 4 minutes ago"), radar.label)
        XCTAssertEqual(radar.shownMoment?.timeUnixS, ServerRadar.newest)
        XCTAssertTrue(radar.spoken.hasSuffix(", 4 minutes ago"), radar.spoken)
        // The one picture, with the veil, pinned by the layer's four edges.
        XCTAssertEqual(radar.pictures.count, 1)
        let picture = try XCTUnwrap(radar.pictures.first)
        XCTAssertEqual(picture.timeUnixS, ServerRadar.newest)
        XCTAssertTrue(picture.drawsUnseen)
        XCTAssertEqual([picture.west, picture.south, picture.east, picture.north], [9.4, 46.291984, 17.2, 48.5])
        XCTAssertNil(radar.pictureProblem)
        XCTAssertEqual(radar.drawnLayers.map(\.credit?.provider), ["Deutscher Wetterdienst"])
    }

    /// The colours of a picture as the core wrote them: after the
    /// signature and the head, the palette: no rain, the six steps from
    /// the first, what was not seen; and after it how solid each is.
    private func colours(of png: Data) -> [UInt32] {
        let palette = [UInt8](png[41..<65])
        let solid = [UInt8](png[77..<85])
        return (0..<8).map { (at: Int) -> UInt32 in
            let red = UInt32(palette[at * 3]) << 24
            let green = UInt32(palette[at * 3 + 1]) << 16
            let blue = UInt32(palette[at * 3 + 2]) << 8
            return red | green | blue | UInt32(solid[at])
        }
    }

    private let analysisNewest = "/v1/radar/geosphere-inca-rr/20261121T1300Z.png"
    private let radarsNewest = "/v1/radar/dwd-radar-ry/20261121T1315Z.png"
    private let radarsBefore = "/v1/radar/dwd-radar-ry/20261121T1310Z.png"

    private func recorded(_ server: RadarServer) throws -> (RadarModel, TestClock) {
        let clock = TestClock()
        clock.unixS = ServerRadar.recordedAt
        return (try model(server, clock: clock), clock)
    }

    func testAMomentDrawsBothLayersWithTheRadarsOnTop() async throws {
        let server = RadarServer([.body(200, ServerRadar.recorded()), .picture, .picture])
        let (radar, _) = try recorded(server)

        await radar.load()

        // The moments are the radars' five minutes; the card opens on the
        // newest, and each layer's picture of it is asked for, the lower
        // one first.
        let view = try XCTUnwrap(radar.view)
        XCTAssertEqual(view.moments.map(\.timeUnixS), [ServerRadar.recordedAt - 540, ServerRadar.recordedAt - 240])
        XCTAssertEqual(radar.chosenIndex, 1)
        let asked = await server.asked
        XCTAssertEqual(asked, [list, analysisNewest, radarsNewest])

        // On the map: the analysis of 13:00 at the bottom, the radars'
        // picture of 13:15 over it, each pinned by its own layer's edges.
        XCTAssertEqual(radar.pictures.map(\.source), [ServerRadar.analysisSource, ServerRadar.radarsSource])
        XCTAssertEqual(
            radar.pictures.map(\.timeUnixS), [ServerRadar.recordedAt - 19 * 60, ServerRadar.recordedAt - 4 * 60]
        )
        XCTAssertEqual(Set(radar.pictures.map(\.id)).count, 2)
        XCTAssertNil(radar.pictureProblem)

        // What the radars did not see is clear in their picture, so that
        // the analysis shows through; the analysis draws the veil. Rain
        // has the same colours in both.
        XCTAssertEqual(radar.pictures.map(\.drawsUnseen), [true, false])
        let below = colours(of: try XCTUnwrap(radar.pictures.first?.png))
        let above = colours(of: try XCTUnwrap(radar.pictures.last?.png))
        XCTAssertEqual(below[7], RadarColours.rgba(Palette.notFlyableOnMap))
        XCTAssertEqual(above[7], 0)
        XCTAssertEqual(above[7] & 0xFF, 0, "What the radars did not see must be fully clear.")
        XCTAssertEqual(Array(below[0..<7]), Array(above[0..<7]))
        XCTAssertEqual(Array(below[1..<7]), RadarColours.palette(for: view.key).rain)

        // The line is the moment's: the radars' time and age, and that
        // the analysis under it is of another moment.
        XCTAssertNotNil(
            radar.label.wholeMatch(
                of: #/Rain radar, \d\d:\d\d, 4 minutes ago\. Austria's analysis is of \d\d:\d\d\./#
            ),
            radar.label
        )
        XCTAssertEqual(radar.label, radar.shownMoment?.label)
        XCTAssertNotNil(radar.spoken.wholeMatch(of: #/\d\d:\d\d, 4 minutes ago/#), radar.spoken)
        XCTAssertNil(radar.caution)

        // What each layer is, and both credits, the topmost first.
        XCTAssertEqual(
            view.layers.reversed().map(\.what),
            ["Germany's radars, every 5 minutes", "Austria's analysis of radar and gauges, every 15 minutes"]
        )
        XCTAssertEqual(
            radar.drawnLayers.map(\.credit?.provider), ["Deutscher Wetterdienst", "GeoSphere Austria"]
        )
    }

    func testAnotherMomentAsksOnlyForThePictureItDoesNotHave() async throws {
        let server = RadarServer([.body(200, ServerRadar.recorded()), .picture, .picture, .picture])
        let (radar, _) = try recorded(server)
        await radar.load()

        await radar.choose(0)

        // The moment before has the same analysis under another picture
        // of the radars: one picture is fetched, the other was kept.
        var asked = await server.asked
        XCTAssertEqual(asked, [list, analysisNewest, radarsNewest, radarsBefore])
        XCTAssertEqual(radar.chosenTime, ServerRadar.recordedAt - 540)
        XCTAssertEqual(
            radar.pictures.map(\.timeUnixS), [ServerRadar.recordedAt - 19 * 60, ServerRadar.recordedAt - 9 * 60]
        )
        XCTAssertEqual(radar.pictures.map(\.drawsUnseen), [true, false])
        XCTAssertTrue(radar.label.contains(", 9 minutes ago. Austria's analysis is of "), radar.label)

        // Back and forth, and played: nothing more is asked.
        await radar.choose(1)
        await radar.choose(0)
        radar.play()
        for _ in 0..<200 where radar.isPlaying {
            try await Task.sleep(for: .milliseconds(10))
        }
        asked = await server.asked
        XCTAssertEqual(asked.count, 4)
        XCTAssertEqual(radar.chosenIndex, 1)
        XCTAssertNil(radar.chosenTime)
        XCTAssertEqual(radar.pictures.last?.timeUnixS, ServerRadar.recordedAt - 240)
    }

    func testAMomentIsNotDrawnInPart() async throws {
        let server = RadarServer([.body(200, ServerRadar.recorded()), .picture, .unreachable, .picture])
        let (radar, _) = try recorded(server)

        await radar.load()

        // The analysis came and the radars' picture did not: the map has
        // neither, as the moment's line speaks of both.
        XCTAssertEqual(radar.state, .loaded)
        XCTAssertTrue(radar.pictures.isEmpty)
        XCTAssertNil(radar.shownMoment)
        XCTAssertNotNil(radar.pictureProblem)

        await radar.tryPictureAgain()

        // Only the one that was missing is asked for again.
        let asked = await server.asked
        XCTAssertEqual(asked, [list, analysisNewest, radarsNewest, radarsNewest])
        XCTAssertEqual(radar.pictures.count, 2)
        XCTAssertNil(radar.pictureProblem)
    }

    func testTheCautionIsOfTheTopmostLayerThatHasFrames() async throws {
        let server = RadarServer([.body(200, ServerRadar.recorded()), .picture, .picture])
        let (radar, clock) = try recorded(server)
        await radar.load()
        XCTAssertNil(radar.caution)

        // Twelve minutes on, the radars' newest picture is 16 minutes old,
        // more than three of their intervals; the analysis, 31 minutes
        // old, is within three of its own. The card says the radars'.
        clock.unixS += 12 * 60
        await radar.readAgain()
        XCTAssertEqual(radar.caution, "The newest radar picture is 16 minutes old. It is not the rain of now.")
        XCTAssertEqual(radar.view?.layers.first?.stale, false)
        // The pictures stay on the map.
        XCTAssertEqual(radar.pictures.count, 2)
    }

    func testWithoutTheRadarsTheAnalysisStandsAloneAndSpeaksForItself() async throws {
        let server = RadarServer([.body(200, ServerRadar.recorded(radars: [])), .picture])
        let (radar, clock) = try recorded(server)

        await radar.load()

        // The moments are the analysis' quarters of an hour, and its
        // picture is the only one, with the veil.
        let view = try XCTUnwrap(radar.view)
        XCTAssertEqual(view.layers.count, 2)
        XCTAssertEqual(view.moments.count, 2)
        XCTAssertEqual(radar.pictures.map(\.source), [ServerRadar.analysisSource])
        XCTAssertEqual(radar.pictures.map(\.drawsUnseen), [true])
        XCTAssertTrue(radar.label.hasPrefix("Rain analysis, "), radar.label)
        // Only the layer drawn is credited.
        XCTAssertEqual(radar.drawnLayers.map(\.credit?.provider), ["GeoSphere Austria"])
        // The radars' layer has no frames and says nothing here: the
        // analysis is the topmost that has, and it is not behind.
        XCTAssertEqual(view.layers.last?.stale, true)
        XCTAssertNil(radar.caution)

        // Half an hour on it is 49 minutes old, and says so itself.
        clock.unixS += 30 * 60
        await radar.readAgain()
        XCTAssertEqual(radar.caution, "The newest picture of the analysis is 49 minutes old. It is not the rain of now.")
    }

    func testTheRadarsAloneDrawTheVeilThemselves() async throws {
        let server = RadarServer([.body(200, ServerRadar.recorded(analysis: [])), .picture])
        let (radar, _) = try recorded(server)

        await radar.load()

        // No picture of the analysis to lie under the radars': what the
        // radars did not see is veiled in their own picture, never dry.
        XCTAssertEqual(radar.pictures.map(\.source), [ServerRadar.radarsSource])
        XCTAssertEqual(radar.pictures.map(\.drawsUnseen), [true])
        let only = colours(of: try XCTUnwrap(radar.pictures.first?.png))
        XCTAssertEqual(only[7], RadarColours.rgba(Palette.notFlyableOnMap))
        XCTAssertTrue(radar.label.hasSuffix(", 4 minutes ago"), radar.label)
    }

    func testThePictureIsInTheColoursTheKeyNames() async throws {
        let server = RadarServer([.body(200, ServerRadar.answer()), .picture])
        let radar = try model(server)

        await radar.load()

        let png = try XCTUnwrap(radar.pictures.first?.png)
        // A picture the iPhone reads, of the size it had.
        let image = try XCTUnwrap(UIImage(data: png))
        XCTAssertEqual(image.size, CGSize(width: 2, height: 2))
        // No rain, the six steps from the first, the veil.
        let all = colours(of: png)
        func colour(_ at: Int) -> UInt32 {
            all[at]
        }
        let theme = VentiTheme.light
        XCTAssertEqual(colour(0), 0)
        let named = [theme.vario[3], theme.vario[2], theme.vario[1], theme.vario[0], theme.wind[5], theme.wind[6]]
        for (step, token) in named.enumerated() {
            XCTAssertEqual(colour(step + 1), RadarColours.rgba(UIColor(token)), "step \(step + 1)")
        }
        XCTAssertEqual(colour(7), RadarColours.rgba(Palette.notFlyableOnMap))
        // The key's lines name the same colours, from the most rain down.
        let key = try XCTUnwrap(radar.view?.key)
        XCTAssertEqual(key.map(\.step), [6, 5, 4, 3, 2, 1])
        XCTAssertEqual(RadarColours.palette(for: key).rain, (1...6).map { colour($0) })
    }

    func testAnotherFrameIsFetchedOnceAndKeptForThisRun() async throws {
        let server = RadarServer([.body(200, ServerRadar.answer()), .picture, .picture])
        let radar = try model(server)
        await radar.load()

        await radar.choose(0)

        XCTAssertEqual(radar.chosenTime, ServerRadar.before)
        XCTAssertEqual(radar.pictures.map(\.timeUnixS), [ServerRadar.before])
        XCTAssertTrue(radar.label.hasSuffix(", 9 minutes ago"), radar.label)

        // Back and forth: nothing more is asked.
        await radar.choose(1)
        await radar.choose(0)
        await radar.choose(1)
        let asked = await server.asked
        XCTAssertEqual(asked, [list, newest, before])
        XCTAssertNil(radar.chosenTime)
        XCTAssertEqual(radar.pictures.first?.timeUnixS, ServerRadar.newest)
    }

    func testAPictureThatCannotBeHadLeavesTheOneBeforeWithItsOwnTime() async throws {
        let server = RadarServer([.body(200, ServerRadar.answer()), .picture, .unreachable, .picture])
        let radar = try model(server)
        await radar.load()

        await radar.choose(0)

        // The newest frame's picture is still on the map, and the line
        // says its time, not the chosen frame's.
        XCTAssertEqual(radar.chosenIndex, 0)
        XCTAssertEqual(radar.pictures.first?.timeUnixS, ServerRadar.newest)
        XCTAssertTrue(radar.label.hasSuffix(", 4 minutes ago"), radar.label)
        let problem = try XCTUnwrap(radar.pictureProblem)
        XCTAssertTrue(problem.hasPrefix("Venti could not be reached for the radar picture."), problem)

        await radar.tryPictureAgain()

        XCTAssertNil(radar.pictureProblem)
        XCTAssertEqual(radar.pictures.first?.timeUnixS, ServerRadar.before)
    }

    func testWhatIsNoPictureIsSaidInTheCoresWords() async throws {
        let server = RadarServer([.body(200, ServerRadar.answer()), .body(200, "<html>")])
        let radar = try model(server)

        await radar.load()

        XCTAssertEqual(radar.state, .loaded)
        XCTAssertTrue(radar.pictures.isEmpty)
        XCTAssertEqual(radar.pictureProblem, "The radar picture could not be read.")
    }

    func testAVentiWithoutRadarSaysSoAndIsNotAskedAgain() async throws {
        let server = RadarServer([.body(501, ServerRadar.noRadar)])
        let radar = try model(server)

        await radar.load()

        XCTAssertEqual(radar.state, .none("This server has no rain radar."))
        XCTAssertNil(radar.view)
        XCTAssertTrue(radar.pictures.isEmpty)
        XCTAssertFalse(radar.isDue)
    }

    func testVentiNotReachedIsSaidAndAskedAgain() async throws {
        let server = RadarServer([.unreachable, .body(200, ServerRadar.answer()), .picture])
        let radar = try model(server)

        await radar.load()

        guard case .failed(let words) = radar.state else {
            return XCTFail("Not reached is a failure, not \(radar.state)")
        }
        XCTAssertTrue(words.hasPrefix("Venti could not be reached for the rain radar."), words)
        XCTAssertNil(radar.view)

        // "Try again".
        await radar.load()
        XCTAssertEqual(radar.state, .loaded)
        XCTAssertFalse(radar.pictures.isEmpty)
    }

    func testARefusalIsSaidInTheCoresWords() async throws {
        let refusal = """
        {"error": {"code": "unavailable", "message": "Venti is being looked after. Try again in a minute."}}
        """
        let server = RadarServer([.body(503, refusal)])
        let radar = try model(server)

        await radar.load()

        guard case .failed(let words) = radar.state else {
            return XCTFail("A refusal is a failure, not \(radar.state)")
        }
        XCTAssertFalse(words.isEmpty)
        XCTAssertFalse(words.hasPrefix("Venti could not be reached"), words)
    }

    func testTheFramesGrowOldInWordsWithoutAsking() async throws {
        let clock = TestClock()
        let server = RadarServer([.body(200, ServerRadar.answer()), .picture])
        let radar = try model(server, clock: clock)
        await radar.load()
        XCTAssertNil(radar.caution)

        // Ten minutes on: the same answer, read again.
        clock.unixS += 600
        await radar.readAgain()
        XCTAssertTrue(radar.label.hasSuffix(", 14 minutes ago"), radar.label)
        XCTAssertNil(radar.caution)

        // And ten more: old rain is not the rain of now.
        clock.unixS += 600
        await radar.readAgain()
        XCTAssertEqual(radar.caution, "The newest radar picture is 24 minutes old. It is not the rain of now.")
        // The picture stays, and nothing was asked.
        XCTAssertEqual(radar.pictures.first?.timeUnixS, ServerRadar.newest)
        let asked = await server.asked
        XCTAssertEqual(asked.count, 2)
    }

    func testFramesFetchedEarlierStayWhenVentiIsNotReached() async throws {
        let clock = TestClock()
        let server = RadarServer([.body(200, ServerRadar.answer()), .picture, .unreachable])
        let radar = try model(server, clock: clock)
        await radar.load()

        clock.unixS += 1200
        await radar.load()

        XCTAssertEqual(radar.state, .loaded)
        XCTAssertEqual(radar.pictures.first?.timeUnixS, ServerRadar.newest)
        XCTAssertNotNil(radar.caution)
    }

    func testANewFrameIsShownOnlyWhileTheNewestIs() async throws {
        let later = ServerRadar.answer(frames: ["1135", "1140", "1145"], time: "11:47:00")
        let server = RadarServer([
            .body(200, ServerRadar.answer()), .picture, .body(200, later), .picture, .picture, .body(200, later),
        ])
        let clock = TestClock()
        let radar = try model(server, clock: clock)
        await radar.load()

        // The newest was shown: the new frame takes its place.
        clock.unixS += 180
        await radar.load()
        XCTAssertEqual(radar.view?.moments.count, 3)
        XCTAssertEqual(radar.chosenIndex, 2)
        XCTAssertEqual(radar.pictures.first?.timeUnixS, ServerRadar.newest + 300)

        // The pilot looks at an earlier one: it stays when the list comes again.
        await radar.choose(0)
        await radar.load()
        XCTAssertEqual(radar.chosenIndex, 0)
        XCTAssertEqual(radar.pictures.first?.timeUnixS, ServerRadar.before)
    }

    func testPlayingStepsThroughTheFramesAndStopsAtTheNewest() async throws {
        let server = RadarServer([.body(200, ServerRadar.answer()), .picture, .picture])
        let radar = try model(server)
        await radar.load()
        await radar.choose(0)
        await radar.choose(1)

        radar.play()
        XCTAssertTrue(radar.isPlaying)
        var shown: Set<Int64> = []
        for _ in 0..<200 where radar.isPlaying {
            if let time = radar.pictures.first?.timeUnixS {
                shown.insert(time)
            }
            try await Task.sleep(for: .milliseconds(10))
        }

        XCTAssertFalse(radar.isPlaying)
        XCTAssertEqual(shown, [ServerRadar.before, ServerRadar.newest])
        XCTAssertEqual(radar.chosenIndex, 1)
        XCTAssertEqual(radar.pictures.first?.timeUnixS, ServerRadar.newest)
    }

    func testWithoutAVentiNothingIsAsked() async throws {
        let server = RadarServer([])
        let radar = RadarModel(transport: { url in try await server.answer(url) })

        await radar.load()

        XCTAssertEqual(radar.state, .idle)
        let asked = await server.asked
        XCTAssertTrue(asked.isEmpty)
    }

    func testManyFramesHaveTheirTimesUnderTheStrip() {
        // A time in each cell while a cell has the room.
        XCTAssertNil(RadarStrip.named(of: 2))
        XCTAssertNil(RadarStrip.named(of: 6))
        // Then the newest and every third before it,
        XCTAssertEqual(RadarStrip.named(of: 7), [6, 3])
        XCTAssertEqual(RadarStrip.named(of: 12), [11, 8, 5, 2])
        // and of two hours of frames every sixth: every half hour.
        XCTAssertEqual(RadarStrip.named(of: 13), [12, 6])
        XCTAssertEqual(RadarStrip.named(of: 25), [24, 18, 12, 6])
    }

    /// The strip with two hours of frames, kept as a picture to look at:
    /// the development server has two frames, and a real Venti twenty-five.
    func testTheStripOfTwoHoursOfFramesIsKeptAsAPicture() throws {
        let clocks = (0..<25).map { String(format: "%02d:%02d", 10 + ($0 * 5 + 40) / 60, ($0 * 5 + 40) % 60) }
        for (name, scheme) in [("light", ColorScheme.light), ("dark", .dark)] {
            let strip = CellStrip(
                cells: clocks, named: RadarStrip.named(of: clocks.count), chosen: .constant(24),
                name: "Radar picture", spoken: "", identifier: "radar-frames"
            )
            .padding(VentiSpace.s3)
            .frame(width: 290)
            .background(.venti(.surface))
            .ventiThemed()
            .environment(\.colorScheme, scheme)
            let renderer = ImageRenderer(content: strip)
            renderer.scale = 3
            let image = try XCTUnwrap(renderer.uiImage)
            let attachment = XCTAttachment(image: image)
            attachment.name = "radar-strip-25-frames-\(name)"
            attachment.lifetime = .keepAlways
            add(attachment)
        }
    }
}
