import CoreLocation
import VentiCore

/// The phone's satellite positions, for the cockpit.
///
/// Updates are asked for in the foreground when a flight starts and keep
/// coming with the screen off: the app declares the location background
/// mode, and a background activity session tells the system the pilot
/// started this on purpose.
@MainActor
final class LocationProvider: NSObject, @preconcurrency CLLocationManagerDelegate {
    var onSample: ((LocationSample) -> Void)?
    /// Called with words for the pilot when positions cannot be had.
    var onProblem: ((String) -> Void)?

    private let manager = CLLocationManager()
    private var background: CLBackgroundActivitySession?

    override init() {
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyBest
        manager.distanceFilter = kCLDistanceFilterNone
        manager.activityType = .airborne
        // A glider can hang nearly still in the air for a long time.
        manager.pausesLocationUpdatesAutomatically = false
    }

    func start() {
        manager.requestWhenInUseAuthorization()
        manager.allowsBackgroundLocationUpdates = true
        manager.showsBackgroundLocationIndicator = true
        background = CLBackgroundActivitySession()
        manager.startUpdatingLocation()
    }

    func stop() {
        manager.stopUpdatingLocation()
        manager.allowsBackgroundLocationUpdates = false
        background?.invalidate()
        background = nil
    }

    /// A position worse than this, in metres, is a guess from the phone
    /// network and not a fix: the next one can lie a kilometre away, which
    /// between two seconds looks like speed.
    nonisolated static let worstAccuracyM = 100.0
    /// A position older than this, in seconds, is one the system kept
    /// from before: where the iPhone was, not where it is.
    nonisolated static let oldestS = 10.0

    /// Whether a position is one to fly by: a fix, and of now.
    nonisolated static func isUsable(_ location: CLLocation, now: Date = Date()) -> Bool {
        // Negative accuracy is how the system says "unknown".
        location.horizontalAccuracy >= 0 && location.horizontalAccuracy <= worstAccuracyM
            && now.timeIntervalSince(location.timestamp) <= oldestS
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        // Negative speed or course is how the system says "unknown".
        for location in locations where Self.isUsable(location) {
            onSample?(
                LocationSample(
                    timeUnixS: location.timestamp.timeIntervalSince1970,
                    lat: location.coordinate.latitude,
                    lon: location.coordinate.longitude,
                    altitudeM: location.verticalAccuracy > 0 ? location.altitude : nil,
                    groundSpeedMs: location.speed >= 0 ? location.speed : nil,
                    courseDeg: location.course >= 0 ? location.course : nil
                )
            )
        }
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: any Error) {
        // While the system's question whether Venti may use the position
        // is still open, positions fail as "denied" too. That is no
        // answer: only a pilot's no, or a restriction, is said.
        if (error as? CLError)?.code == .denied, Self.isRefused(manager.authorizationStatus) {
            reportDenied()
        }
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        if Self.isRefused(manager.authorizationStatus) {
            reportDenied()
        }
    }

    /// Whether the app has been told it may not use the position: by the
    /// pilot or by a restriction on the iPhone. Not yet asked is not that.
    nonisolated static func isRefused(_ status: CLAuthorizationStatus) -> Bool {
        status == .denied || status == .restricted
    }

    private func reportDenied() {
        onProblem?(
            "Venti is not allowed to use your position, so it cannot record a flight. You can allow it in Settings, under Privacy & Security, Location Services."
        )
    }
}
