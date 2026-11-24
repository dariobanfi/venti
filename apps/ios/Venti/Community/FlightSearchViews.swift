import CoreLocation
import MapLibre
import SwiftUI
import VentiCore

/// The search among public flights, out of the way until asked for: a
/// button "Filter" with the count of the choices made, and the choices in
/// a sheet, as on the web's Flights, a pilot's page and the Ranking
/// (`SearchFilter.svelte`), in its words.
struct FlightSearchButton: View {
    let search: FlightSearch
    /// Told the search the pilot chose, when the sheet closes.
    let apply: (FlightSearch) -> Void

    @State private var isOpen = false

    var body: some View {
        let count = search.count
        Button {
            isOpen = true
        } label: {
            HStack(spacing: VentiSpace.s1) {
                Image(systemName: SitesFilterIcon.name)
                    .accessibilityHidden(true)
                Text("Filter")
                if count > 0 {
                    Text(count, format: .number)
                        .monospacedDigit()
                        .padding(.horizontal, VentiSpace.s2)
                        .foregroundStyle(.venti(.onPrimary))
                        .background(.venti(.primary), in: .capsule)
                }
            }
            .font(.ventiType(.label))
            .foregroundStyle(.venti(.link))
            .frame(minHeight: VentiSize.touch)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .fixedSize()
        .accessibilityLabel("Filter")
        .accessibilityValue(count == 0 ? "" : search.chips.map(\.words).joined(separator: ", "))
        .accessibilityIdentifier("flight-search")
        .sheet(isPresented: $isOpen) {
            FlightSearchSheet(search: search, apply: apply)
        }
    }
}

/// What a search chooses, in a line of the core's words under the list's
/// controls: "Within 40 km · EN C". Nothing for a search of everything.
struct FlightSearchLine: View {
    let search: FlightSearch

    var body: some View {
        if search.isSearching {
            Text(search.chips.map(\.words).joined(separator: " · "))
                .font(.venti(.subheadline))
                .foregroundStyle(.venti(.textSecondary))
                .monospacedDigit()
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("flight-search-line")
        }
    }
}

/// The choices of a search: the area, the glider's class, the kind of
/// route, the least distance and the take-off's country. The list behind
/// is asked again once, as the sheet closes.
struct FlightSearchSheet: View {
    let apply: (FlightSearch) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var draft: FlightSearch
    /// The least distance as typed.
    @State private var least: String

