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

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        // Negative accuracy, speed or course is how the system says "unknown".
        for location in locations where location.horizontalAccuracy >= 0 {
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
        if (error as? CLError)?.code == .denied {
            reportDenied()
        }
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        if manager.authorizationStatus == .denied || manager.authorizationStatus == .restricted {
            reportDenied()
        }
    }

    private func reportDenied() {
        onProblem?(
            "Venti is not allowed to use your position, so it cannot record a flight. You can allow it in Settings, under Privacy & Security, Location Services."
        )
    }
}
