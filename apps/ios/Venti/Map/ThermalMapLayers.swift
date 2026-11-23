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

    /// How large the forecast's arrows are against a station's: smaller,
    /// so that the two are not taken for each other.
    static let arrowScale = 0.6
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
        symbols.iconScale = NSExpression(forConstantValue: arrowScale)
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
    /// are drawn by `show(_:in:)` alone.
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
}
