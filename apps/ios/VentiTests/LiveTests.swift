import SwiftUI
import VentiCore
import XCTest

@testable import Venti

/// Live tracking on the iPhone: the pilot's choice as it is kept and as
/// the core names it, and what the cockpit's button shows.
@MainActor
final class LiveTests: XCTestCase {
    func testNobodyIsNoAudience() {
        XCTAssertNil(LiveChoice.nobody.audience)
        XCTAssertEqual(LiveChoice.link.audience, .link)
        XCTAssertEqual(LiveChoice.everyone.audience, .public)
        // Kept in the words of the server, so that a choice survives a
        // renaming here; anything else is nobody.
        XCTAssertEqual(LiveChoice(rawValue: "public"), .everyone)
        XCTAssertEqual(LiveChoice(rawValue: "link"), .link)
        XCTAssertNil(LiveChoice(rawValue: "followers"))
        XCTAssertEqual(LiveChoice.allCases.map(\.short), ["Off", "With a link", "Everyone"])
    }

    func testEveryChoiceSaysWhatItMeans() {
        for choice in LiveChoice.allCases {
            XCTAssertFalse(LiveScreen.explanation(of: choice).isEmpty)
        }
        XCTAssertTrue(LiveScreen.explanation(of: .nobody).contains("Nothing is sent"))
    }

    func testTheButtonShowsWhetherTheFlightGoesOut() {
        XCTAssertEqual(LiveButton.tint(for: .sharing), .green)
        XCTAssertEqual(LiveButton.tint(for: .waiting), .orange)
        XCTAssertNil(LiveButton.tint(for: .stopped))
        XCTAssertNil(LiveButton.tint(for: nil))
        XCTAssertEqual(LiveButton.symbol(for: nil), "antenna.radiowaves.left.and.right.slash")
        XCTAssertEqual(LiveButton.symbol(for: .waiting), "wifi.exclamationmark")
        XCTAssertEqual(LiveButton.symbol(for: .sharing), "antenna.radiowaves.left.and.right")
    }

    /// The server's answer about a part of the map with two gliders, one
    /// flying and one landed, as its development mode gives it.
    private static let inTheAir = """
        {"time": "2026-11-18T19:05:00Z", "truncated": false, "tracks": [
          {"id": "11111111111111111111111111", "pilot": {"handle": "demo", "display_name": "Demo Pilot"},
           "visibility": "public", "landed": false, "started_at": "2026-11-18T18:44:56Z",
           "latest": {"time": "2026-11-18T19:04:56Z", "position": {"lat": 47.6638, "lon": 11.7432},
                      "altitude_m": 1596, "ground_speed_ms": 10.5, "climb_ms": -1.0, "course_deg": 20.0},
           "fixes": 1201, "expires_at": "2026-11-20T19:04:56Z"},
          {"id": "22222222222222222222222222", "pilot": {"handle": "demo", "display_name": "Demo Pilot"},
           "visibility": "public", "landed": true, "started_at": "2026-11-18T18:45:01Z",
           "latest": {"time": "2026-11-18T18:56:00Z", "position": {"lat": 47.6922, "lon": 11.7675},
                      "altitude_m": 786, "ground_speed_ms": 0.0, "climb_ms": 0.0},
           "fixes": 660, "expires_at": "2026-11-20T18:56:00Z"}]}
        """

    private actor Asked {
        var addresses: [URL] = []
        func add(_ url: URL) { addresses.append(url) }
    }

