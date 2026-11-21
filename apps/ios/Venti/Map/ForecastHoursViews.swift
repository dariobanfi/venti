import SwiftUI
import VentiCore

/// The parts of the forecast map's card for the hours of a day: the row of
/// layers, the level of the wind, the line of hours and a layer's key.

extension ThermalHoursLayer {
    /// The layer as the preference keeps it.
    var stored: String {
        switch self {
        case .thermals: "thermals"
        case .wind: "wind"
        case .gusts: "gusts"
        case .cloud: "cloud"
        case .rain: "rain"
        }
    }

    init(stored: String) {
        self =
            switch stored {
            case "wind": .wind
            case "gusts": .gusts
            case "cloud": .cloud
            case "rain": .rain
            default: .thermals
            }
    }
}

/// The layers the map offers, as the pack's chips of which one is on, in a
/// row that scrolls sideways where it does not fit; with the wind, the
/// level beside it as a menu.
struct ForecastLayerRow: View {
    let layers: [ThermalMapLayerChoice]
    @Binding var layer: ThermalHoursLayer
    let levels: [ThermalMapLevel]
    /// The height of the wind above the sea, metres; `nil` at the ground.
    @Binding var heightM: Int32?

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: VentiSpace.s2) {
                ForEach(layers, id: \.name) { choice in
                    VentiChip(
                        title: choice.name,
                        isOn: Binding(
                            get: { layer == choice.layer },
                            set: { if $0 { layer = choice.layer } }
                        )
                    )
                    .accessibilityIdentifier("forecast-layer-\(choice.layer.stored)")
                }
                if layers.first(where: { $0.layer == layer })?.hasLevels == true, !levels.isEmpty {
                    levelMenu
                }
            }
            .padding(.horizontal, VentiSpace.s1)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Layer")
        .accessibilityIdentifier("forecast-layers")
    }

    /// The seven levels are more than a row of chips takes: a menu, its
    /// label the level chosen.
    private var levelMenu: some View {
        let chosen = levels.first { $0.heightM == heightM } ?? levels[0]
        return Menu {
            ForEach(levels, id: \.name) { level in
                Button {
                    heightM = level.heightM
                } label: {
                    if level.heightM == chosen.heightM {
                        Label(level.name, systemImage: "checkmark")
                    } else {
                        Text(level.name)
                    }
                }
            }
        } label: {
            HStack(spacing: VentiSpace.s1) {
                Text(chosen.name)
                    .font(.ventiType(.label))
                    .lineLimit(1)
                Image(systemName: "chevron.down")
                    .font(.ventiType(.caption))
                    .accessibilityHidden(true)
            }
            .padding(.horizontal, VentiSpace.s3)
            .frame(height: 36)
            .foregroundStyle(.venti(.text))
            .ventiFloating(in: .capsule)
            .frame(minHeight: VentiSize.touch)
            .contentShape(.rect)
        }
        .accessibilityLabel("Height")
        .accessibilityValue(chosen.words)
        .accessibilityIdentifier("forecast-level")
    }
}

/// The line of hours, the pack's hour strip (`.v-hours`): a cell for each
/// of the day's hours with the clock's hour in it, the chosen one marked;
/// a tap chooses an hour and a finger drawn along the strip chooses hour
/// by hour. On "Thermals" a first cell, "Day", shows the day as a whole.
/// VoiceOver reads it as one value to adjust, the hour.
struct ForecastHourStrip: View {
    let hours: [ThermalMapHourMark]
    /// Whether the strip begins with "Day".
    let hasDay: Bool
    /// The chosen hour by its place among `hours`; `nil` for the day.
    @Binding var hour: Int?

