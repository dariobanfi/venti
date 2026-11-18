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

    /// Metres per second with one decimal and always a sign, except for zero.
    static func climb(metresPerSecond: Double) -> String {
        metresPerSecond.formatted(
            .number.precision(.fractionLength(1)).sign(strategy: .always(includingZero: false))
        )
    }

    static func flights(_ count: UInt32) -> String {
        count == 1 ? "1 flight" : "\(count.formatted()) flights"
    }
}
