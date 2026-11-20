import CoreLocation
import MapLibre
import UIKit
import VentiCore

/// What every map in the app has in common.
///
/// The map engine is MapLibre Native, wrapped directly.
@MainActor
enum MapSetup {
    /// A map view with the app's style, drawn from the given source, and
    /// no camera yet.
    static func makeMapView(from source: MapStyle.Source = .standard) -> MLNMapView {
        // If the style cannot be prepared the map falls back to MapLibre's
        // default style; the tests check that it can.
        let map = MLNMapView(frame: .zero, styleURL: try? MapStyle.url(for: source))
        map.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        map.logoView.isHidden = true
        // Each screen says how much of the map its own controls cover.
        map.automaticallyAdjustsContentInset = false
        map.attributionButtonPosition = .bottomLeft
        // The map is flat; tilting it shows nothing more.
        map.isPitchEnabled = false
        map.tintColor = Palette.accentOnMap
        map.attributionButton.tintColor = creditsTint(aerial: false)
        return map
    }

    /// The layers of the map's two views, read from the shared style once.
    private static let views = try? MapStyle.views()

    /// The credits button is the map's, not one of the app's controls:
    /// quiet on the pale drawn map, and light on photographs, which are dark.
    private static func creditsTint(aerial: Bool) -> UIColor {
        aerial ? Palette.haloOnMap.withAlphaComponent(0.9) : UIColor(white: 0.25, alpha: 0.75)
    }

    /// Switches a map between the drawn ground and the aerial view:
    /// photographs from above, under the map's names and under everything
    /// the app draws. The photographs are fetched only while they show.
    /// Nothing happens before the map has its style.
    static func showAerial(_ aerial: Bool, on map: MLNMapView) {
        guard let views, let style = map.style else { return }
        for name in views.aerial {
            style.layer(withIdentifier: name)?.isVisible = aerial
        }
        for name in views.drawn {
            style.layer(withIdentifier: name)?.isVisible = !aerial
        }
        map.attributionButton.tintColor = creditsTint(aerial: aerial)
    }

    /// Moves the camera so the box is in view, clear of what covers the map.
    static func show(_ box: Bounds, on map: MLNMapView, animated: Bool) {
        // A box with no extent, such as a single position, would zoom in
        // without limit.
        let margin = 0.002
        let padded = MLNCoordinateBounds(
            sw: CLLocationCoordinate2D(latitude: box.south - margin, longitude: box.west - margin),
            ne: CLLocationCoordinate2D(latitude: box.north + margin, longitude: box.east + margin)
        )
        map.setVisibleCoordinateBounds(
            padded,
            edgePadding: UIEdgeInsets(top: 28, left: 36, bottom: 28, right: 36),
            animated: animated,
            completionHandler: nil
        )
    }

    /// A line layer with round ends and corners.
    static func line(
        _ identifier: String, from source: MLNSource, color: UIColor, width: Double
    ) -> MLNLineStyleLayer {
        let layer = MLNLineStyleLayer(identifier: identifier, source: source)
        layer.lineColor = NSExpression(forConstantValue: color)
        layer.lineWidth = NSExpression(forConstantValue: width)
        layer.lineJoin = NSExpression(forConstantValue: "round")
        layer.lineCap = NSExpression(forConstantValue: "round")
        return layer
    }

    /// A round dot with a ring around it.
    static func dot(
        _ identifier: String, from source: MLNSource, radius: Double, color: UIColor, ring: UIColor
    ) -> MLNCircleStyleLayer {
        let layer = MLNCircleStyleLayer(identifier: identifier, source: source)
        layer.circleRadius = NSExpression(forConstantValue: radius)
        layer.circleColor = NSExpression(forConstantValue: color)
        layer.circleStrokeColor = NSExpression(forConstantValue: ring)
        layer.circleStrokeWidth = NSExpression(forConstantValue: 2.5)
        return layer
    }
}

extension TrackPoint {
    var coordinate: CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: lat, longitude: lon)
    }
}

extension Coordinate {
    var location: CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: lat, longitude: lon)
    }
}
