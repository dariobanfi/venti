import CoreLocation
import MapLibre
import SwiftUI
import VentiCore

/// The map of one flight: its track coloured by climb, where it started and
/// ended, and a marker at the moment the pilot is inspecting.
struct FlightMap: UIViewRepresentable {
    let flight: FlightView
    /// The moment to mark, if any.
    var cursor: TrackPoint?
    /// The scored route to draw, once the flight has been analysed.
    var score: FlightScore?
    /// The airspace around the flight that the app has, to draw under it.
    var airspace: [AirspaceShape] = []
    /// How much of the map the bars and the panel cover, from each edge.
    var covered: UIEdgeInsets
    /// What the map is drawn from. Read when the map is made; the screen
    /// makes the map anew when it changes.
    var source: MapStyle.Source = .standard
    /// A second flight laid beside this one, to draw under it.
    var other: FlightView?
    /// Where that flight's glider was at the moment marked, if it was in
    /// the air then.
    var otherCursor: TrackPoint?

    func makeCoordinator() -> Coordinator {
        Coordinator(flight: flight)
    }

    func makeUIView(context: Context) -> MLNMapView {
        let map = MapSetup.makeMapView(from: source)
        map.delegate = context.coordinator
        if let first = flight.track.first {
            map.setCenter(first.coordinate, zoomLevel: 11, animated: false)
        }
        return map
    }

    func updateUIView(_ map: MLNMapView, context: Context) {
        context.coordinator.update(map, cursor: cursor, covered: covered, score: score, airspace: airspace)
        context.coordinator.update(map, other: other, otherCursor: otherCursor)
    }

    @MainActor
    final class Coordinator: NSObject, @preconcurrency MLNMapViewDelegate {
        private let flight: FlightView
        private var cursor: TrackPoint?
        private var score: FlightScore?
        private var covered = UIEdgeInsets.zero
        private var airspace: [AirspaceShape] = []
        private var drawnAirspace: [AirspaceShape]?
        private var isStyleLoaded = false
        private var other: FlightView?
        /// Which other flight is drawn: by when it began and how many
        /// points it has, which is cheaper to compare than its track.
        private var drawnOther: String?
        private var otherCursor: TrackPoint?

        init(flight: FlightView) {
            self.flight = flight
        }

        func update(
            _ map: MLNMapView, cursor: TrackPoint?, covered: UIEdgeInsets, score: FlightScore?,
            airspace: [AirspaceShape]
        ) {
            self.airspace = airspace
            drawAirspace(on: map)
            if score != self.score {
                self.score = score
                drawRoute(on: map)
            }
            if covered != self.covered {
                self.covered = covered
                frame(map)
            }
            if cursor != self.cursor {
                self.cursor = cursor
                markCursor(on: map)
            }
        }

        /// Takes the flight laid beside this one, and where its glider is.
        func update(_ map: MLNMapView, other: FlightView?, otherCursor: TrackPoint?) {
            self.other = other
            drawOther(on: map)
            if otherCursor != self.otherCursor {
                self.otherCursor = otherCursor
                markOther(on: map)
            }
        }

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            // Airspace lowest of all, under the route and the track.
            AirspaceLayers.add(to: style)
            addRoute(to: style)
            addOther(to: style)
            addTrack(to: style)
            addEnds(to: style)
            addCursor(to: style)
            isStyleLoaded = true
            frame(mapView)
            drawnAirspace = nil
            drawAirspace(on: mapView)
            drawRoute(on: mapView)
            drawnOther = nil
            drawOther(on: mapView)
            markCursor(on: mapView)
            markOther(on: mapView)
        }

        /// The other flight goes under this one: a line of one colour,
        /// with the same white edge, and a dot for its glider.
        private func addOther(to style: MLNStyle) {
            let track = MLNShapeSource(identifier: "flight-other", shape: nil, options: nil)
            style.addSource(track)
            style.addLayer(MapSetup.line("flight-other-edge", from: track, color: Palette.haloOnMap, width: 5.5))
            style.addLayer(MapSetup.line("flight-other-line", from: track, color: Palette.otherFlightOnMap, width: 3))
            let glider = MLNShapeSource(identifier: "flight-other-cursor", shape: nil, options: nil)
            style.addSource(glider)
            let dot = MapSetup.dot(
                "flight-other-cursor-dot", from: glider, radius: 7, color: Palette.otherFlightOnMap, ring: Palette.haloOnMap
            )
            dot.isVisible = false
            style.addLayer(dot)
        }

        /// Draws the other flight's track when it has changed, and fits
        /// both flights into view.
        private func drawOther(on map: MLNMapView) {
            let key = other.map { "\($0.startUnixS)-\($0.track.count)" } ?? ""
            guard isStyleLoaded, key != drawnOther,
                  let source = map.style?.source(withIdentifier: "flight-other") as? MLNShapeSource
            else { return }
            drawnOther = key
            if let other {
                let coordinates = other.track.map(\.coordinate)
                source.shape = MLNPolylineFeature(coordinates: coordinates, count: UInt(coordinates.count))
            } else {
                source.shape = nil
            }
            frame(map)
        }

        private func markOther(on map: MLNMapView) {
            guard isStyleLoaded,
                  let style = map.style,
                  let source = style.source(withIdentifier: "flight-other-cursor") as? MLNShapeSource,
                  let dot = style.layer(withIdentifier: "flight-other-cursor-dot")
            else { return }
            if let otherCursor {
                let point = MLNPointFeature()
                point.coordinate = otherCursor.coordinate
                source.shape = point
            }
            dot.isVisible = otherCursor != nil
        }

