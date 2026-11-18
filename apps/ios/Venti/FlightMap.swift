import MapLibre
import SwiftUI
import VentiCore

/// A map showing one flight's track.
///
/// The map engine is MapLibre Native, wrapped directly. All map data comes
/// from files inside the app, so the map works with no connection.
struct FlightMap: UIViewRepresentable {
    let flight: FlightView

    func makeCoordinator() -> Coordinator {
        Coordinator(flight: flight)
    }

    func makeUIView(context: Context) -> MLNMapView {
        // If the bundled style is missing the map falls back to MapLibre's
        // default style, which needs a connection; the tests check it is there.
        let map = MLNMapView(frame: .zero, styleURL: try? MapStyle.demoStyleURL())
        map.delegate = context.coordinator
        map.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        map.logoView.isHidden = true
        map.setCenter(context.coordinator.centre, zoomLevel: 11, animated: false)
        return map
    }

    func updateUIView(_ map: MLNMapView, context: Context) {}

    @MainActor
    final class Coordinator: NSObject, @preconcurrency MLNMapViewDelegate {
        private let flight: FlightView

        init(flight: FlightView) {
            self.flight = flight
        }

        var centre: CLLocationCoordinate2D {
            CLLocationCoordinate2D(
                latitude: (flight.bounds.south + flight.bounds.north) / 2,
                longitude: (flight.bounds.west + flight.bounds.east) / 2
            )
        }

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            let coordinates = flight.track.map {
                CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon)
            }
            let line = MLNPolylineFeature(coordinates: coordinates, count: UInt(coordinates.count))
            let source = MLNShapeSource(identifier: "flight-track", shape: line, options: nil)
            style.addSource(source)

            // A white edge keeps the track readable over any ground colour.
            let edge = MLNLineStyleLayer(identifier: "flight-track-edge", source: source)
            edge.lineColor = NSExpression(forConstantValue: UIColor.white)
            edge.lineWidth = NSExpression(forConstantValue: 5)
            edge.lineJoin = NSExpression(forConstantValue: "round")
            edge.lineCap = NSExpression(forConstantValue: "round")
            style.addLayer(edge)

            let track = MLNLineStyleLayer(identifier: "flight-track-line", source: source)
            track.lineColor = NSExpression(forConstantValue: UIColor.systemPink)
            track.lineWidth = NSExpression(forConstantValue: 3)
            track.lineJoin = NSExpression(forConstantValue: "round")
            track.lineCap = NSExpression(forConstantValue: "round")
            style.addLayer(track)

            let bounds = MLNCoordinateBounds(
                sw: CLLocationCoordinate2D(latitude: flight.bounds.south, longitude: flight.bounds.west),
                ne: CLLocationCoordinate2D(latitude: flight.bounds.north, longitude: flight.bounds.east)
            )
            mapView.setVisibleCoordinateBounds(
                bounds,
                edgePadding: UIEdgeInsets(top: 40, left: 40, bottom: 40, right: 40),
                animated: false,
                completionHandler: nil
            )
        }
    }
}
