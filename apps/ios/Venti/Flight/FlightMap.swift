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
    /// How much of the map the bars and the panel cover, from each edge.
    var covered: UIEdgeInsets

    func makeCoordinator() -> Coordinator {
        Coordinator(flight: flight)
    }

    func makeUIView(context: Context) -> MLNMapView {
        let map = MapSetup.makeMapView()
        map.delegate = context.coordinator
        if let first = flight.track.first {
            map.setCenter(first.coordinate, zoomLevel: 11, animated: false)
        }
        return map
    }

    func updateUIView(_ map: MLNMapView, context: Context) {
        context.coordinator.update(map, cursor: cursor, covered: covered)
    }

    @MainActor
    final class Coordinator: NSObject, @preconcurrency MLNMapViewDelegate {
        private let flight: FlightView
        private var cursor: TrackPoint?
        private var covered = UIEdgeInsets.zero
        private var isStyleLoaded = false

        init(flight: FlightView) {
            self.flight = flight
        }

        func update(_ map: MLNMapView, cursor: TrackPoint?, covered: UIEdgeInsets) {
            if covered != self.covered {
                self.covered = covered
                frame(map)
            }
            if cursor != self.cursor {
                self.cursor = cursor
                markCursor(on: map)
            }
        }

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            addTrack(to: style)
            addEnds(to: style)
            addCursor(to: style)
            isStyleLoaded = true
            frame(mapView)
            markCursor(on: mapView)
        }

        /// Fits the whole track into the part of the map that shows.
        private func frame(_ map: MLNMapView) {
            map.contentInset = covered
            guard isStyleLoaded else { return }
            MapSetup.show(flight.bounds, on: map, animated: false)
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
            style.addLayer(MapSetup.line("flight-track-edge", from: source, color: .white, width: 6.5))
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

        /// A hollow dot where the recording starts and a solid one where it ends.
        private func addEnds(to style: MLNStyle) {
            guard let first = flight.track.first, let last = flight.track.last else { return }
            let ink = UIColor(hex: 0x1F1F1D)

            let start = MLNPointFeature()
            start.coordinate = first.coordinate
            let startSource = MLNShapeSource(identifier: "flight-start", shape: start, options: nil)
            style.addSource(startSource)
            style.addLayer(MapSetup.dot("flight-start-dot", from: startSource, radius: 5, color: .white, ring: ink))

            let end = MLNPointFeature()
            end.coordinate = last.coordinate
            let endSource = MLNShapeSource(identifier: "flight-end", shape: end, options: nil)
            style.addSource(endSource)
            style.addLayer(MapSetup.dot("flight-end-dot", from: endSource, radius: 5, color: ink, ring: .white))
        }

        private func addCursor(to style: MLNStyle) {
            let source = MLNShapeSource(identifier: "flight-cursor", shape: nil, options: nil)
            style.addSource(source)
            let dot = MapSetup.dot(
                "flight-cursor-dot", from: source, radius: 8, color: Palette.accentOnMap, ring: .white
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