    func testWhoIsInTheAirComesFromTheServerForThePartOfTheMapInView() async throws {
        let asked = Asked()
        let live = LiveModel { url in
            await asked.add(url)
            return (200, Data(Self.inTheAir.utf8))
        }
        let server = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        live.server = { server }

        await live.load(Bounds(south: 47.55, west: 11.6, north: 47.85, east: 12.0), from: server)
        let address = try XCTUnwrap(await asked.addresses.first)
        XCTAssertEqual(address.absoluteString, "http://127.0.0.1:8080/v1/live?bbox=11.6,47.55,12,47.85")
        XCTAssertEqual(live.gliders.count, 2)
        XCTAssertFalse(live.isTruncated)

        // The flying one: four seconds old when the server answered, and
        // older by this iPhone's clock since.
        let flying = live.gliders[0]
        XCTAssertEqual(flying.label, "Demo Pilot · 1596 m")
        XCTAssertEqual(live.age(of: flying, at: live.answeredAt.addingTimeInterval(10)), 14, accuracy: 0.01)
        XCTAssertEqual(live.ageInWords(of: flying, at: live.answeredAt), "Now")
        XCTAssertEqual(
            live.ageInWords(of: flying, at: live.answeredAt.addingTimeInterval(300)), "No position for 5 min"
        )
        let landed = live.gliders[1]
        XCTAssertEqual(live.ageInWords(of: landed, at: live.answeredAt), "Landed 9 min ago")
        XCTAssertEqual(LiveGliderRow.detail(of: flying, age: "Now"), "1,596 m · -1.0 m/s · Now")
        XCTAssertEqual(LiveGliderRow.detail(of: landed, age: "Landed 9 min ago"), "786 m · Landed 9 min ago")

        // On the map: a wing turned the way it flies, and a disc that
        // points nowhere for the one on the ground.
        let wing = OverviewMap.Coordinator.gliderFeature(for: flying)
        XCTAssertEqual(wing.attribute(forKey: "picture") as? String, LiveSymbols.flying)
        XCTAssertEqual(wing.attribute(forKey: "heading") as? Double, 20)
        XCTAssertEqual(wing.attribute(forKey: "id") as? String, "11111111111111111111111111")
        let disc = OverviewMap.Coordinator.gliderFeature(for: landed)
        XCTAssertEqual(disc.attribute(forKey: "picture") as? String, LiveSymbols.landed)
        XCTAssertEqual(disc.attribute(forKey: "heading") as? Double, 0)

        // A map of half a continent asks for nobody, and shows nobody.
        live.show(Bounds(south: 40, west: 0, north: 55, east: 20))
        XCTAssertTrue(live.gliders.isEmpty)
    }

    func testAnAnswerThatDoesNotComeLeavesTheGlidersWhereTheyWere() async throws {
        let answers = Asked()
        let live = LiveModel { url in
            await answers.add(url)
            if await answers.addresses.count > 1 {
                throw URLError(.notConnectedToInternet)
            }
            return (200, Data(Self.inTheAir.utf8))
        }
        let server = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        live.server = { server }
        let area = Bounds(south: 47.55, west: 11.6, north: 47.85, east: 12.0)
        await live.load(area, from: server)
        let first = live.answeredAt
        await live.load(area, from: server)
        XCTAssertEqual(live.gliders.count, 2)
        // Their ages go on counting from the answer that did come.
        XCTAssertEqual(live.answeredAt, first)
    }

    func testThereIsAPictureForEveryKindOfGlider() {
        XCTAssertEqual(
            Set(LiveSymbols.all.map(\.name)), [LiveSymbols.flying, LiveSymbols.quiet, LiveSymbols.landed]
        )
        for picture in LiveSymbols.all {
            XCTAssertEqual(picture.image.size, CGSize(width: LiveSymbols.side, height: LiveSymbols.side))
        }
    }

    func testAReplayIsNeverShared() throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-live-test-\(UUID().uuidString)", isDirectory: true)
        let url = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        let session = FlightSession(dataFolder: folder.path)
        _ = try Replay(igcText: try String(contentsOf: url, encoding: .utf8))
            .advance(session: session, seconds: 120)
        XCTAssertThrowsError(
            try session.shareLive(server: "https://venti.example.org", random: Data(0..<26), audience: .public)
        )
        XCTAssertNil(session.liveShare(nowUnixS: 0))
    }
}
