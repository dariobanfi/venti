import CoreLocation
import MapLibre
import SwiftUI
import VentiCore

/// The map of the map tab: terrain, live wind, flying sites, every flight
/// in the logbook as a line, and the pilot's own position when asked for.
struct OverviewMap: UIViewRepresentable {
    /// The shapes of the logbook's flights.
    let outlines: [[Coordinate]]
    /// The wind stations to draw.
    let stations: [WindStation]
    /// The airspaces to draw.
    let airspace: [AirspaceShape]
    /// The flying sites to draw.
    var sites: [SiteView] = []
    /// Other pilots in the air, to draw.
    var gliders: [LiveGlider] = []
    /// Where the pilot's groups plan to fly today and tomorrow, to mark.
    var plans: [GroupsModel.Pin] = []
    /// The track of the pilot whose flight is being followed, to draw.
    var followed: [TrailPointView] = []
    /// Whether the map keeps the pilot's position in its centre.
    @Binding var isFollowing: Bool
    /// Set when the position cannot be shown, with words for the pilot.
    @Binding var problem: String?
    /// Called with the part of the map in view whenever it has changed.
    let onRegion: (Bounds) -> Void
    /// Called with a station's identifier when the pilot taps its arrow.
    let onStation: (String) -> Void
    /// Called with a site's identifier when the pilot taps its sign.
    var onSite: (String) -> Void = { _ in }
    /// Called with a live track's identifier when the pilot taps its
    /// glider.
    var onGlider: (String) -> Void = { _ in }
    /// Called with a plan's identifier when the pilot taps its mark.
    var onPlan: (String) -> Void = { _ in }
    /// Called with the place under the finger after a long press on the
    /// map.
    var onPlace: (Coordinate) -> Void = { _ in }
    /// What the map is drawn from. Read when the map is made; the screen
    /// makes the map anew when it changes.
    var source: MapStyle.Source = .standard
    /// Whether the ground is photographs from above instead of the drawn map.
    var aerial = false
    /// The day of the forecast map to draw, square by square; nothing
    /// while the layer is off.
    var forecast: ThermalMapDay?
    /// Called with a square's place among the day's cells when the pilot
    /// taps it where nothing else the app draws is under the finger.
    var onSquare: (Int) -> Void = { _ in }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> MLNMapView {
        let map = MapSetup.makeMapView(from: source)
        map.delegate = context.coordinator
        map.automaticallyAdjustsContentInset = true
        // The northern Alps between Lake Constance and Salzburg.
        map.setCenter(CLLocationCoordinate2D(latitude: 47.45, longitude: 11.6), zoomLevel: 7.4, animated: false)

        let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.tapped(_:)))
        // The map's own double tap zooms; a single tap must wait for it.
        for recognizer in map.gestureRecognizers ?? [] where recognizer is UITapGestureRecognizer {
            tap.require(toFail: recognizer)
        }
        map.addGestureRecognizer(tap)
        // A long press asks about the place under the finger.
        let press = UILongPressGestureRecognizer(
            target: context.coordinator, action: #selector(Coordinator.pressed(_:))
        )
        map.addGestureRecognizer(press)
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
        coordinator.onRegion = onRegion
        coordinator.onStation = onStation
        coordinator.onSite = onSite
        coordinator.onGlider = onGlider
        coordinator.onPlan = onPlan
        coordinator.onPlace = onPlace
        coordinator.onSquare = onSquare
        coordinator.showForecast(forecast, on: map)
        coordinator.show(plans, on: map)
        coordinator.show(sites, on: map)
        coordinator.show(gliders, on: map)
        coordinator.showFollowed(followed, on: map)
        coordinator.show(outlines, on: map)
        coordinator.show(stations, on: map)
        coordinator.show(airspace, on: map)
        coordinator.showAerial(aerial, on: map)
        coordinator.follow(isFollowing, on: map)
    }

    @MainActor
    final class Coordinator: NSObject, @preconcurrency MLNMapViewDelegate {
        /// Tells the screen whether the map is following the pilot, and
        /// about a problem if there is one.
        var report: (Bool, String?) -> Void = { _, _ in }
        var onRegion: (Bounds) -> Void = { _ in }
        var onStation: (String) -> Void = { _ in }
        var onSite: (String) -> Void = { _ in }
        var onGlider: (String) -> Void = { _ in }
        var onPlan: (String) -> Void = { _ in }
        private var plans: [GroupsModel.Pin] = []
        private var drawnPlans: [GroupsModel.Pin]?
        var onPlace: (Coordinate) -> Void = { _ in }
        var onSquare: (Int) -> Void = { _ in }

        private var forecast: ThermalMapDay?
        private var drawnForecast: ThermalMapDay?
        private var isForecastDrawn = false
        private var gliders: [LiveGlider] = []
        private var drawnGliders: [LiveGlider]?
        private var followed: [TrailPointView] = []
        private var drawnFollowed: [TrailPointView]?
        private var outlines: [[Coordinate]] = []
        private var drawnOutlines: [[Coordinate]]?
        private var stations: [WindStation] = []
        private var drawnStations: [WindStation]?
        private var airspace: [AirspaceShape] = []
        private var drawnAirspace: [AirspaceShape]?
        private var sites: [SiteView] = []
        private var drawnSites: [SiteView]?
        private var isStyleLoaded = false
        private var requested = false
        private var aerial = false

        private static let tracks = "logbook-tracks"
        private static let wind = "wind-stations"
        private static let sites = "flying-sites"
        private static let live = "live-gliders"
        private static let plans = "plan-pins"
        private static let liveTrack = "live-track"

        func show(_ gliders: [LiveGlider], on map: MLNMapView) {
            self.gliders = gliders
            drawGliders(on: map)
        }

        func show(_ plans: [GroupsModel.Pin], on map: MLNMapView) {
            self.plans = plans
            drawPlans(on: map)
        }

        /// Marks the plans, once the style is there and only when they
        /// have changed.
        private func drawPlans(on map: MLNMapView) {
            guard isStyleLoaded, plans != drawnPlans,
                  let source = map.style?.source(withIdentifier: Self.plans) as? MLNShapeSource
            else { return }
            drawnPlans = plans
            source.shape = MLNShapeCollectionFeature(
                shapes: plans.map { plan in
                    let feature = MLNPointFeature()
                    feature.coordinate = plan.position.location
                    feature.attributes = ["id": plan.id, "label": plan.label]
                    return feature
                }
            )
        }

        func showForecast(_ day: ThermalMapDay?, on map: MLNMapView) {
            forecast = day
            drawForecast(on: map)
        }

        /// Draws the forecast map's squares, once the style is there and
        /// only when the day has changed.
        private func drawForecast(on map: MLNMapView) {
            guard isStyleLoaded, !isForecastDrawn || forecast != drawnForecast, let style = map.style else { return }
            isForecastDrawn = true
            drawnForecast = forecast
            ThermalMapLayers.show(forecast, in: style)
        }

        func showFollowed(_ track: [TrailPointView], on map: MLNMapView) {
            followed = track
            drawFollowed(on: map)
        }

        func show(_ sites: [SiteView], on map: MLNMapView) {
            self.sites = sites
            drawSites(on: map)
        }

        func show(_ outlines: [[Coordinate]], on map: MLNMapView) {
            self.outlines = outlines
            drawOutlines(on: map)
        }

        func show(_ stations: [WindStation], on map: MLNMapView) {
            self.stations = stations
            drawStations(on: map)
        }

        func show(_ airspace: [AirspaceShape], on map: MLNMapView) {
            self.airspace = airspace
            drawAirspace(on: map)
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

        /// Shows the drawn ground or the aerial view, now or once the
        /// style is there.
        func showAerial(_ aerial: Bool, on map: MLNMapView) {
            guard aerial != self.aerial else { return }
            self.aerial = aerial
            if isStyleLoaded {
                MapSetup.showAerial(aerial, on: map)
            }
        }

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            prepare(style)
            MapSetup.showAerial(aerial, on: mapView)
            isStyleLoaded = true
            drawOutlines(on: mapView)
            drawStations(on: mapView)
            drawSites(on: mapView)
            drawPlans(on: mapView)
            drawGliders(on: mapView)
            drawFollowed(on: mapView)
            drawAirspace(on: mapView)
            drawForecast(on: mapView)
            tellRegion(of: mapView)
        }

        func mapView(_ mapView: MLNMapView, regionDidChangeAnimated animated: Bool) {
            tellRegion(of: mapView)
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

        /// A long press hands over the place under the finger, once, as
        /// soon as the press is long enough.
        @objc func pressed(_ recognizer: UILongPressGestureRecognizer) {
            guard recognizer.state == .began, let map = recognizer.view as? MLNMapView else { return }
            let place = map.convert(recognizer.location(in: map), toCoordinateFrom: map)
            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
            onPlace(Coordinate(lat: place.latitude, lon: place.longitude))
        }

        /// A tap on a station's arrow opens the station. A finger is wider
        /// than an arrow, so the nearest station within its width counts.
        @objc func tapped(_ recognizer: UITapGestureRecognizer) {
            guard recognizer.state == .ended, let map = recognizer.view as? MLNMapView else { return }
            let point = recognizer.location(in: map)
            let reach = CGRect(x: point.x - 22, y: point.y - 22, width: 44, height: 44)
            // Gliders lie over the wind and the wind over the sites: what
            // is on top wins where several are under the finger.
            for (layer, open) in [
                (Self.live, onGlider), (Self.plans, onPlan), (Self.wind, onStation), (Self.sites, onSite),
            ] {
                let hits = map.visibleFeatures(in: reach, styleLayerIdentifiers: [layer])
                let nearest = hits.min { first, second in
                    Self.distance(from: point, to: first, on: map) < Self.distance(from: point, to: second, on: map)
                }
                if let id = nearest?.attribute(forKey: "id") as? String {
                    open(id)
                    return
                }
            }
            // Nothing of those: the forecast map's square under the finger
            // itself, if one is drawn there.
            let under = CGRect(x: point.x - 1, y: point.y - 1, width: 2, height: 2)
            let squares = map.visibleFeatures(in: under, styleLayerIdentifiers: [ThermalMapLayers.layer])
            if let id = squares.first?.attribute(forKey: "id") as? String, let index = Int(id) {
                onSquare(index)
            }
        }

        private static func distance(from point: CGPoint, to feature: any MLNFeature, on map: MLNMapView) -> CGFloat {
            let place = map.convert(feature.coordinate, toPointTo: map)
            return hypot(place.x - point.x, place.y - point.y)
        }

        /// The map calls its delegate while SwiftUI is updating the view, so
        /// the screen is told afterwards.
        private func tell(following: Bool, problem: String?) {
            Task { @MainActor in
                report(following, problem)
            }
        }

        private func tellRegion(of map: MLNMapView) {
            let visible = map.visibleCoordinateBounds
            let region = Bounds(
                south: visible.sw.latitude, west: visible.sw.longitude,
                north: visible.ne.latitude, east: visible.ne.longitude
            )
            Task { @MainActor in
                onRegion(region)
            }
        }

        /// Adds what this map draws on top of the base map, empty at first:
        /// the forecast map under the map's names, then airspace, the
        /// flights over it, the wind above them.
        private func prepare(_ style: MLNStyle) {
            ThermalMapLayers.add(to: style)
            AirspaceLayers.add(to: style)
            let empty = MLNShapeCollectionFeature(shapes: [])

            let tracks = MLNShapeSource(identifier: Self.tracks, shape: empty, options: nil)
            style.addSource(tracks)
            style.addLayer(MapSetup.line("logbook-tracks-edge", from: tracks, color: Palette.haloOnMap, width: 5))
            style.addLayer(
                MapSetup.line("logbook-tracks-line", from: tracks, color: Palette.accentOnMap, width: 2.5)
            )

            // The flight of a pilot who is being followed: a line in the
            // colours of climb, like a flight of the logbook when it is
            // opened, over the logbook's own lines.
            let flight = MLNShapeSource(identifier: Self.liveTrack, shape: empty, options: nil)
            style.addSource(flight)
            style.addLayer(MapSetup.line("live-track-edge", from: flight, color: Palette.haloOnMap, width: 5.5))
            for band in ClimbBand.allCases {
                let layer = MapSetup.line(
                    "live-track-\(band.rawValue)", from: flight, color: band.uiColor, width: 3
                )
                layer.predicate = NSPredicate(format: "band == %d", band.rawValue)
                style.addLayer(layer)
            }

            // Flying sites, from the zoom at which they can be told apart;
            // a name where there is room for one.
            for picture in SiteSymbols.all {
                style.setImage(picture.image, forName: picture.name)
            }
            let sites = MLNShapeSource(identifier: Self.sites, shape: empty, options: nil)
            style.addSource(sites)
            let signs = MLNSymbolStyleLayer(identifier: Self.sites, source: sites)
            signs.iconImageName = NSExpression(forKeyPath: "picture")
            signs.iconAllowsOverlap = NSExpression(forConstantValue: true)
            // A pin stands on the site with its tip; the name is under it.
            signs.iconAnchor = NSExpression(forConstantValue: "bottom")
            signs.text = NSExpression(forKeyPath: "label")
            signs.textFontNames = NSExpression(forConstantValue: ["noto-sans-regular"])
            signs.textFontSize = NSExpression(forConstantValue: 12)
            signs.textAnchor = NSExpression(forConstantValue: "top")
            signs.textOffset = NSExpression(forConstantValue: NSValue(cgVector: CGVector(dx: 0, dy: 0.2)))
            signs.textColor = NSExpression(forConstantValue: Palette.inkOnMap)
            signs.textHaloColor = NSExpression(forConstantValue: Palette.haloOnMap)
            signs.textHaloWidth = NSExpression(forConstantValue: 1.4)
            signs.textOptional = NSExpression(forConstantValue: true)
            signs.minimumZoomLevel = 9
            style.addLayer(signs)

            for picture in WindSymbols.all {
                style.setImage(picture.image, forName: picture.name)
            }
            let wind = MLNShapeSource(identifier: Self.wind, shape: empty, options: nil)
            style.addSource(wind)
            let symbols = MLNSymbolStyleLayer(identifier: Self.wind, source: wind)
            symbols.iconImageName = NSExpression(forKeyPath: "picture")
            symbols.iconRotation = NSExpression(forKeyPath: "heading")
            // Turned against north, not against the screen: the arrow keeps
            // pointing where the wind blows when the map is rotated.
            symbols.iconRotationAlignment = NSExpression(forConstantValue: "map")
            symbols.iconScale = NSExpression(forKeyPath: "scale")
            symbols.text = NSExpression(forKeyPath: "label")
            symbols.textFontNames = NSExpression(forConstantValue: ["noto-sans-regular"])
            symbols.textFontSize = NSExpression(forConstantValue: 12)
            symbols.textAnchor = NSExpression(forConstantValue: "top")
            symbols.textOffset = NSExpression(forConstantValue: NSValue(cgVector: CGVector(dx: 0, dy: 1.15)))
            symbols.textColor = NSExpression(forConstantValue: Palette.inkOnMap)
            symbols.textHaloColor = NSExpression(forConstantValue: Palette.haloOnMap)
            symbols.textHaloWidth = NSExpression(forConstantValue: 1.6)
            // Where stations crowd, the windiest are drawn first and the
            // others give way; a station keeps its arrow when only its
            // numbers have no room.
            symbols.symbolSortKey = NSExpression(forKeyPath: "rank")
            symbols.textOptional = NSExpression(forConstantValue: true)
            style.addLayer(symbols)

            // Where the pilot's groups plan to fly today and tomorrow: a
            // dot in the app's colour with the place and the time. Over
            // the sites, since a plan is often for one of them.
            let pins = MLNShapeSource(identifier: Self.plans, shape: MLNShapeCollectionFeature(shapes: []), options: nil)
            style.addSource(pins)
            style.addLayer(MapSetup.dot(Self.plans, from: pins, radius: 7, color: Palette.accentOnMap, ring: Palette.haloOnMap))
            let names = MLNSymbolStyleLayer(identifier: Self.plans + "-names", source: pins)
            names.text = NSExpression(forKeyPath: "label")
            names.textFontNames = NSExpression(forConstantValue: ["noto-sans-regular"])
            names.textFontSize = NSExpression(forConstantValue: 12)
            names.textAnchor = NSExpression(forConstantValue: "top")
            names.textOffset = NSExpression(forConstantValue: NSValue(cgVector: CGVector(dx: 0, dy: 0.9)))
            names.textColor = NSExpression(forConstantValue: Palette.inkOnMap)
            names.textHaloColor = NSExpression(forConstantValue: Palette.haloOnMap)
            names.textHaloWidth = NSExpression(forConstantValue: 1.6)
            names.textAllowsOverlap = NSExpression(forConstantValue: true)
            style.addLayer(names)

            // Other pilots in the air, above everything: a wing turned the
            // way it flies, with the pilot's name and altitude where there
            // is room. Gliders are never left out for want of room.
            LiveLayers.add(to: style, named: Self.live)
        }

        /// Draws the gliders, once the style is there and only when they
        /// have changed.
        private func drawGliders(on map: MLNMapView) {
            guard isStyleLoaded, gliders != drawnGliders,
                  let source = map.style?.source(withIdentifier: Self.live) as? MLNShapeSource
            else { return }
            drawnGliders = gliders
            source.shape = MLNShapeCollectionFeature(shapes: gliders.map(Self.gliderFeature))
        }

        /// Draws the followed flight, and when it first appears moves the
        /// map to show all of it above the page that lies over the lower
        /// half.
        private func drawFollowed(on map: MLNMapView) {
            guard isStyleLoaded, followed != drawnFollowed,
                  let source = map.style?.source(withIdentifier: Self.liveTrack) as? MLNShapeSource
            else { return }
            let isNew = (drawnFollowed ?? []).isEmpty && !followed.isEmpty
            drawnFollowed = followed
            source.shape = MLNShapeCollectionFeature(shapes: CockpitMap.Coordinator.trailFeatures(followed))
            if isNew, let box = Self.box(around: [followed.map { Coordinate(lat: $0.lat, lon: $0.lon) }]) {
                let margin = 0.002
                map.setVisibleCoordinateBounds(
                    MLNCoordinateBounds(
                        sw: CLLocationCoordinate2D(latitude: box.south - margin, longitude: box.west - margin),
                        ne: CLLocationCoordinate2D(latitude: box.north + margin, longitude: box.east + margin)
                    ),
                    edgePadding: UIEdgeInsets(top: 90, left: 40, bottom: map.bounds.height / 2 + 24, right: 40),
                    animated: true,
                    completionHandler: nil
                )
            }
        }

        /// One glider as the map's symbol layer reads it. A glider on the
        /// ground points nowhere.
        static func gliderFeature(for glider: LiveGlider) -> MLNPointFeature {
            LiveLayers.feature(for: glider)
        }

        /// Draws the flights, once the style is there and only when they
        /// have changed, then moves the camera to show them all.
        private func drawOutlines(on map: MLNMapView) {
            guard isStyleLoaded, outlines != drawnOutlines,
                  let source = map.style?.source(withIdentifier: Self.tracks) as? MLNShapeSource
            else { return }
            drawnOutlines = outlines

            let lines = outlines.filter { $0.count >= 2 }.map { outline in
                let coordinates = outline.map(\.location)
                return MLNPolyline(coordinates: coordinates, count: UInt(coordinates.count))
            }
            source.shape = MLNMultiPolylineFeature(polylines: lines)

            if let box = Self.box(around: outlines) {
                MapSetup.show(box, on: map, animated: false)
            }
        }

        /// Draws the airspace, once the style is there and only when it has
        /// changed.
        private func drawAirspace(on map: MLNMapView) {
            guard isStyleLoaded, airspace != drawnAirspace, let style = map.style else { return }
            drawnAirspace = airspace
            AirspaceLayers.show(airspace, in: style)
        }

        /// Draws the flying sites, once the style is there and only when
        /// they have changed.
        private func drawSites(on map: MLNMapView) {
            guard isStyleLoaded, sites != drawnSites,
                  let source = map.style?.source(withIdentifier: Self.sites) as? MLNShapeSource
            else { return }
            drawnSites = sites
            source.shape = MLNShapeCollectionFeature(shapes: sites.map(Self.siteFeature))
        }

        /// One site as the map's symbol layer reads it. Only a name the
        /// source gives is written on the map: "Take-off" under every
        /// take-off sign would say nothing.
        static func siteFeature(for site: SiteView) -> MLNPointFeature {
            let feature = MLNPointFeature()
            feature.coordinate = site.position.location
            feature.attributes = [
                "id": site.id,
                "picture": site.kind.pictureName,
                "label": site.isNamed ? site.name : "",
            ]
            return feature
        }

        /// Draws the stations, once the style is there and only when they
        /// have changed.
        private func drawStations(on map: MLNMapView) {
            guard isStyleLoaded, stations != drawnStations,
                  let source = map.style?.source(withIdentifier: Self.wind) as? MLNShapeSource
            else { return }
            drawnStations = stations
            source.shape = MLNShapeCollectionFeature(shapes: stations.map(Self.feature))
        }

        /// One station as the map's symbol layer reads it.
        static func feature(for station: WindStation) -> MLNPointFeature {
            let mark = station.mark
            var scale = 1.0
            // Lower ranks are placed first: strong wind before weak, and
            // stations with nothing current to say last.
            var rank = 1000.0
            if case .arrow(let band) = mark.symbol {
                scale = band.arrowScale
            }
            if mark.symbol != .old, let reading = station.latest {
                rank = -(reading.gustMs ?? reading.speedMs ?? 0)
            }
            let feature = MLNPointFeature()
            feature.coordinate = station.position.location
            feature.attributes = [
                "id": station.id,
                "picture": WindSymbols.name(of: mark.symbol),
                "heading": mark.headingDeg,
                "scale": scale,
                "label": mark.label,
                "rank": rank,
            ]
            return feature
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
