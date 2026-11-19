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

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> MLNMapView {
        let map = MapSetup.makeMapView()
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
        coordinator.show(sites, on: map)
        coordinator.show(outlines, on: map)
        coordinator.show(stations, on: map)
        coordinator.show(airspace, on: map)
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

        private static let tracks = "logbook-tracks"
        private static let wind = "wind-stations"
        private static let sites = "flying-sites"

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

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            prepare(style)
            isStyleLoaded = true
            drawOutlines(on: mapView)
            drawStations(on: mapView)
            drawSites(on: mapView)
            drawAirspace(on: mapView)
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

        /// A tap on a station's arrow opens the station. A finger is wider
        /// than an arrow, so the nearest station within its width counts.
        @objc func tapped(_ recognizer: UITapGestureRecognizer) {
            guard recognizer.state == .ended, let map = recognizer.view as? MLNMapView else { return }
            let point = recognizer.location(in: map)
            let reach = CGRect(x: point.x - 22, y: point.y - 22, width: 44, height: 44)
            // Wind lies over the sites, and wins where both are under the
            // finger.
            for (layer, open) in [(Self.wind, onStation), (Self.sites, onSite)] {
                let hits = map.visibleFeatures(in: reach, styleLayerIdentifiers: [layer])
                let nearest = hits.min { first, second in
                    Self.distance(from: point, to: first, on: map) < Self.distance(from: point, to: second, on: map)
                }
                if let id = nearest?.attribute(forKey: "id") as? String {
                    open(id)
                    return
                }
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
        /// airspace lowest, the flights over it, the wind above them.
        private func prepare(_ style: MLNStyle) {
            AirspaceLayers.add(to: style)
            let empty = MLNShapeCollectionFeature(shapes: [])

            let tracks = MLNShapeSource(identifier: Self.tracks, shape: empty, options: nil)
            style.addSource(tracks)
            style.addLayer(MapSetup.line("logbook-tracks-edge", from: tracks, color: .white, width: 5))
            style.addLayer(
                MapSetup.line("logbook-tracks-line", from: tracks, color: Palette.accentOnMap, width: 2.5)
            )

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
            signs.text = NSExpression(forKeyPath: "label")
            signs.textFontNames = NSExpression(forConstantValue: ["noto-sans-regular"])
            signs.textFontSize = NSExpression(forConstantValue: 11)
            signs.textAnchor = NSExpression(forConstantValue: "top")
            signs.textOffset = NSExpression(forConstantValue: NSValue(cgVector: CGVector(dx: 0, dy: 1.1)))
            signs.textColor = NSExpression(forConstantValue: UIColor(hex: 0x1F1F1D))
            signs.textHaloColor = NSExpression(forConstantValue: UIColor.white)
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
            symbols.textColor = NSExpression(forConstantValue: UIColor(hex: 0x0B0B0B))
            symbols.textHaloColor = NSExpression(forConstantValue: UIColor.white)
            symbols.textHaloWidth = NSExpression(forConstantValue: 1.6)
            // Where stations crowd, the windiest are drawn first and the
            // others give way; a station keeps its arrow when only its
            // numbers have no room.
            symbols.symbolSortKey = NSExpression(forKeyPath: "rank")
            symbols.textOptional = NSExpression(forConstantValue: true)
            style.addLayer(symbols)
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
