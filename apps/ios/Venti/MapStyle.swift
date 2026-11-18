import Foundation

/// Prepares the shared map style for the map data bundled with the app.
///
/// The style in `map/style.json` is the same file for every platform. It
/// names its tiles and fonts with placeholders, because where they live
/// differs: here they are files inside the app, later also downloaded
/// regions and the server.
enum MapStyle {
    enum StyleError: Error, Equatable {
        case missing(String)
    }

    /// The folder inside the app that holds the bundled demo region.
    static func demoRegion(in bundle: Bundle = .main) throws -> URL {
        guard let folder = bundle.url(forResource: "MapDemo", withExtension: nil) else {
            throw StyleError.missing("MapDemo")
        }
        return folder
    }

    /// The style as JSON text, with its placeholders replaced by the URLs of
    /// the bundled files.
    static func demoStyleJSON(in bundle: Bundle = .main) throws -> String {
        guard let template = bundle.url(forResource: "style", withExtension: "json") else {
            throw StyleError.missing("style.json")
        }
        let region = try demoRegion(in: bundle)
        let fonts = region.appendingPathComponent("fonts", isDirectory: false).absoluteString

        var style = try String(contentsOf: template, encoding: .utf8)
        let replacements = [
            "__BASEMAP_URL__": "pmtiles://" + region.appendingPathComponent("basemap.pmtiles").absoluteString,
            "__TERRAIN_URL__": "pmtiles://" + region.appendingPathComponent("terrain.pmtiles").absoluteString,
            "__GLYPHS_URL__": fonts,
        ]
        for (placeholder, url) in replacements {
            style = style.replacingOccurrences(of: placeholder, with: url)
        }
        return style
    }

    /// Writes the prepared style to the caches folder and returns its
    /// location. The map view loads a style from a URL, not from text.
    static func demoStyleURL(in bundle: Bundle = .main) throws -> URL {
        let caches = try FileManager.default.url(
            for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true
        )
        let file = caches.appendingPathComponent("map-style.json")
        try demoStyleJSON(in: bundle).write(to: file, atomically: true, encoding: .utf8)
        return file
    }
}
