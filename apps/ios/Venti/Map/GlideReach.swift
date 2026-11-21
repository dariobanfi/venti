import CoreLocation
import Foundation
import MapLibre
import Observation
import VentiCore

/// What a glide reaches from a place on the map: the place pressed, the
/// pilot's three settings, the ground around the place and the core's
/// answer (`glideReach`).
///
/// Every rule and every word of the answer are the core's. This class
/// brings the ground around the place once (`TerrainModel.ground(covering:)`),
/// asks the core again for every change of a setting without asking any
/// server, and where the core says a direction ran off the ground loaded
/// (`cutShort`) brings the ground of a wider box and asks again, a few
/// times at most.
@MainActor
@Observable
final class GlideReachModel {
    /// The place the glide starts from; `nil` while nothing is shown.
    private(set) var place: Coordinate?
    /// The glide ratio in still air.
    var glideRatio = Preferences.defaultGlideRatio
    /// The height to start at, metres; `nil` for the ground at the place.
    var startHeightM: Double?
    /// Whether a wind is reckoned with; off at first.
    var windIsOn = false
    /// Where the wind comes from, degrees, and how strong it is, km/h.
    var windFromDeg = GlideReachWords.windFromDeg
    var windKmh = GlideReachWords.windKmh
    /// The core's answer for the setting last asked; `nil` while there is
    /// none.
    private(set) var answer: GlideReach?
    /// Whether the ground is being brought or the core asked.
    private(set) var isWorking = false

    /// The ground under an area; `nil` for none, and the core then says
    /// the ground is not loaded.
    @ObservationIgnored var ground: (@MainActor (Bounds) async -> Terrain)?
    /// The ground brought for the place, and how far around it reaches.
    @ObservationIgnored private var terrain: Terrain?
    @ObservationIgnored private var radiusKm = GlideReachWords.firstRadiusKm
    @ObservationIgnored private var askedFor: GlideReachSetting?

    init(ground: (@MainActor (Bounds) async -> Terrain)? = nil) {
        self.ground = ground
    }

    /// What the core is asked, for the place and the settings; `nil`
    /// while nothing is shown.
    var setting: GlideReachSetting? {
        guard let place else { return nil }
        return GlideReachSetting(
            lat: place.lat,
            lon: place.lon,
            glideRatio: glideRatio,
            startHeightM: startHeightM,
            windFromDeg: windIsOn ? windFromDeg : nil,
            windKmh: windIsOn ? Double(windKmh) : nil,
            marginM: nil
        )
    }

    /// Starts anew from a place: the pilot's glide, a start on the ground,
    /// no wind. The ground and the answer follow with `ask()`.
    func start(from place: Coordinate, glideRatio: Double) {
        self.place = place
        self.glideRatio = GlideReachWords.held(glideRatio)
        startHeightM = nil
        windIsOn = false
        windFromDeg = GlideReachWords.windFromDeg
        windKmh = GlideReachWords.windKmh
        answer = nil
        terrain = nil
        askedFor = nil
        radiusKm = GlideReachWords.firstRadiusKm
    }

    /// Takes the area away.
    func clear() {
        place = nil
        answer = nil
        terrain = nil
        askedFor = nil
        isWorking = false
    }

    /// Asks the core for the setting as it is now, bringing the ground
    /// first where there is none yet, and a wider box of it while a
    /// direction runs off what is loaded.
    func ask() async {
        guard let setting else { return }
        askedFor = setting
        isWorking = true
        defer {
            if askedFor == setting {
                isWorking = false
            }
        }
        if terrain == nil {
            terrain = await bring(around: setting)
        }
        while let terrain {
            let answer = await Self.reach(setting, over: terrain)
            guard askedFor == setting, !Task.isCancelled else { return }
            self.answer = answer
            guard answer.cutShort, radiusKm < GlideReachWords.widestRadiusKm, ground != nil else { return }
            radiusKm *= 2
            self.terrain = await bring(around: setting)
            guard askedFor == setting, !Task.isCancelled else { return }
        }
    }

