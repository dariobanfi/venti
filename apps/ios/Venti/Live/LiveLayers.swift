import MapLibre
import UIKit
import VentiCore

/// The gliders of other pilots as a map draws them: one layer, the same
/// on the map tab and in the cockpit. A wing turned the way it flies,
/// with the pilot's name and altitude where there is room; a glider is
/// never left out for want of room.
@MainActor
enum LiveLayers {
    /// Adds a source and a symbol layer for gliders to a style, both
    /// under one name, with nothing in them yet.
    static func add(to style: MLNStyle, named name: String) {
        for picture in LiveSymbols.all {
            style.setImage(picture.image, forName: picture.name)
        }
        let gliders = MLNShapeSource(identifier: name, shape: MLNShapeCollectionFeature(shapes: []), options: nil)
        style.addSource(gliders)
        let wings = MLNSymbolStyleLayer(identifier: name, source: gliders)
        wings.iconImageName = NSExpression(forKeyPath: "picture")
        wings.iconRotation = NSExpression(forKeyPath: "heading")
        wings.iconRotationAlignment = NSExpression(forConstantValue: "map")
        wings.iconAllowsOverlap = NSExpression(forConstantValue: true)
        wings.text = NSExpression(forKeyPath: "label")
        wings.textFontNames = NSExpression(forConstantValue: ["noto-sans-regular"])
        wings.textFontSize = NSExpression(forConstantValue: 12)
        wings.textAnchor = NSExpression(forConstantValue: "top")
        wings.textOffset = NSExpression(forConstantValue: NSValue(cgVector: CGVector(dx: 0, dy: 1.15)))
        wings.textColor = NSExpression(forConstantValue: UIColor(hex: 0x0B0B0B))
        wings.textHaloColor = NSExpression(forConstantValue: UIColor.white)
        wings.textHaloWidth = NSExpression(forConstantValue: 1.6)
        wings.textOptional = NSExpression(forConstantValue: true)
        style.addLayer(wings)
    }

    /// One glider as the layer reads it. A glider on the ground points
    /// nowhere.
    static func feature(for glider: LiveGlider) -> MLNPointFeature {
        let feature = MLNPointFeature()
        feature.coordinate = glider.position.location
        feature.attributes = [
            "id": glider.id,
            "picture": LiveSymbols.name(for: glider),
            "heading": glider.landed ? 0 : glider.courseDeg ?? 0,
            "label": glider.label,
        ]
        return feature
    }

    /// Puts gliders into the source of that name.
    static func show(_ gliders: [LiveGlider], in style: MLNStyle, named name: String) {
        guard let source = style.source(withIdentifier: name) as? MLNShapeSource else { return }
        source.shape = MLNShapeCollectionFeature(shapes: gliders.map(feature))
    }
}
