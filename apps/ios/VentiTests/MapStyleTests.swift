import XCTest

@testable import Venti

/// Checks that the style is prepared correctly for both places tiles can
/// come from, and that the map data bundled with the app is complete.
final class MapStyleTests: XCTestCase {
    /// A Venti server, as a pilot's settings name one.
    private let server = URL(string: "https://venti.example")

    private func style(for source: MapStyle.Source, server: URL? = nil) throws -> [String: Any] {
        let text = try MapStyle.json(for: source, server: server)
        XCTAssertFalse(text.contains("__"), "a placeholder was not replaced")
        let style = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
        )
        XCTAssertEqual(style["version"] as? Int, 8)
        return style
    }

    /// Every place a map can be drawn from: the network, the region in
    /// the app, and a region kept on the iPhone, which the one in the app
    /// stands in for.
    private func everySource() throws -> [MapStyle.Source] {
        let region = try MapStyle.bundledRegion()
        let kept = MapStyle.Kept(
            id: "tegernsee-demo", version: "2026-11-18",
            basemap: region.appendingPathComponent("basemap.pmtiles"),
            terrain: region.appendingPathComponent("terrain.pmtiles")
        )
        return [.online, .bundled, .kept(kept)]
    }

    /// The two sources each place has in its own way: the base map and
    /// the terrain. The aerial view's photographs come from their
    /// providers wherever the rest is drawn from.
    private func sources(of style: [String: Any]) throws -> [String: [String: Any]] {
        let sources = try XCTUnwrap(style["sources"] as? [String: [String: Any]])
        XCTAssertEqual(Set(sources.keys), ["basemap", "terrain", "aerial-world", "aerial-bavaria"])
        return sources.filter { !$0.key.hasPrefix("aerial-") }
    }

    /// Austria's photographs come through Venti, which cuts them at the
    /// border: with a server they are asked of it, and without one the
    /// source and its layer are not in the style at all.
    func testAustriasPhotographsAreAskedOfThePilotsServer() throws {
        for source in try everySource() {
            let with = try style(for: source, server: server)
            let asked = try XCTUnwrap((with["sources"] as? [String: [String: Any]])?["aerial-austria"])
            XCTAssertEqual(asked["tiles"] as? [String], ["https://venti.example/v1/imagery/basemap-at/{z}/{x}/{y}"])
            XCTAssertTrue((asked["attribution"] as? String)?.contains("basemap.at") == true)
            let layers = try XCTUnwrap(with["layers"] as? [[String: Any]])
            XCTAssertTrue(layers.contains { $0["id"] as? String == "aerial-austria" })

            let without = try style(for: source)
            XCTAssertNil((without["sources"] as? [String: [String: Any]])?["aerial-austria"])
            let left = try XCTUnwrap(without["layers"] as? [[String: Any]])
            XCTAssertFalse(left.contains { $0["source"] as? String == "aerial-austria" })
            // Nothing else goes with it.
            XCTAssertEqual(left.count, layers.count - 1)
        }
    }

    /// The shared style marks which layers belong to the drawn ground and
    /// which to the aerial view, and stands in the drawn view: nothing of
    /// the photographs is fetched until the pilot asks for them.
    func testTheAerialViewIsHiddenUntilItIsAskedFor() throws {
        let views = try MapStyle.views()
        XCTAssertEqual(
            views.aerial,
            ["aerial-world", "aerial-austria", "aerial-bavaria", "aerial-road", "aerial-aerialway-casing"]
        )
        XCTAssertEqual(views.drawn, ["path", "track", "road-minor", "road-major", "rail"])

        for source in try everySource() {
            let style = try style(for: source)
            let layers = try XCTUnwrap(style["layers"] as? [[String: Any]])
            for layer in layers {
                let name = try XCTUnwrap(layer["id"] as? String)
                let visibility = (layer["layout"] as? [String: Any])?["visibility"] as? String
                XCTAssertEqual(visibility ?? "visible", views.aerial.contains(name) ? "none" : "visible", name)
            }
            // Whoever made the photographs is named with them.
            let sources = try XCTUnwrap(style["sources"] as? [String: [String: Any]])
            for name in ["aerial-world", "aerial-bavaria"] {
                XCTAssertEqual(sources[name]?["type"] as? String, "raster", name)
                XCTAssertFalse((sources[name]?["attribution"] as? String ?? "").isEmpty, name)
            }
        }
    }

    func testEveryTileSourceOfTheBundledStyleIsAFileInTheApp() throws {
        for (name, source) in try sources(of: style(for: .bundled)) {
            let url = try XCTUnwrap(source["url"] as? String, name)
            XCTAssertTrue(url.hasPrefix("pmtiles://file://"), url)
            let file = try XCTUnwrap(URL(string: String(url.dropFirst("pmtiles://".count))))
            XCTAssertTrue(FileManager.default.fileExists(atPath: file.path), "\(name): \(file.path)")
        }
    }

    func testAKeptRegionIsDrawnFromItsOwnTwoFiles() throws {
        let kept = try XCTUnwrap(try everySource().last)
        let sources = try sources(of: style(for: kept))

        let basemap = try XCTUnwrap(sources["basemap"]?["url"] as? String)
        XCTAssertTrue(basemap.hasPrefix("pmtiles://file://"), basemap)
        XCTAssertTrue(basemap.hasSuffix("/basemap.pmtiles"), basemap)
        let terrain = try XCTUnwrap(sources["terrain"])
        XCTAssertTrue((terrain["url"] as? String)?.hasSuffix("/terrain.pmtiles") == true)
        // Nothing of it is fetched.
        XCTAssertNil(terrain["tiles"])
        XCTAssertEqual(terrain["encoding"] as? String, "terrarium")
        // Each source has a name of its own, by which its style is kept
        // and by which a map knows that it has to be made anew.
        XCTAssertEqual(Set(try everySource().map(\.key)).count, 3)
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
        for source in try everySource() {
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
        for source in try everySource() {
            let url = try MapStyle.url(for: source)
            XCTAssertTrue(url.isFileURL)
            XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
        }
    }
}
