import CoreLocation
import MapLibre
import UIKit
import VentiCore

/// How airspace is drawn on a map: an outline and a light wash in the
/// colour of how the app treats it, and its name with its floor and
/// ceiling.
///
/// Magenta for airspace to stay out of, as aviation charts draw their
/// boundaries; amber for airspace that deserves attention; a thin grey
/// line for the rest. None of them is a colour a track or the wind uses.
@MainActor
enum AirspaceLayers {
    static let source = "airspace"

    /// Below this zoom a country's airspace is a tangle; it is left off.
    static let minimumZoom: Float = 7

    static func color(for policy: AirspacePolicy) -> UIColor {
        switch policy {
        case .avoid: UIColor(hex: 0xD55181)
        case .caution: UIColor(hex: 0xC98500)
        case .free: UIColor(hex: 0x85847F)
        }
    }

    private static func rank(_ policy: AirspacePolicy) -> Int {
        switch policy {
        case .free: 0
        case .caution: 1
        case .avoid: 2
        }
    }

    /// Adds the airspace layers to a style, empty. Call it before adding
    /// whatever is to lie on top.
    static func add(to style: MLNStyle) {
        let airspace = MLNShapeSource(identifier: source, shape: MLNShapeCollectionFeature(shapes: []), options: nil)
        style.addSource(airspace)

        // The least restrictive first, so that what matters most lies on top.
        for policy in [AirspacePolicy.free, .caution, .avoid] {
            let only = NSPredicate(format: "policy == %d", rank(policy))
            if policy != .free {
                let wash = MLNFillStyleLayer(identifier: "airspace-wash-\(rank(policy))", source: airspace)
                wash.fillColor = NSExpression(forConstantValue: color(for: policy))
                // On a map of a whole region the washes of airspace lying
                // over airspace would paint the wind and the valleys
                // over: there only the edges are drawn, and the wash
                // comes in as the pilot zooms to a valley.
                let full = policy == .avoid ? 0.1 : 0.06
                wash.fillOpacity = NSExpression(
                    forMLNInterpolating: .zoomLevelVariable,
                    curveType: .linear,
                    parameters: nil,
                    stops: NSExpression(forConstantValue: [8: 0.0, 10: full])
                )
                wash.predicate = only
                wash.minimumZoomLevel = minimumZoom
                style.addLayer(wash)
            }
            let edge = MapSetup.line(
                "airspace-edge-\(rank(policy))", from: airspace, color: color(for: policy),
                width: policy == .avoid ? 2 : 1.2
            )
            edge.predicate = only
            edge.minimumZoomLevel = minimumZoom
            style.addLayer(edge)
        }

        let names = MLNSymbolStyleLayer(identifier: "airspace-names", source: airspace)
        names.text = NSExpression(forKeyPath: "label")
        names.textFontNames = NSExpression(forConstantValue: ["noto-sans-regular"])
        names.textFontSize = NSExpression(forConstantValue: 11)
        names.textColor = NSExpression(forConstantValue: UIColor(hex: 0x7A1F48))
        names.textHaloColor = NSExpression(forConstantValue: Palette.haloOnMap)
        names.textHaloWidth = NSExpression(forConstantValue: 1.4)
        // Airspace open to a paraglider is drawn, but not named.
        names.predicate = NSPredicate(format: "policy > 0")
        names.minimumZoomLevel = 9
        style.addLayer(names)
    }

    /// Draws these airspaces, replacing whatever was drawn.
    static func show(_ shapes: [AirspaceShape], in style: MLNStyle) {
        guard let airspace = style.source(withIdentifier: source) as? MLNShapeSource else { return }
        airspace.shape = MLNShapeCollectionFeature(shapes: shapes.map(feature))
    }

    static func feature(for shape: AirspaceShape) -> MLNPolygonFeature {
        let outline = shape.outline.map(\.location)
        let holes = shape.holes.map { hole in
            let ring = hole.map(\.location)
            return MLNPolygon(coordinates: ring, count: UInt(ring.count))
        }
        let feature = MLNPolygonFeature(coordinates: outline, count: UInt(outline.count), interiorPolygons: holes)
        feature.attributes = [
            "id": shape.id,
            "policy": rank(shape.policy),
            "label": label(for: shape),
        ]
        return feature
    }

    /// The name with the floor and ceiling under it, as on a chart.
    static func label(for shape: AirspaceShape) -> String {
        let limits = "\(shape.lower) – \(shape.upper)"
        return shape.name.isEmpty ? limits : "\(shape.name)\n\(limits)"
    }
}
