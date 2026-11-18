import XCTest

@testable import Venti

/// Checks that the map data bundled with the app is complete and that the
/// prepared style points at it.
final class MapStyleTests: XCTestCase {
    func testThePreparedStyleIsJSONAndHasNoPlaceholdersLeft() throws {
        let text = try MapStyle.demoStyleJSON()

        XCTAssertFalse(text.contains("__"), "a placeholder was not replaced")
        let style = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
        )
        XCTAssertEqual(style["version"] as? Int, 8)
    }

    func testEveryTileSourceInTheStyleIsAFileInTheApp() throws {
        let text = try MapStyle.demoStyleJSON()
        let style = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
        )
        let sources = try XCTUnwrap(style["sources"] as? [String: [String: Any]])
        XCTAssertEqual(Set(sources.keys), ["basemap", "terrain"])

        for (name, source) in sources {
            let url = try XCTUnwrap(source["url"] as? String, name)
            XCTAssertTrue(url.hasPrefix("pmtiles://file://"), url)
            let file = try XCTUnwrap(URL(string: String(url.dropFirst("pmtiles://".count))))
            XCTAssertTrue(FileManager.default.fileExists(atPath: file.path), "\(name): \(file.path)")
        }
    }

    func testTheFontsTheStyleAsksForAreInTheApp() throws {
        let fonts = try MapStyle.demoRegion().appendingPathComponent("fonts/noto-sans-regular")
        for range in ["0-255", "256-511"] {
            let file = fonts.appendingPathComponent("\(range).pbf")
            XCTAssertTrue(FileManager.default.fileExists(atPath: file.path), file.path)
        }
    }

    func testThePreparedStyleIsWrittenWhereTheMapCanLoadIt() throws {
        let url = try MapStyle.demoStyleURL()
        XCTAssertTrue(url.isFileURL)
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
    }
}
