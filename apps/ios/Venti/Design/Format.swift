import Foundation

/// Turns the core's plain numbers into text for the screen.
///
/// Metric units only for now. A units setting comes with the settings screen.
/// Values and units are separate so a screen can set them in different sizes.
enum Format {
    private static func date(_ unixSeconds: Int64) -> Date {
        Date(timeIntervalSince1970: TimeInterval(unixSeconds))
    }

    /// "Sat, 4 Jul 2026"
    static func day(unixSeconds: Int64) -> String {
        date(unixSeconds).formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated).year())
    }

    /// "Sat, 4 Jul", for lists already grouped by year.
    static func dayInYear(unixSeconds: Int64) -> String {
        date(unixSeconds).formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated))
    }

    /// "10:30"
    static func time(unixSeconds: Int64) -> String {
        date(unixSeconds).formatted(date: .omitted, time: .shortened)
    }

    /// Hours and minutes the way pilots log airtime: "0:29", "5:42".
    static func hoursMinutes(seconds: Int64) -> String {
        Duration.seconds(seconds).formatted(.time(pattern: .hourMinute))
    }

    /// Kilometres with one decimal.
    static func kilometres(metres: Double) -> String {
        (metres / 1000).formatted(.number.precision(.fractionLength(1)))
    }

    /// Whole kilometres, for totals.
    static func wholeKilometres(metres: Double) -> String {
        (metres / 1000).formatted(.number.precision(.fractionLength(0)))
    }

    /// Whole metres.
    static func metres(_ metres: Double) -> String {
        metres.formatted(.number.precision(.fractionLength(0)))
    }

    /// Whole metres with a sign, for a gain or a loss.
    static func signedMetres(_ metres: Double) -> String {
        metres.formatted(
            .number.precision(.fractionLength(0)).sign(strategy: .always(includingZero: false))
        )
    }

    /// Metres per second with one decimal and always a sign, except for zero.
    static func climb(metresPerSecond: Double) -> String {
        metresPerSecond.formatted(
            .number.precision(.fractionLength(1)).sign(strategy: .always(includingZero: false))
        )
    }

    /// Kilometres per hour, whole, from metres per second.
    static func kilometresPerHour(metresPerSecond: Double) -> String {
        (metresPerSecond * 3.6).formatted(.number.precision(.fractionLength(0)))
    }

    /// A glide ratio with one decimal.
    static func glideRatio(_ ratio: Double) -> String {
        ratio.formatted(.number.precision(.fractionLength(1)))
    }

    /// Hours, minutes and seconds of a running flight: "0:12:34".
    static func stopwatch(seconds: Int64) -> String {
        Duration.seconds(seconds).formatted(.time(pattern: .hourMinuteSecond))
    }

    /// A compass direction in one or two letters: "N", "SW".
    static func compassPoint(degrees: Double) -> String {
        let points = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"]
        let index = Int((degrees.truncatingRemainder(dividingBy: 360) + 360 + 22.5) / 45) % 8
        return points[index]
    }

    static func flights(_ count: UInt32) -> String {
        count == 1 ? "1 flight" : "\(count.formatted()) flights"
    }
}
