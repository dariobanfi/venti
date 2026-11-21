import MapLibre
import UIKit

/// How the rain radar is drawn: each picture of the chosen moment as an
/// image of its own, pinned to the map by its layer's four corners, in a
/// raster layer of its own that is not smoothed, so that a step of rain
/// stays a step; the pictures in the moment's order, the later above the
/// earlier, never merged into one. They lie where the forecast map's
/// squares lie, over them: over the ground and the aerial view's
/// photographs, under the base map's cable cars, borders and names and
/// under everything else the app draws. They are as see-through as the
/// forecast map's squares. Choosing another moment swaps the pictures,
/// and a layer that has none at the moment is taken off the map.
@MainActor
enum RadarLayers {
    /// What the name of a picture's source and of its layer begins with;
    /// the radar source's own name follows.
    static let prefix = "rain-radar-"

    /// Draws a moment's pictures, the first at the bottom, replacing
    /// those before; takes a layer off the map that has no picture among
    /// them, and all of them for none.
    static func show(_ pictures: [RadarPicture], in style: MLNStyle) {
        let wanted = Set(pictures.map { prefix + $0.source })
        for drawn in style.layers where drawn.identifier.hasPrefix(prefix) && !wanted.contains(drawn.identifier) {
            style.removeLayer(drawn)
        }
        for pinned in style.sources where pinned.identifier.hasPrefix(prefix) && !wanted.contains(pinned.identifier) {
            style.removeSource(pinned)
        }
        // The picture drawn last, which the next lies over.
        var under: MLNStyleLayer?
        for picture in pictures {
            guard let image = UIImage(data: picture.png) else { continue }
            let name = prefix + picture.source
            let corners = MLNCoordinateQuad(
                topLeft: CLLocationCoordinate2D(latitude: picture.north, longitude: picture.west),
                bottomLeft: CLLocationCoordinate2D(latitude: picture.south, longitude: picture.west),
                bottomRight: CLLocationCoordinate2D(latitude: picture.south, longitude: picture.east),
                topRight: CLLocationCoordinate2D(latitude: picture.north, longitude: picture.east)
            )
            if let pinned = style.source(withIdentifier: name) as? MLNImageSource,
               let raster = style.layer(withIdentifier: name)
            {
                pinned.image = image
                pinned.coordinates = corners
                under = raster
                continue
            }
            let pinned = MLNImageSource(identifier: name, coordinateQuad: corners, image: image)
            style.addSource(pinned)
            let raster = MLNRasterStyleLayer(identifier: name, source: pinned)
            raster.rasterResamplingMode = NSExpression(forConstantValue: "nearest")
            // A picture takes the place of the one before at once: one
            // that fades in would show two moments' rain at a time.
            raster.rasterFadeDuration = NSExpression(forConstantValue: 0)
            raster.rasterOpacity = NSExpression(
                forMLNInterpolating: .zoomLevelVariable,
                curveType: .linear,
                parameters: nil,
                stops: NSExpression(forConstantValue: ThermalMapLayers.opacity)
            )
            if let under {
                style.insertLayer(raster, above: under)
            } else if let above = style.layers.first(where: { $0.identifier.hasPrefix(prefix) }) {
                // A layer that comes back under one that stayed.
                style.insertLayer(raster, below: above)
            } else if let squares = style.layer(withIdentifier: ThermalMapLayers.hourLayer)
                ?? style.layer(withIdentifier: ThermalMapLayers.layer)
            {
                style.insertLayer(raster, above: squares)
            } else if let names = style.layers.first(where: { $0 is MLNSymbolStyleLayer }) {
                style.insertLayer(raster, below: names)
            } else {
                style.addLayer(raster)
            }
            under = raster
        }
    }
}