        /// Draws the airspace around the flight, when it has changed.
        private func drawAirspace(on map: MLNMapView) {
            guard isStyleLoaded, airspace != drawnAirspace, let style = map.style else { return }
            drawnAirspace = airspace
            AirspaceLayers.show(airspace, in: style)
        }

        /// Fits the whole track into the part of the map that shows, and
        /// with it the track of a flight laid beside it.
        private func frame(_ map: MLNMapView) {
            map.contentInset = covered
            guard isStyleLoaded else { return }
            var box = flight.bounds
            if let beside = other?.bounds {
                box = Bounds(
                    south: min(box.south, beside.south), west: min(box.west, beside.west),
                    north: max(box.north, beside.north), east: max(box.east, beside.east)
                )
            }
            MapSetup.show(box, on: map, animated: false)
        }

        private func addTrack(to style: MLNStyle) {
            let track = flight.track
            let coordinates = track.map(\.coordinate)

            // One feature per climb band, each holding every stretch of the
            // track flown in that band.
            var stretches: [ClimbBand: [MLNPolyline]] = [:]
            for run in TrackBands.runs(in: track) {
                let piece = Array(coordinates[run.points])
                stretches[run.band, default: []].append(
                    MLNPolyline(coordinates: piece, count: UInt(piece.count))
                )
            }
            let features: [MLNShape & MLNFeature] = stretches.map { band, polylines in
                let feature = MLNMultiPolylineFeature(polylines: polylines)
                feature.attributes = ["band": band.rawValue]
                return feature
            }
            let source = MLNShapeSource(identifier: "flight-track", features: features, options: nil)
            style.addSource(source)

            // A white edge keeps the track readable over any ground colour.
            style.addLayer(MapSetup.line("flight-track-edge", from: source, color: Palette.haloOnMap, width: 6.5))
            // Sink first and climbs last, so that where the track crosses
            // itself the thermals stay on top.
            for band in ClimbBand.allCases {
                let layer = MapSetup.line(
                    "flight-track-\(band.rawValue)", from: source, color: band.uiColor, width: 3.5
                )
                layer.predicate = NSPredicate(format: "band == %d", band.rawValue)
                style.addLayer(layer)
            }
        }

        /// The scored route goes under the track: a thin dashed line.
        private func addRoute(to style: MLNStyle) {
            let source = MLNShapeSource(identifier: "flight-route", shape: nil, options: nil)
            style.addSource(source)
            let line = MapSetup.line(
                "flight-route-line", from: source, color: Palette.inkOnMap.withAlphaComponent(0.7), width: 1.5
            )
            line.lineDashPattern = NSExpression(forConstantValue: [4, 3])
            style.addLayer(line)
        }

        private func drawRoute(on map: MLNMapView) {
            guard isStyleLoaded,
                  let source = map.style?.source(withIdentifier: "flight-route") as? MLNShapeSource
            else { return }
            guard let score else {
                source.shape = nil
                return
            }
            let points = score.route.map(\.position.location)
            // A triangle's route is start, three turnpoints, finish: draw
            // the triangle itself closed, and the way to and from it.
            let pieces: [[CLLocationCoordinate2D]] =
                if score.kind != .freeFlight, points.count == 5 {
                    [[points[0], points[1]], [points[1], points[2], points[3], points[1]], [points[3], points[4]]]
                } else {
                    [points]
                }
            source.shape = MLNMultiPolylineFeature(
                polylines: pieces.map { MLNPolyline(coordinates: $0, count: UInt($0.count)) }
            )
        }

        /// A hollow dot where the recording starts and a solid one where it ends.
        private func addEnds(to style: MLNStyle) {
            guard let first = flight.track.first, let last = flight.track.last else { return }
            let ink = Palette.inkOnMap

            let start = MLNPointFeature()
            start.coordinate = first.coordinate
            let startSource = MLNShapeSource(identifier: "flight-start", shape: start, options: nil)
            style.addSource(startSource)
            style.addLayer(MapSetup.dot("flight-start-dot", from: startSource, radius: 5, color: Palette.haloOnMap, ring: ink))

            let end = MLNPointFeature()
            end.coordinate = last.coordinate
            let endSource = MLNShapeSource(identifier: "flight-end", shape: end, options: nil)
            style.addSource(endSource)
            style.addLayer(MapSetup.dot("flight-end-dot", from: endSource, radius: 5, color: ink, ring: Palette.haloOnMap))
        }

        private func addCursor(to style: MLNStyle) {
            let source = MLNShapeSource(identifier: "flight-cursor", shape: nil, options: nil)
            style.addSource(source)
            let dot = MapSetup.dot(
                "flight-cursor-dot", from: source, radius: 8, color: Palette.accentOnMap, ring: Palette.haloOnMap
            )
            dot.isVisible = false
            style.addLayer(dot)
        }

        private func markCursor(on map: MLNMapView) {
            guard isStyleLoaded,
                  let style = map.style,
                  let source = style.source(withIdentifier: "flight-cursor") as? MLNShapeSource,
                  let dot = style.layer(withIdentifier: "flight-cursor-dot")
            else { return }

            if let cursor {
                let point = MLNPointFeature()
                point.coordinate = cursor.coordinate
                source.shape = point
            }
            dot.isVisible = cursor != nil
        }
    }
}
