import Foundation
import VentiCore

/// A search among public flights as the iPhone keeps it: the take-off
/// within a radius of a place or inside an area drawn on the map, the
/// glider's class, the kind of route, the least distance and the
/// take-off's country, as on the web (`flight-search.ts`).
///
/// What a search means, its bounds and its words are the core's
/// (`FlightSearch`, `flightSearchWords`, `flightSearchBounds`). This file
/// only takes a choice apart into its chips and counts them.
enum FlightSearchPart: CaseIterable, Hashable {
    case near, area, gliderClass, kind, minKm, country
}

extension FlightSearch {
    /// The search of everything.
    static let everything = FlightSearch()

    /// Whether the search chooses anything at all.
    var isSearching: Bool { !parts.isEmpty }

    /// The choices made, in the order a filter shows them.
    var parts: [FlightSearchPart] {
        FlightSearchPart.allCases.filter { !flightSearchWords(search: only($0)).isEmpty }
    }

    /// How many choices are made: the count on the filter's button.
    var count: Int { parts.count }

    /// The chips of the search, one a choice, in the core's words and
    /// order: "Within 10 km", "EN C".
    var chips: [(part: FlightSearchPart, words: String)] {
        FlightSearchPart.allCases.compactMap { part in
            flightSearchWords(search: only(part)).first.map { (part, $0) }
        }
    }

    /// The same search without one of its choices.
    func without(_ part: FlightSearchPart) -> FlightSearch {
        var rest = self
        switch part {
        case .near:
            rest.near = nil
            rest.radiusKm = nil
        case .area: rest.area = []
        case .gliderClass: rest.gliderClass = nil
        case .kind: rest.kind = nil
        case .minKm: rest.minKm = nil
        case .country: rest.country = nil
        }
        return rest
    }

    /// The search of one choice alone, for that choice's chip.
    func only(_ part: FlightSearchPart) -> FlightSearch {
        switch part {
        case .near:
            guard let near, let radiusKm else { return .everything }
            return FlightSearch(near: near, radiusKm: radiusKm)
        case .area: return FlightSearch(area: area)
        case .gliderClass: return FlightSearch(gliderClass: gliderClass)
        case .kind: return FlightSearch(kind: kind)
        case .minKm: return FlightSearch(minKm: minKm)
        case .country: return FlightSearch(country: country)
        }
    }

    /// The same search with another area: a circle, or the corners of a
    /// drawn one, in place of either.
    func withArea(near: Coordinate?, radiusKm: Double?, area: [Coordinate]) -> FlightSearch {
        var next = without(.near).without(.area)
        next.near = near
        next.radiusKm = radiusKm
        next.area = area
        return next
    }

    /// Why the search cannot be asked, in the core's words; nothing where
    /// it can. The core says so as it writes the path.
    var refusal: String? {
        do {
            _ = try publicFlightsSearchPath(
                order: .newest, area: nil, fromUnixS: nil, toUnixS: nil, search: self, after: nil
            )
            return nil
        } catch {
            return error.words
        }
    }
}

/// What the search's sheet offers beside the core's bounds: the classes
/// and the kinds of route by the core's names.
enum FlightSearchChoices {
    static let bounds = flightSearchBounds()
    static let classes: [GliderClassKind] = [.a, .b, .c, .d, .ccc]
    static let kinds: [RouteKind] = [.freeFlight, .freeTriangle, .faiTriangle]

    /// The middle of the Alps north and south of the border, where a new
    /// circle starts, as on the web.
    static let middle = Coordinate(lat: 47.45, lon: 11.6)

    /// The radius a new circle starts with, kilometres.
    static let radiusKm = 10.0
}

/// Places on the sphere, for drawing a circle on the map: the take-off's
/// search itself is the core's and the server's.
enum SphereGeometry {
    /// The mean radius of the Earth the map is drawn on, metres.
    private static let earthM = 6_371_008.8

    /// The place so many kilometres from another in a direction, degrees
    /// clockwise from north.
    static func place(from start: Coordinate, km: Double, bearingDeg: Double) -> Coordinate {
        let rad = Double.pi / 180
        let angle = km * 1000 / earthM
        let bearing = bearingDeg * rad
        let lat1 = start.lat * rad
        let lon1 = start.lon * rad
        let lat2 = asin(sin(lat1) * cos(angle) + cos(lat1) * sin(angle) * cos(bearing))
        let lon2 = lon1 + atan2(sin(bearing) * sin(angle) * cos(lat1), cos(angle) - sin(lat1) * sin(lat2))
        let lon = (lon2 / rad + 540).truncatingRemainder(dividingBy: 360) - 180
        return Coordinate(lat: lat2 / rad, lon: lon)
    }

    /// A circle as a ring of corners, closed.
    static func ring(around centre: Coordinate, km: Double, corners: Int = 72) -> [Coordinate] {
        (0...corners).map { place(from: centre, km: km, bearingDeg: Double($0) * 360 / Double(corners)) }
    }
}
