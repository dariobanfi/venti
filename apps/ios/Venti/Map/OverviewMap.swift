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
    /// The flying sites to draw: those the filter of the sites leaves.
    var sites: [SiteView] = []
    /// The take-offs that say nothing to what the filter asks: drawn
    /// apart from the others, muted and without their names.
    var unsaidSites: [SiteView] = []
    /// The places where pilots take off and no site stands, while their
    /// layer is on: small rings, not pins.
    var candidates: [SiteCandidateView] = []
    /// Other pilots in the air, to draw.
    var gliders: [LiveGlider] = []
    /// The aircraft other networks hear, to draw apart from Venti's
    /// pilots: each network's, as the core's layer holds them.
    var others: [LiveOthersNetwork] = []
    /// Where the pilot's groups plan to fly today and tomorrow, to mark.
    var plans: [GroupsModel.Pin] = []
    /// The track of the pilot whose flight is being followed, to draw.
    var followed: [TrailPointView] = []
    /// Where the map stays: each new place re-centres it above the page
    /// over its lower half, at the zoom it has. None while nothing is kept.
    var keptOn: Coordinate?
    /// Called when the pilot moves the map by hand.
    var onMovedByHand: () -> Void = {}
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
    /// Called with a place's position among `candidates` when the pilot
    /// taps its ring.
    var onCandidate: (Int) -> Void = { _ in }
    /// Called with a live track's identifier when the pilot taps its
    /// glider.
    var onGlider: (String) -> Void = { _ in }
    /// Called with an aircraft's identifier when the pilot taps one of
    /// another network.
    var onOther: (String) -> Void = { _ in }
    /// Called with the map's zoom whenever the part in view has changed,
    /// just before `onRegion`.
    var onZoom: (Double) -> Void = { _ in }
    /// Called with a plan's identifier when the pilot taps its mark.
    var onPlan: (String) -> Void = { _ in }
    /// Called with the place under the finger after a long press on the
    /// map, and whether a site's sign lies under it.
    var onPlace: (Coordinate, Bool) -> Void = { _, _ in }
    /// What the map is drawn from. Read when the map is made; the screen
    /// makes the map anew when it changes.
    var source: MapStyle.Source = .standard
    /// Whether the ground is photographs from above instead of the drawn map.
    var aerial = false
    /// Whether the thermal map of thermal.kk7.ch lies over the ground.
    var thermalMap = false
    /// Where the map says who made the photographs in view.
    var photos: PhotoCredits?
    /// The day of the forecast map to draw, square by square; nothing
    /// while the layer is off.
    var forecast: ThermalMapDay?
    /// Called with a square's place among the day's cells when the pilot
    /// taps it where nothing else the app draws is under the finger.
    var onSquare: (Int) -> Void = { _ in }
    /// Called with the place under the finger after a tap where nothing
    /// else the app draws lies, while the map is close enough for airspace
    /// to be drawn. The screen decides whether airspace lies there.
    var onAirspace: (Coordinate) -> Void = { _ in }
    /// An hour of a layer of the forecast map, drawn in place of the day;
    /// nothing while the day is drawn.
    var forecastHour: ForecastHourDrawing?
    /// Called with the place under the finger after a tap while an hour
    /// is drawn, where nothing else the app draws lies: the screen finds
    /// the square from the place, drawn or not.
    var onHourPlace: (Coordinate) -> Void = { _ in }
    /// The rain radar's pictures to draw, the first at the bottom; none
    /// while the layer is off.
    var radar: [RadarPicture] = []
    /// What a glide reaches from a place pressed on, while it is shown.
    var reach: GlideReachDrawing?
    /// The cables around, as the core's layer has them; none while the
    /// layer is off.
    var cables: [CableLine] = []
    /// Called with a cable's identifier when the pilot taps its line.
    var onCable: (String) -> Void = { _ in }
    /// Where pilots climb now, as the core lists them; none while the
    /// layer is off.
    var climbs: [ClimbView] = []
    /// Called with a climb's place among `climbs` when the pilot taps its
    /// mark.
    var onClimb: (Int) -> Void = { _ in }
    /// Where thermals have been; `nil` while the layer is off.
    var flown: FlownThermals?
    /// Called with a cell's place among `flown`'s when the pilot taps it.
    var onFlownCell: (Int) -> Void = { _ in }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> MLNMapView {
        let map = MapSetup.makeMapView(from: source)
        map.delegate = context.coordinator
        map.automaticallyAdjustsContentInset = true
        // The map tab has a credits button of its own in the same corner,
        // which lists what this one would, and the data's credits with it.
        map.attributionButton.isHidden = true
        // The northern Alps between Lake Constance and Salzburg, or where
        // a UI test asks for.
        let at = LaunchOptions.mapAt ?? (lat: 47.45, lon: 11.6, zoom: 7.4)
        map.setCenter(CLLocationCoordinate2D(latitude: at.lat, longitude: at.lon), zoomLevel: at.zoom, animated: false)

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
        coordinator.onCandidate = { id in
            if let index = Int(id) {
                onCandidate(index)
            }
        }
        coordinator.onGlider = onGlider
        coordinator.onOther = onOther
        coordinator.onZoom = onZoom
        coordinator.onPlan = onPlan
        coordinator.onPlace = onPlace
        coordinator.onSquare = onSquare
        coordinator.onAirspace = onAirspace
        coordinator.onHourPlace = onHourPlace
        coordinator.onCable = onCable
        coordinator.showCables(cables, on: map)
        coordinator.onClimb = onClimb
        coordinator.onFlownCell = onFlownCell
        coordinator.showClimbs(climbs, on: map)
        coordinator.showFlown(flown, on: map)
        coordinator.showForecast(forecast, on: map)
        coordinator.showHour(forecastHour, on: map)
        coordinator.showRadar(radar, on: map)
        coordinator.showReach(reach, on: map)
        coordinator.show(plans, on: map)
        coordinator.show(sites, unsaid: unsaidSites, on: map)
        coordinator.show(candidates, on: map)
        coordinator.show(gliders, on: map)
        coordinator.showOthers(others, on: map)
        coordinator.showFollowed(followed, on: map)
        coordinator.onMovedByHand = onMovedByHand
        coordinator.keep(on: keptOn, on: map)
        coordinator.show(outlines, on: map)
        coordinator.show(stations, on: map)
        coordinator.show(airspace, on: map)
        coordinator.photos = photos
        coordinator.showAerial(aerial, on: map)
        coordinator.showThermalMap(thermalMap, on: map)
        coordinator.follow(isFollowing, on: map)
    }

    @MainActor
    final class Coordinator: NSObject, @preconcurrency MLNMapViewDelegate {
        var photos: PhotoCredits?
        /// Tells the screen whether the map is following the pilot, and
        /// about a problem if there is one.
        var report: (Bool, String?) -> Void = { _, _ in }
        var onRegion: (Bounds) -> Void = { _ in }
        var onStation: (String) -> Void = { _ in }
        var onSite: (String) -> Void = { _ in }
        var onCable: (String) -> Void = { _ in }
        private var cables: [CableLine] = []
        private var drawnCables: [CableLine]?
        var onClimb: (Int) -> Void = { _ in }
        var onFlownCell: (Int) -> Void = { _ in }
        private var climbs: [ClimbView] = []
        private var drawnClimbs: [ClimbView]?
        private var flown: FlownThermals?
        private var drawnFlown: FlownThermals??
        var onCandidate: (String) -> Void = { _ in }
        var onGlider: (String) -> Void = { _ in }
        var onOther: (String) -> Void = { _ in }
        var onZoom: (Double) -> Void = { _ in }
        var onPlan: (String) -> Void = { _ in }
        private var plans: [GroupsModel.Pin] = []
        private var drawnPlans: [GroupsModel.Pin]?
        var onPlace: (Coordinate, Bool) -> Void = { _, _ in }
        var onSquare: (Int) -> Void = { _ in }
        var onAirspace: (Coordinate) -> Void = { _ in }
        var onHourPlace: (Coordinate) -> Void = { _ in }
        private var hour: ForecastHourDrawing?
        /// What was drawn of an hour: its identity, `nil` for none.
        private var drawnHour: String??

        private var radar: [RadarPicture] = []
        /// Which pictures of the radar were drawn; `nil` before the first
        /// time.
        private var drawnRadar: [String]?

        private var reach: GlideReachDrawing?
        /// What was drawn of a glide's reach: its identity, `nil` for none.
        private var drawnReach: String??
        /// The place the reach drawn last starts from.
        private var drawnReachPlace: Coordinate?

        private var forecast: ThermalMapDay?
        private var drawnForecast: ThermalMapDay?
        private var isForecastDrawn = false
        private var gliders: [LiveGlider] = []
        private var drawnGliders: [LiveGlider]?
        private var others: [LiveOthersNetwork] = []
        private var drawnOthers: [LiveOthersNetwork]?
        private var followed: [TrailPointView] = []
        private var drawnFollowed: [TrailPointView]?
        private var outlines: [[Coordinate]] = []
        private var drawnOutlines: [[Coordinate]]?
        private var stations: [WindStation] = []
        private var drawnStations: [WindStation]?
        var onMovedByHand: () -> Void = {}
        /// The place the map was last re-centred on.
        private var keptOn: Coordinate?
        private var airspace: [AirspaceShape] = []
        private var drawnAirspace: [AirspaceShape]?
        private var sites: [SiteView] = []
        private var drawnSites: [SiteView]?
        private var unsaidSites: [SiteView] = []
        private var candidates: [SiteCandidateView] = []
        private var drawnCandidates: [SiteCandidateView]?
        private var drawnUnsaidSites: [SiteView]?
        /// The sites' areas that are drawn: those larger than their sign
        /// at the map's scale.
        private var drawnFields: [SiteFields.Field]?
        private var isStyleLoaded = false
        private var requested = false
        private var aerial = false

        private static let tracks = "logbook-tracks"
        private static let wind = "wind-stations"
        private static let sites = "flying-sites"
        private static let candidates = "site-candidates"
        private static let live = "live-gliders"
        private static let others = "live-others"
        private static let plans = "plan-pins"
        private static let liveTrack = "live-track"

        func show(_ gliders: [LiveGlider], on map: MLNMapView) {
            self.gliders = gliders
            drawGliders(on: map)
        }

        func showOthers(_ others: [LiveOthersNetwork], on map: MLNMapView) {
            self.others = others
            drawOthers(on: map)
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

        func showHour(_ hour: ForecastHourDrawing?, on map: MLNMapView) {
            self.hour = hour
            drawHour(on: map)
        }

        func showRadar(_ pictures: [RadarPicture], on map: MLNMapView) {
            radar = pictures
            drawRadar(on: map)
        }

        /// Draws the radar's pictures, once the style is there and only
        /// when they are others.
        private func drawRadar(on map: MLNMapView) {
            guard isStyleLoaded, let style = map.style else { return }
            let ids = radar.map(\.id)
            if drawnRadar == ids { return }
            drawnRadar = ids
            RadarLayers.show(radar, in: style)
        }

        func showCables(_ cables: [CableLine], on map: MLNMapView) {
            self.cables = cables
            drawCables(on: map)
        }

        private func drawCables(on map: MLNMapView) {
            guard isStyleLoaded, cables != drawnCables, let style = map.style else { return }
            drawnCables = cables
            CablesLayers.show(cables, in: style)
        }

        func showClimbs(_ climbs: [ClimbView], on map: MLNMapView) {
            self.climbs = climbs
            drawClimbs(on: map)
        }

        /// Draws the climbs, once the style is there and only when they
        /// have changed.
        private func drawClimbs(on map: MLNMapView) {
            guard isStyleLoaded, climbs != drawnClimbs, let style = map.style else { return }
            drawnClimbs = climbs
            AirLayers.show(climbs, in: style)
        }

        func showFlown(_ flown: FlownThermals?, on map: MLNMapView) {
            self.flown = flown
            drawFlown(on: map)
        }

        /// Draws where thermals have been, once the style is there and
        /// only when the answer is another.
        private func drawFlown(on map: MLNMapView) {
            guard isStyleLoaded, drawnFlown != .some(flown), let style = map.style else { return }
            drawnFlown = .some(flown)
            AirLayers.show(flown, in: style)
        }

        func showReach(_ reach: GlideReachDrawing?, on map: MLNMapView) {
            self.reach = reach
            drawReach(on: map)
        }

        /// Draws the glide's reach, once the style is there and only when
        /// another answer has come; a reach from another place is brought
        /// into view above the card that lies over the lower half.
        private func drawReach(on map: MLNMapView) {
            guard isStyleLoaded, let style = map.style else { return }
            let id = reach?.id
            if let drawn = drawnReach, drawn == id { return }
            let isNewPlace = reach != nil && reach?.place != drawnReachPlace
            drawnReach = .some(id)
            drawnReachPlace = reach?.place
            GlideReachLayers.show(reach, in: style)
            // Brought into view for a new place, and when a setting has
            // grown the area past what is in view.
            if let box = reach?.box, isNewPlace || !Self.isInView(box, on: map) {
                let margin = 0.005
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

        /// Whether a box lies wholly in the part of the map that the chips
        /// above and the card over the lower half leave in view.
        private static func isInView(_ box: Bounds, on map: MLNMapView) -> Bool {
            let open = map.bounds.inset(by: UIEdgeInsets(top: 90, left: 40, bottom: map.bounds.height / 2 + 24, right: 40))
            return [
                CLLocationCoordinate2D(latitude: box.south, longitude: box.west),
                CLLocationCoordinate2D(latitude: box.north, longitude: box.east),
            ].allSatisfy { open.contains(map.convert($0, toPointTo: map)) }
        }

        /// Draws an hour's squares and arrows, once the style is there and
        /// only when the hour, the layer or the day has changed.
        private func drawHour(on map: MLNMapView) {
            guard isStyleLoaded, let style = map.style else { return }
            let id = hour?.id
            if let drawn = drawnHour, drawn == id { return }
            drawnHour = .some(id)
            ThermalMapLayers.showHour(hour?.shapes, key: hour?.key ?? [], in: style)
        }

        /// Draws the forecast map's squares, once the style is there and
        /// only when the day has changed.
        private func drawForecast(on map: MLNMapView) {
            guard isStyleLoaded, !isForecastDrawn || forecast != drawnForecast, let style = map.style else { return }
            isForecastDrawn = true
            drawnForecast = forecast
            ThermalMapLayers.show(forecast, in: style)
        }

        /// Re-centres the map on a place that is new, at the zoom it has,
        /// in the middle of what the page over the lower half leaves.
        func keep(on place: Coordinate?, on map: MLNMapView) {
            guard place != keptOn else { return }
            keptOn = place
            if let place, isStyleLoaded {
                centre(on: place, map)
            }
        }

        private func centre(on place: Coordinate, _ map: MLNMapView) {
            let target = CGPoint(x: map.bounds.midX, y: (90 + map.bounds.height / 2 - 24) / 2)
            let at = map.convert(place.location, toPointTo: map)
            let middle = map.convert(map.centerCoordinate, toPointTo: map)
            let moved = CGPoint(x: middle.x + at.x - target.x, y: middle.y + at.y - target.y)
            map.setCenter(map.convert(moved, toCoordinateFrom: map), animated: true)
        }

        /// A drag by the pilot lets the map go from what it is kept on.
        func mapView(_ mapView: MLNMapView, regionWillChangeWith reason: MLNCameraChangeReason, animated: Bool) {
            if reason.contains(.gesturePan) {
                onMovedByHand()
            }
        }

        func showFollowed(_ track: [TrailPointView], on map: MLNMapView) {
            followed = track
            drawFollowed(on: map)
        }

        func show(_ sites: [SiteView], unsaid: [SiteView], on map: MLNMapView) {
            self.sites = sites
            unsaidSites = unsaid
            drawSites(on: map)
        }

        func show(_ candidates: [SiteCandidateView], on map: MLNMapView) {
            self.candidates = candidates
            drawCandidates(on: map)
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

        private var thermalMap = false

        /// Shows or hides the thermal map, now or once the style is there.
        func showThermalMap(_ isOn: Bool, on map: MLNMapView) {
            guard isOn != thermalMap else { return }
            thermalMap = isOn
            if isStyleLoaded {
                MapSetup.showThermalMap(isOn, on: map)
            }
        }

        func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
            prepare(style)
            MapSetup.showAerial(aerial, on: mapView)
            MapSetup.showThermalMap(thermalMap, on: mapView)
            photos?.update(from: mapView)
            isStyleLoaded = true
            drawOutlines(on: mapView)
            drawStations(on: mapView)
            drawSites(on: mapView)
            drawCandidates(on: mapView)
            drawPlans(on: mapView)
            drawOthers(on: mapView)
            drawGliders(on: mapView)
            drawFollowed(on: mapView)
            if let keptOn {
                centre(on: keptOn, mapView)
            }
            drawAirspace(on: mapView)
            drawForecast(on: mapView)
            drawHour(on: mapView)
            drawRadar(on: mapView)
            drawReach(on: mapView)
            drawCables(on: mapView)
            drawFlown(on: mapView)
            drawClimbs(on: mapView)
            tellRegion(of: mapView)
        }

        func mapView(_ mapView: MLNMapView, regionDidChangeAnimated animated: Bool) {
            photos?.update(from: mapView)
            drawFields(on: mapView)
            tellRegion(of: mapView)
        }

        /// While the pilot zooms: a site's area comes when it has grown
        /// larger than its sign, and goes when it is smaller again.
        func mapViewRegionIsChanging(_ mapView: MLNMapView) {
            drawFields(on: mapView)
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
            let point = recognizer.location(in: map)
            let place = map.convert(point, toCoordinateFrom: map)
            // Whether a site's sign lies under the finger, as a tap would
            // find it: no site is added on top of one.
            let reach = CGRect(x: point.x - 22, y: point.y - 22, width: 44, height: 44)
            let isOnASite = !map.visibleFeatures(in: reach, styleLayerIdentifiers: [Self.sites]).isEmpty
            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
            onPlace(Coordinate(lat: place.latitude, lon: place.longitude), isOnASite)
        }

        /// A tap on a station's arrow opens the station. A finger is wider
        /// than an arrow, so the nearest station within its width counts.
        @objc func tapped(_ recognizer: UITapGestureRecognizer) {
            guard recognizer.state == .ended, let map = recognizer.view as? MLNMapView else { return }
            let point = recognizer.location(in: map)
            let reach = CGRect(x: point.x - 22, y: point.y - 22, width: 44, height: 44)
            // Gliders lie over the wind and the wind over the sites: what
            // is on top wins where several are under the finger. Venti's
            // pilots lie over the other networks' aircraft.
            for (layer, open) in [
                (Self.live, onGlider), (Self.others, onOther), (Self.plans, onPlan), (Self.wind, onStation),
                (Self.sites, onSite), (Self.candidates, onCandidate),
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
            // A climb's mark, under the signs above: it is a place in the
            // air, and a site or a station under the finger is the thing
            // the pilot more likely means.
            let marks = map.visibleFeatures(in: reach, styleLayerIdentifiers: [AirLayers.climbs])
            let nearest = marks.min { first, second in
                Self.distance(from: point, to: first, on: map) < Self.distance(from: point, to: second, on: map)
            }
            if let id = nearest?.attribute(forKey: "id") as? String, let index = Int(id) {
                onClimb(index)
                return
            }
            // A cable's line under the finger, where nothing above it is.
            let wires = map.visibleFeatures(in: reach, styleLayerIdentifiers: Set(CablesLayers.lineLayers))
            if let id = wires.first?.attribute(forKey: "id") as? String {
                onCable(id)
                return
            }
            let under = CGRect(x: point.x - 1, y: point.y - 1, width: 2, height: 2)
            // A cell of where thermals have been, under the finger itself.
            let cells = map.visibleFeatures(in: under, styleLayerIdentifiers: [AirLayers.flown])
            if let id = cells.first?.attribute(forKey: "id") as? String, let index = Int(id) {
                onFlownCell(index)
                return
            }
            // Nothing of those: the forecast map's square under the finger
            // itself, if one is drawn there.
            let squares = map.visibleFeatures(in: under, styleLayerIdentifiers: [ThermalMapLayers.layer])
            if let id = squares.first?.attribute(forKey: "id") as? String, let index = Int(id) {
                onSquare(index)
                return
            }
            // An hour of the forecast map: its square under the finger,
            // drawn or not ("Dry" is a reading too).
            if hour != nil {
                let place = map.convert(point, toCoordinateFrom: map)
                onHourPlace(Coordinate(lat: place.latitude, lon: place.longitude))
                return
            }
            // Nothing else: what airspace lies over the place, where the
            // map is close enough to draw it.
            if map.zoomLevel >= Double(AirspaceLayers.minimumZoom) {
                let place = map.convert(point, toCoordinateFrom: map)
                onAirspace(Coordinate(lat: place.latitude, lon: place.longitude))
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
            let zoom = map.zoomLevel
            Task { @MainActor in
                onZoom(zoom)
                onRegion(region)
            }
        }

        /// Adds what this map draws on top of the base map, empty at first:
        /// the forecast map under the map's names, then airspace, the
        /// flights over it, the wind above them.
        private func prepare(_ style: MLNStyle) {
            ThermalMapLayers.add(to: style)
            ThermalMapLayers.addHours(to: style)
            // Where thermals have been, under the airspace and all the
            // app's lines and signs.
            AirLayers.addFlown(to: style)
            AirspaceLayers.add(to: style)
            // The cables, over the airspace.
            CablesLayers.add(to: style)
            // What a glide reaches, over the airspace and under the
            // flights, sites and wind.
            GlideReachLayers.add(to: style)
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
            // A site mapped as an area, under its sign.
            SiteFields.add(to: style)
            // The places where pilots take off and no site stands, under
            // the sites' signs: a small hollow ring in the muted ink, the
            // size of the smallest dot the app's maps draw, so that it is
            // not taken for a site.
            let candidates = MLNShapeSource(identifier: Self.candidates, shape: empty, options: nil)
            style.addSource(candidates)
            let rings = MapSetup.dot(
                Self.candidates, from: candidates, radius: MapSetup.smallestDot,
                color: Palette.haloOnMap, ring: Palette.candidateOnMap
            )
            rings.minimumZoomLevel = 9
            style.addLayer(rings)
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
            // The sites that fit lie over the ones shown apart.
            signs.symbolSortKey = NSExpression(forKeyPath: "rank")
            signs.minimumZoomLevel = 9
            style.addLayer(signs)

            for picture in WindSymbols.all {
                style.setImage(picture.image, forName: picture.name)
            }
            // The forecast's arrows, under the stations' own.
            ThermalMapLayers.addArrows(to: style)
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

            // Where pilots climb now: a dot in the colour of the climb with
            // the core's label, under the gliders themselves.
            AirLayers.addClimbs(to: style)

            // Other pilots in the air, above everything: a wing turned the
            // way it flies, with the pilot's name and altitude where there
            // is room. Gliders are never left out for want of room. The
            // aircraft other networks hear lie under Venti's own pilots.
            LiveLayers.add(to: style, named: Self.others)
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

        /// Draws the other networks' aircraft, once the style is there and
        /// only when they have changed.
        private func drawOthers(on map: MLNMapView) {
            guard isStyleLoaded, others != drawnOthers,
                  let source = map.style?.source(withIdentifier: Self.others) as? MLNShapeSource
            else { return }
            drawnOthers = others
            source.shape = MLNShapeCollectionFeature(shapes: LiveLayers.features(for: others))
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

            // Where a UI test names the place the map opens at, it stays
            // there.
            if LaunchOptions.mapAt == nil, let box = Self.box(around: outlines) {
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
            guard isStyleLoaded, sites != drawnSites || unsaidSites != drawnUnsaidSites,
                  let source = map.style?.source(withIdentifier: Self.sites) as? MLNShapeSource
            else { return }
            drawnSites = sites
            drawnUnsaidSites = unsaidSites
            source.shape = MLNShapeCollectionFeature(
                shapes: unsaidSites.map { Self.siteFeature(for: $0, isUnsaid: true) }
                    + sites.map { Self.siteFeature(for: $0) }
            )
            drawFields(on: map)
        }

        /// Draws the places where pilots take off and no site stands, once
        /// the style is there and only when they have changed.
        private func drawCandidates(on map: MLNMapView) {
            guard isStyleLoaded, candidates != drawnCandidates,
                  let source = map.style?.source(withIdentifier: Self.candidates) as? MLNShapeSource
            else { return }
            drawnCandidates = candidates
            source.shape = MLNShapeCollectionFeature(
                shapes: candidates.enumerated().map { index, candidate in
                    let feature = MLNPointFeature()
                    feature.coordinate = candidate.position.location
                    feature.attributes = ["id": String(index)]
                    return feature
                }
            )
        }

        /// Draws the areas of the sites that are mapped as one, where
        /// the map is close enough for an area to be larger than its
        /// sign, and only when they have changed.
        private func drawFields(on map: MLNMapView) {
            guard isStyleLoaded, let style = map.style else { return }
            let fields = SiteFields.fields(
                fitting: sites, unsaid: unsaidSites,
                metresPerPoint: map.metersPerPoint(atLatitude: map.centerCoordinate.latitude),
                signPoints: SiteSymbols.size.width
            )
            guard fields != drawnFields else { return }
            drawnFields = fields
            SiteFields.show(fields, in: style)
        }

        /// One site as the map's symbol layer reads it. Only a name the
        /// source gives is written on the map: "Take-off" under every
        /// take-off sign would say nothing. A take-off that says nothing
        /// to what the filter asks has the muted pin and no name.
        static func siteFeature(for site: SiteView, isUnsaid: Bool = false) -> MLNPointFeature {
            let feature = MLNPointFeature()
            feature.coordinate = site.position.location
            feature.attributes = [
                "id": site.id,
                "picture": SiteSymbols.name(of: site.kind, isUnsaid: isUnsaid),
                "label": isUnsaid ? "" : Self.siteLabel(for: site),
                "rank": isUnsaid ? 0 : 1,
            ]
            return feature
        }

        /// What is written under a site's pin: its name where its source
        /// gives one, and under it the tier of a place from pilots, in the
        /// core's words, so that it is not taken for an official site. The
        /// pin is the one its use has: the pack has none for the tier.
        static func siteLabel(for site: SiteView) -> String {
            var lines = site.isNamed ? [site.name] : []
            if let by = site.byPilots, by.fromPilots {
                lines.append(by.label)
            }
            return lines.joined(separator: "\n")
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
