import CoreLocation
import MapLibre
import SwiftUI
import VentiCore

/// Draws a route on the map: the pilot taps the places to fly by, in
/// order, and sees how long each leg is and what the route would score if
/// it were flown as drawn. Kept, it is the task the cockpit leads along,
/// like a route made of waypoints.
///
/// What the route comes to is worked out by the core, with the rules a
/// flown flight is scored by: a route that comes back towards its start
/// is found to be a triangle without the pilot saying so.
struct RouteMapPlanner: View {
    @Environment(TaskModel.self) private var tasks
    @Environment(OfflineMapsModel.self) private var maps
    @Environment(\.dismiss) private var dismiss

    /// The places of the route so far, in order.
    @State private var places: [Coordinate]
    @State private var problem: String?
    @AppStorage(Preferences.mapShowsAerial) private var showsAerial = false
    /// Who made the photographs in view on the map.
    @State private var photos = PhotoCredits()

    /// A route with nothing drawn yet, or one that begins at a place: a
    /// take-off whose page the pilot came from.
    init(from start: Coordinate? = nil) {
        _places = State(initialValue: start.map { [$0] } ?? [])
    }

    var body: some View {
        let plan = planRoute(points: places)
        NavigationStack {
            ZStack(alignment: .bottom) {
                RouteDrawingMap(
                    places: places,
                    scored: plan.score?.route ?? [],
                    faiAreas: plan.faiAreas,
                    source: maps.browsingSource(at: nil),
                    aerial: showsAerial,
                    photos: photos
                ) { place in
                    add(place)
                }
                .ignoresSafeArea(edges: .bottom)
                // One element: the map's own labels are not read, and
                // the route is in the panel in words.
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("Map to draw the route on")
                .accessibilityHint("Tap where the route goes next")
                .accessibilityIdentifier("route-map")

                // The ground the route is drawn on: the drawn map, or
                // photographs from above, to see a take-off or a field.
                AerialButton()
                    .padding(16)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                // Whose photographs: at the top, where the panel and the
                // map's credits button leave room.
                PhotographCredit(credits: photos)
                    .padding(.leading, 12)
                    .padding(.top, 16)
                    .padding(.trailing, 80)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

                panel(for: plan)
            }
            .navigationTitle("Draw a route")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        dismiss()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Keep") {
                        if tasks.keepDrawnRoute(places) {
                            dismiss()
                        } else {
                            problem = tasks.problem
                        }
                    }
                    .disabled(places.count < 2)
                }
            }
        }
    }

    /// What the route comes to, and what can be done with it.
    private func panel(for plan: RoutePlan) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(problem ?? plan.words)
                .font(.venti(.subheadline))
                .foregroundStyle(problem == nil ? Color.venti(.text) : Color.venti(.danger))
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("route-words")
            if !plan.legs.isEmpty {
                Text(Self.legs(of: plan))
                    .font(.venti(.footnote))
                    .monospacedDigit()
                    .foregroundStyle(.venti(.textSecondary))
                    .lineLimit(2)
                    .accessibilityLabel("Legs")
                    .accessibilityValue(Self.legs(of: plan))
            }
            HStack(spacing: 10) {
                // The sign alone, like the bin: the words of the button
                // between them need the room.
                Button("Undo", systemImage: "arrow.uturn.backward") {
                    problem = nil
                    places.removeLast()
                }
                .labelStyle(.iconOnly)
                .disabled(places.isEmpty)
                // A triangle ends where it began: one tap instead of
                // aiming a finger at the first place.
                Button {
                    problem = nil
                    places.append(places[0])
                } label: {
                    // The words are what says it: the sign goes first
                    // where a narrow iPhone has no room for both.
                    ViewThatFits(in: .horizontal) {
                        Label("Back to the start", systemImage: "arrow.triangle.capsulepath")
                        Text("Back to the start")
                    }
                }
                .disabled(places.count < 3 || places.last == places.first)
                Spacer(minLength: 0)
                Button("Clear", systemImage: "trash", role: .destructive) {
                    problem = nil
                    places.removeAll()
                }
                .labelStyle(.iconOnly)
                .disabled(places.isEmpty)
            }
            .buttonStyle(.venti(.quiet))
            .font(.venti(.subheadline))
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .ventiFloating(in: .rect(cornerRadius: 28))
        .padding(12)
    }

    /// The legs in a line: "1  10.0 km E · 2  9.8 km NW".
    static func legs(of plan: RoutePlan) -> String {
        plan.legs.enumerated()
            .map { index, leg in "\(index + 1)  \(leg.words)" }
            .joined(separator: " · ")
    }

    /// Takes a tapped place into the route, unless it lies on the place
    /// before: a finger that taps twice means one place.
    private func add(_ place: Coordinate) {
        problem = nil
        if let last = places.last {
            let apart = CLLocation(latitude: last.lat, longitude: last.lon)
                .distance(from: CLLocation(latitude: place.lat, longitude: place.lon))
            guard apart >= Self.apartAtLeastM else { return }
        }
        places.append(place)
    }

    /// Two places of a route lie at least this far apart, metres.
    static let apartAtLeastM = 150.0
}