    /// The ground around the place, as far as `radiusKm`; a terrain that
    /// knows none where there is no ground to be had.
    private func bring(around setting: GlideReachSetting) async -> Terrain {
        guard let ground else { return Terrain() }
        let place = Coordinate(lat: setting.lat, lon: setting.lon)
        return await ground(GlideReachWords.box(around: place, radiusKm: radiusKm))
    }

    /// The core's answer, off the main actor: 72 directions over the
    /// ground take a moment.
    nonisolated private static func reach(_ setting: GlideReachSetting, over terrain: Terrain) async -> GlideReach {
        await Task.detached(priority: .userInitiated) {
            glideReach(setting: setting, terrain: terrain)
        }.value
    }

    /// The start height one step up: from the ground, the next whole step
    /// above it; at most the core's highest start.
    func raiseStart() {
        let ground = answer?.groundM ?? 0
        let step = GlideReachWords.startStepM
        let next = startHeightM.map { $0 + step } ?? ((ground / step).rounded(.down) + 1) * step
        startHeightM = min(next, GlideReachWords.highestStartM)
    }

    /// The start height one step down, and the ground again once a step
    /// would reach it.
    func lowerStart() {
        guard let height = startHeightM else { return }
        let next = height - GlideReachWords.startStepM
        startHeightM = next <= (answer?.groundM ?? 0) ? nil : next
    }
}

/// What the glide's reach says that is not the core's: the entry, the
/// settings' names, their steps.
enum GlideReachWords {
    static let entry = "What a glide reaches from here"
    static let title = "What a glide reaches"
    static let working = "Reading the ground around the place."
    static let clear = "Clear"

    /// The glide moves by this, between these: as in the settings.
    static let glideStep = 0.5
    static let glideRatios = 4.0...14.0

    /// The start moves by this, metres, up to the core's highest start.
    static let startStepM = 50.0
    static let highestStartM = 9000.0

    /// The wind's direction moves a compass point at a time; its speed by
    /// 5 km/h. The first wind is a west wind of 15 km/h.
    static let windStepDeg = 45.0
    static let windFromDeg = 270.0
    static let windStepKmh = 5
    static let windsKmh = 5...60
    static let windKmh = 15

    /// The ground is brought this far around the place first, km, and the
    /// box doubled while a direction runs off it, up to the widest.
    static let firstRadiusKm = 10.0
    static let widestRadiusKm = 80.0

    /// A glide within the steps.
    static func held(_ glide: Double) -> Double {
        let stepped = (glide / glideStep).rounded() * glideStep
        return min(max(stepped, glideRatios.lowerBound), glideRatios.upperBound)
    }

    /// The box reaching `radiusKm` from a place each way.
    static func box(around place: Coordinate, radiusKm: Double) -> Bounds {
        let lat = radiusKm / 111.32
        let lon = radiusKm / (111.32 * max(cos(place.lat * .pi / 180), 0.01))
        return Bounds(south: place.lat - lat, west: place.lon - lon, north: place.lat + lat, east: place.lon + lon)
    }
}

/// What the map draws of a glide's reach: the area, its edge cut into
/// the stretches that end on the ground (a line) and those that end where
/// the ground loaded ends (dashed), and the start.
struct GlideReachDrawing: Equatable {
    /// Which answer it is, to draw again only when another comes.
    let id: String
    /// The place, to bring the area into view only when another is shown.
    let place: Coordinate
    let outline: [Coordinate]
    let edges: [[Coordinate]]
    let unloaded: [[Coordinate]]

    /// The drawing of an answer; `nil` where it has no area.
    init?(_ reach: GlideReach, from place: Coordinate) {
        guard reach.outline.count >= 4, reach.rays.count >= 3 else { return nil }
        self.place = place
        outline = reach.outline
        let stretches = Self.stretches(reach.rays)
        edges = stretches.filter { !$0.isUnloaded }.map(\.points)
        unloaded = stretches.filter(\.isUnloaded).map(\.points)
        id = "\(place.lat),\(place.lon) \(reach.words) \(reach.outline.count)"
    }

