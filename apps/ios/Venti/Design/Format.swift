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

    /// A day without its weekday, for a line that has more to say:
    /// "6 Sep 2026".
    static func dayShort(unixSeconds: Int64) -> String {
        date(unixSeconds).formatted(.dateTime.day().month(.abbreviated).year())
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

    /// A minute of the day on a 24-hour clock, as the core writes its
    /// times in words: "09:30", "14:00".
    static func clock(minuteOfDay: Int) -> String {
        String(format: "%02d:%02d", minuteOfDay / 60, minuteOfDay % 60)
    }

    /// Hours and minutes the way pilots log airtime: "0:29", "5:42".
    static func hoursMinutes(seconds: Int64) -> String {
        Duration.seconds(seconds).formatted(.time(pattern: .hourMinute))
    }

    /// Minutes and seconds of a short stretch: "4:12".
    static func minutesSeconds(seconds: Int64) -> String {
        Duration.seconds(seconds).formatted(.time(pattern: .minuteSecond))
    }

    /// How numbers are written: the English way, with a point before the
    /// decimals and a comma between thousands, whatever region the iPhone
    /// is set to. The core words its own numbers that way ("10.0 km NE"),
    /// and a screen must not write one number with a comma and the next
    /// with a point: on an iPhone set to English in Germany it did. This
    /// changes with the app's language, when the app is translated.
    static let numbers = Locale(identifier: "en_US")

    /// A number with a fixed count of decimals: "59.36", "1,596".
    static func decimal(_ value: Double, places: Int) -> String {
        minus(value.formatted(.number.precision(.fractionLength(places)).locale(numbers)))
    }

    /// A number on a chart's axis, with no more decimals than it has, two
    /// at most: "12", "0.6". A chart's own labels follow the iPhone's
    /// region and would write "0,6" beside the app's "12.0 km".
    static func axis(_ value: Double) -> String {
        minus(value.formatted(.number.precision(.fractionLength(0...2)).grouping(.never).locale(numbers)))
    }

    /// The same with a sign, except for zero: "+2.4", "−1.4".
    private static func signed(_ value: Double, places: Int) -> String {
        minus(
            value.formatted(
                .number.precision(.fractionLength(places)).sign(strategy: .always(includingZero: false))
                    .locale(numbers)
            )
        )
    }

    /// A number with a sign and a fixed count of decimals: "+0.6", "−1.2".
    static func signedDecimal(_ value: Double, places: Int) -> String {
        signed(value, places: places)
    }

    /// The design language's minus, U+2212, in place of the hyphen the
    /// system writes before a negative number.
    private static func minus(_ text: String) -> String {
        text.replacingOccurrences(of: "-", with: "\u{2212}")
    }

    /// A whole number of things: "1,745".
    static func count<Value: BinaryInteger>(_ value: Value) -> String {
        IntegerFormatStyle<Value>(locale: numbers).format(value)
    }

    /// A share in whole per cent: "23%".
    static func percent(_ share: Double) -> String {
        share.formatted(.percent.precision(.fractionLength(0)).locale(numbers))
    }

    /// Kilometres with one decimal.
    static func kilometres(metres: Double) -> String {
        decimal(metres / 1000, places: 1)
    }

    /// Whole kilometres, for totals.
    static func wholeKilometres(metres: Double) -> String {
        decimal(metres / 1000, places: 0)
    }

    /// Whole metres, as a height is written: "2410", with no mark between
    /// the thousands. The design language writes heights so, an altimeter
    /// does, and a comma there reads as a decimal to half of Europe.
    static func metres(_ metres: Double) -> String {
        minus(metres.formatted(.number.precision(.fractionLength(0)).grouping(.never).locale(numbers)))
    }

    /// Whole metres with a sign, for a gain or a loss: "+1140", "−135".
    static func signedMetres(_ metres: Double) -> String {
        minus(
            metres.formatted(
                .number.precision(.fractionLength(0)).grouping(.never)
                    .sign(strategy: .always(includingZero: false)).locale(numbers)
            )
        )
    }

    /// Metres per second with one decimal and always a sign, except for zero.
    static func climb(metresPerSecond: Double) -> String {
        signed(metresPerSecond, places: 1)
    }

    /// Kilometres per hour, whole, from metres per second.
    static func kilometresPerHour(metresPerSecond: Double) -> String {
        decimal(metresPerSecond * 3.6, places: 0)
    }

    /// A speed as VoiceOver says it: "1 kilometre per hour", "12 kilometres
    /// per hour".
    static func spokenKilometresPerHour(metresPerSecond: Double) -> String {
        let whole = kilometresPerHour(metresPerSecond: metresPerSecond)
        return whole == "1" ? "1 kilometre per hour" : "\(whole) kilometres per hour"
    }

    /// A glide ratio with one decimal.
    static func glideRatio(_ ratio: Double) -> String {
        decimal(ratio, places: 1)
    }

    /// Hours, minutes and seconds of a running flight: "0:12:34".
    static func stopwatch(seconds: Int64) -> String {
        Duration.seconds(seconds).formatted(.time(pattern: .hourMinuteSecond))
    }

    /// A place in degrees with four decimals, which is to about ten
    /// metres: "47.6642° N, 11.7967° E".
    static func place(lat: Double, lon: Double) -> String {
        let degrees = { (value: Double) in decimal(abs(value), places: 4) }
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
        return decimal(rounded == 0 ? 0 : rounded, places: 0)
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
        count == 1 ? "1 flight" : "\(Format.count(count)) flights"
    }
}