    init(search: FlightSearch, apply: @escaping (FlightSearch) -> Void) {
        self.apply = apply
        _draft = State(initialValue: search)
        _least = State(initialValue: search.minKm.map { Format.axis($0) } ?? "")
    }

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    NavigationLink {
                        FlightSearchAreaScreen(search: draft) { chosen in
                            draft = chosen
                        }
                    } label: {
                        LabeledContent("Area", value: areaWords)
                    }
                    .accessibilityIdentifier("flight-search-area")
                    if draft.near != nil || !draft.area.isEmpty {
                        Button("Remove the area", role: .destructive) {
                            draft = draft.without(.near).without(.area)
                        }
                    }
                }
                Section {
                    Picker("Glider", selection: $draft.gliderClass) {
                        Text("Any class").tag(GliderClassKind?.none)
                        ForEach(FlightSearchChoices.classes, id: \.self) { kind in
                            Text(gliderClassName(class: kind)).tag(GliderClassKind?.some(kind))
                        }
                    }
                    .accessibilityIdentifier("flight-search-class")
                    Picker("Kind", selection: $draft.kind) {
                        Text("Any kind").tag(RouteKind?.none)
                        ForEach(FlightSearchChoices.kinds, id: \.self) { kind in
                            Text(routeKindWords(kind: kind)).tag(RouteKind?.some(kind))
                        }
                    }
                    .accessibilityIdentifier("flight-search-kind")
                    LabeledContent("At least") {
                        HStack(spacing: VentiSpace.s1) {
                            TextField("0", text: $least)
                                .keyboardType(.decimalPad)
                                .multilineTextAlignment(.trailing)
                                .monospacedDigit()
                                .accessibilityLabel("At least, km")
                                .accessibilityIdentifier("flight-search-least")
                            Text("km")
                                .foregroundStyle(.secondary)
                        }
                    }
                    NavigationLink {
                        CountryChoiceList(
                            chosen: draft.country, none: "Worldwide", title: "Take-off in", prompt: "Find a country"
                        ) { code in
                            draft.country = code
                        }
                    } label: {
                        LabeledContent("Take-off in") {
                            Text(draft.country.map(Countries.named) ?? "Worldwide")
                                .accessibilityLabel(draft.country.map(Countries.name) ?? "Worldwide")
                        }
                    }
                    .accessibilityIdentifier("flight-search-country")
                }
            }
            .navigationTitle("Filter")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Clear") {
                        draft = .everything
                        least = ""
                    }
                    .disabled(!draft.isSearching)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(commonWords().done) {
                        dismiss()
                    }
                }
            }
        }
        .onChange(of: least) { _, typed in
            draft.minKm = Self.km(typed)
        }
        // However it closes, by "Done" or by a swipe: the list behind is
        // asked once, for what was chosen.
        .onDisappear {
            apply(draft)
        }
    }

    /// The area as its chip says it, or "Anywhere".
    private var areaWords: String {
        draft.chips.first { $0.part == .near || $0.part == .area }?.words ?? "Anywhere"
    }

    /// The least distance as typed: nothing, or nought, is none; more than
    /// the server takes is the most it takes.
    static func km(_ typed: String) -> Double? {
        let text = typed.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: ",", with: ".")
        guard let km = Double(text), km.isFinite, km > 0 else { return nil }
        return min(km, FlightSearchChoices.bounds.maxMinKm)
    }
}

/// The area a search's take-offs lie in, on a map: a mark the pilot taps
/// with a radius around it, or the corners of an area tapped one after
/// the other, 3 to 50 of them. The core says whether it can be asked.
struct FlightSearchAreaScreen: View {
    let use: (FlightSearch) -> Void
    private let search: FlightSearch

    private enum Mode: Hashable {
        case radius, draw
    }

    @Environment(OfflineMapsModel.self) private var maps
    @Environment(\.dismiss) private var dismiss
    @State private var mode: Mode
    @State private var centre: Coordinate
    @State private var radiusKm: Double
    @State private var corners: [Coordinate]
    @State private var closed: Bool
    @State private var refused: String?

    init(search: FlightSearch, use: @escaping (FlightSearch) -> Void) {
        self.search = search
        self.use = use
        _mode = State(initialValue: search.area.isEmpty ? .radius : .draw)
        _centre = State(initialValue: search.near ?? FlightSearchChoices.middle)
        _radiusKm = State(initialValue: search.radiusKm ?? FlightSearchChoices.radiusKm)
        _corners = State(initialValue: search.area)
        _closed = State(initialValue: !search.area.isEmpty)
    }

    private var most: Int { Int(FlightSearchChoices.bounds.maxAreaPoints) }

    var body: some View {
        VStack(spacing: 0) {
            VentiSegmented(
                options: [(value: Mode.radius, name: "Radius"), (value: Mode.draw, name: "Draw an area")],
                selection: Binding(get: { mode }, set: { mode = $0; refused = nil })
            )
            .padding(.horizontal, VentiSpace.s4)
            .padding(.vertical, VentiSpace.s2)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("search-area-mode")
            SearchAreaMap(
                centre: mode == .radius ? centre : nil,
                ring: mode == .radius ? SphereGeometry.ring(around: centre, km: radiusKm) : [],
                corners: mode == .draw ? corners : [],
                closed: closed,
                framed: search.near != nil || !search.area.isEmpty ? frame : nil,
                source: maps.browsingSource(at: nil),
                onTap: tapped
            )
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(mode == .draw ? "Map: tap the corners of the area" : "Map: the place and the radius")
            .accessibilityIdentifier("search-area-map")
            panel
        }
        .background(.venti(.bg))
        .navigationTitle("Area")
        .navigationBarTitleDisplayMode(.inline)
    }

