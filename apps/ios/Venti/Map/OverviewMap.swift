import CoreLocation
import MapLibre
import SwiftUI
import VentiCore

/// The map of the map tab: terrain, every flight in the logbook as a line,
/// and the pilot's own position when asked for.
struct OverviewMap: UIViewRepresentable {
    /// The shapes of the logbook's flights.
    let outlines: [[Coordinate]]
    /// Whether the map keeps the pilot's position in its centre.
    @Binding var isFollowing: Bool
    /// Set when the position cannot be shown, with words for the pilot.
    @Binding var problem: String?

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> MLNMapView {
        let map = MapSetup.makeMapView()
        map.delegate = context.coordinator
        map.automaticallyAdjustsContentInset = true
        // The northern Alps between Lake Constance and Salzburg.
        map.setCenter(CLLocationCoordinate2D(latitude: 47.45, longitude: 11.6), zoomLevel: 7.4, animated: false)
        return map
    }

    func updateUIView(_ map: MLNMapView, context: Context) {
        let coordinator = context.coordinator
        coordinator.report = { following, problem in
            if isFollowing != following {
                isFollowing = following
            }
            if let problem {
                self.problem = problem
            }
        }
        coordinator.show(outlines, on: map)
        coordinator.follow(isFollowing, on: map)
    }

    @MainActor
    final class Coordinator: NSObject, @preconcurrency MLNMapViewDelegate {
        /// Tells the screen whether the map is following the pilot, and
        /// about a problem if there is one.
        var report: (Bool, String?) -> Void = { _, _ in }

        private var outlines: [[Coordinate]] = []
        private var drawn: [[Coordinate]]?
        private var isStyleLoaded = false
        private var requested = false

        func show(_ outlines: [[Coordinate]], on map: MLNMapView) {
            self.outlines = outlines
            draw(on: map)
        }

        /// Acts only when the screen asks for something new. The map also
        /// stops following by itself when the pilot drags it, and must not
        /// be switched back by a stale request.
        func follow(_ following: Bool, on map: MLNMapView) {
            guard following != requested else { return }
            requested = following
            if following {
                map.showsUserLocation = true
            }
            map.userTrackingMode = following ? .follow : .none
        }

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            isStyleLoaded = true
            draw(on: mapView)
        }

        func mapView(_ mapView: MLNMapView, didChange mode: MLNUserTrackingMode, animated: Bool) {
            requested = mode != .none
            tell(following: requested, problem: nil)
        }

        func mapView(_ mapView: MLNMapView, didFailToLocateUserWithError error: any Error) {
            requested = false
            mapView.userTrackingMode = .none
            let denied = (error as? CLError)?.code == .denied
            tell(
                following: false,
                problem: denied
                    ? "Venti is not allowed to use your position. You can allow it in Settings, under Privacy & Security, Location Services."
                    : "Your position could not be found. Try again under open sky."
            )
        }

        /// The map calls its delegate while SwiftUI is updating the view, so
        /// the screen is told afterwards.
        private func tell(following: Bool, problem: String?) {
            Task { @MainActor in
                report(following, problem)
            }
        }

        /// Draws the flights, once the style is there and only when they
        /// have changed, then moves the camera to show them all.
        private func draw(on map: MLNMapView) {
            guard isStyleLoaded, let style = map.style, outlines != drawn else { return }
            drawn = outlines

            let lines = outlines.filter { $0.count >= 2 }.map { outline in
                let coordinates = outline.map(\.location)
                return MLNPolyline(coordinates: coordinates, count: UInt(coordinates.count))
            }
            let shape = MLNMultiPolylineFeature(polylines: lines)

            if let source = style.source(withIdentifier: "logbook-tracks") as? MLNShapeSource {
                source.shape = shape
            } else {
                let source = MLNShapeSource(identifier: "logbook-tracks", shape: shape, options: nil)
                style.addSource(source)
                style.addLayer(MapSetup.line("logbook-tracks-edge", from: source, color: .white, width: 5))
                style.addLayer(
                    MapSetup.line("logbook-tracks-line", from: source, color: Palette.accentOnMap, width: 2.5)
                )
            }

            if let box = Self.box(around: outlines) {
                MapSetup.show(box, on: map, animated: false)
            }
        }

        /// The rectangle that contains every outline, or `nil` if there are none.
        static func box(around outlines: [[Coordinate]]) -> Bounds? {
            let points = outlines.joined()
            guard let first = points.first else { return nil }
            var box = Bounds(south: first.lat, west: first.lon, north: first.lat, east: first.lon)
            for point in points {
                box.south = min(box.south, point.lat)
                box.north = max(box.north, point.lat)
                box.west = min(box.west, point.lon)
                box.east = max(box.east, point.lon)
            }
            return box
        }
    }
}