    var body: some View {
        GeometryReader { proxy in
            HStack(spacing: 2) {
                if hasDay {
                    cell(words: "Day", isChosen: hour == nil)
                }
                ForEach(hours.indices, id: \.self) { index in
                    cell(words: String(hours[index].clockHour), isChosen: hour == index)
                }
            }
            .contentShape(.rect)
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { drag in
                        choose(at: drag.location.x, width: proxy.size.width)
                    }
            )
        }
        .frame(height: VentiSize.touch)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Hour")
        .accessibilityValue(spoken)
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: step(by: 1)
            case .decrement: step(by: -1)
            @unknown default: break
            }
        }
        .accessibilityIdentifier("forecast-hours")
    }

    /// One cell: the clock's hour, on the primary colour when chosen.
    private func cell(words: String, isChosen: Bool) -> some View {
        Text(words)
            .font(.ventiType(.label))
            .monospacedDigit()
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .foregroundStyle(.venti(isChosen ? .onPrimary : .textSecondary))
            .background(.venti(isChosen ? .primary : .surfaceRaised), in: .rect(cornerRadius: VentiRadius.s))
            .layoutPriority(words.count > 2 ? 1 : 0)
    }

    /// The cell under a finger at this distance from the strip's start.
    private func choose(at x: CGFloat, width: CGFloat) {
        let count = hours.count + (hasDay ? 1 : 0)
        guard count > 0, width > 0 else { return }
        let place = min(max(Int(x / width * CGFloat(count)), 0), count - 1)
        let chosen: Int? = hasDay ? (place == 0 ? nil : place - 1) : place
        if chosen != hour {
            hour = chosen
        }
    }

    /// One hour later or earlier, as VoiceOver's swipe asks.
    private func step(by offset: Int) {
        let places = (hasDay ? [nil] : []) + hours.indices.map { Optional($0) }
        guard let now = places.firstIndex(of: hour) else { return }
        hour = places[min(max(now + offset, 0), places.count - 1)]
    }

    private var spoken: String {
        guard let hour, hours.indices.contains(hour) else { return "The whole day" }
        return hours[hour].label
    }
}

/// A layer's key, the core's lines in the colours they name: two columns
/// where they fit, one where the text is large or the phone narrow.
struct ForecastHoursKey: View {
    let key: [ThermalMapKey]

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .top, spacing: VentiSpace.s4) {
                column(Array(key.prefix((key.count + 1) / 2)))
                column(Array(key.dropFirst((key.count + 1) / 2)))
            }
            column(key)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Key")
        .accessibilityIdentifier("forecast-key")
    }

    private func column(_ lines: [ThermalMapKey]) -> some View {
        VStack(alignment: .leading, spacing: VentiSpace.s1) {
            ForEach(lines, id: \.step) { line in
                HStack(alignment: .top, spacing: VentiSpace.s2) {
                    Rectangle()
                        .fill(ThermalKeyColour.onMap(line))
                        .overlay {
                            Rectangle()
                                .strokeBorder(.venti(.borderStrong), lineWidth: 1)
                        }
                        .frame(width: 16, height: 12)
                        .padding(.top, 3)
                        .accessibilityHidden(true)
                    Text(line.words)
                        .font(.ventiType(.label))
                        .foregroundStyle(.venti(.text))
                        .fixedSize(horizontal: false, vertical: true)
                }
                .accessibilityElement(children: .combine)
            }
        }
    }
}

/// A square of an hour of the forecast map that the pilot tapped, with
/// the square's days side by side.
struct ChosenHourSquare: Identifiable {
    let square: ThermalMapHourSquare
    let layer: ThermalHoursLayer
    /// Every day of the map for this square.
    let days: [ThermalStripDay]
    /// The day the map shows.
    let date: String
    /// The layer's note and the hours' note.
    let note: String
    let credits: [Credit]
    /// The day of the map, for the square's forecast to open on.
    let opening: ForecastOpening
    let id: String

    var middle: Coordinate {
        Coordinate(lat: (square.south + square.north) / 2, lon: (square.west + square.east) / 2)
    }
}