    private var panel: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            if mode == .draw {
                Text(cornersLine)
                    .font(.venti(.subheadline))
                    .monospacedDigit()
                    .accessibilityIdentifier("search-area-corners")
                HStack(spacing: VentiSpace.s4) {
                    Button("Undo") {
                        corners.removeLast()
                        closed = false
                        refused = nil
                    }
                    .disabled(corners.isEmpty)
                    Button("Start again") {
                        corners = []
                        closed = false
                        refused = nil
                    }
                    .disabled(corners.isEmpty)
                    Button("Close the shape") {
                        closed = true
                    }
                    .disabled(corners.count < 3 || closed)
                }
                .font(.venti(.subheadline, .semibold))
                .frame(minHeight: VentiSize.touch)
            } else {
                HStack(spacing: VentiSpace.s3) {
                    Text("Radius")
                    Slider(
                        value: $radiusKm,
                        in: FlightSearchChoices.bounds.minRadiusKm...FlightSearchChoices.bounds.maxRadiusKm,
                        step: 1
                    )
                    .accessibilityLabel("Radius")
                    .accessibilityValue("\(Int(radiusKm)) km")
                    .accessibilityIdentifier("search-area-radius")
                    Text("\(Int(radiusKm)) km")
                        .monospacedDigit()
                        .accessibilityHidden(true)
                }
                .font(.venti(.subheadline))
                .frame(minHeight: VentiSize.touch)
            }
            if let refused {
                Label {
                    Text(refused)
                } icon: {
                    Image("icon-caution")
                }
                .font(.venti(.subheadline))
                .foregroundStyle(.venti(.danger))
                .accessibilityIdentifier("search-area-refused")
            }
            Button("Use this area") {
                choose()
            }
            .buttonStyle(.venti(.primary))
            .disabled(mode == .draw && corners.count < 3)
            .accessibilityIdentifier("search-area-use")
        }
        .padding(VentiSpace.s4)
    }

    /// What has been drawn, as the web says it.
    private var cornersLine: String {
        if closed { return "\(corners.count) corners" }
        if corners.isEmpty { return "Tap the corners on the map." }
        if corners.count < most { return "\(corners.count) of at most \(most) corners" }
        return "\(most) corners, the most an area has"
    }

    /// The area the search had when the screen opened, for the map to
    /// show it whole.
    private var frame: [Coordinate] {
        if let near = search.near, let radius = search.radiusKm {
            return SphereGeometry.ring(around: near, km: radius, corners: 8)
        }
        return search.area
    }

    private func tapped(_ place: Coordinate) {
        refused = nil
        switch mode {
        case .radius:
            centre = place
        case .draw:
            guard !closed, corners.count < most else { return }
            corners.append(place)
        }
    }

    private func choose() {
        let chosen = mode == .draw
            ? search.withArea(near: nil, radiusKm: nil, area: corners)
            : search.withArea(near: centre, radiusKm: radiusKm.rounded(), area: [])
        let part: FlightSearchPart = mode == .draw ? .area : .near
        if let refusal = chosen.only(part).refusal {
            refused = refusal
            return
        }
        use(chosen)
        dismiss()
    }
}

