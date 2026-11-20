import CoreLocation
import MapLibre
import SwiftUI
import VentiCore

/// The cockpit's map: the glider, the track behind it coloured by climb,
/// the thermals it has left, each with its climb, the take-off, and a line
/// to the place the cockpit is navigating to. It
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
    /// Where to fly for an FAI triangle, if the pilot wants it drawn.
    var fai: FaiAreas?
    /// How far the glider glides over the ground as it is, every way.
    var range: GlideRange?
    /// Whether the map shows the whole flight, with the scored route and
    /// the FAI triangle areas, instead of following the glider closely.
    var showsWholeFlight = false
    /// The task being flown, for its cylinders.
    var task: TaskView?
    /// Where the flight stands in it, for the way still to fly.
    var taskStatus: TaskStatusView?

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
        context.coordinator.fai = fai
        context.coordinator.range = range
        context.coordinator.showsWholeFlight = showsWholeFlight
        context.coordinator.task = task
        context.coordinator.taskStatus = taskStatus
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
        private var drawnThermals: [ThermalMarkView]?
        private var drawnLift: LiftView?
        /// The zoom the map was last set to by itself.
        private var chosenZoom: Double?
        var fai: FaiAreas?
        /// The side the drawn FAI triangle areas stand on.
        private var drawnFaiSide: [Coordinate]?
        var range: GlideRange?
        private var drawnRange: [Coordinate]?
        var showsWholeFlight = false
        /// The box the map shows while it shows the whole flight.
        private var wholeFlight: Bounds?
        var task: TaskView?
        var taskStatus: TaskStatusView?
        /// The task whose cylinders are drawn, and which of them as the next.
        private var drawnTask: TaskView?
        private var drawnTaskNext: UInt32?
        private var drawnTaskWay: [Coordinate]?

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            // Airspace lowest of all.
            AirspaceLayers.add(to: style)

            // How far the glider glides: a green line around what a
            // straight glide still reaches with height to spare, over the
            // ground as it is. Low in the pile: it is there to be glanced
            // at, under everything the pilot steers by.
            let reach = MLNShapeSource(identifier: "cockpit-range", shape: nil, options: nil)
            style.addSource(reach)
            let reachEdge = MapSetup.line("cockpit-range-edge", from: reach, color: .white, width: 4)
            reachEdge.lineOpacity = NSExpression(forConstantValue: 0.7)
            style.addLayer(reachEdge)
            style.addLayer(MapSetup.line("cockpit-range-line", from: reach, color: UIColor(hex: 0x1B7F5C), width: 2))

            // Where to fly for an FAI triangle: two dashed outlines, under
            // the route they belong to.
            let areas = MLNShapeSource(identifier: "cockpit-fai", shape: nil, options: nil)
            style.addSource(areas)
            let edges = MapSetup.line("cockpit-fai-line", from: areas, color: Palette.accentOnMap, width: 1.5)
            edges.lineDashPattern = NSExpression(forConstantValue: [3, 2])
            edges.lineOpacity = NSExpression(forConstantValue: 0.85)
            style.addLayer(edges)

            // The task: its cylinders and goal in the map's ink, the one to
            // fly to next with a heavier line, and the shortest way still
            // to fly as a thin line through them.
            let zones = MLNShapeSource(identifier: "cockpit-task", shape: nil, options: nil)
            style.addSource(zones)
            let rings = MapSetup.line("cockpit-task-line", from: zones, color: UIColor(hex: 0x1F1F1D), width: 1.5)
            rings.predicate = NSPredicate(format: "next == 0")
            style.addLayer(rings)
            let nextRing = MapSetup.line("cockpit-task-next", from: zones, color: UIColor(hex: 0x1F1F1D), width: 3.5)
            nextRing.predicate = NSPredicate(format: "next == 1")
            style.addLayer(nextRing)
            let ahead = MLNShapeSource(identifier: "cockpit-task-way", shape: nil, options: nil)
            style.addSource(ahead)
            let aheadLine = MapSetup.line(
                "cockpit-task-way-line", from: ahead, color: UIColor(hex: 0x1F1F1D), width: 1.5
            )
            aheadLine.lineOpacity = NSExpression(forConstantValue: 0.6)
            style.addLayer(aheadLine)

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

            // The thermals already used: a dot in the colour of climb with
            // the climb beside it, over the trail, so that the lift can be
            // found again. A number that has no room gives way; its dot stays.
            let thermals = MLNShapeSource(identifier: "cockpit-thermals", shape: nil, options: nil)
            style.addSource(thermals)
            style.addLayer(
                MapSetup.dot(
                    "cockpit-thermal-dot", from: thermals, radius: 5, color: ClimbBand.climb.uiColor, ring: .white
                )
            )
            let climbs = MLNSymbolStyleLayer(identifier: "cockpit-thermal-climb", source: thermals)
            climbs.text = NSExpression(forKeyPath: "label")
            climbs.textFontNames = NSExpression(forConstantValue: ["noto-sans-regular"])
            climbs.textFontSize = NSExpression(forConstantValue: 13)
            climbs.textAnchor = NSExpression(forConstantValue: "left")
            climbs.textOffset = NSExpression(forConstantValue: NSValue(cgVector: CGVector(dx: 0.7, dy: 0)))
            climbs.textColor = NSExpression(forConstantValue: UIColor(hex: 0x0B0B0B))
            climbs.textHaloColor = NSExpression(forConstantValue: UIColor.white)
            climbs.textHaloWidth = NSExpression(forConstantValue: 1.6)
            style.addLayer(climbs)

            // The thermal being circled in. Around where the lift is, the
            // circle to fly as a dashed ring; over it the last circles as
            // dots, each where the air it was flown through is now, in the
            // colour of its climb and larger where the climb is stronger.
            // The dots lie over the ring: a circle that is centred covers
            // it, and the dots are what there is to read.
            let ring = MLNShapeSource(identifier: "cockpit-lift-ring", shape: nil, options: nil)
            style.addSource(ring)
            style.addLayer(MapSetup.line("cockpit-lift-ring-edge", from: ring, color: .white, width: 4.5))
            let circle = MapSetup.line(
                "cockpit-lift-ring-line", from: ring, color: UIColor(hex: 0x1F1F1D), width: 2
            )
            circle.lineDashPattern = NSExpression(forConstantValue: [2, 2])
            style.addLayer(circle)
            let lift = MLNShapeSource(identifier: "cockpit-lift", shape: nil, options: nil)
            style.addSource(lift)
            for band in ClimbBand.allCases {
                let dots = MLNCircleStyleLayer(identifier: "cockpit-lift-\(band.rawValue)", source: lift)
                dots.predicate = NSPredicate(format: "band == %d", band.rawValue)
                dots.circleRadius = NSExpression(forKeyPath: "radius")
                dots.circleColor = NSExpression(forConstantValue: band.uiColor)
                dots.circleStrokeColor = NSExpression(forConstantValue: UIColor.white)
                dots.circleStrokeWidth = NSExpression(forConstantValue: 1)
                style.addLayer(dots)
            }
            let core = MLNShapeSource(identifier: "cockpit-lift-centre", shape: nil, options: nil)
            style.addSource(core)
            style.addLayer(
                MapSetup.dot(
                    "cockpit-lift-centre-dot", from: core, radius: 4, color: UIColor(hex: 0x1F1F1D), ring: .white
                )
            )

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
            drawnThermals = nil
            drawnLift = nil
            drawnFaiSide = nil
            drawnRoute = nil
            drawnTask = nil
            drawnTaskNext = nil
            drawnTaskWay = nil
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
            let isWholeFlightShown = wholeFlight != nil
            let nextTurnpoint = taskStatus?.next?.number
            guard view.position != shown?.position || view.circling != shown?.circling || target != shownTarget
                || fai?.side != drawnFaiSide || showsWholeFlight != isWholeFlightShown
                || task != drawnTask || nextTurnpoint != drawnTaskNext
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
            if view.thermals != drawnThermals,
               let source = style.source(withIdentifier: "cockpit-thermals") as? MLNShapeSource
            {
                drawnThermals = view.thermals
                source.shape = MLNShapeCollectionFeature(shapes: view.thermals.map(Self.thermalFeature))
            }
            if view.lift != drawnLift {
                drawnLift = view.lift
                if let source = style.source(withIdentifier: "cockpit-lift") as? MLNShapeSource {
                    source.shape = MLNShapeCollectionFeature(shapes: Self.liftFeatures(view.lift))
                }
                let centre = view.lift?.centre
                if let source = style.source(withIdentifier: "cockpit-lift-ring") as? MLNShapeSource {
                    let corners = (centre?.outline ?? []).map(\.location)
                    source.shape = corners.count >= 2
                        ? MLNPolylineFeature(coordinates: corners, count: UInt(corners.count))
                        : nil
                }
                if let source = style.source(withIdentifier: "cockpit-lift-centre") as? MLNShapeSource {
                    if let centre {
                        let middle = MLNPointFeature()
                        middle.coordinate = centre.position.location
                        source.shape = middle
                    } else {
                        source.shape = nil
                    }
                }
            }

            if range?.outline != drawnRange, let source = style.source(withIdentifier: "cockpit-range") as? MLNShapeSource {
                drawnRange = range?.outline
                let edge = (range?.outline ?? []).map(\.location)
                source.shape = edge.count >= 3
                    ? MLNPolylineFeature(coordinates: edge, count: UInt(edge.count))
                    : nil
            }

            if fai?.side != drawnFaiSide, let source = style.source(withIdentifier: "cockpit-fai") as? MLNShapeSource {
                drawnFaiSide = fai?.side
                let outlines = (fai?.outlines ?? []).map { outline in
                    let corners = outline.map(\.location)
                    return MLNPolylineFeature(coordinates: corners, count: UInt(corners.count))
                }
                source.shape = MLNShapeCollectionFeature(shapes: outlines)
            }

            if task != drawnTask || nextTurnpoint != drawnTaskNext,
               let source = style.source(withIdentifier: "cockpit-task") as? MLNShapeSource
            {
                drawnTask = task
                drawnTaskNext = nextTurnpoint
                source.shape = MLNShapeCollectionFeature(
                    shapes: Self.taskFeatures(task?.turnpoints ?? [], next: nextTurnpoint)
                )
            }
            let ahead = taskStatus?.way ?? []
            if ahead != drawnTaskWay, let source = style.source(withIdentifier: "cockpit-task-way") as? MLNShapeSource {
                drawnTaskWay = ahead
                let corners = ahead.map(\.location)
                source.shape = corners.count >= 2
                    ? MLNPolylineFeature(coordinates: corners, count: UInt(corners.count))
                    : nil
            }

            // The whole flight: everything it has to show in one view,
            // refitted only when something has grown out of it.
            if showsWholeFlight {
                var places = [position] + route
                places += fai?.outlines.flatMap { $0 } ?? []
                places += task?.turnpoints.flatMap(\.outline) ?? []
                places += [view.takeoff, target].compactMap { $0 }
                let box = Self.box(around: places)
                if wholeFlight.map({ !$0.contains(box) }) ?? true {
                    wholeFlight = box
                    MapSetup.show(box, on: map, animated: !LaunchOptions.isUITesting)
                }
                return
            }
            if wholeFlight != nil {
                // Back to following the glider at the usual zoom.
                wholeFlight = nil
                chosenZoom = nil
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

        /// The smallest box that holds every one of these places.
        static func box(around places: [Coordinate]) -> Bounds {
            let (lats, lons) = (places.map(\.lat), places.map(\.lon))
            return Bounds(
                south: lats.min() ?? 0, west: lons.min() ?? 0, north: lats.max() ?? 0, east: lons.max() ?? 0
            )
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

        /// A thermal the glider has left, as the map draws it.
        /// The outlines of a task's turnpoints, each marked with whether it
        /// is the one to fly to next.
        static func taskFeatures(_ turnpoints: [TaskTurnpointView], next: UInt32?) -> [MLNPolylineFeature] {
            turnpoints.filter { $0.outline.count >= 2 }.map { turnpoint in
                let corners = turnpoint.outline.map(\.location)
                let ring = MLNPolylineFeature(coordinates: corners, count: UInt(corners.count))
                ring.attributes = ["next": turnpoint.number == next ? 1 : 0]
                return ring
            }
        }

        static func thermalFeature(for mark: ThermalMarkView) -> MLNPointFeature {
            let feature = MLNPointFeature()
            feature.coordinate = mark.position.location
            feature.attributes = ["label": mark.label]
            return feature
        }

        /// The places of a thermal's picture, a dot each: its climb band for
        /// the colour and its size in points.
        static func liftFeatures(_ lift: LiftView?) -> [MLNPointFeature] {
            (lift?.points ?? []).map { point in
                let feature = MLNPointFeature()
                feature.coordinate = CLLocationCoordinate2D(latitude: point.lat, longitude: point.lon)
                feature.attributes = [
                    "band": ClimbBand(metresPerSecond: point.climbMs).rawValue,
                    "radius": liftDotRadius(strength: point.strength),
                ]
                return feature
            }
        }

        /// From 2 points where the thermal lifts least to 5 where it lifts
        /// most; 3.5 all around an even one, which at a point a second
        /// makes a string of beads.
        static func liftDotRadius(strength: Double) -> Double {
            2 + 3 * min(max(strength, 0), 1)
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
