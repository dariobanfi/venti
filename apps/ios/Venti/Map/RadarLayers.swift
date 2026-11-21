import MapLibre
import UIKit

/// How the rain radar is drawn: the chosen frame's picture as an image
/// pinned to the map by the layer's four corners, in a raster layer that
/// is not smoothed, so that a step of rain stays a step. It lies where the
/// forecast map's squares lie, over them: over the ground and the aerial
/// view's photographs, under the base map's cable cars, borders and names
/// and under everything else the app draws. It is as see-through as the
/// forecast map's squares. Choosing another frame swaps the picture.
@MainActor
enum RadarLayers {
    static let source = "rain-radar"
    static let layer = "rain-radar"

    /// Draws a picture, replacing the one before; takes the layer off the
    /// map for none.
    static func show(_ picture: RadarPicture?, in style: MLNStyle) {
        guard let picture, let image = UIImage(data: picture.png) else {
            if let drawn = style.layer(withIdentifier: layer) {
                style.removeLayer(drawn)
            }
            if let pinned = style.source(withIdentifier: source) {
                style.removeSource(pinned)
            }
            return
        }
        let corners = MLNCoordinateQuad(
            topLeft: CLLocationCoordinate2D(latitude: picture.north, longitude: picture.west),
            bottomLeft: CLLocationCoordinate2D(latitude: picture.south, longitude: picture.west),
            bottomRight: CLLocationCoordinate2D(latitude: picture.south, longitude: picture.east),
            topRight: CLLocationCoordinate2D(latitude: picture.north, longitude: picture.east)
        )
        if let pinned = style.source(withIdentifier: source) as? MLNImageSource {
            pinned.image = image
            pinned.coordinates = corners
            return
        }
        let pinned = MLNImageSource(identifier: source, coordinateQuad: corners, image: image)
        style.addSource(pinned)
        let raster = MLNRasterStyleLayer(identifier: layer, source: pinned)
        raster.rasterResamplingMode = NSExpression(forConstantValue: "nearest")
        // A frame takes the place of the one before at once: a picture
        // that fades in would show two moments' rain at a time.
        raster.rasterFadeDuration = NSExpression(forConstantValue: 0)
        raster.rasterOpacity = NSExpression(
            forMLNInterpolating: .zoomLevelVariable,
            curveType: .linear,
            parameters: nil,
            stops: NSExpression(forConstantValue: ThermalMapLayers.opacity)
        )
        if let squares = style.layer(withIdentifier: ThermalMapLayers.hourLayer)
            ?? style.layer(withIdentifier: ThermalMapLayers.layer)
        {
            style.insertLayer(raster, above: squares)
        } else if let names = style.layers.first(where: { $0 is MLNSymbolStyleLayer }) {
            style.insertLayer(raster, below: names)
        } else {
            style.addLayer(raster)
        }
    }
}
