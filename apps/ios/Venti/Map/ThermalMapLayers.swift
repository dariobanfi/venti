import MapLibre
import SwiftUI
import UIKit
import VentiCore

/// How the forecast map is drawn: every square of the chosen day that has
/// a kind of day, filled in the step of the design language's thermal
/// scale the core names, and a day that is not flyable as a quiet veil in
/// the muted ink. One source and one fill layer: choosing another day swaps the
/// source's shape.
///
/// The fills are see-through, half at the zoom of a region and less as
/// the pilot zooms to a valley, where one square fills the screen. They
/// lie over the drawn ground, its relief shading, its roads and the aerial
/// view's photographs, and under the base map's cable cars, borders and
/// names and everything else the app draws, as on Android.
@MainActor
enum ThermalMapLayers {
    static let source = "thermal-squares"
    static let layer = "thermal-squares"

    /// How see-through the squares are, by the map's zoom: half over a
    /// region, fading to 30 % in a valley.
    static let opacity: [Int: Double] = [8: 0.5, 12: 0.3]

    /// The base map's layers the squares lie under, the lowest first: the
    /// cable cars (the aerial view's casing of them first), the borders.
    /// The names come after them.
    private static let over = ["aerial-aerialway-casing", "aerialway", "boundary"]

    /// Adds the layer to a style, empty: under the base map's cable cars,
    /// borders and names, or on top where the style has none of them. Call
    /// it before adding what the app draws, so that all of that lies over
    /// it.
    static func add(to style: MLNStyle) {
        let squares = MLNShapeSource(identifier: source, shape: MLNShapeCollectionFeature(shapes: []), options: nil)
        style.addSource(squares)

        let fill = MLNFillStyleLayer(identifier: layer, source: squares)
        // The map is light in both appearances: the light theme's steps.
        var steps: [NSNumber: UIColor] = [:]
        for step in 1...6 {
            steps[NSNumber(value: step)] = ThermalBand(scaleStep: UInt8(step)).uiColorOnMap
        }
        fill.fillColor = NSExpression(
            forMLNStepping: NSExpression(forKeyPath: "fill"),
            from: NSExpression(forConstantValue: Palette.notFlyableOnMap),
            stops: NSExpression(forConstantValue: steps)
        )
        // No antialiased outline: with see-through fills it adds up where
        // neighbours meet, and a day of one kind would read as a grid.
        fill.fillAntialiased = NSExpression(forConstantValue: false)
        fill.fillOpacity = NSExpression(
            forMLNInterpolating: .zoomLevelVariable,
            curveType: .linear,
            parameters: nil,
            stops: NSExpression(forConstantValue: opacity)
        )
        let base = over.compactMap { name in style.layer(withIdentifier: name) }
        let names = style.layers.first(where: { $0 is MLNSymbolStyleLayer })
        if let sibling = base.first ?? names {
            style.insertLayer(fill, below: sibling)
        } else {
            style.addLayer(fill)
        }
    }

    /// Draws a day's squares, replacing whatever was drawn; nothing for
    /// no day.
    static func show(_ day: ThermalMapDay?, in style: MLNStyle) {
        guard let squares = style.source(withIdentifier: source) as? MLNShapeSource else { return }
        let drawn = day.map(ThermalSquares.squares(of:)) ?? []
        squares.shape = MLNShapeCollectionFeature(shapes: drawn.map(feature))
    }

    /// One square as the fill layer reads it: its fill, and its place
    /// among the day's cells as its identifier.
    static func feature(for square: ThermalSquares.Square) -> MLNPolygonFeature {
        let ring = square.ring.map(\.location)
        let feature = MLNPolygonFeature(coordinates: ring, count: UInt(ring.count), interiorPolygons: [])
        feature.attributes = [
            "id": String(square.index),
            "fill": square.fill,
        ]
        return feature
    }
    // MARK: The hours of a day

    /// The squares of an hour of a layer, and the wind's arrows over them.
    static let hourSource = "thermal-hour-squares"
    static let hourLayer = "thermal-hour-squares"
    static let arrowSource = "thermal-hour-arrows"
    static let arrowLayer = "thermal-hour-arrows"

