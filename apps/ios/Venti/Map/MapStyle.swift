import Foundation
import VentiCore

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

    /// The layers that belong to one of the map's two views only: the
    /// drawn ground, or the aerial view, photographs from above. Every
    /// other layer is in both.
    struct Views: Equatable {
        let aerial: [String]
        let drawn: [String]
        /// The layers of the thermal map, which a chip switches on and
        /// off. A map has them only when the app has a domain to name to
        /// the map's author: see `json(for:server:domain:in:)`.
        let thermals: [String]
    }

    /// Which layers of the shared style belong to which view. The style
    /// says so itself, as `venti:view` in a layer's metadata, and stands
    /// in the drawn view: the aerial view's layers are hidden in the file.
    static func views(in bundle: Bundle = .main) throws -> Views {
        guard let template = bundle.url(forResource: "style", withExtension: "json") else {
            throw StyleError.missing("style.json")
        }
        guard let style = try JSONSerialization.jsonObject(with: Data(contentsOf: template)) as? [String: Any],
              let layers = style["layers"] as? [[String: Any]]
        else {
            throw StyleError.malformed
        }
        func names(_ mark: String, _ value: String) -> [String] {
            layers
                .filter { ($0["metadata"] as? [String: Any])?[mark] as? String == value }
                .compactMap { $0["id"] as? String }
        }
        return Views(
            aerial: names("venti:view", "aerial"), drawn: names("venti:view", "drawn"),
            thermals: names("venti:layer", "thermals")
        )
    }

    /// A layer of photographs of the aerial view and who made them: the
    /// layer's `venti:credit`, the box its source covers (none for one that
    /// covers the world) and the zoom from which it is drawn.
    struct AerialMaker: Equatable {
        let layer: String
        let credit: String
        let bounds: Bounds?
        let minZoom: Double
        /// The provider's own wording, as the source's attribution has it,
        /// without its links.
        var wording = ""
        /// Every link of that attribution, in its order: the provider's
        /// page first, and its licence where the attribution links it.
        var links: [String] = []
    }

    /// The layers of photographs in the shared style, in its order: every
    /// raster layer of the aerial view that names its maker.
    static func aerialMakers(in bundle: Bundle = .main) throws -> [AerialMaker] {
        guard let template = bundle.url(forResource: "style", withExtension: "json") else {
            throw StyleError.missing("style.json")
        }
        guard let style = try JSONSerialization.jsonObject(with: Data(contentsOf: template)) as? [String: Any],
              let layers = style["layers"] as? [[String: Any]],
              let sources = style["sources"] as? [String: [String: Any]]
        else {
            throw StyleError.malformed
        }
        return layers.compactMap { layer -> AerialMaker? in
            let metadata = layer["metadata"] as? [String: Any]
            guard layer["type"] as? String == "raster",
                  metadata?["venti:view"] as? String == "aerial",
                  let id = layer["id"] as? String,
                  let credit = metadata?["venti:credit"] as? String
            else { return nil }
            let source = (layer["source"] as? String).flatMap { sources[$0] }
            let box = (source?["bounds"] as? [Double]).flatMap { edges -> Bounds? in
                guard edges.count == 4 else { return nil }
                return Bounds(south: edges[1], west: edges[0], north: edges[3], east: edges[2])
            }
            let zoom = max(
                (layer["minzoom"] as? Double) ?? 0,
                (source?["minzoom"] as? Double) ?? 0
            )
            let attribution = source?["attribution"] as? String ?? ""
            return AerialMaker(
                layer: id, credit: credit, bounds: box, minZoom: zoom,
                wording: plainText(attribution), links: links(in: attribution)
            )
        }
    }

    /// An attribution's words without its markup: "© swisstopo".
    static func plainText(_ html: String) -> String {
        html.replacingOccurrences(of: "<[^>]+>", with: "", options: .regularExpression)
            .replacingOccurrences(of: "&amp;", with: "&")
            .trimmingCharacters(in: .whitespaces)
    }

    /// The addresses of an attribution's links, each once, in its order.
    static func links(in html: String) -> [String] {
        var found: [String] = []
        var rest = html[...]
        while let range = rest.range(of: #"href="[^"]+""#, options: .regularExpression) {
            let address = String(rest[range].dropFirst(6).dropLast())
            if !found.contains(address) { found.append(address) }
            rest = rest[range.upperBound...]
        }
        return found
    }

    /// The short line that names the makers of the photographs in view:
    /// "Photos: " and the credit of every layer whose box meets the part of
    /// the map on screen and whose zoom the map has reached, in the
    /// style's order with the layers that cover the world last, each name
    /// once. `makers` are the layers of the style the map loaded.
    static func photoCredit(_ makers: [AerialMaker], visible: Bounds, zoom: Double) -> String {
        let drawn = makers.filter { maker in
            guard zoom >= maker.minZoom else { return false }
            guard let box = maker.bounds else { return true }
            return box.south <= visible.north && box.north >= visible.south
                && box.west <= visible.east && box.east >= visible.west
        }
        var names: [String] = []
        for maker in drawn.filter({ $0.bounds != nil }) + drawn.filter({ $0.bounds == nil })
        where !names.contains(maker.credit) {
            names.append(maker.credit)
        }
        return "Photos: " + names.joined(separator: " · ")
    }

    /// The folder inside the app that holds the bundled region and the fonts.
    static func bundledRegion(in bundle: Bundle = .main) throws -> URL {
        guard let folder = bundle.url(forResource: "MapDemo", withExtension: nil) else {
            throw StyleError.missing("MapDemo")
        }
        return folder
    }

    /// What stands in the shared style for the address of the Venti
    /// server the app talks to.
    static let serverPlaceholder = "__VENTI_SERVER__"

    /// What stands in the shared style for the domain the app names itself
    /// by to a provider that asks who is calling.
    static let domainPlaceholder = "__APP_DOMAIN__"

    /// The domain Venti names itself by. The thermal map's author asks
    /// every request to say whose app is asking (docs/data-sources.md).
    /// Venti has no domain yet, so there is nothing true to say, and the
    /// style then leaves the thermal map out: nothing is asked of his
    /// server without it.
    static let appDomain: String? = nil

    /// The Venti server the pilot has set, if the address is one.
    static var savedServer: URL? {
        VentiServer.url
    }

    /// The style as JSON text, pointing at the given source.
    ///
    /// Label fonts always come from inside the app, so place names never
    /// wait for the network. What the style asks of Venti itself, the
    /// photographs of Austria and France that Venti cuts at the border, is asked of
    /// `server`; what has to be told who is asking, the thermal map, is
    /// told `domain`. A source that needs one of the two and has none is
    /// left out, with its layers.
    static func json(
        for source: Source, server: URL? = savedServer, domain: String? = appDomain,
        in bundle: Bundle = .main
    ) throws -> String {
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

        var layers = style["layers"] as? [[String: Any]] ?? []
        // What each placeholder stands for, where the app has it.
        var values: [String: String] = [:]
        values[serverPlaceholder] = server?.absoluteString
        values[domainPlaceholder] = domain
        for (name, described) in sources {
            guard let tiles = described["tiles"] as? [String] else { continue }
            let named = [serverPlaceholder, domainPlaceholder].filter { placeholder in
                tiles.contains { $0.contains(placeholder) }
            }
            guard !named.isEmpty else { continue }
            if named.contains(where: { values[$0] == nil }) {
                sources[name] = nil
                layers.removeAll { $0["source"] as? String == name }
            } else {
                var asked = described
                asked["tiles"] = tiles.map { address in
                    named.reduce(address) { filled, placeholder in
                        filled.replacingOccurrences(of: placeholder, with: values[placeholder] ?? "")
                    }
                }
                sources[name] = asked
            }
        }
        style["layers"] = layers
        style["sources"] = sources

        let data = try JSONSerialization.data(withJSONObject: style, options: [.sortedKeys, .withoutEscapingSlashes])
        return String(decoding: data, as: UTF8.self)
    }

    /// Writes the prepared style to the caches folder and returns its
    /// location. The map view loads a style from a URL, not from text.
    static func url(
        for source: Source, server: URL? = savedServer, domain: String? = appDomain, in bundle: Bundle = .main
    ) throws -> URL {
        let caches = try FileManager.default.url(
            for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true
        )
        let file = caches.appendingPathComponent("map-style-\(source.key).json")
        try json(for: source, server: server, domain: domain, in: bundle)
            .write(to: file, atomically: true, encoding: .utf8)
        return file
    }
}
