import CoreMotion
import Foundation

/// The phone's own pressure sensor. It gives about one reading a second,
/// which makes a usable but slow vario; an external vario replaces it.
@MainActor
final class BarometerProvider {
    /// Called with the time (seconds since 1970) and the pressure in
    /// hectopascals.
    var onPressure: ((Double, Double) -> Void)?

    private let altimeter = CMAltimeter()

    /// Whether the pilot has answered the iPhone's question about motion
    /// and fitness with yes, so that the sensor is read without asking.
    static var isAllowed: Bool {
        CMAltimeter.authorizationStatus() == .authorized
    }

    func start() {
        guard CMAltimeter.isRelativeAltitudeAvailable() else { return }
        altimeter.startRelativeAltitudeUpdates(to: .main) { [weak self] data, _ in
            guard let data else { return }
            // The sensor reports kilopascals.
            let hectopascals = data.pressure.doubleValue * 10
            let now = Date().timeIntervalSince1970
            // The handler runs on the main queue, as asked for above.
            MainActor.assumeIsolated {
                self?.onPressure?(now, hectopascals)
            }
        }
    }

    func stop() {
        altimeter.stopRelativeAltitudeUpdates()
    }
}
