import Foundation
import Observation
import SwiftUI
import VentiCore

/// The thermal forecast by region (decision 27, the web's
/// `regions.svelte.ts`): the regions' outlines, asked for once
/// (`GET /v1/regions`), and each day asked for as the pilot goes through
/// the thermals' days (`GET /v1/regions/forecast`). Every level, word and
/// credit is the core's (`ForecastRegions`, `RegionForecast`); what is
/// drawn of them is `RegionLayers`. A day that cannot be had is not kept,
/// and the map then shows the grid alone, without a word.
@MainActor
@Observable
final class RegionForecastModel {
    typealias Transport = ThermalMapModel.Transport

    /// The regions, once read.
    private(set) var regions: ForecastRegions?
    /// The regions' outlines as polygons, by the region's identifier, for
    /// finding the squares of the grid that lie in a region.
    @ObservationIgnored private(set) var polygons: [String: [RegionShapes.Polygon]] = [:]
    /// The days read in this run of the app, by date.
    private(set) var days: [String: RegionForecast] = [:]
    /// Why the last asking brought nothing, in words: the grid then stands
    /// alone and nothing is said on the map.
    private(set) var problem: String?

    /// The server to ask, the thermal map's.
    @ObservationIgnored var server: () -> URL? = { nil }
    /// The time by the server's clock, seconds since 1970-01-01 UTC.
    @ObservationIgnored var clock: () async -> Int64 = { Int64(Date().timeIntervalSince1970) }

    @ObservationIgnored private let transport: Transport
    @ObservationIgnored private var asking: Set<String> = []
    @ObservationIgnored private var askedServer: URL?
    /// The squares of the grid under the regions with a level, worked out
    /// once for a day's grid and a set of regions.
    @ObservationIgnored private var under: [String: Set<Int>] = [:]

    init(transport: @escaping Transport = WindModel.fetch) {
        self.transport = transport
    }

    /// The language of the regions' names: the iPhone's, where the
    /// regions have names in it, else English (the core decides).
    static var language: String {
        Locale.current.language.languageCode?.identifier ?? "en"
    }

    /// Asks for a day, and for the regions where they are not at hand or
    /// the day names another set of them. Nothing is asked twice at once.
    func load(date: String) async {
        guard let server = server() else { return }
        if askedServer != server {
            forget()
            askedServer = server
        }
        guard days[date] == nil, !asking.contains(date) else { return }
        asking.insert(date)
        defer { asking.remove(date) }
        do {
            let path = try regionForecastPath(date: date)
            guard let url = URL(string: server.absoluteString + path) else { return }
            let (status, body) = try await transport(url)
            let nowUnixS = await clock()
            let offset = TimeZone.current.secondsFromGMT(
                for: nowUnixS == 0 ? Date() : Date(timeIntervalSince1970: TimeInterval(nowUnixS))
            )
            let day = try RegionForecast.read(
                status: UInt16(clamping: status), body: body, utcOffsetS: Int32(offset), nowUnixS: nowUnixS
            )
            let version = day.view().regionsVersion
            if regions?.version() != version {
                guard let url = URL(string: server.absoluteString + regionsPath()) else { return }
                let (status, body) = try await transport(url)
                let read = try ForecastRegions.read(
                    status: UInt16(clamping: status), body: body, language: Self.language
                )
                guard server == self.server() else { return }
                polygons = RegionShapes.polygons(of: read.outlines())
                under = [:]
                regions = read
            }
            guard server == self.server() else { return }
            days[date] = day
            problem = nil
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            problem = error is CoreError ? error.words : error.localizedDescription
        }
    }

    /// Forgets the days read: they belong to another map of the days, or
    /// another server.
    func forget() {
        days = [:]
        under = [:]
        if askedServer != server() {
            regions = nil
            polygons = [:]
        }
    }

    /// The squares of a day's grid, by their place among its cells, whose
    /// middle lies in one of the regions named: those the regions' fill
    /// stands in place of. Worked out once for a day and a set of regions.
    func cellsUnder(_ ids: Set<String>, in day: ThermalMapDay) -> Set<Int> {
        guard !ids.isEmpty else { return [] }
        let key = "\(day.date) \(day.cells.count) \(ids.sorted().joined(separator: ","))"
        if let kept = under[key] {
            return kept
        }
        let found = RegionShapes.cellsUnder(
            day.cells.map { RegionShapes.Box(south: $0.south, west: $0.west, north: $0.north, east: $0.east) },
            polygons: ids.flatMap { polygons[$0] ?? [] }
        )
        under[key] = found
        return found
    }
}

