import XCTest

@testable import Venti

/// Checks that the style is prepared correctly for both places tiles can
/// come from, and that the map data bundled with the app is complete.
final class MapStyleTests: XCTestCase {
    private func style(for source: MapStyle.Source) throws -> [String: Any] {
        let text = try MapStyle.json(for: source)
        XCTAssertFalse(text.contains("__"), "a placeholder was not replaced")
        let style = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
        )
        XCTAssertEqual(style["version"] as? Int, 8)
        return style
    }

    private func sources(of style: [String: Any]) throws -> [String: [String: Any]] {
        let sources = try XCTUnwrap(style["sources"] as? [String: [String: Any]])
        XCTAssertEqual(Set(sources.keys), ["basemap", "terrain"])
        return sources
    }

    func testEveryTileSourceOfTheBundledStyleIsAFileInTheApp() throws {
        for (name, source) in try sources(of: style(for: .bundled)) {
            let url = try XCTUnwrap(source["url"] as? String, name)
            XCTAssertTrue(url.hasPrefix("pmtiles://file://"), url)
            let file = try XCTUnwrap(URL(string: String(url.dropFirst("pmtiles://".count))))
            XCTAssertTrue(FileManager.default.fileExists(atPath: file.path), "\(name): \(file.path)")
        }
    }

    func testTheOnlineStyleFetchesTilesAndKeepsTheTerrainSettings() throws {
        let sources = try sources(of: style(for: .online))

        XCTAssertEqual(sources["basemap"]?["url"] as? String, "https://tiles.openfreemap.org/planet")
        let terrain = try XCTUnwrap(sources["terrain"])
        XCTAssertNil(terrain["url"])
        XCTAssertEqual(terrain["tiles"] as? [String], ["https://tiles.mapterhorn.com/{z}/{x}/{y}.webp"])
        XCTAssertEqual(terrain["maxzoom"] as? Int, 12)
        XCTAssertEqual(terrain["encoding"] as? String, "terrarium")
        XCTAssertEqual(terrain["tileSize"] as? Int, 512)
        XCTAssertEqual(terrain["type"] as? String, "raster-dem")
    }

    func testLabelFontsAlwaysComeFromInsideTheApp() throws {
        for source in MapStyle.Source.allCases {
            let glyphs = try XCTUnwrap(style(for: source)["glyphs"] as? String)
            XCTAssertTrue(glyphs.hasPrefix("file://"), glyphs)
            XCTAssertTrue(glyphs.hasSuffix("/{fontstack}/{range}.pbf"), glyphs)
        }
        // Every range a label can ask for has a file. The map engine never
        // gets an answer for one that is missing, and the tile the label
        // belongs to then stays unfinished: an airspace named with a dash
        // was not drawn at all.
        let fonts = try MapStyle.bundledRegion().appendingPathComponent("fonts/noto-sans-regular")
        for start in stride(from: 0, to: 65536, by: 256) {
            let file = fonts.appendingPathComponent("\(start)-\(start + 255).pbf")
            XCTAssertTrue(FileManager.default.fileExists(atPath: file.path), file.path)
        }
        // Latin, the dashes and quotation marks, Greek and Cyrillic are real
        // glyphs, not placeholders.
        for range in ["0-255", "256-511", "768-1023", "1024-1279", "8192-8447"] {
            let file = fonts.appendingPathComponent("\(range).pbf")
            let size = try XCTUnwrap(try FileManager.default.attributesOfItem(atPath: file.path)[.size] as? Int)
            XCTAssertGreaterThan(size, 10_000, range)
        }
    }

    func testThePreparedStyleIsWrittenWhereTheMapCanLoadIt() throws {
        for source in MapStyle.Source.allCases {
            let url = try MapStyle.url(for: source)
            XCTAssertTrue(url.isFileURL)
            XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
        }
    }
}
