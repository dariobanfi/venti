import Foundation
import VentiCore

/// What the map tab shows over the ground, one at a time (the row of chips
/// along its top): the wind, the thermals, the rain or the cloud, each
/// along one line of time from what was measured to what is forecast. The
/// wind until the pilot chooses; nothing, where the pilot pressed the one
/// chosen again.
enum MapWhat: String, CaseIterable {
    case wind, thermals, rain, cloud

    /// What the preference keeps: one of the four, `nil` for "none", and
    /// the wind for anything else (never chosen).
    init?(stored: String) {
        if stored == "none" {
            return nil
        }
        self = MapWhat(rawValue: stored) ?? .wind
    }

    /// The layer of the forecast's hours it shows: the wind's mean or its
    /// gusts.
    func layer(gusts: Bool) -> ThermalHoursLayer {
        switch self {
        case .wind: gusts ? .gusts : .wind
        case .thermals: .thermals
        case .rain: .rain
        case .cloud: .cloud
        }
    }

    /// Whether its line begins with what was measured: the wind stations
    /// of now, the rain radar's pictures.
    var hasMeasured: Bool {
        self == .wind || self == .rain
    }
}

/// A stop of the map's line of time: what the map draws while the line
/// stands there.
enum MapStop: Hashable {
    /// One of the radar's moments, by its place in the radar's list.
    case radar(Int)
    /// The wind stations, as they measure now.
    case stations
    /// A day of the forecast as a whole: the kind of day, square by square.
    case day(String)
    /// An hour of the forecast, by its place in its day's hours.
    case hour(String, Int)

    var isMeasured: Bool {
        switch self {
        case .radar, .stations: true
        case .day, .hour: false
        }
    }
}

/// A cell of the line of time.
struct MapTimeCell: Equatable {
    let stop: MapStop
    /// What the cell shows: the clock's hour, "13", "Day", or nothing.
    let short: String
    /// What VoiceOver and a test say of it: "Today, 13:00", "Now".
    let label: String
    /// A narrow cell: a radar picture every few minutes.
    var isNarrow = false
    /// What stands over the cell: a day's name over its first cell, a
    /// radar's full hour over its moment, "Now".
    var over: String?
    /// Whether "Now" stands over the cell.
    var isNow = false
}

/// The map's one line of time, for what it shows: from what was measured
/// in the last hours, the rain radar's pictures or the wind stations of
/// now, to the forecast's hours, day after day. "Now" stands where the one
/// meets the other. Every time and word in it is the core's (the radar's
/// moments, the forecast map's days and hours, `mapLayersWords()`); this
/// lays them along one line. The web's `timeline.ts` is the same.
enum MapTimeLine {
    /// The cells of the line, the oldest first: what was measured, then
    /// each day of the forecast, today's from the hour that has begun. The
    /// thermals have the day as a whole before each day's hours. A day
    /// whose hours are not there yet has none of them.
    static func cells(
        what: MapWhat, radar: RadarView?, stations: Bool, days: [ThermalMapDay],
        hours: [String: ThermalMapHoursView], words: MapLayersWords
    ) -> [MapTimeCell] {
        var cells: [MapTimeCell] = []
        if what == .rain, let moments = radar?.moments {
            for (index, moment) in moments.enumerated() {
                cells.append(
                    MapTimeCell(
                        stop: .radar(index), short: "", label: moment.clock, isNarrow: true,
                        over: radarOver(moments.map(\.clock), index)
                    )
                )
            }
        }
        let measuredNow = what == .wind && stations
        if measuredNow {
            cells.append(MapTimeCell(stop: .stations, short: "", label: words.now, over: words.now, isNow: true))
        }
        for (index, day) in days.enumerated() {
            var ofDay: [MapTimeCell] = []
            if what == .thermals {
                ofDay.append(MapTimeCell(stop: .day(day.date), short: dayWord, label: day.name))
            }
            // Today from the hour that has begun: what came before it was
            // measured or is past. Which day is today the core says.
            let isToday = index == 0 && day.today
            let view = hours[day.date]
            let from = isToday ? Int(view?.firstHour ?? 0) : 0
            for (hour, mark) in (view?.hours ?? []).enumerated() where hour >= from {
                let isNow = isToday && hour == from && !measuredNow
                ofDay.append(
                    MapTimeCell(
                        stop: .hour(day.date, hour), short: String(mark.clockHour), label: "\(day.name), \(mark.label)",
                        over: isNow ? words.now : nil, isNow: isNow
                    )
                )
            }
            // The day's name over its first cell, unless "Now" stands there.
            if let first = ofDay.first, !ofDay.contains(where: \.isNow) {
                ofDay[0] = MapTimeCell(
                    stop: first.stop, short: first.short, label: first.label, isNarrow: first.isNarrow,
                    over: day.name, isNow: false
                )
            }
            cells.append(contentsOf: ofDay)
        }
        return cells
    }

    /// The cell "Day" on the thermals' line: the kind of day as a whole.
    static let dayWord = "Day"

    /// The time over a radar's moment: a full hour's, and the first
    /// moment's where no full hour follows soon. A short run of moments
    /// has none, its cells too narrow for the words, which the panel's
    /// line says.
    static func radarOver(_ clocks: [String], _ index: Int) -> String? {
        let soon = 6
        guard clocks.count >= soon, clocks.indices.contains(index) else { return nil }
        let clock = clocks[index]
        let first = index == 0 && !clocks.dropFirst().prefix(soon - 1).contains { $0.hasSuffix(":00") }
        return clock.hasSuffix(":00") || first ? clock : nil
    }

    /// The words of a key's line, short: up to its first comma. "Light
    /// rain, 0.2 to 1 mm an hour" is "Light rain"; the whole stands behind
    /// the "i".
    static func short(_ words: String) -> String {
        words.split(separator: ",", maxSplits: 1).first.map(String.init) ?? words
    }
}
