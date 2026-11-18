import Foundation

/// Turns the core's plain numbers into text for the screen.
///
/// Metric units only for now. A units setting comes with the settings screen.
enum Format {
    static func date(unixSeconds: Int64) -> String {
        Date(timeIntervalSince1970: TimeInterval(unixSeconds))
            .formatted(date: .abbreviated, time: .shortened)
    }

    static func duration(seconds: Int64) -> String {
        Duration.seconds(seconds)
            .formatted(.units(allowed: [.hours, .minutes], width: .abbreviated))
    }

    static func distance(metres: Double) -> String {
        let kilometres = (metres / 1000).formatted(.number.precision(.fractionLength(1)))
        return "\(kilometres) km"
    }

    static func altitude(metres: Double) -> String {
        "\(metres.formatted(.number.precision(.fractionLength(0)))) m"
    }

    static func verticalSpeed(metresPerSecond: Double) -> String {
        let value = metresPerSecond.formatted(
            .number.precision(.fractionLength(1)).sign(strategy: .always(includingZero: false))
        )
        return "\(value) m/s"
    }
}