/// The geometry of the regions, without a map: their outlines read from
/// the core's GeoJSON, and which squares of the grid lie in them.
enum RegionShapes {
    /// A box of degrees.
    struct Box: Equatable {
        let south: Double
        let west: Double
        let north: Double
        let east: Double
    }

    /// One polygon: its outer ring and its holes, each ring as
    /// longitude and latitude pairs, and the box round the outer ring.
    struct Polygon {
        let rings: [[(lon: Double, lat: Double)]]
        let box: Box
    }

    /// The polygons of each region in a GeoJSON `FeatureCollection` of
    /// `Polygon`s or `MultiPolygon`s, by the features' `id`.
    static func polygons(of geoJSON: String) -> [String: [Polygon]] {
        guard let object = try? JSONSerialization.jsonObject(with: Data(geoJSON.utf8)) as? [String: Any],
              let features = object["features"] as? [[String: Any]]
        else { return [:] }
        var found: [String: [Polygon]] = [:]
        for feature in features {
            guard let properties = feature["properties"] as? [String: Any],
                  let id = properties["id"] as? String,
                  let geometry = feature["geometry"] as? [String: Any],
                  let type = geometry["type"] as? String
            else { continue }
            let polygons: [Any]
            switch type {
            case "Polygon": polygons = [geometry["coordinates"] as Any]
            case "MultiPolygon": polygons = geometry["coordinates"] as? [Any] ?? []
            default: polygons = []
            }
            for polygon in polygons {
                guard let rings = polygon as? [[[Double]]] else { continue }
                let read = rings.map { ring in
                    ring.compactMap { pair in pair.count >= 2 ? (lon: pair[0], lat: pair[1]) : nil }
                }
                guard let outer = read.first, !outer.isEmpty else { continue }
                let box = Box(
                    south: outer.map(\.lat).min() ?? 0, west: outer.map(\.lon).min() ?? 0,
                    north: outer.map(\.lat).max() ?? 0, east: outer.map(\.lon).max() ?? 0
                )
                found[id, default: []].append(Polygon(rings: read, box: box))
            }
        }
        return found
    }

    /// The places among the squares of those whose middle lies in one of
    /// the polygons.
    static func cellsUnder(_ cells: [Box], polygons: [Polygon]) -> Set<Int> {
        guard !polygons.isEmpty else { return [] }
        var under = Set<Int>()
        for (index, cell) in cells.enumerated() {
            let lon = (cell.west + cell.east) / 2
            let lat = (cell.south + cell.north) / 2
            let inside = polygons.contains { polygon in
                lon >= polygon.box.west && lon <= polygon.box.east
                    && lat >= polygon.box.south && lat <= polygon.box.north
                    && contains(polygon, lon: lon, lat: lat)
            }
            if inside {
                under.insert(index)
            }
        }
        return under
    }

    /// Whether a place lies in a polygon: in its outer ring and in none of
    /// its holes.
    static func contains(_ polygon: Polygon, lon: Double, lat: Double) -> Bool {
        guard let outer = polygon.rings.first else { return false }
        return inRing(outer, lon: lon, lat: lat)
            && !polygon.rings.dropFirst().contains { inRing($0, lon: lon, lat: lat) }
    }

    /// Counts the crossings of a ray to the east.
    private static func inRing(_ ring: [(lon: Double, lat: Double)], lon: Double, lat: Double) -> Bool {
        var inside = false
        var before = ring.count - 1
        for at in ring.indices {
            let (x1, y1) = (ring[at].lon, ring[at].lat)
            let (x2, y2) = (ring[before].lon, ring[before].lat)
            if (y1 > lat) != (y2 > lat), lon < (x2 - x1) * (lat - y1) / (y2 - y1) + x1 {
                inside.toggle()
            }
            before = at
        }
        return inside
    }
}

extension RegionLevel {
    /// The level's place in the pack's `level` tokens, which run worst to
    /// best.
    var tokenIndex: Int {
        switch self {
        case .notFlyable: 0
        case .glidesOnly: 1
        case .weak: 2
        case .good: 3
        case .veryGood: 4
        }
    }

    /// The level's colour and its fill's opacity on the map, which is
    /// light in both appearances: the light theme's (design/DESIGN.md,
    /// "Day levels").
    var colorOnMap: Color { VentiTheme.light.level[tokenIndex] }
    var fillOpacityOnMap: Double { VentiTheme.light.levelFillOpacity[tokenIndex] }
}
