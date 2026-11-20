import Foundation

/// Prepares the shared map style for the place its tiles come from.
///
/// The style in `map/style.json` is the same file for every platform. It
/// leaves open where its two tile sources and its fonts live, because that
/// differs: a region stored on the device, or tiles fetched as the pilot
/// moves the map.
enum MapStyle {
    enum Source: Equatable {
        /// Tiles fetched from public servers of open map data: OpenFreeMap
        /// for the base map, Mapterhorn for terrain. This stands in for the
        /// project's own tile server until there is one.
        case online
        /// The small demo region stored inside the app. Needs no connection.
        case bundled
        /// A region the pilot downloaded from a Venti server, kept on the
        /// iPhone. Needs no connection.
        case kept(Kept)

        /// What a map is drawn from unless a kept region takes its place.
        static var standard: Source {
            LaunchOptions.usesBundledMap ? .bundled : .online
        }

        /// A name for the source that differs whenever what is drawn
        /// from differs: a map is made anew when it changes, and the
        /// prepared style is kept in a file of this name.
        var key: String {
            switch self {
            case .online: "online"
            case .bundled: "bundled"
            case .kept(let kept): "kept-\(kept.id)-\(kept.version)"
            }
        }
    }

    /// The two archives of a region kept on the iPhone.
    struct Kept: Equatable {
        /// The region's identifier and the version of its files. Both are
        /// names of one step of a path: the core lets no other through.
        let id: String
        let version: String
        let basemap: URL
        let terrain: URL
    }

    enum StyleError: Error, Equatable {
        case missing(String)
        case malformed
    }

    /// The public endpoints the online source uses.
    enum Online {
        static let basemap = "https://tiles.openfreemap.org/planet"
        static let terrainTiles = "https://tiles.mapterhorn.com/{z}/{x}/{y}.webp"
        /// Mapterhorn serves terrain to this zoom everywhere; finer tiles
        /// exist only for some countries. Asking for no more keeps the
        /// hillshade free of holes.
        static let terrainMaxZoom = 12
        static let terrainAttribution = "<a href=\"https://mapterhorn.com/attribution\">© Mapterhorn</a>"
    }

    /// The folder inside the app that holds the bundled region and the fonts.
    static func bundledRegion(in bundle: Bundle = .main) throws -> URL {
        guard let folder = bundle.url(forResource: "MapDemo", withExtension: nil) else {
            throw StyleError.missing("MapDemo")
        }
        return folder
    }

    /// The style as JSON text, pointing at the given source.
    ///
    /// Label fonts always come from inside the app, so place names never
    /// wait for the network.
    static func json(for source: Source, in bundle: Bundle = .main) throws -> String {
        guard let template = bundle.url(forResource: "style", withExtension: "json") else {
            throw StyleError.missing("style.json")
        }
        guard var style = try JSONSerialization.jsonObject(with: Data(contentsOf: template)) as? [String: Any],
              var sources = style["sources"] as? [String: [String: Any]],
              var basemap = sources["basemap"],
              var terrain = sources["terrain"]
        else {
            throw StyleError.malformed
        }

        let region = try bundledRegion(in: bundle)
        let fonts = region.appendingPathComponent("fonts", isDirectory: false).absoluteString
        style["glyphs"] = fonts + "/{fontstack}/{range}.pbf"

        switch source {
        case .online:
            basemap["url"] = Online.basemap
            terrain["url"] = nil
            terrain["tiles"] = [Online.terrainTiles]
            terrain["maxzoom"] = Online.terrainMaxZoom
            terrain["attribution"] = Online.terrainAttribution
        case .bundled:
            basemap["url"] = "pmtiles://" + region.appendingPathComponent("basemap.pmtiles").absoluteString
            terrain["url"] = "pmtiles://" + region.appendingPathComponent("terrain.pmtiles").absoluteString
        case .kept(let kept):
            basemap["url"] = "pmtiles://" + kept.basemap.absoluteString
            terrain["url"] = "pmtiles://" + kept.terrain.absoluteString
        }
        sources["basemap"] = basemap
        sources["terrain"] = terrain
        style["sources"] = sources

        let data = try JSONSerialization.data(withJSONObject: style, options: [.sortedKeys, .withoutEscapingSlashes])
        return String(decoding: data, as: UTF8.self)
    }

    /// Writes the prepared style to the caches folder and returns its
    /// location. The map view loads a style from a URL, not from text.
    static func url(for source: Source, in bundle: Bundle = .main) throws -> URL {
        let caches = try FileManager.default.url(
            for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true
        )
        let file = caches.appendingPathComponent("map-style-\(source.key).json")
        try json(for: source, in: bundle).write(to: file, atomically: true, encoding: .utf8)
        return file
    }
}
