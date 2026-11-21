import MapLibre
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
}