    /// How large the forecast's arrows are against a station's of their
    /// band: a little smaller, so that the two are not taken for each
    /// other, and larger as the wind is stronger, as the stations' are.
    static let arrowScale = 0.8
    /// From this zoom a square is wide enough for its arrow's label.
    static let labelsFrom = 9.0

    /// Adds the fill layer of an hour's squares, empty, beside the day's.
    /// Call it with `add(to:)`.
    static func addHours(to style: MLNStyle) {
        let squares = MLNShapeSource(identifier: hourSource, shape: MLNShapeCollectionFeature(shapes: []), options: nil)
        style.addSource(squares)
        let fill = MLNFillStyleLayer(identifier: hourLayer, source: squares)
        fill.fillColor = NSExpression(forConstantValue: UIColor.clear)
        fill.fillAntialiased = NSExpression(forConstantValue: false)
        fill.fillOpacity = NSExpression(
            forMLNInterpolating: .zoomLevelVariable,
            curveType: .linear,
            parameters: nil,
            stops: NSExpression(forConstantValue: opacity)
        )
        if let day = style.layer(withIdentifier: layer) {
            style.insertLayer(fill, above: day)
        } else {
            style.addLayer(fill)
        }
    }

    /// Adds the layer of the forecast's arrows, empty. Call it just before
    /// the stations' layer is added, so that the stations lie over it. The
    /// pictures are the stations' (`WindSymbols`), drawn smaller.
    static func addArrows(to style: MLNStyle) {
        let arrows = MLNShapeSource(identifier: arrowSource, shape: MLNShapeCollectionFeature(shapes: []), options: nil)
        style.addSource(arrows)
        let symbols = MLNSymbolStyleLayer(identifier: arrowLayer, source: arrows)
        // "wind-calm" for a dot, else the picture of the band's arrow.
        symbols.iconImageName = NSExpression(
            format: "TERNARY(symbol == 'calm', 'wind-calm', mgl_join({'wind-arrow-', band}))"
        )
        symbols.iconRotation = NSExpression(forKeyPath: "heading")
        symbols.iconRotationAlignment = NSExpression(forConstantValue: "map")
        var sizes: [String: NSNumber] = [:]
        for band in WindBand.all {
            sizes[band.name] = NSNumber(value: arrowScale * band.arrowScale)
        }
        symbols.iconScale = NSExpression(
            forMLNMatchingKey: NSExpression(forKeyPath: "band"),
            in: sizes.reduce(into: [NSExpression: NSExpression]()) { all, size in
                all[NSExpression(forConstantValue: size.key)] = NSExpression(forConstantValue: size.value)
            },
            default: NSExpression(forConstantValue: arrowScale)
        )
        symbols.iconAllowsOverlap = NSExpression(forConstantValue: true)
        style.addLayer(symbols)
        // The labels, from the zoom at which a square has room for one: a
        // layer of their own, under each arrow.
        let labels = MLNSymbolStyleLayer(identifier: arrowLayer + "-labels", source: arrows)
        labels.minimumZoomLevel = Float(labelsFrom)
        labels.text = NSExpression(forKeyPath: "label")
        labels.textFontNames = NSExpression(forConstantValue: ["noto-sans-regular"])
        labels.textFontSize = NSExpression(forConstantValue: 11)
        labels.textAnchor = NSExpression(forConstantValue: "top")
        labels.textOffset = NSExpression(forConstantValue: NSValue(cgVector: CGVector(dx: 0, dy: 0.8)))
        labels.textColor = NSExpression(forConstantValue: Palette.inkOnMap)
        labels.textHaloColor = NSExpression(forConstantValue: Palette.haloOnMap)
        labels.textHaloWidth = NSExpression(forConstantValue: 1.4)
        style.addLayer(labels)
    }

