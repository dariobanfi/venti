import SwiftUI
import VentiCore
import XCTest
import zlib

@testable import Venti

/// What a Venti server answers for the rain radar: the development mode's
/// two frames, and its radar's clock four minutes after the second.
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
        let layer = try XCTUnwrap(radar.layer)
        XCTAssertEqual(layer.frames.count, 2)
        XCTAssertFalse(layer.stale)
        // It opens on the newest frame, which is no choice of the pilot's.
        XCTAssertEqual(radar.chosenIndex, 1)
        XCTAssertNil(radar.chosenTime)
        XCTAssertEqual(radar.frame?.ageS, 240)
        XCTAssertTrue(radar.label.hasPrefix("Rain radar, "), radar.label)
        XCTAssertTrue(radar.label.hasSuffix(", 4 minutes ago"), radar.label)
        // The picture is pinned by the layer's four edges.
        let picture = try XCTUnwrap(radar.picture)
        XCTAssertEqual(picture.timeUnixS, ServerRadar.newest)
        XCTAssertEqual([picture.west, picture.south, picture.east, picture.north], [9.4, 46.291984, 17.2, 48.5])
        XCTAssertNil(radar.pictureProblem)
        XCTAssertEqual(layer.credit?.provider, "Deutscher Wetterdienst")
    }

    func testThePictureIsInTheColoursTheKeyNames() async throws {
        let server = RadarServer([.body(200, ServerRadar.answer()), .picture])
        let radar = try model(server)

        await radar.load()

        let png = try XCTUnwrap(radar.picture?.png)
        // A picture the iPhone reads, of the size it had.
        let image = try XCTUnwrap(UIImage(data: png))
        XCTAssertEqual(image.size, CGSize(width: 2, height: 2))
        // After the signature and the head, the palette: no rain, the six
        // steps from the first, the veil; and after it how solid each is.
        let palette = [UInt8](png[41..<65])
        let solid = [UInt8](png[77..<85])
        func colour(_ at: Int) -> UInt32 {
            UInt32(palette[at * 3]) << 24 | UInt32(palette[at * 3 + 1]) << 16 | UInt32(palette[at * 3 + 2]) << 8
                | UInt32(solid[at])
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
        XCTAssertEqual(radar.picture?.timeUnixS, ServerRadar.before)
        XCTAssertTrue(radar.label.hasSuffix(", 9 minutes ago"), radar.label)

        // Back and forth: nothing more is asked.
        await radar.choose(1)
        await radar.choose(0)
        await radar.choose(1)
        let asked = await server.asked
        XCTAssertEqual(asked, [list, newest, before])
        XCTAssertNil(radar.chosenTime)
        XCTAssertEqual(radar.picture?.timeUnixS, ServerRadar.newest)
    }

    func testAPictureThatCannotBeHadLeavesTheOneBeforeWithItsOwnTime() async throws {
        let server = RadarServer([.body(200, ServerRadar.answer()), .picture, .unreachable, .picture])
        let radar = try model(server)
        await radar.load()

        await radar.choose(0)

        // The newest frame's picture is still on the map, and the line
        // says its time, not the chosen frame's.
        XCTAssertEqual(radar.chosenIndex, 0)
        XCTAssertEqual(radar.picture?.timeUnixS, ServerRadar.newest)
        XCTAssertTrue(radar.label.hasSuffix(", 4 minutes ago"), radar.label)
        let problem = try XCTUnwrap(radar.pictureProblem)
        XCTAssertTrue(problem.hasPrefix("Venti could not be reached for the radar picture."), problem)

        await radar.tryPictureAgain()

        XCTAssertNil(radar.pictureProblem)
        XCTAssertEqual(radar.picture?.timeUnixS, ServerRadar.before)
    }

    func testWhatIsNoPictureIsSaidInTheCoresWords() async throws {
        let server = RadarServer([.body(200, ServerRadar.answer()), .body(200, "<html>")])
        let radar = try model(server)

        await radar.load()

        XCTAssertEqual(radar.state, .loaded)
        XCTAssertNil(radar.picture)
        XCTAssertEqual(radar.pictureProblem, "The radar picture could not be read.")
    }

    func testAVentiWithoutRadarSaysSoAndIsNotAskedAgain() async throws {
        let server = RadarServer([.body(501, ServerRadar.noRadar)])
        let radar = try model(server)

        await radar.load()

        XCTAssertEqual(radar.state, .none("This server has no rain radar."))
        XCTAssertNil(radar.view)
        XCTAssertNil(radar.picture)
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
        XCTAssertNotNil(radar.picture)
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
        XCTAssertFalse(try XCTUnwrap(radar.layer).stale)

        // Ten minutes on: the same answer, read again.
        clock.unixS += 600
        await radar.readAgain()
        XCTAssertTrue(radar.label.hasSuffix(", 14 minutes ago"), radar.label)
        XCTAssertFalse(try XCTUnwrap(radar.layer).stale)

        // And ten more: old rain is not the rain of now.
        clock.unixS += 600
        await radar.readAgain()
        let layer = try XCTUnwrap(radar.layer)
        XCTAssertTrue(layer.stale)
        XCTAssertEqual(layer.staleWords, "The newest radar picture is 24 minutes old. It is not the rain of now.")
        // The picture stays, and nothing was asked.
        XCTAssertEqual(radar.picture?.timeUnixS, ServerRadar.newest)
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
        XCTAssertEqual(radar.picture?.timeUnixS, ServerRadar.newest)
        XCTAssertTrue(try XCTUnwrap(radar.layer).stale)
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
        XCTAssertEqual(radar.layer?.frames.count, 3)
        XCTAssertEqual(radar.chosenIndex, 2)
        XCTAssertEqual(radar.picture?.timeUnixS, ServerRadar.newest + 300)

        // The pilot looks at an earlier one: it stays when the list comes again.
        await radar.choose(0)
        await radar.load()
        XCTAssertEqual(radar.chosenIndex, 0)
        XCTAssertEqual(radar.picture?.timeUnixS, ServerRadar.before)
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
            if let time = radar.picture?.timeUnixS {
                shown.insert(time)
            }
            try await Task.sleep(for: .milliseconds(10))
        }

        XCTAssertFalse(radar.isPlaying)
        XCTAssertEqual(shown, [ServerRadar.before, ServerRadar.newest])
        XCTAssertEqual(radar.chosenIndex, 1)
        XCTAssertEqual(radar.picture?.timeUnixS, ServerRadar.newest)
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