/// A tapped square of an hour, as a sheet: the square's days side by
/// side, a tap on one changing the map's day; the hour and the layer in
/// the core's words, with the status on "Thermals" and the arrow in its
/// band on "Wind" and "Gusts"; the model's ground; the way to the
/// forecast for its place; the notes and whose data it is.
struct ForecastHourSquareSheet: View {
    let chosen: ChosenHourSquare
    /// Changes the map's day.
    let choose: (String) -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: VentiSpace.s3) {
                    if !chosen.days.isEmpty {
                        days
                    }
                    Text(chosen.square.hour)
                        .font(.ventiType(.label))
                        .foregroundStyle(.venti(.textSecondary))
                    HStack(alignment: .top, spacing: VentiSpace.s3) {
                        if chosen.layer == .wind || chosen.layer == .gusts, let wind = chosen.square.wind {
                            ForecastArrow(mark: wind)
                        }
                        VStack(alignment: .leading, spacing: VentiSpace.s1) {
                            Text(chosen.square.title)
                                .font(.ventiType(.heading))
                                .accessibilityAddTraits(.isHeader)
                                .accessibilityIdentifier("forecast-hour-title")
                            if !chosen.square.detail.isEmpty {
                                Text(chosen.square.detail)
                                    .foregroundStyle(.venti(.textSecondary))
                                    .fixedSize(horizontal: false, vertical: true)
                                    .accessibilityIdentifier("forecast-hour-detail")
                            }
                        }
                        Spacer(minLength: 0)
                        if chosen.layer == .thermals, let cell = chosen.square.thermals {
                            VentiStatus(cell)
                                .accessibilityIdentifier("forecast-square-status")
                        }
                    }
                    if let ground = chosen.square.groundM {
                        Text("Judged at the model's ground, \(Format.metres(ground)) m")
                            .foregroundStyle(.venti(.textSecondary))
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    NavigationLink {
                        ForecastScreen(
                            place: chosen.middle, title: Format.place(lat: chosen.middle.lat, lon: chosen.middle.lon),
                            opening: chosen.opening
                        )
                    } label: {
                        Label("Forecast for this place", systemImage: "cloud.sun")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.venti(.quiet))
                    .accessibilityIdentifier("forecast-square-forecast")
                    VStack(alignment: .leading, spacing: VentiSpace.s2) {
                        if !chosen.note.isEmpty {
                            Text(chosen.note)
                        }
                        ForEach(chosen.credits, id: \.source) { credit in
                            Text(credit.text)
                        }
                    }
                    .font(.ventiType(.caption))
                    .foregroundStyle(.venti(.textSecondary))
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, VentiSpace.s2)
                }
                .padding(VentiSpace.s4)
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("forecast-hour-square")
            }
            .background(.venti(.bg))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }

    /// The square's days side by side: the day's name, and a mark in the
    /// step of its kind of day with its status, as Today's rows have them.
    /// A tap on another day shows that day on the map.
    private var days: some View {
        HStack(spacing: VentiSpace.s1) {
            ForEach(chosen.days, id: \.date) { day in
                let isChosen = day.date == chosen.date
                Button {
                    if !isChosen {
                        choose(day.date)
                        dismiss()
                    }
                } label: {
                    VStack(spacing: VentiSpace.s1) {
                        Text(day.name)
                            .font(.ventiType(.label))
                            .lineLimit(1)
                            .minimumScaleFactor(0.7)
                        Circle()
                            .fill(Self.mark(day.square))
                            .overlay { Circle().strokeBorder(.venti(.borderStrong), lineWidth: 1) }
                            .frame(width: 14, height: 14)
                            .accessibilityHidden(true)
                        Text(day.square?.status ?? "–")
                            .font(.ventiType(.caption))
                            .lineLimit(1)
                            .minimumScaleFactor(0.7)
                    }
                    .padding(.vertical, VentiSpace.s2)
                    .frame(maxWidth: .infinity, minHeight: VentiSize.touch)
                    .foregroundStyle(.venti(isChosen ? .onPrimary : .text))
                    .background(.venti(isChosen ? .primary : .surfaceRaised), in: .rect(cornerRadius: VentiRadius.s))
                }
                .buttonStyle(.plain)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(day.name)
                .accessibilityValue(
                    [day.square?.status, day.square?.thermalWords].compactMap { $0 }.filter { !$0.isEmpty }
                        .joined(separator: ", ")
                )
                .accessibilityAddTraits(isChosen ? .isSelected : [])
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("forecast-square-days")
    }

    /// The mark of a day: the step of its kind of day, the veil of a day
    /// that is not flyable, the surface where there is no estimate.
    private static func mark(_ cell: ThermalMapCell?) -> Color {
        guard let cell else { return Color.venti(.surface) }
        if let step = cell.scaleStep {
            return ThermalBand(scaleStep: step).colorOnMap
        }
        return cell.verdict == .notFlyable ? Color(uiColor: Palette.notFlyableOnMap) : Color.venti(.surface)
    }
}

/// The forecast's arrow for a square, in the colour of its band and turned
/// to where the wind blows, with its label; a dot for a calm.
struct ForecastArrow: View {
    let mark: WindMark

    var body: some View {
        VStack(spacing: VentiSpace.s1) {
            Image(uiImage: picture)
                .rotationEffect(.degrees(isArrow ? mark.headingDeg : 0))
                .accessibilityHidden(true)
            Text(mark.label)
                .font(.ventiType(.label))
                .monospacedDigit()
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Wind")
        .accessibilityValue(mark.label)
        .accessibilityIdentifier("forecast-hour-arrow")
    }

    private var isArrow: Bool {
        if case .arrow = mark.symbol { return true }
        return false
    }

    private var picture: UIImage {
        switch mark.symbol {
        case .arrow(let band): WindSymbols.arrow(color: band.uiColor)
        case .calm: WindSymbols.dot(color: WindBand.light.uiColor)
        case .old: WindSymbols.dot(color: Palette.quietOnMap)
        }
    }
}
