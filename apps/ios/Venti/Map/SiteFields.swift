import CoreLocation
import MapLibre
import UIKit
import VentiCore

/// A flying site that its source maps as an area, a landing field or a
/// take-off's meadow, drawn as that area under the site's sign: an
/// outline and a faint fill in the sign's own colour, as the design
/// language draws an area on the map (its rule for airspace: a 2 px
/// outline and a 12 to 16 % fill). The field of a take-off that says
/// nothing to what the filter asks is drawn in the muted ink, as its sign
/// is.
///
/// A field is drawn only where it is larger than its sign: further out it
/// would be a speck under the pin.
@MainActor
enum SiteFields {
    static let source = "flying-site-fields"
    static let fill = "flying-site-fields"
    static let edge = "flying-site-fields-edge"

    /// The design language's fill of an area on the map, the lower end of
    /// its 12 to 16 %.
    static let fillOpacity = 0.12
    /// The design language's outline of an area on the map, points.
    static let edgeWidth = 2.0

    /// Adds the fields' layers to a style, empty. Call it before adding
    /// the sites' signs, which stand on them.
    static func add(to style: MLNStyle) {
        let fields = MLNShapeSource(identifier: source, shape: MLNShapeCollectionFeature(shapes: []), options: nil)
        style.addSource(fields)
        let colour = NSExpression(
            forConditional: NSPredicate(format: "unsaid == 1"),
            trueExpression: NSExpression(forConstantValue: Palette.siteUnsaidOnMap),
            falseExpression: NSExpression(forConstantValue: Palette.siteOnMap)
        )
        let wash = MLNFillStyleLayer(identifier: fill, source: fields)
        wash.fillColor = colour
        wash.fillOpacity = NSExpression(forConstantValue: fillOpacity)
        style.addLayer(wash)
        let line = MLNLineStyleLayer(identifier: edge, source: fields)
        line.lineColor = colour
        line.lineWidth = NSExpression(forConstantValue: edgeWidth)
        line.lineJoin = NSExpression(forConstantValue: "round")
        style.addLayer(line)
    }

    /// Draws these fields, replacing whatever was drawn.
    static func show(_ fields: [Field], in style: MLNStyle) {
        guard let source = style.source(withIdentifier: source) as? MLNShapeSource else { return }
        source.shape = MLNShapeCollectionFeature(shapes: fields.map(feature))
    }

    /// A site's area, and whether it is one of those shown apart.
    struct Field: Equatable {
        let id: String
        let outline: [Coordinate]
        let isUnsaid: Bool
    }

    /// The fields to draw of the sites on the map, at a scale of the map:
    /// those of the sites mapped as an area whose area is larger than the
    /// sign that stands on it.
    nonisolated static func fields(
        fitting: [SiteView], unsaid: [SiteView], metresPerPoint: Double, signPoints: Double
    ) -> [Field] {
        let all = fitting.map { ($0, false) } + unsaid.map { ($0, true) }
        return all.compactMap { site, isUnsaid in
            guard isLarger(site.outline, than: signPoints, metresPerPoint: metresPerPoint) else { return nil }
            return Field(id: site.id, outline: site.outline, isUnsaid: isUnsaid)
        }
    }

    /// Whether an area is drawn larger than a sign so many points wide,
    /// on a map with so many metres to a point: by the longer side of the
    /// box around it. A site drawn as a point has no area.
    nonisolated static func isLarger(_ outline: [Coordinate], than points: Double, metresPerPoint: Double) -> Bool {
        // A closed ring of fewer than four points is no area.
        guard outline.count >= 4, metresPerPoint > 0 else { return false }
        return extentM(of: outline) / metresPerPoint > points
    }

    /// The longer side of the box around an outline, metres.
    nonisolated static func extentM(of outline: [Coordinate]) -> Double {
        let lats = outline.map(\.lat)
        let lons = outline.map(\.lon)
        guard let south = lats.min(), let north = lats.max(), let west = lons.min(), let east = lons.max() else {
            return 0
        }
        // A degree of latitude in metres, and a degree of longitude as
        // much shorter as the place lies from the equator.
        let degreeM = 111_195.0
        let tall = (north - south) * degreeM
        let wide = (east - west) * degreeM * cos((south + north) / 2 * .pi / 180)
        return max(tall, wide)
    }

    static func feature(for field: Field) -> MLNPolygonFeature {
        let ring = field.outline.map(\.location)
        let feature = MLNPolygonFeature(coordinates: ring, count: UInt(ring.count))
        feature.attributes = ["id": field.id, "unsaid": field.isUnsaid ? 1 : 0]
        return feature
    }
}
