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
