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

    /// "Thu, 19 Nov at 15:00"
    static func dayAndTime(unixSeconds: Int64) -> String {
        "\(dayInYear(unixSeconds: unixSeconds)) at \(time(unixSeconds: unixSeconds))"
    }

    /// "10:30"
    static func time(unixSeconds: Int64) -> String {
        date(unixSeconds).formatted(date: .omitted, time: .shortened)
    }

    /// Hours and minutes the way pilots log airtime: "0:29", "5:42".
    static func hoursMinutes(seconds: Int64) -> String {
        Duration.seconds(seconds).formatted(.time(pattern: .hourMinute))
    }

    /// Minutes and seconds of a short stretch: "4:12".
    static func minutesSeconds(seconds: Int64) -> String {
        Duration.seconds(seconds).formatted(.time(pattern: .minuteSecond))
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

    /// A speed as VoiceOver says it: "1 kilometre per hour", "12 kilometres
    /// per hour".
    static func spokenKilometresPerHour(metresPerSecond: Double) -> String {
        let whole = kilometresPerHour(metresPerSecond: metresPerSecond)
        return whole == "1" ? "1 kilometre per hour" : "\(whole) kilometres per hour"
    }

    /// A glide ratio with one decimal.
    static func glideRatio(_ ratio: Double) -> String {
        ratio.formatted(.number.precision(.fractionLength(1)))
    }

    /// Hours, minutes and seconds of a running flight: "0:12:34".
    static func stopwatch(seconds: Int64) -> String {
        Duration.seconds(seconds).formatted(.time(pattern: .hourMinuteSecond))
    }

    /// A place in degrees with four decimals, which is to about ten
    /// metres: "47.6642° N, 11.7967° E".
    static func place(lat: Double, lon: Double) -> String {
        let degrees = { (value: Double) in abs(value).formatted(.number.precision(.fractionLength(4))) }
        return "\(degrees(lat))° \(lat < 0 ? "S" : "N"), \(degrees(lon))° \(lon < 0 ? "W" : "E")"
    }

    /// A compass direction in one or two letters: "N", "SW".
    static func compassPoint(degrees: Double) -> String {
        let points = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"]
        let index = Int((degrees.truncatingRemainder(dividingBy: 360) + 360 + 22.5) / 45) % 8
        return points[index]
    }

    /// Whole degrees Celsius.
    static func temperature(celsius: Double) -> String {
        // Minus zero would read "-0".
        let rounded = celsius.rounded()
        return (rounded == 0 ? 0 : rounded).formatted(.number.precision(.fractionLength(0)))
    }

    /// Whole minutes between two moments, never negative.
    static func minutes(from earlier: Int64, to later: Int64) -> Int64 {
        max(0, (later - earlier) / 60)
    }

    /// How long ago something was measured: "just now", "25 minutes ago",
    /// "3 hours ago", "2 days ago".
    static func age(minutes: Int64) -> String {
        switch minutes {
        case ..<1: "just now"
        case 1: "1 minute ago"
        case ..<90: "\(minutes) minutes ago"
        case ..<(36 * 60): "\((minutes + 30) / 60) hours ago"
        default: "\((minutes + 720) / 1440) days ago"
        }
    }

    static func flights(_ count: UInt32) -> String {
        count == 1 ? "1 flight" : "\(count.formatted()) flights"
    }
}
