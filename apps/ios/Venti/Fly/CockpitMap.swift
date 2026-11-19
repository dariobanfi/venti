import CoreLocation
import MapLibre
import SwiftUI
import VentiCore

/// The cockpit's map: the glider, the track behind it coloured by climb,
/// and the take-off. It keeps the glider in the middle by itself, north up,
/// and zooms in while the glider circles.
struct CockpitMap: UIViewRepresentable {
    let view: CockpitView

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> MLNMapView {
        let map = MapSetup.makeMapView()
        map.delegate = context.coordinator
        // The map follows the glider; dragging it away would only have to
        // be undone with a gloved hand. Zooming stays.
        map.isScrollEnabled = false
        map.isRotateEnabled = false
        if let position = view.position {
            map.setCenter(position.location, zoomLevel: Coordinator.cruiseZoom, animated: false)
        }
        return map
    }

    func updateUIView(_ map: MLNMapView, context: Context) {
        context.coordinator.show(view, on: map)
    }

    @MainActor
    final class Coordinator: NSObject, @preconcurrency MLNMapViewDelegate {
        static let cruiseZoom = 13.0
        static let thermalZoom = 15.5

        private var isStyleLoaded = false
        private var shown: CockpitView?
        private var wasCircling: Bool?

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            let trail = MLNShapeSource(identifier: "cockpit-trail", shape: nil, options: nil)
            style.addSource(trail)
            style.addLayer(MapSetup.line("cockpit-trail-edge", from: trail, color: .white, width: 6))
            for band in ClimbBand.allCases {
                let layer = MapSetup.line(
                    "cockpit-trail-\(band.rawValue)", from: trail, color: band.uiColor, width: 3.5
                )
                layer.predicate = NSPredicate(format: "band == %d", band.rawValue)
                style.addLayer(layer)
            }

            let takeoff = MLNShapeSource(identifier: "cockpit-takeoff", shape: nil, options: nil)
            style.addSource(takeoff)
            style.addLayer(
                MapSetup.dot(
                    "cockpit-takeoff-dot", from: takeoff, radius: 5, color: .white, ring: UIColor(hex: 0x1F1F1D)
                )
            )

            let glider = MLNShapeSource(identifier: "cockpit-glider", shape: nil, options: nil)
            style.addSource(glider)
            style.addLayer(
                MapSetup.dot(
                    "cockpit-glider-dot", from: glider, radius: 9, color: Palette.accentOnMap, ring: .white
                )
            )

            isStyleLoaded = true
            if let shown {
                self.shown = nil
                show(shown, on: mapView)
            }
        }

        /// Redraws when the glider has moved, about once a second.
        func show(_ view: CockpitView, on map: MLNMapView) {
            guard view.position != shown?.position || view.circling != shown?.circling else { return }
            shown = view
            guard isStyleLoaded, let style = map.style, let position = view.position else { return }

            point(position, in: "cockpit-glider", of: style)
            if let takeoff = view.takeoff {
                point(takeoff, in: "cockpit-takeoff", of: style)
            }
            if let source = style.source(withIdentifier: "cockpit-trail") as? MLNShapeSource {
                source.shape = MLNShapeCollectionFeature(shapes: Self.trailFeatures(view.trail))
            }

            // Zoom only when the kind of flying changes, so a zoom level the
            // pilot pinched to is kept until then.
            if wasCircling != view.circling {
                wasCircling = view.circling
                let zoom = view.circling ? Self.thermalZoom : Self.cruiseZoom
                map.setCenter(position.location, zoomLevel: zoom, animated: !LaunchOptions.isUITesting)
            } else {
                map.setCenter(position.location, animated: !LaunchOptions.isUITesting)
            }
        }

        private func point(_ coordinate: Coordinate, in identifier: String, of style: MLNStyle) {
            guard let source = style.source(withIdentifier: identifier) as? MLNShapeSource else { return }
            let point = MLNPointFeature()
            point.coordinate = coordinate.location
            source.shape = point
        }

        /// The trail as one feature per climb band, like a finished flight's
        /// track.
        static func trailFeatures(_ trail: [TrailPointView]) -> [MLNShape & MLNFeature] {
            let coordinates = trail.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) }
            var stretches: [ClimbBand: [MLNPolyline]] = [:]
            for run in TrackBands.runs(climbs: trail.map(\.climbMs)) {
                let piece = Array(coordinates[run.points])
                stretches[run.band, default: []].append(
                    MLNPolyline(coordinates: piece, count: UInt(piece.count))
                )
            }
            return stretches.map { band, polylines in
                let feature = MLNMultiPolylineFeature(polylines: polylines)
                feature.attributes = ["band": band.rawValue]
                return feature
            }
        }
    }
}