/// The map a route is drawn on: the places tapped so far with their
/// numbers, the line through them, and the route as it would be scored.
struct RouteDrawingMap: UIViewRepresentable {
    /// The places of the route, in order.
    var places: [Coordinate]
    /// The route as it would be scored: start, turnpoints, finish.
    var scored: [Coordinate]
    /// With one leg drawn: the outlines of the areas in which a third
    /// place makes an FAI triangle of it.
    var faiAreas: [[Coordinate]] = []
    /// What the map is drawn from. Read when the map is made.
    var source: MapStyle.Source = .standard
    /// Whether the ground is photographs from above instead of the drawn map.
    var aerial = false
    /// Where the map says who made the photographs in view.
    var photos: PhotoCredits?
    /// Called with the place under the finger after a tap.
    var onTap: (Coordinate) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> MLNMapView {
        let map = MapSetup.makeMapView(from: source)
        map.delegate = context.coordinator
        map.automaticallyAdjustsContentInset = true
        if let start = places.first {
            // A route that begins at a place: the country around it.
            map.setCenter(start.location, zoomLevel: 9.5, animated: false)
        } else {
            // The northern Alps between Lake Constance and Salzburg, as
            // the map tab begins.
            map.setCenter(CLLocationCoordinate2D(latitude: 47.45, longitude: 11.6), zoomLevel: 7.4, animated: false)
        }

        let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.tapped(_:)))
        // The map's own double tap zooms; a single tap must wait for it.
        for recognizer in map.gestureRecognizers ?? [] where recognizer is UITapGestureRecognizer {
            tap.require(toFail: recognizer)
        }
        map.addGestureRecognizer(tap)
        return map
    }

    func updateUIView(_ map: MLNMapView, context: Context) {
        context.coordinator.onTap = onTap
        context.coordinator.photos = photos
        context.coordinator.showAerial(aerial, on: map)
        context.coordinator.show(places, scored: scored, faiAreas: faiAreas, on: map)
    }

    @MainActor
    final class Coordinator: NSObject, @preconcurrency MLNMapViewDelegate {
        var photos: PhotoCredits?
        var onTap: (Coordinate) -> Void = { _ in }
        private var places: [Coordinate] = []
        private var scored: [Coordinate] = []
        private var faiAreas: [[Coordinate]] = []
        private var drawn: ([Coordinate], [Coordinate], [[Coordinate]])?
        private var isStyleLoaded = false
        private var aerial = false

        /// Shows the drawn ground or the aerial view, now or once the
        /// style is there.
        func showAerial(_ aerial: Bool, on map: MLNMapView) {
            guard aerial != self.aerial else { return }
            self.aerial = aerial
            if isStyleLoaded {
                MapSetup.showAerial(aerial, on: map)
            }
        }

        func show(_ places: [Coordinate], scored: [Coordinate], faiAreas: [[Coordinate]], on map: MLNMapView) {
            self.places = places
            self.scored = scored
            self.faiAreas = faiAreas
            draw(on: map)
        }

        @objc func tapped(_ recognizer: UITapGestureRecognizer) {
            guard recognizer.state == .ended, let map = recognizer.view as? MLNMapView else { return }
            let place = map.convert(recognizer.location(in: map), toCoordinateFrom: map)
            onTap(Coordinate(lat: place.latitude, lon: place.longitude))
        }

        /// The map has come to rest: who made the photographs now in view.
        func mapView(_ mapView: MLNMapView, regionDidChangeAnimated animated: Bool) {
            photos?.update(from: mapView)
        }

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            MapSetup.showAerial(aerial, on: mapView)
            photos?.update(from: mapView)
            let ink = Palette.inkOnMap

            // Where a third place makes an FAI triangle of the first leg:
            // two dashed outlines, under the route, as in the cockpit.
            let areas = MLNShapeSource(identifier: "route-fai", shape: nil, options: nil)
            style.addSource(areas)
            let edges = MapSetup.line("route-fai-line", from: areas, color: Palette.accentOnMap, width: 1.5)
            edges.lineDashPattern = NSExpression(forConstantValue: [3, 2])
            edges.lineOpacity = NSExpression(forConstantValue: 0.85)
            style.addLayer(edges)

            // The line through the places, with a white edge that keeps it
            // readable over any ground.
            let line = MLNShapeSource(identifier: "route-drawn", shape: nil, options: nil)
            style.addSource(line)
            style.addLayer(MapSetup.line("route-drawn-edge", from: line, color: Palette.haloOnMap, width: 6))
            style.addLayer(MapSetup.line("route-drawn-line", from: line, color: ink, width: 3))

            // The route as it would be scored, dashed in the app's colour
            // over the drawn line: where it leaves the line, start and
            // finish were found on a leg.
            let scored = MLNShapeSource(identifier: "route-scored", shape: nil, options: nil)
            style.addSource(scored)
            let dashes = MapSetup.line("route-scored-line", from: scored, color: Palette.accentOnMap, width: 2)
            dashes.lineDashPattern = NSExpression(forConstantValue: [3, 2])
            style.addLayer(dashes)

            // The places, each with its number.
            let places = MLNShapeSource(identifier: "route-places", shape: nil, options: nil)
            style.addSource(places)
            style.addLayer(MapSetup.dot("route-places-dot", from: places, radius: 11, color: Palette.haloOnMap, ring: ink))
            let numbers = MLNSymbolStyleLayer(identifier: "route-places-number", source: places)
            numbers.text = NSExpression(forKeyPath: "label")
            numbers.textFontNames = NSExpression(forConstantValue: ["noto-sans-regular"])
            numbers.textFontSize = NSExpression(forConstantValue: 13)
            numbers.textColor = NSExpression(forConstantValue: ink)
            numbers.textAllowsOverlap = NSExpression(forConstantValue: true)
            style.addLayer(numbers)

            isStyleLoaded = true
            drawn = nil
            draw(on: mapView)
        }

        /// Draws the route when it has changed.
        private func draw(on map: MLNMapView) {
            guard isStyleLoaded, let style = map.style else { return }
            if let drawn, drawn.0 == places, drawn.1 == scored, drawn.2 == faiAreas {
                return
            }
            drawn = (places, scored, faiAreas)

            if let source = style.source(withIdentifier: "route-fai") as? MLNShapeSource {
                let outlines = faiAreas.map { outline in
                    let corners = outline.map(\.location)
                    return MLNPolylineFeature(coordinates: corners, count: UInt(corners.count))
                }
                source.shape = MLNShapeCollectionFeature(shapes: outlines)
            }

            if let source = style.source(withIdentifier: "route-drawn") as? MLNShapeSource {
                let corners = places.map(\.location)
                source.shape = corners.count >= 2
                    ? MLNPolylineFeature(coordinates: corners, count: UInt(corners.count))
                    : nil
            }
            if let source = style.source(withIdentifier: "route-scored") as? MLNShapeSource {
                let corners = scored.map(\.location)
                source.shape = corners.count >= 2
                    ? MLNPolylineFeature(coordinates: corners, count: UInt(corners.count))
                    : nil
            }
            if let source = style.source(withIdentifier: "route-places") as? MLNShapeSource {
                // A place the route comes back to keeps its first number.
                var numbered: [(Coordinate, Int)] = []
                for (index, place) in places.enumerated() where !numbered.contains(where: { $0.0 == place }) {
                    numbered.append((place, index + 1))
                }
                source.shape = MLNShapeCollectionFeature(
                    shapes: numbered.map { place, number in
                        let feature = MLNPointFeature()
                        feature.coordinate = place.location
                        feature.attributes = ["label": "\(number)"]
                        return feature
                    }
                )
            }
        }
    }
}