    /// Draws an hour: the squares filled by their step through the key,
    /// and the arrows. Nothing for no hour, and then the day's squares
    /// are drawn by `show(_:in:)` alone. On the wind's layers the screen
    /// hands no squares: the arrows stand in their place.
    static func showHour(_ shapes: ThermalMapShapes?, key: [ThermalMapKey], in style: MLNStyle) {
        guard let squares = style.source(withIdentifier: hourSource) as? MLNShapeSource,
              let arrows = style.source(withIdentifier: arrowSource) as? MLNShapeSource
        else { return }
        if let fill = style.layer(withIdentifier: hourLayer) as? MLNFillStyleLayer {
            fill.fillColor = fillColor(for: key)
        }
        squares.shape = shapes.flatMap { shape(from: $0.squares) } ?? MLNShapeCollectionFeature(shapes: [])
        arrows.shape = shapes.flatMap { shape(from: $0.arrows) } ?? MLNShapeCollectionFeature(shapes: [])
    }

    /// The core's GeoJSON text as a shape for a source.
    static func shape(from text: String) -> MLNShape? {
        try? MLNShape(data: Data(text.utf8), encoding: String.Encoding.utf8.rawValue)
    }

    /// A square's colour by its `step`, as the key names it; a step the
    /// key does not name is not drawn.
    static func fillColor(for key: [ThermalMapKey]) -> NSExpression {
        guard !key.isEmpty else { return NSExpression(forConstantValue: UIColor.clear) }
        var colours: [NSExpression: NSExpression] = [:]
        for line in key {
            colours[NSExpression(forConstantValue: NSNumber(value: line.step))] =
                NSExpression(forConstantValue: UIColor(ThermalKeyColour.onMap(line)))
        }
        return NSExpression(
            forMLNMatchingKey: NSExpression(forKeyPath: "step"),
            in: colours,
            default: NSExpression(forConstantValue: UIColor.clear)
        )
    }
}

/// The colour of a line of a layer's key, which the core names as a scale
/// of the pack and a step of it: the light theme's, as the map is light
/// in both appearances. The app chooses no colour of its own.
enum ThermalKeyColour {
    static func onMap(_ key: ThermalMapKey) -> Color {
        onMap(scale: key.scale, scaleStep: key.scaleStep)
    }

    /// The colour of a step of a scale, counted from 1; the rain radar's
    /// key names its colours so too.
    static func onMap(scale: ThermalMapScale, scaleStep: UInt8) -> Color {
        let theme = VentiTheme.light
        let index = Int(scaleStep) - 1
        func pick(_ scale: [Color]) -> Color {
            scale[min(max(index, 0), scale.count - 1)]
        }
        return switch scale {
        case .thermal: pick(theme.thermal)
        case .wind: pick(theme.wind)
        case .vario: pick(theme.vario)
        case .veil: Color(uiColor: Palette.notFlyableOnMap)
        }
    }
}

/// An hour of a layer of the forecast map as the map draws it: the core's
/// squares and arrows, the key its squares are filled through, and what
/// it is, so that the map draws it again only when that changes.
struct ForecastHourDrawing {
    /// The day, the layer, the level and the hour.
    let id: String
    let shapes: ThermalMapShapes
    let key: [ThermalMapKey]
    /// Where the hour's wind comes from at a place, degrees, by the
    /// square the place lies in (`SiteFans.windFrom(arrows:)`); `nil`
    /// where the hour has no direction there, and on a layer without
    /// arrows. The take-offs' fans are coloured by it.
    var windFrom: (Coordinate) -> Double? = { _ in nil }
    /// Whether the hour has a wind direction anywhere, and so colours the
    /// fans: the map's panel then has their key.
    var colorsFans = false
}

/// What the map draws of the thermal forecast by region: the regions'
/// outlines, the levels of the day or the hour shown by region, and the
/// squares of the grid that lie under a region with a level, which are
/// not drawn.
struct RegionsDrawing {
    /// The set of regions, the day and the hour: the map draws again
    /// only when it changes.
    let id: String
    let regions: ForecastRegions
    /// Each region's level, by its identifier; a region without one is
    /// not filled.
    let levels: [String: RegionLevel]
    /// The squares of the grid, by their place among the day's cells,
    /// that the regions' fill stands in place of.
    let under: Set<Int>
}

