import CoreLocation
import MapLibre
import SwiftUI
import VentiCore

/// The cockpit's map: the glider, the track behind it coloured by climb,
/// the take-off, and a line to the place the cockpit is navigating to. It
/// keeps the glider in the middle by itself, north up, zooms in while the
/// glider circles, and out again as far as it takes to show the airspace
/// a warning is about.
struct CockpitMap: UIViewRepresentable {
    let view: CockpitView
    /// The way to the place being navigated to, if any.
    var way: WayToTarget?
    /// The pilot's airspace, to draw what lies around the glider.
    var airspace: AirspaceSet?
    /// How far the airspace of the most urgent warning is, in metres, so
    /// that the map shows it. `nil` without a warning.
    var airspaceDistance: Double?
    /// The route the flight scores along so far.
    var route: [Coordinate] = []

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
        context.coordinator.airspace = airspace
        context.coordinator.airspaceDistance = airspaceDistance
        context.coordinator.show(route, on: map)
        context.coordinator.show(view, way: way, on: map)
    }

    @MainActor
    final class Coordinator: NSObject, @preconcurrency MLNMapViewDelegate {
        static let cruiseZoom = 13.0
        static let thermalZoom = 15.5
        /// The map never zooms further out than this by itself.
        static let widestZoom = 10.0

        private var isStyleLoaded = false
        private var shown: CockpitView?
        private var route: [Coordinate] = []
        private var drawnRoute: [Coordinate]?
        /// The pilot's airspace, and where it was last drawn around.
        var airspace: AirspaceSet?
        var airspaceDistance: Double?
        private var airspaceDrawnAt: Coordinate?
        private var shownTarget: Coordinate?
        private var target: Coordinate?
        /// The zoom the map was last set to by itself.
        private var chosenZoom: Double?

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            // Airspace lowest of all.
            AirspaceLayers.add(to: style)

            // The route the flight scores along: a thin line in the accent
            // colour, under everything that moves.
            let scored = MLNShapeSource(identifier: "cockpit-route", shape: nil, options: nil)
            style.addSource(scored)
            let legs = MapSetup.line("cockpit-route-line", from: scored, color: Palette.accentOnMap, width: 1.5)
            legs.lineOpacity = NSExpression(forConstantValue: 0.7)
            style.addLayer(legs)

            // The line to the place being flown to, added next so that it
            // lies under the trail and the glider.
            let course = MLNShapeSource(identifier: "cockpit-course", shape: nil, options: nil)
            style.addSource(course)
            let line = MapSetup.line("cockpit-course-line", from: course, color: UIColor(hex: 0x1F1F1D), width: 2)
            line.lineDashPattern = NSExpression(forConstantValue: [2, 2])
            style.addLayer(line)

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

            let place = MLNShapeSource(identifier: "cockpit-target", shape: nil, options: nil)
            style.addSource(place)
            style.addLayer(
                MapSetup.dot(
                    "cockpit-target-dot", from: place, radius: 6, color: UIColor(hex: 0x1F1F1D), ring: .white
                )
            )

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
            drawnRoute = nil
            show(route, on: mapView)
            if let shown {
                self.shown = nil
                show(shown, way: nil, on: mapView)
            }
        }

        /// Draws the scored route when it has changed: every half minute.
        func show(_ route: [Coordinate], on map: MLNMapView) {
            self.route = route
            guard isStyleLoaded, route != drawnRoute,
                  let source = map.style?.source(withIdentifier: "cockpit-route") as? MLNShapeSource
            else { return }
            drawnRoute = route
            let corners = route.map(\.location)
            source.shape = corners.count >= 2
                ? MLNPolylineFeature(coordinates: corners, count: UInt(corners.count))
                : nil
        }

        /// Redraws when the glider has moved, about once a second, or when
        /// another place has been chosen.
        func show(_ view: CockpitView, way: WayToTarget?, on map: MLNMapView) {
            if let way {
                target = way.position
            }
            guard view.position != shown?.position || view.circling != shown?.circling || target != shownTarget
            else { return }
            shown = view
            guard isStyleLoaded, let style = map.style, let position = view.position else { return }
            shownTarget = target

            point(position, in: "cockpit-glider", of: style)
            drawAirspace(around: position, in: style)
            if let target {
                point(target, in: "cockpit-target", of: style)
                if let source = style.source(withIdentifier: "cockpit-course") as? MLNShapeSource {
                    let ends = [position.location, target.location]
                    source.shape = MLNPolylineFeature(coordinates: ends, count: UInt(ends.count))
                }
            }
            if let takeoff = view.takeoff {
                point(takeoff, in: "cockpit-takeoff", of: style)
            }
            if let source = style.source(withIdentifier: "cockpit-trail") as? MLNShapeSource {
                source.shape = MLNShapeCollectionFeature(shapes: Self.trailFeatures(view.trail))
            }

            // Zoom only when the kind of flying changes or the airspace a
            // warning is about comes into or goes out of reach, so a zoom
            // level the pilot pinched to is kept until then.
            let zoom = Self.zoom(
                circling: view.circling, showing: airspaceDistance, latitude: position.lat, in: map.bounds.size
            )
            if zoom != chosenZoom {
                chosenZoom = zoom
                map.setCenter(position.location, zoomLevel: zoom, animated: !LaunchOptions.isUITesting)
            } else {
                map.setCenter(position.location, animated: !LaunchOptions.isUITesting)
            }
        }

        /// The zoom for the kind of flying, or less if it takes less to
        /// show something this many metres from the glider: the airspace a
        /// warning is about. In half steps, so that the map does not
        /// breathe with every metre flown.
        static func zoom(circling: Bool, showing metres: Double?, latitude: Double, in size: CGSize) -> Double {
            let usual = circling ? thermalZoom : cruiseZoom
            // The glider is in the middle; four fifths of the way to the
            // nearer edge is as far out as something is still seen well.
            let reach = Double(min(size.width, size.height)) / 2 * 0.8
            guard let metres, metres > 0, reach > 0 else { return usual }
            // The map's zoom 0 shows the world 512 points wide.
            let metresPerPoint = 40_075_016.686 * cos(latitude * .pi / 180) / 512
            let fitting = log2(metresPerPoint * reach / metres)
            return min(usual, max(widestZoom, (fitting * 2).rounded(.down) / 2))
        }

        /// Draws the airspace within about 40 km of the glider, again once
        /// the glider has flown some 10 km from where it was last drawn.
        private func drawAirspace(around position: Coordinate, in style: MLNStyle) {
            guard let airspace else { return }
            if let drawn = airspaceDrawnAt,
               abs(drawn.lat - position.lat) < 0.09, abs(drawn.lon - position.lon) < 0.13
            {
                return
            }
            airspaceDrawnAt = position
            let area = Bounds(
                south: position.lat - 0.36, west: position.lon - 0.54,
                north: position.lat + 0.36, east: position.lon + 0.54
            )
            AirspaceLayers.show(airspace.inArea(area: area), in: style)
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
