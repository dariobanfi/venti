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
/// row of their own that scrolls sideways where it does not fit, from one
/// edge of the card to the other.
struct ForecastLayerRow: View {
    let layers: [ThermalMapLayerChoice]
    @Binding var layer: ThermalHoursLayer
    /// How far the card's edge lies from the row: the row reaches it.
    var inset = VentiSpace.s3

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
            }
            .padding(.horizontal, inset)
            // Room for the chips' shadows, which the scrolling row would
            // cut off; taken back below, so that the row is as high as
            // its chips.
            .padding(.vertical, VentiSpace.s4)
        }
        .padding(.horizontal, -inset)
        .padding(.vertical, -VentiSpace.s4)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Layer")
        .accessibilityIdentifier("forecast-layers")
    }
}

/// The height of the wind: the seven levels are more than a row of chips
/// takes, so a menu, its label the level chosen.
struct ForecastLevelMenu: View {
    let levels: [ThermalMapLevel]
    /// The height of the wind above the sea, metres; `nil` at the ground.
    @Binding var heightM: Int32?

    var body: some View {
        let chosen = levels.first { $0.heightM == heightM } ?? levels.first
        Menu {
            ForEach(levels, id: \.name) { level in
                Button {
                    heightM = level.heightM
                } label: {
                    if level.heightM == chosen?.heightM {
                        Label(level.name, systemImage: "checkmark")
                    } else {
                        Text(level.name)
                    }
                }
            }
        } label: {
            HStack(spacing: VentiSpace.s1) {
                Text(chosen?.name ?? "")
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
        .accessibilityValue(chosen?.words ?? "")
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

    /// How the strip's width is dealt out among its cells, and which cell
    /// lies under a finger.
    struct Cells: Equatable {
        /// How many hours the strip has.
        let hours: Int
        /// Whether "Day" stands before them.
        let hasDay: Bool
        /// The strip's width.
        let width: CGFloat

        /// The gap between two cells.
        static let gap: CGFloat = 2
        /// "Day" is a word where an hour is two figures: as wide as two
        /// hours.
        static let dayShare: CGFloat = 2

        /// The width of an hour's cell.
        var hourWidth: CGFloat {
            let cells = hours + (hasDay ? 1 : 0)
            let shares = CGFloat(hours) + (hasDay ? Self.dayShare : 0)
            guard shares > 0 else { return 0 }
            return max((width - Self.gap * CGFloat(cells - 1)) / shares, 0)
        }

        /// The width of the cell "Day"; nothing where there is none.
        var dayWidth: CGFloat {
            hasDay ? hourWidth * Self.dayShare : 0
        }

        /// The hour under a finger at this distance from the strip's
        /// start, by its place among the hours; `nil` for "Day". Before
        /// the strip it is the first cell, past it the last.
        func hour(at x: CGFloat) -> Int? {
            let start = hasDay ? dayWidth + Self.gap : 0
            if hasDay, x < start - Self.gap / 2 {
                return nil
            }
            guard hours > 0, hourWidth > 0 else { return nil }
            let place = Int(((x - start + Self.gap / 2) / (hourWidth + Self.gap)).rounded(.down))
            return min(max(place, 0), hours - 1)
        }
    }

    var body: some View {
        CellStrip(
            cells: hours.map { String($0.clockHour) }, leading: hasDay ? "Day" : nil, chosen: $hour,
            name: "Hour", spoken: spoken, identifier: "forecast-hours"
        )
    }

    private var spoken: String {
        guard let hour, hours.indices.contains(hour) else { return "The whole day" }
        return hours[hour].label
    }
}

/// The pack's hour strip (`.v-hours`) itself, for the hours of a day and
/// for the frames of the rain radar: a cell for each with its words in it,
/// the chosen one marked; a tap chooses a cell and a finger drawn along
/// the strip chooses cell by cell. VoiceOver reads it as one value to
/// adjust.
struct CellStrip: View {
    /// The words of each cell.
    let cells: [String]
    /// A word in a first cell before them, twice as wide: "Day".
    var leading: String?
    /// Where the cells are too many to hold their words: the places of
    /// those whose words stand under the strip, each ending where its
    /// cell ends. The cells are then empty. `nil` for words in every cell.
    var named: Set<Int>?
    /// The chosen cell by its place among `cells`; `nil` for the first
    /// cell, where there is one.
    @Binding var chosen: Int?
    /// What VoiceOver calls the strip, and what it says is chosen.
    let name: String
    let spoken: String
    let identifier: String

    private typealias Cells = ForecastHourStrip.Cells

    /// The gap between two cells, for what stands beside the strip.
    static let gap = ForecastHourStrip.Cells.gap

    var body: some View {
        VStack(spacing: VentiSpace.s1) {
            GeometryReader { proxy in
                let cells = Cells(hours: self.cells.count, hasDay: leading != nil, width: proxy.size.width)
                HStack(spacing: Cells.gap) {
                    if let leading {
                        cell(words: leading, isChosen: chosen == nil, width: cells.dayWidth)
                    }
                    ForEach(self.cells.indices, id: \.self) { index in
                        cell(
                            words: named == nil ? self.cells[index] : "", isChosen: chosen == index,
                            width: cells.hourWidth
                        )
                    }
                }
                .contentShape(.rect)
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { drag in
                            let under = cells.hour(at: drag.location.x)
                            if under != chosen {
                                chosen = under
                            }
                        }
                )
            }
            .frame(height: VentiSize.touch)
            if let named {
                names(of: named)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(name)
        .accessibilityValue(spoken)
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: step(by: 1)
            case .decrement: step(by: -1)
            @unknown default: break
            }
        }
        .accessibilityIdentifier(identifier)
    }

    /// One cell: its words, on the primary colour when chosen.
    private func cell(words: String, isChosen: Bool, width: CGFloat) -> some View {
        Text(words)
            .font(.ventiType(.label))
            .monospacedDigit()
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            // A little air at the sides: in narrow cells the figures of
            // neighbours would else run into one number.
            .padding(.horizontal, VentiSpace.s1 / 2)
            .frame(width: width)
            .frame(maxHeight: .infinity)
            .foregroundStyle(.venti(isChosen ? .onPrimary : .textSecondary))
            .background(.venti(isChosen ? .primary : .surfaceRaised), in: .rect(cornerRadius: VentiRadius.s))
    }

    /// The words of the named cells in a line under the strip, as high as
    /// a line of them is, each ending where its cell ends.
    private func names(of named: Set<Int>) -> some View {
        Text(cells.last ?? "")
            .font(.ventiType(.caption))
            .monospacedDigit()
            .lineLimit(1)
            .hidden()
            .frame(maxWidth: .infinity)
            .overlay {
                GeometryReader { proxy in
                    let cells = Cells(hours: self.cells.count, hasDay: leading != nil, width: proxy.size.width)
                    let start = leading == nil ? 0 : cells.dayWidth + Cells.gap
                    ForEach(named.sorted().filter { self.cells.indices.contains($0) }, id: \.self) { index in
                        Text(self.cells[index])
                            .font(.ventiType(.caption))
                            .monospacedDigit()
                            .lineLimit(1)
                            .fixedSize()
                            .foregroundStyle(.venti(.textSecondary))
                            .frame(
                                width: start + CGFloat(index) * (cells.hourWidth + Cells.gap) + cells.hourWidth,
                                alignment: .trailing
                            )
                    }
                }
            }
    }

    /// One cell later or earlier, as VoiceOver's swipe asks.
    private func step(by offset: Int) {
        let places = (leading == nil ? [] : [nil]) + cells.indices.map { Optional($0) }
        guard let now = places.firstIndex(of: chosen) else { return }
        chosen = places[min(max(now + offset, 0), places.count - 1)]
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
                MapKeyLine(colour: ThermalKeyColour.onMap(line), words: line.words)
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