/// How the thermal forecast by region is drawn (decision 27, the web's
/// `regions.ts`): each region that has a level filled with its level's
/// colour at the level's own opacity (design/DESIGN.md, "Day levels"), the
/// light theme's, the map being light in both appearances; a thin edge in
/// the labels' paper round each; over the grid's squares and under the
/// base map's cable cars, borders and names, as a weather layer.
@MainActor
enum RegionLayers {
    static let source = "forecast-regions"
    static let fill = "forecast-regions"
    static let edge = "forecast-regions-edge"

    /// Adds the two layers, empty, over the hours' squares. Call it after
    /// `ThermalMapLayers.addHours(to:)`.
    static func add(to style: MLNStyle) {
        let shapes = MLNShapeSource(identifier: source, shape: MLNShapeCollectionFeature(shapes: []), options: nil)
        style.addSource(shapes)
        let fill = MLNFillStyleLayer(identifier: fill, source: shapes)
        fill.fillColor = NSExpression(forConstantValue: UIColor.clear)
        fill.fillOpacity = NSExpression(forConstantValue: 0)
        fill.fillAntialiased = NSExpression(forConstantValue: false)
        let line = MLNLineStyleLayer(identifier: edge, source: shapes)
        line.lineColor = NSExpression(forConstantValue: Palette.haloOnMap)
        line.lineWidth = NSExpression(
            forMLNInterpolating: .zoomLevelVariable,
            curveType: .linear,
            parameters: nil,
            stops: NSExpression(forConstantValue: [7: 0.5, 11: 1.5])
        )
        line.lineOpacity = NSExpression(forConstantValue: 0)
        if let hours = style.layer(withIdentifier: ThermalMapLayers.hourLayer) {
            style.insertLayer(fill, above: hours)
        } else {
            style.addLayer(fill)
        }
        style.insertLayer(line, above: fill)
    }

    /// Sets the regions' outlines, a GeoJSON `FeatureCollection` whose
    /// features carry their `id`; none for no regions.
    static func showOutlines(_ geoJSON: String?, in style: MLNStyle) {
        guard let shapes = style.source(withIdentifier: source) as? MLNShapeSource else { return }
        shapes.shape = geoJSON.flatMap(ThermalMapLayers.shape(from:)) ?? MLNShapeCollectionFeature(shapes: [])
    }

    /// Fills each region by its level, and leaves out the grid's squares
    /// under them; nothing filled and the whole grid for no levels.
    static func show(levels: [String: RegionLevel], under: Set<Int>, in style: MLNStyle) {
        if let fill = style.layer(withIdentifier: fill) as? MLNFillStyleLayer {
            fill.fillColor = byRegion(levels, { NSExpression(forConstantValue: UIColor($0.colorOnMap)) }, otherwise: UIColor.clear)
            fill.fillOpacity = byRegion(levels, { NSExpression(forConstantValue: $0.fillOpacityOnMap) }, otherwise: 0)
        }
        if let line = style.layer(withIdentifier: edge) as? MLNLineStyleLayer {
            line.lineOpacity = byRegion(levels, { _ in NSExpression(forConstantValue: 0.9) }, otherwise: 0)
        }
        let cells = under.sorted()
        if let day = style.layer(withIdentifier: ThermalMapLayers.layer) as? MLNFillStyleLayer {
            day.predicate = cells.isEmpty ? nil : NSPredicate(format: "NOT (id IN %@)", cells.map(String.init))
        }
        if let hour = style.layer(withIdentifier: ThermalMapLayers.hourLayer) as? MLNFillStyleLayer {
            hour.predicate = cells.isEmpty ? nil : NSPredicate(format: "NOT (cell IN %@)", cells.map { NSNumber(value: $0) })
        }
    }

    /// A value by the region's `id`: its level's, or `otherwise`.
    private static func byRegion(
        _ levels: [String: RegionLevel], _ value: (RegionLevel) -> NSExpression, otherwise: Any
    ) -> NSExpression {
        guard !levels.isEmpty else { return NSExpression(forConstantValue: otherwise) }
        var values: [NSExpression: NSExpression] = [:]
        for (id, level) in levels {
            values[NSExpression(forConstantValue: id)] = value(level)
        }
        return NSExpression(
            forMLNMatchingKey: NSExpression(forKeyPath: "id"),
            in: values,
            default: NSExpression(forConstantValue: otherwise)
        )
    }
}