    /// The edge from each direction's end to the next, joined where they
    /// are alike: a stretch is unloaded where both its ends lie where the
    /// ground loaded ends.
    static func stretches(_ rays: [ReachRay]) -> [(points: [Coordinate], isUnloaded: Bool)] {
        var stretches: [(points: [Coordinate], isUnloaded: Bool)] = []
        for (index, ray) in rays.enumerated() {
            let next = rays[(index + 1) % rays.count]
            let isUnloaded = ray.end == .notLoaded && next.end == .notLoaded
            if let last = stretches.last, last.isUnloaded == isUnloaded {
                stretches[stretches.count - 1].points.append(next.endAt)
            } else {
                stretches.append((points: [ray.endAt, next.endAt], isUnloaded: isUnloaded))
            }
        }
        return stretches
    }

    /// The box around the area.
    var box: Bounds {
        let lats = (outline + [place]).map(\.lat)
        let lons = (outline + [place]).map(\.lon)
        return Bounds(
            south: lats.min() ?? place.lat, west: lons.min() ?? place.lon,
            north: lats.max() ?? place.lat, east: lons.max() ?? place.lon
        )
    }
}

/// The glide's reach on the overview map: a wash in the colour of what is
/// in reach at low opacity, its edge a line with the pack's casing, dashed
/// where the ground loaded ends, and the start as the map's smallest dot.
@MainActor
enum GlideReachLayers {
    static let source = "glide-reach"
    /// As faint as the airspace's wash (12 to 16%): the map under it
    /// stays readable.
    static let fillOpacity = 0.14

    static func add(to style: MLNStyle) {
        let shapes = MLNShapeSource(identifier: source, shape: MLNShapeCollectionFeature(shapes: []), options: nil)
        style.addSource(shapes)
        let wash = MLNFillStyleLayer(identifier: "glide-reach-wash", source: shapes)
        wash.fillColor = NSExpression(forConstantValue: Palette.goodOnMap)
        wash.fillOpacity = NSExpression(forConstantValue: fillOpacity)
        wash.predicate = NSPredicate(format: "kind == 'area'")
        style.addLayer(wash)
        let casing = MapSetup.line("glide-reach-casing", from: shapes, color: Palette.haloOnMap, width: 5)
        casing.predicate = NSPredicate(format: "kind == 'edge' OR kind == 'unloaded'")
        style.addLayer(casing)
        let edge = MapSetup.line("glide-reach-edge", from: shapes, color: Palette.goodOnMap, width: 2)
        edge.predicate = NSPredicate(format: "kind == 'edge'")
        style.addLayer(edge)
        let unloaded = MapSetup.line("glide-reach-unloaded", from: shapes, color: Palette.goodOnMap, width: 2)
        unloaded.lineDashPattern = NSExpression(forConstantValue: [3, 2])
        unloaded.lineCap = NSExpression(forConstantValue: "butt")
        unloaded.predicate = NSPredicate(format: "kind == 'unloaded'")
        style.addLayer(unloaded)
        let start = MapSetup.dot(
            "glide-reach-start", from: shapes, radius: MapSetup.smallestDot,
            color: Palette.inkOnMap, ring: Palette.haloOnMap
        )
        start.predicate = NSPredicate(format: "kind == 'start'")
        style.addLayer(start)
    }

    /// Draws this reach, or nothing, in place of what was drawn.
    static func show(_ drawing: GlideReachDrawing?, in style: MLNStyle) {
        guard let shapes = style.source(withIdentifier: source) as? MLNShapeSource else { return }
        guard let drawing else {
            shapes.shape = MLNShapeCollectionFeature(shapes: [])
            return
        }
        var features: [MLNShape & MLNFeature] = []
        let ring = drawing.outline.map(\.location)
        let area = MLNPolygonFeature(coordinates: ring, count: UInt(ring.count))
        area.attributes = ["kind": "area"]
        features.append(area)
        for (kind, lines) in [("edge", drawing.edges), ("unloaded", drawing.unloaded)] {
            for line in lines {
                let points = line.map(\.location)
                let feature = MLNPolylineFeature(coordinates: points, count: UInt(points.count))
                feature.attributes = ["kind": kind]
                features.append(feature)
            }
        }
        let start = MLNPointFeature()
        start.coordinate = drawing.place.location
        start.attributes = ["kind": "start"]
        features.append(start)
        shapes.shape = MLNShapeCollectionFeature(shapes: features)
    }
}