/// The map of a search's area: the mark and its circle, or the corners
/// tapped and the line through them, closed once the shape is.
private struct SearchAreaMap: UIViewRepresentable {
    var centre: Coordinate?
    var ring: [Coordinate]
    var corners: [Coordinate]
    var closed: Bool
    /// What to show whole as the map opens; the Alps where nothing.
    var framed: [Coordinate]?
    var source: MapStyle.Source
    var onTap: (Coordinate) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> MLNMapView {
        let map = MapSetup.makeMapView(from: source)
        map.delegate = context.coordinator
        if let framed, let box = Self.box(of: framed) {
            map.setVisibleCoordinateBounds(
                box, edgePadding: UIEdgeInsets(top: 24, left: 24, bottom: 24, right: 24), animated: false
            )
        } else {
            // The northern Alps, as the web's area opens.
            let at = FlightSearchChoices.middle
            map.setCenter(at.location, zoomLevel: 7.4, animated: false)
        }
        let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.tapped(_:)))
        // The map's own double tap zooms; a single tap waits for it.
        for recognizer in map.gestureRecognizers ?? [] where recognizer is UITapGestureRecognizer {
            tap.require(toFail: recognizer)
        }
        map.addGestureRecognizer(tap)
        return map
    }

    func updateUIView(_ map: MLNMapView, context: Context) {
        context.coordinator.onTap = onTap
        context.coordinator.show(
            line: !ring.isEmpty ? ring : (closed && corners.count >= 3 ? corners + [corners[0]] : corners),
            dots: centre.map { [$0] } ?? corners,
            on: map
        )
    }

    static func box(of places: [Coordinate]) -> MLNCoordinateBounds? {
        guard let first = places.first else { return nil }
        var sw = first.location
        var ne = first.location
        for place in places {
            sw.latitude = min(sw.latitude, place.lat)
            sw.longitude = min(sw.longitude, place.lon)
            ne.latitude = max(ne.latitude, place.lat)
            ne.longitude = max(ne.longitude, place.lon)
        }
        return MLNCoordinateBounds(sw: sw, ne: ne)
    }

    @MainActor
    final class Coordinator: NSObject, @preconcurrency MLNMapViewDelegate {
        var onTap: (Coordinate) -> Void = { _ in }
        private var line: [Coordinate] = []
        private var dots: [Coordinate] = []
        private var isStyleLoaded = false

        func show(line: [Coordinate], dots: [Coordinate], on map: MLNMapView) {
            self.line = line
            self.dots = dots
            draw(on: map)
        }

        @objc func tapped(_ recognizer: UITapGestureRecognizer) {
            guard recognizer.state == .ended, let map = recognizer.view as? MLNMapView else { return }
            let place = map.convert(recognizer.location(in: map), toCoordinateFrom: map)
            onTap(Coordinate(lat: place.latitude, lon: place.longitude))
        }

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            let ink = Palette.inkOnMap
            let line = MLNShapeSource(identifier: "search-area-line", shape: nil, options: nil)
            style.addSource(line)
            style.addLayer(MapSetup.line("search-area-edge", from: line, color: Palette.haloOnMap, width: 5))
            style.addLayer(MapSetup.line("search-area-stroke", from: line, color: Palette.accentOnMap, width: 2.5))
            let dots = MLNShapeSource(identifier: "search-area-dots", shape: nil, options: nil)
            style.addSource(dots)
            style.addLayer(
                MapSetup.dot("search-area-dot", from: dots, radius: MapSetup.smallestDot, color: Palette.haloOnMap, ring: ink)
            )
            isStyleLoaded = true
            draw(on: mapView)
        }

        private func draw(on map: MLNMapView) {
            guard isStyleLoaded, let style = map.style else { return }
            if let source = style.source(withIdentifier: "search-area-line") as? MLNShapeSource {
                let points = line.map(\.location)
                source.shape = points.count >= 2 ? MLNPolylineFeature(coordinates: points, count: UInt(points.count)) : nil
            }
            if let source = style.source(withIdentifier: "search-area-dots") as? MLNShapeSource {
                source.shape = MLNShapeCollectionFeature(
                    shapes: dots.map { place in
                        let feature = MLNPointFeature()
                        feature.coordinate = place.location
                        return feature
                    }
                )
            }
        }
    }
}
